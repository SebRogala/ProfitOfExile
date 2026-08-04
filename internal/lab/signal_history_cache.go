package lab

import (
	"context"
	"fmt"
	"sort"

	"profitofexile/internal/league"
)

// SignalHistoryDepth is how many transitions each gem/variant ring keeps. The
// endpoint caps its limit at 20, so a deeper ring could never be served and a
// shallower one would answer a legal request short.
//
// Sizing, measured 2026-08-04: ~7,300 (name, variant) series × 20 × ~80 B ≈
// 12 MB, against 1,788 MB available on the box.
const SignalHistoryDepth = 20

// signalKey identifies one gem's transition ring. Variant is part of the key for
// the same reason it is part of a sparkline's: a 20/20 gem and a 1/0 gem are
// different markets and their signals are never one series.
type signalKey struct {
	name    string
	variant string
}

// signalHistorySource is the seed read the ring cache needs. The repository
// satisfies it; tests substitute a fake so the seed-once decision and the merge
// can be exercised without a database.
type signalHistorySource interface {
	SignalHistoryWindow(ctx context.Context, scope league.Scope, depth int) (map[signalKey][]SignalChange, error)
}

// populateSignalHistory folds this tick's signals into the rings, seeding from
// the database on the first pass only.
//
// THE COLD WINDOW, AND WHY THIS SEEDS RATHER THAN FILLING UP
//
// Every other cached field is complete after one tick. A ring is not: appending
// one point per tick, at roughly one snapshot every 30 minutes, it takes ~10
// hours to reach SignalHistoryDepth and ~2 hours to reach the endpoint's default
// limit of 4. Deploys are frequent enough that "wait for it to fill" means the
// endpoint spends much of its life partially cold — and a partially filled ring
// is worse than an empty one, because it answers short instead of falling back.
//
// So the first pass pays for one bounded query and every pass after it is free.
// The alternative — accept the repository fallback for the first hours after
// each deploy — was rejected because those hours are exactly when the cost
// lands: this endpoint is called three times per gem scan from the desktop's
// in-game path, where the query's 5.3 ms of planning over 925 planning buffers
// dwarfs its 0.348 ms of execution.
//
// Idempotent, as RunV2 requires — it runs twice per snapshot. The merge dedupes
// on timestamp and the seed is skipped once the rings are warm, so the second
// pass issues no query and changes nothing.
func populateSignalHistory(ctx context.Context, src signalHistorySource, cache *Cache, scope league.Scope, signals []GemSignal, features []GemFeature) error {
	c := cache.For(scope)

	incoming := signalChangesFromTick(signals, features)

	existing, warm := c.signalHistorySnapshot()
	if !warm {
		seeded, err := src.SignalHistoryWindow(ctx, scope, SignalHistoryDepth)
		if err != nil {
			return fmt.Errorf("seed signal history cache: %w", err)
		}
		existing = seeded
	}

	merged := make(map[signalKey][]SignalChange, len(existing)+len(incoming))
	for k, ring := range existing {
		if out := mergeSignalHistory(ring, incoming[k], SignalHistoryDepth); len(out) > 0 {
			merged[k] = out
		}
	}
	for k, pts := range incoming {
		if _, done := existing[k]; done {
			continue
		}
		if out := mergeSignalHistory(nil, pts, SignalHistoryDepth); len(out) > 0 {
			merged[k] = out
		}
	}

	c.SetSignalHistory(merged)
	return nil
}

// signalChangesFromTick pairs each signal with the feature computed for the same
// gem in the same pass, producing the rows the endpoint serves without querying.
//
// A signal with no matching feature keeps zeroed price and velocity fields,
// matching the LEFT JOIN plus COALESCE the repository query uses — a gem whose
// feature row is missing is still a transition worth showing.
func signalChangesFromTick(signals []GemSignal, features []GemFeature) map[signalKey][]SignalChange {
	byKey := make(map[signalKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		byKey[signalKey{name: f.Name, variant: f.Variant}] = f
	}

	out := make(map[signalKey][]SignalChange, len(signals))
	for _, s := range signals {
		k := signalKey{name: s.Name, variant: s.Variant}
		change := SignalChange{
			Time:     s.Time,
			Signal:   s.Signal,
			Window:   s.WindowSignal,
			Advanced: s.AdvancedSignal,
		}
		if f := byKey[k]; f != nil {
			change.PriceVel = f.VelLongPrice
			change.ListVel = f.VelLongListing
			change.Price = f.Chaos
			change.Listings = f.Listings
		}
		out[k] = append(out[k], change)
	}
	return out
}

// mergeSignalHistory folds incoming transitions into a ring and truncates it to
// depth.
//
// The result is newest first — the order the endpoint serves and the repository
// query's ORDER BY time DESC — and deduplicated on Time with the existing point
// winning, so a repeated pass over a snapshot is a no-op rather than a duplicate
// entry. Existing wins because the stored rows do too: SaveGemSignals inserts
// ON CONFLICT DO NOTHING, so a recompute of the same timestamp leaves the
// database holding the first answer, and the cache must not disagree with it.
//
// The returned slice is always freshly allocated, never a reslice of either
// input. Truncating in place would break the cache's immutability contract for a
// reader holding the ring it read a moment ago, and reslicing the head would pin
// the whole backing array of a gem that stops producing signals.
func mergeSignalHistory(existing, incoming []SignalChange, depth int) []SignalChange {
	if depth <= 0 || (len(existing) == 0 && len(incoming) == 0) {
		return nil
	}

	all := make([]SignalChange, 0, len(existing)+len(incoming))
	all = append(all, existing...)
	all = append(all, incoming...)

	// Stable so equal timestamps keep input order: existing before incoming,
	// which is what "existing wins" means below.
	sort.SliceStable(all, func(i, j int) bool { return all[i].Time.After(all[j].Time) })

	out := make([]SignalChange, 0, depth)
	seen := make(map[int64]struct{}, len(all))
	for _, c := range all {
		at := c.Time.UnixNano()
		if _, dup := seen[at]; dup {
			continue
		}
		seen[at] = struct{}{}
		out = append(out, c)
		if len(out) == depth {
			break
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// signalHistorySnapshot returns the stored rings and their warmth in one lock
// acquisition. The map is the stored one: callers read it and build a
// replacement, never mutate it in place.
func (c *Cache) signalHistorySnapshot() (map[signalKey][]SignalChange, bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.signalHistory, c.signalHistorySet
}
