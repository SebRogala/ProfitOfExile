package lab

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
	"sync"

	"profitofexile/internal/league"
)

// FontOverviewMaxLimit is the largest limit a Font or Dedication overview
// request may ask for.
const FontOverviewMaxLimit = 500

// FontOverviewSource reads the latest Font and Dedication snapshots grouped:
// each one statement covering every variant, mode and pool.
type FontOverviewSource interface {
	FontOverviewRows(ctx context.Context, scope league.Scope) ([]FontResult, error)
	DedicationOverviewRows(ctx context.Context, scope league.Scope) ([]DedicationResult, error)
}

// FontColdFills shares the cold Font and Dedication overview loads of one
// league among the requests that overlap them. A load is held only while in
// flight: it is never stored, never marks a corpus warm and never reaches the
// analyzer's state, so the next request after it completes loads again. The
// load runs detached from any one caller; the last waiter leaving cancels it.
type FontColdFills struct {
	mu      sync.Mutex
	flights map[string]*overviewFlight
}

type overviewFlight struct {
	done    chan struct{}
	cancel  context.CancelFunc
	waiters int
	value   any
	err     error
	// panicked is a panic the load raised; each waiter re-raises it, as the
	// request would have raised it loading alone.
	panicked any
}

const (
	fontOverviewFlight       = "font"
	dedicationOverviewFlight = "dedication"
)

func newFontColdFills() *FontColdFills {
	return &FontColdFills{flights: make(map[string]*overviewFlight)}
}

// load joins the in-flight load of kind, starting it when none runs.
func (f *FontColdFills) load(ctx context.Context, kind string, read func(context.Context) (any, error)) (any, error) {
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	f.mu.Lock()
	fl, ok := f.flights[kind]
	if !ok {
		loadCtx, cancel := context.WithCancel(context.WithoutCancel(ctx))
		fl = &overviewFlight{done: make(chan struct{}), cancel: cancel}
		f.flights[kind] = fl
		go f.run(loadCtx, kind, fl, read)
	}
	fl.waiters++
	f.mu.Unlock()

	select {
	case <-fl.done:
		if fl.panicked != nil {
			panic(fl.panicked)
		}
		return fl.value, fl.err
	case <-ctx.Done():
		f.mu.Lock()
		fl.waiters--
		if fl.waiters == 0 && f.flights[kind] == fl {
			delete(f.flights, kind)
			fl.cancel()
		}
		f.mu.Unlock()
		return nil, ctx.Err()
	}
}

func (f *FontColdFills) run(ctx context.Context, kind string, fl *overviewFlight, read func(context.Context) (any, error)) {
	ctx, cancel := context.WithTimeout(ctx, sharedFillTimeout)
	defer cancel()
	var value any
	var err error
	defer func() {
		recovered := recover()
		if recovered != nil {
			slog.Error("lab: font overview load panicked", "kind", kind, "recover", recovered)
			value, err = nil, fmt.Errorf("lab: font overview load panicked: %v", recovered)
		}
		f.mu.Lock()
		if f.flights[kind] == fl {
			delete(f.flights, kind)
		}
		fl.cancel()
		fl.value, fl.err, fl.panicked = value, err, recovered
		close(fl.done)
		f.mu.Unlock()
	}()
	value, err = read(ctx)
}

// waiting reports how many callers wait on in-flight loads.
func (f *FontColdFills) waiting() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	n := 0
	for _, fl := range f.flights {
		n += fl.waiters
	}
	return n
}

// FontModes answers one Font overview request: from the analyzer's corpus
// when warm, otherwise from the grouped load it shares with every overlapping
// cold request (any variant, any limit), sliced to variant and limit. Warmth
// is checked again once the load completes, so an analyzer answer published
// meanwhile wins. cache may be nil; the load is then unshared.
func FontModes(ctx context.Context, src FontOverviewSource, cache *Cache, scope league.Scope, variant string, limit int) (FontAnalysis, error) {
	if analysis, warm := warmFont(cache, scope); warm {
		return sliceFontModes(analysis, variant, limit), nil
	}
	read := func(ctx context.Context) (any, error) {
		rows, err := src.FontOverviewRows(ctx, scope)
		if err != nil {
			return nil, err
		}
		return groupFontModes(rows), nil
	}
	v, err := loadOverview(ctx, cache, scope, fontOverviewFlight, read)
	if err != nil {
		return FontAnalysis{}, err
	}
	if analysis, warm := warmFont(cache, scope); warm {
		return sliceFontModes(analysis, variant, limit), nil
	}
	return sliceFontModes(v.(FontAnalysis), variant, limit), nil
}

