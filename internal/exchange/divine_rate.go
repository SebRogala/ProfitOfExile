package exchange

import (
	"context"
	"fmt"
	"math"
	"sync"
	"time"
)

// Sources a DivineRate names.
const (
	DivineRateSourceExchange = "exchange"
	DivineRateSourceNinja    = "ninja"
)

// DivineRate is one answer of DivineRateReader.Current.
type DivineRate struct {
	// Chaos is the chaos value of one divine; 0 means no rate (Source "").
	Chaos float64
	// Buy and Sell are the exchange hour's cheapest and dearest divine trade in
	// chaos; non-zero only with Source exchange.
	Buy, Sell float64
	// PriceHour is the exchange's newest hour (Source exchange, or Source "" on
	// a warm horizon, so /divine-rate's priceHour stays as it was), the
	// snapshot time (Source ninja), zero otherwise.
	PriceHour time.Time
	// Source is DivineRateSourceExchange, DivineRateSourceNinja or "".
	Source string
	// ExchangeWarm reports whether the default horizon is warm.
	ExchangeWarm bool
}

// DivineRateFallback reads the latest poe.ninja divine snapshot.
type DivineRateFallback func(ctx context.Context) (chaos float64, at time.Time, err error)

// DivineRateReader is the one app-wide divine→chaos rate (POE-339): the
// Currency Exchange's newest-hour VWAP; the poe.ninja snapshot only while the
// exchange has none (cold, or a warm hour that did not trade the market).
// Display and conversion only — BestPlays keeps valuing each hour at its own
// rate (ADR-016). Never queries: the ninja value is what RefreshFallback last
// stored.
type DivineRateReader struct {
	cache    *Cache
	fallback DivineRateFallback

	mu      sync.RWMutex
	ninja   float64
	ninjaAt time.Time
}

// NewDivineRateReader reads cache's default horizon, falling back to what
// fallback last answered. Either may be nil.
func NewDivineRateReader(cache *Cache, fallback DivineRateFallback) *DivineRateReader {
	return &DivineRateReader{cache: cache, fallback: fallback}
}

// Current returns the app-wide rate. A nil reader is no rate.
func (r *DivineRateReader) Current() DivineRate {
	if r == nil {
		return DivineRate{}
	}
	res, warm := r.cache.Snapshot(DefaultHorizon)
	ex := exchangeDivine{Chaos: res.DivineChaosRate, Buy: res.DivineChaosBuy, Sell: res.DivineChaosSell}
	if last, hasHour := LastUpdated(res); hasHour {
		ex.Hour = last
	}

	r.mu.RLock()
	ninja, ninjaAt := r.ninja, r.ninjaAt
	r.mu.RUnlock()
	return chooseDivineRate(ex, warm, ninja, ninjaAt)
}

// exchangeDivine is the exchange's newest hour as the rate rules read it: its
// divine/chaos VWAP (0 when the hour did not trade divine), spread, and hour
// (zero when there is none).
type exchangeDivine struct {
	Chaos, Buy, Sell float64
	Hour             time.Time
}

// chooseDivineRate holds the rate rules Current and DivineRateAsOf share: the
// exchange's newest-hour rate while warm and non-zero, else the ninja snapshot,
// else no rate (keeping the warm hour as PriceHour).
func chooseDivineRate(ex exchangeDivine, warm bool, ninja float64, ninjaAt time.Time) DivineRate {
	if warm && ex.Chaos > 0 {
		return DivineRate{
			Chaos:        ex.Chaos,
			Buy:          ex.Buy,
			Sell:         ex.Sell,
			PriceHour:    ex.Hour,
			Source:       DivineRateSourceExchange,
			ExchangeWarm: true,
		}
	}
	if ninja > 0 {
		return DivineRate{Chaos: ninja, PriceHour: ninjaAt, Source: DivineRateSourceNinja, ExchangeWarm: warm}
	}
	out := DivineRate{ExchangeWarm: warm}
	if warm {
		out.PriceHour = ex.Hour
	}
	return out
}

// RefreshFallback asks the fallback for the latest poe.ninja snapshot and holds
// it. A failed or non-positive answer keeps the held value; only a failure is
// returned. A nil reader or fallback does nothing.
func (r *DivineRateReader) RefreshFallback(ctx context.Context) error {
	if r == nil || r.fallback == nil {
		return nil
	}
	chaos, at, err := r.fallback(ctx)
	if err != nil {
		return fmt.Errorf("divine rate: ninja fallback: %w", err)
	}
	if math.IsNaN(chaos) || math.IsInf(chaos, 0) || chaos <= 0 {
		return nil
	}
	r.mu.Lock()
	r.ninja, r.ninjaAt = chaos, at
	r.mu.Unlock()
	return nil
}

// DivineRateAsOf is the answer Current would have given at asOf. rows are the
// window Recompute reads, anchored on the newest stored hour before asOf
// (Repository.NewestHourBefore, then LoadRows over the widest window), so a
// stalled feed still answers its last real hour; ninjaChaos and ninjaAt are
// the newest poe.ninja snapshot at or before asOf. Rows at or after asOf are
// ignored. Like Current, an hour without a divine trade serves the ninja rate
// rather than an older hour's VWAP. Never queries.
func DivineRateAsOf(rows []StoredRow, asOf time.Time, ninjaChaos float64, ninjaAt time.Time) DivineRate {
	var newest time.Time
	warm := false
	for _, r := range rows {
		if r.Hour.Before(asOf) && (!warm || r.Hour.After(newest)) {
			newest, warm = r.Hour, true
		}
	}

	var ex exchangeDivine
	if warm {
		var hourRows []Row
		for _, r := range rows {
			if r.Hour.Equal(newest) {
				hourRows = append(hourRows, r.Row)
			}
		}
		ex.Hour = newest
		ex.Chaos, _ = divineRateOf(hourRows)
		if buy, sell, ok := divineSpreadOf(hourRows); ok {
			ex.Buy, ex.Sell = buy, sell
		}
	}
	return chooseDivineRate(ex, warm, ninjaChaos, ninjaAt)
}
