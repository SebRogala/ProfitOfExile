package lab

import (
	"context"
	"fmt"
	"sort"
	"time"

	"profitofexile/internal/league"
)

// SparklineWindowHours is the rolling window served to sparkline consumers.
// Points older than now minus this many hours are dropped on merge.
const SparklineWindowHours = 12

// SparklineTailPoints is the minimum number of points a series keeps. A gem
// that stops appearing in snapshots (delisted, or priced out of the corpus)
// would otherwise decay to an empty sparkline inside SparklineWindowHours;
// keeping a short tail leaves the last known shape visible.
const SparklineTailPoints = 4

// sparklineMaxLookbackHours bounds how far back the tail may reach. Past this
// age a series is stale enough that showing nothing beats showing a flat line
// from last week.
const sparklineMaxLookbackHours = 168

// SparklineBounds bounds a sparkline read so the rows fetched match the rows the
// merge would retain. The cold read is the only caller that needs all three: it
// pulls WindowHours in full, then the newest TailPoints rows per series within
// LookbackHours. Reading LookbackHours flat instead costs roughly fourteen times
// the rows for the same cache contents.
type SparklineBounds struct {
	// WindowHours is the rolling window every series keeps in full.
	WindowHours int
	// TailPoints is the number of trailing rows a series keeps beyond the
	// window, so a delisted gem still renders its last known shape.
	TailPoints int
	// LookbackHours is how far back the tail may reach.
	LookbackHours int
}

// sparklineCacheBounds is what the cache asks the source for. It mirrors the
// retention mergeSparklineSeries applies, so the read and the merge cannot drift
// into fetching rows nothing keeps.
func sparklineCacheBounds() SparklineBounds {
	return SparklineBounds{
		WindowHours:   SparklineWindowHours,
		TailPoints:    SparklineTailPoints,
		LookbackHours: sparklineMaxLookbackHours,
	}
}

// sparklineKey identifies one cached price series: a gem name paired with the
// variant it was priced at. Prices for different variants are different markets
// and are never merged into one series.
type sparklineKey struct {
	name    string
	variant string
}

// sparklinePointAt pairs a point with its parsed timestamp so the merge sorts
// and windows on an instant rather than re-parsing the RFC3339 string.
type sparklinePointAt struct {
	pt SparklinePoint
	at time.Time
}

// mergeSparklineSeries folds incoming points into an existing series and
// applies the rolling window.
//
// The result is sorted ascending by time and deduplicated on the Time field,
// with the first occurrence winning — existing points are never replaced by an
// incoming point bearing the same timestamp, so a repeated pass over the same
// snapshot is a no-op rather than a duplicate append.
//
// Points with a Time that does not parse as RFC3339 are dropped: they cannot be
// windowed, and every producer writes time.Time.Format(time.RFC3339).
//
// Every point newer than now minus SparklineWindowHours is kept. When that
// leaves fewer than SparklineTailPoints, the series extends backwards into the
// older points until it reaches that count, but never past now minus
// sparklineMaxLookbackHours.
//
// The returned slice is always freshly allocated — never a reslice of either
// input. Reslicing the tail would pin the whole history backing array for a
// delisted gem, which never appends again and so never reallocates: a leak that
// lasts the rest of the league. Building fresh also preserves the Cache
// contract that stored data is immutable once assigned, which in-place
// compaction would break for a concurrent reader holding the slice header.
func mergeSparklineSeries(existing, incoming []SparklinePoint, now time.Time) []SparklinePoint {
	if len(existing) == 0 && len(incoming) == 0 {
		return nil
	}

	all := make([]sparklinePointAt, 0, len(existing)+len(incoming))
	all = appendParsedSparklinePoints(all, existing)
	all = appendParsedSparklinePoints(all, incoming)
	if len(all) == 0 {
		return nil
	}

	// Stable so that equal timestamps keep input order: existing before
	// incoming, which is what "first occurrence wins" means below.
	sort.SliceStable(all, func(i, j int) bool { return all[i].at.Before(all[j].at) })

	seen := make(map[string]struct{}, len(all))
	deduped := make([]sparklinePointAt, 0, len(all))
	for _, p := range all {
		if _, dup := seen[p.pt.Time]; dup {
			continue
		}
		seen[p.pt.Time] = struct{}{}
		deduped = append(deduped, p)
	}

	windowStart := now.Add(-SparklineWindowHours * time.Hour)
	tailFloor := now.Add(-sparklineMaxLookbackHours * time.Hour)

	// First index inside the rolling window.
	start := sort.Search(len(deduped), func(i int) bool {
		return deduped[i].at.After(windowStart)
	})

	// Extend backwards to the tail minimum, never past the lookback floor.
	for len(deduped)-start < SparklineTailPoints && start > 0 && deduped[start-1].at.After(tailFloor) {
		start--
	}

	kept := deduped[start:]
	if len(kept) == 0 {
		return nil
	}

	out := make([]SparklinePoint, len(kept))
	for i, p := range kept {
		out[i] = p.pt
	}
	return out
}