// DedicationPools answers one Dedication overview request for variant: every
// warm row of the variant, or the limit most profitable per pool from the
// shared grouped load, with warmth checked again as in FontModes.
func DedicationPools(ctx context.Context, src FontOverviewSource, cache *Cache, scope league.Scope, variant string, limit int) (DedicationAnalysis, error) {
	if ded, warm := warmDedication(cache, scope); warm {
		return filterDedicationPools(ded, variant), nil
	}
	read := func(ctx context.Context) (any, error) {
		rows, err := src.DedicationOverviewRows(ctx, scope)
		if err != nil {
			return nil, err
		}
		return groupDedicationPools(rows), nil
	}
	v, err := loadOverview(ctx, cache, scope, dedicationOverviewFlight, read)
	if err != nil {
		return DedicationAnalysis{}, err
	}
	if ded, warm := warmDedication(cache, scope); warm {
		return filterDedicationPools(ded, variant), nil
	}
	return sliceDedicationPools(v.(DedicationAnalysis), variant, limit), nil
}

func loadOverview(ctx context.Context, cache *Cache, scope league.Scope, kind string, read func(context.Context) (any, error)) (any, error) {
	if cache == nil {
		return read(ctx)
	}
	return cache.For(scope).FontColdFills().load(ctx, kind, read)
}

func warmFont(cache *Cache, scope league.Scope) (FontAnalysis, bool) {
	if cache == nil {
		return FontAnalysis{}, false
	}
	return cache.For(scope).Font()
}

func warmDedication(cache *Cache, scope league.Scope) (DedicationAnalysis, bool) {
	if cache == nil {
		return DedicationAnalysis{}, false
	}
	return cache.For(scope).Dedication()
}

func filterDedicationPools(ded DedicationAnalysis, variant string) DedicationAnalysis {
	return DedicationAnalysis{
		Skills:       FilterDedicationVariant(ded.Skills, variant),
		Transfigured: FilterDedicationVariant(ded.Transfigured, variant),
	}
}

func sliceFontModes(all FontAnalysis, variant string, limit int) FontAnalysis {
	return FontAnalysis{
		Safe:    FilterFont(all.Safe, variant, limit),
		Premium: FilterFont(all.Premium, variant, limit),
		Jackpot: FilterFont(all.Jackpot, variant, limit),
	}
}

// groupFontModes splits grouped rows into their modes.
func groupFontModes(rows []FontResult) FontAnalysis {
	var out FontAnalysis
	for _, r := range rows {
		switch r.Mode {
		case "safe":
			out.Safe = append(out.Safe, r)
		case "premium":
			out.Premium = append(out.Premium, r)
		case "jackpot":
			out.Jackpot = append(out.Jackpot, r)
		}
	}
	return out
}

// groupDedicationPools splits grouped rows into their pools.
func groupDedicationPools(rows []DedicationResult) DedicationAnalysis {
	var out DedicationAnalysis
	for _, r := range rows {
		switch r.GemType {
		case "skill":
			out.Skills = append(out.Skills, r)
		case "transfigured":
			out.Transfigured = append(out.Transfigured, r)
		}
	}
	return out
}

// sliceDedicationPools keeps, per pool, the limit most profitable rows of
// variant across every mode, as one cold per-pool query did.
func sliceDedicationPools(all DedicationAnalysis, variant string, limit int) DedicationAnalysis {
	return DedicationAnalysis{
		Skills:       TopDedication(all.Skills, variant, limit),
		Transfigured: TopDedication(all.Transfigured, variant, limit),
	}
}

// FilterFont narrows Font results to variant ("" keeps every variant), orders
// them by profit descending and keeps the first limit.
func FilterFont(all []FontResult, variant string, limit int) []FontResult {
	var filtered []FontResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}
	sort.Slice(filtered, func(i, j int) bool {
		if filtered[i].Profit == nil {
			return false
		}
		return filtered[j].Profit == nil || *filtered[i].Profit > *filtered[j].Profit
	})
	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// TopDedication narrows Dedication results to variant and keeps the limit most
// profitable across every mode.
func TopDedication(all []DedicationResult, variant string, limit int) []DedicationResult {
	filtered := FilterDedicationVariant(all, variant)
	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Profit > filtered[j].Profit
	})
	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}
