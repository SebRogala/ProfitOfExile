package lab

import (
	"sort"
	"time"
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
