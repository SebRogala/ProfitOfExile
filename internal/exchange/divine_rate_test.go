package exchange

import (
	"context"
	"errors"
	"testing"
	"time"
)

// ninjaAt is when the fallback's poe.ninja snapshot was taken.
var ninjaAt = time.Date(2026, 11, 13, 8, 0, 0, 0, time.UTC)

// warmDivineCache holds a warm one-hour default-horizon result at rate, with the
// hour's buy 196 and sell 203, shaped as the handlers' divineRateCache so
// LastUpdated answers feedHour.
func warmDivineCache(rate float64) *Cache {
	cache := NewCache()
	cache.Set(DefaultHorizon, Result{
		League:          "Allflame",
		From:            feedHour,
		To:              feedHour.Add(time.Hour),
		Hours:           1,
		DivineChaosRate: rate,
		DivineChaosBuy:  196,
		DivineChaosSell: 203,
	})
	return cache
}

func ninjaFallback(chaos float64, at time.Time) DivineRateFallback {
	return func(context.Context) (float64, time.Time, error) { return chaos, at, nil }
}

// refreshed builds a reader and refreshes its fallback once, failing the test
// when that refresh errors.
func refreshed(t *testing.T, cache *Cache, fallback DivineRateFallback) *DivineRateReader {
	t.Helper()
	r := NewDivineRateReader(cache, fallback)
	if err := r.RefreshFallback(context.Background()); err != nil {
		t.Fatalf("RefreshFallback: %v", err)
	}
	return r
}

func TestDivineRateReader_warmExchangeRateBeatsTheNinjaFallback(t *testing.T) {
	r := refreshed(t, warmDivineCache(199), ninjaFallback(180, ninjaAt))

	got := r.Current()

	want := DivineRate{Chaos: 199, Buy: 196, Sell: 203, PriceHour: feedHour, Source: DivineRateSourceExchange, ExchangeWarm: true}
	if got != want {
		t.Errorf("Current() = %+v, want %+v", got, want)
	}
}

func TestDivineRateReader_coldExchangeServesTheNinjaSnapshot(t *testing.T) {
	r := refreshed(t, NewCache(), ninjaFallback(180, ninjaAt))

	got := r.Current()

	want := DivineRate{Chaos: 180, PriceHour: ninjaAt, Source: DivineRateSourceNinja}
	if got != want {
		t.Errorf("Current() = %+v, want %+v", got, want)
	}
}

func TestDivineRateReader_warmHourWithoutADivineTradeServesTheNinjaSnapshot(t *testing.T) {
	r := refreshed(t, warmDivineCache(0), ninjaFallback(180, ninjaAt))

	got := r.Current()

	if got.Chaos != 180 || got.Source != DivineRateSourceNinja || !got.ExchangeWarm {
		t.Errorf("Current() = %+v, want Chaos 180, Source ninja, ExchangeWarm true", got)
	}
}

func TestDivineRateReader_noExchangeRateAndNoFallbackIsNoRate(t *testing.T) {
	r := NewDivineRateReader(NewCache(), ninjaFallback(180, ninjaAt))

	got := r.Current()

	if got.Chaos != 0 || got.Source != "" {
		t.Errorf("Current() = %+v, want Chaos 0, Source \"\" — the fallback was never refreshed", got)
	}
}

func TestDivineRateReader_failedRefreshKeepsTheHeldNinjaRate(t *testing.T) {
	fail := false
	r := refreshed(t, NewCache(), func(context.Context) (float64, time.Time, error) {
		if fail {
			return 0, time.Time{}, errors.New("db down")
		}
		return 180, ninjaAt, nil
	})
	fail = true

	err := r.RefreshFallback(context.Background())

	if err == nil {
		t.Error("RefreshFallback() = nil, want the fallback's error")
	}
	if got := r.Current().Chaos; got != 180 {
		t.Errorf("Current().Chaos = %v, want the held 180", got)
	}
}

func TestDivineRateReader_zeroNinjaAnswerKeepsTheHeldNinjaRate(t *testing.T) {
	answer := 180.0
	r := refreshed(t, NewCache(), func(context.Context) (float64, time.Time, error) {
		if answer == 0 {
			return 0, time.Time{}, nil
		}
		return answer, ninjaAt, nil
	})
	answer = 0

	if err := r.RefreshFallback(context.Background()); err != nil {
		t.Fatalf("RefreshFallback: %v", err)
	}

	if got := r.Current().Chaos; got != 180 {
		t.Errorf("Current().Chaos = %v, want the held 180", got)
	}
}

func TestDivineRateReader_nilReaderIsNoRate(t *testing.T) {
	var r *DivineRateReader

	if got := r.Current(); got != (DivineRate{}) {
		t.Errorf("Current() = %+v, want the zero DivineRate", got)
	}
	if err := r.RefreshFallback(context.Background()); err != nil {
		t.Errorf("RefreshFallback() = %v, want nil", err)
	}
}