// sparklineSource is the population-time read the sparkline cache needs. The
// repository satisfies it; tests substitute a fake so the merge and high-water
// logic can be exercised without a database or a live tick loop.
type sparklineSource interface {
	SparklineWindow(ctx context.Context, scope league.Scope, since time.Time, bounds SparklineBounds) (map[sparklineKey][]SparklinePoint, map[sparklineKey][]SparklinePoint, error)
}

// populateSparklineCache reads the sparkline window and folds it into the
// cache, replacing both maps and the high-water mark in one assignment.
//
// The read is incremental: only rows newer than the cached high-water mark are
// fetched. A cold cache instead reads from scratch, bounded by
// sparklineCacheBounds to the window plus a per-series tail rather than the flat
// lookback — see SparklineBounds. Series that receive no incoming points are
// still re-merged so points aging out of the rolling window are trimmed.
//
// The call is idempotent by construction, which RunV2 requires — it runs twice
// per snapshot (the gem tick and the T+15-minute delayed recompute). A second
// pass over an unchanged snapshot reads no rows, merges nothing new (and the
// merge dedupes on timestamp regardless), observes no time past the mark, and
// so leaves the mark where it was.
func populateSparklineCache(ctx context.Context, src sparklineSource, cache *Cache, scope league.Scope, now time.Time) error {
	c := cache.For(scope)

	existing, existingCorrupted, highWater := c.sparklineSnapshot()
	since := highWater
	if len(existing) == 0 && len(existingCorrupted) == 0 {
		// Cold cache: load the whole window rather than trusting a mark that
		// no series backs.
		since = time.Time{}
	}

	incoming, incomingCorrupted, err := src.SparklineWindow(ctx, scope, since, sparklineCacheBounds())
	if err != nil {
		return fmt.Errorf("populate sparkline cache: %w", err)
	}

	merged, observed := mergeSparklineMaps(existing, incoming, now)
	mergedCorrupted, observedCorrupted := mergeSparklineMaps(existingCorrupted, incomingCorrupted, now)

	if observedCorrupted.After(observed) {
		observed = observedCorrupted
	}
	// Advance only to what was actually seen; an empty read leaves the mark.
	if observed.After(highWater) {
		highWater = observed
	}

	c.SetSparklines(merged, mergedCorrupted, highWater)
	return nil
}

// mergeSparklineMaps folds incoming into existing key by key, returning a fresh
// map and the newest timestamp observed among the incoming points.
//
// Keys present only in existing are re-merged with no incoming points so the
// rolling window still trims them. Keys whose merge leaves nothing are dropped
// rather than kept as empty series.
func mergeSparklineMaps(existing, incoming map[sparklineKey][]SparklinePoint, now time.Time) (map[sparklineKey][]SparklinePoint, time.Time) {
	out := make(map[sparklineKey][]SparklinePoint, len(existing)+len(incoming))
	var newest time.Time

	for k, pts := range existing {
		if merged := mergeSparklineSeries(pts, incoming[k], now); len(merged) > 0 {
			out[k] = merged
		}
	}
	for k, pts := range incoming {
		if _, done := existing[k]; !done {
			if merged := mergeSparklineSeries(nil, pts, now); len(merged) > 0 {
				out[k] = merged
			}
		}
		if at := newestSparklineTime(pts); at.After(newest) {
			newest = at
		}
	}

	return out, newest
}

// newestSparklineTime returns the newest parsable timestamp among pts (zero
// when pts is empty or nothing parses). Unparsable points are skipped for the
// same reason mergeSparklineSeries drops them: they cannot be positioned in
// time, so they must not be allowed to move the high-water mark.
func newestSparklineTime(pts []SparklinePoint) time.Time {
	var newest time.Time
	for _, p := range pts {
		at, err := time.Parse(time.RFC3339, p.Time)
		if err != nil {
			continue
		}
		if at.After(newest) {
			newest = at
		}
	}
	return newest
}

// sparklineSnapshot returns both cached maps and the high-water mark in one
// lock acquisition. The maps are the stored ones: callers read them and build
// replacements, never mutate them in place — the Cache contract is that stored
// data is immutable once assigned.
func (c *Cache) sparklineSnapshot() (map[sparklineKey][]SparklinePoint, map[sparklineKey][]SparklinePoint, time.Time) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.sparklines, c.sparklinesCorrupted, c.sparklineHighWater
}

// appendParsedSparklinePoints appends pts to dst, dropping any point whose Time
// is not valid RFC3339.
func appendParsedSparklinePoints(dst []sparklinePointAt, pts []SparklinePoint) []sparklinePointAt {
	for _, p := range pts {
		at, err := time.Parse(time.RFC3339, p.Time)
		if err != nil {
			continue
		}
		dst = append(dst, sparklinePointAt{pt: p, at: at})
	}
	return dst
}
