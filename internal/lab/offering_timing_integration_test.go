//go:build integration

package lab

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// The writer-side twin (POE-158). The tick used to store offering timings only
// when the result was non-empty, which leaves the cache COLD for a league with
// no fragment rows — and a cold cache is exactly what sends every subsequent
// MarketOverview request back to the ten queries the tick just ran.
//
// The observable is what the cache holds afterwards, not what the call returns:
// an assertion on the returned count passes with the bug present, because the
// count is zero either way.
func TestRefreshOfferingTimings_StoresAnEmptyAnswerRatherThanLeavingTheCacheCold(t *testing.T) {
	pool := labIntegrationPool(t)
	// A league that owns no fragment_snapshots rows, so every offering's
	// current-price query returns NULL and the computed answer is empty.
	scope := league.Historical("POE158-NoFragments")
	cache := NewCache(scope)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	n, err := RefreshOfferingTimings(ctx, NewRepository(pool), cache, scope)
	if err != nil {
		t.Fatalf("RefreshOfferingTimings: %v", err)
	}
	if n != 0 {
		t.Fatalf("computed %d offerings for a league with no fragment rows, want 0; the fixture is not exercising the empty answer", n)
	}

	stored := cache.For(scope).OfferingTiming()
	if stored == nil {
		t.Fatal("the tick stored nothing for an empty answer; the cache stays cold and every request recomputes it")
	}
	if string(stored) != "[]" {
		t.Errorf("cached offering timing = %s, want [] — `null` decodes to nil and reads back as never-stored", stored)
	}
}
