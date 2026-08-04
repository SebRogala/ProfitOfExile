//go:build integration

package handlers

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

func offeringIntegrationPool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(pool.Close)

	var exists bool
	if err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'fragment_snapshots')").
		Scan(&exists); err != nil {
		t.Fatalf("check fragment_snapshots table: %v", err)
	}
	if !exists {
		t.Skip("fragment_snapshots table not found, skipping (migrations not applied)")
	}
	return pool
}

// The production sequence behind POE-152's dormant half, measured against a real
// database: a league with no fragment_snapshots rows makes ComputeOfferingTimings
// return nothing, and that nothing must be stored so the next request is served
// from the cache instead of re-running the ten queries that produced it.
//
// The acquire count is the observable. It is the direct form of the acceptance
// criterion — "served from cache, zero database work" — rather than a proxy for
// it, and it fails if the empty answer is either not stored or not trusted.
func TestMarketOverview_EmptyOfferingTimingIsStoredAndThenServedWithoutTheDatabase(t *testing.T) {
	pool := offeringIntegrationPool(t)
	// A league that owns no fragment_snapshots rows: nothing is inserted for it,
	// so every offering's current-price query returns NULL.
	scope := league.Historical("POE152-NoFragments")
	cache := lab.NewCache(scope)
	handler := MarketOverview(cache, pool, scope)

	serve := func() {
		t.Helper()
		w := httptest.NewRecorder()
		handler.ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
		if w.Code != http.StatusOK {
			t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
		}
	}

	beforeFirst := pool.Stat().AcquireCount()
	serve()
	if pool.Stat().AcquireCount() == beforeFirst {
		t.Fatal("first request did not query the database; the test is not exercising the compute path")
	}

	stored := cache.For(scope).OfferingTiming()
	if stored == nil {
		t.Fatal("cache holds no offering timing after a request that computed an empty one; every later request will recompute it")
	}
	if string(stored) != "[]" {
		t.Errorf("cached offering timing = %s, want []", stored)
	}

	beforeSecond := pool.Stat().AcquireCount()
	serve()
	if got := pool.Stat().AcquireCount(); got != beforeSecond {
		t.Errorf("second request acquired %d connection(s), want 0 — the empty answer was recomputed", got-beforeSecond)
	}
}
