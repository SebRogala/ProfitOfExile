//go:build integration

package main

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/trade"
)

func TestSeedCaches_seedsThePriceBookFromStoredLookups(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM trade_lookups WHERE league = $1`, scope.ID()); err != nil {
			t.Logf("cleanup warning: delete trade_lookups for league %q: %v", scope.ID(), err)
		}
	})
	fetched := time.Now().Add(-time.Hour)
	// Trade S3: 40c, 50c, 55c in the 20/20 uncorrupted market → median 50.
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO trade_lookups (league, time, gem, variant, total_listings, price_floor, listings)
		VALUES ($1, $2, 'Seed Caches Gem', '20/20', 3, 40,
		        '[{"price":40,"currency":"chaos","gemLevel":20,"gemQuality":20,"corrupted":false},
		          {"price":50,"currency":"chaos","gemLevel":20,"gemQuality":20,"corrupted":false},
		          {"price":55,"currency":"chaos","gemLevel":20,"gemQuality":20,"corrupted":false}]'::jsonb)`,
		scope.ID(), fetched); err != nil {
		t.Fatalf("seed trade_lookups: %v", err)
	}
	seedCachesSnapshot(t, pool, scope, fetched.Add(-time.Hour))

	labRepo := lab.NewRepository(pool)
	r := &recomputeRuntime{ctx: context.Background(), scope: scope, pool: pool, labCache: lab.NewCache(scope),
		latestSnapshotTimes: labRepo.LatestTwoSnapshotTimes,
		priceBook:           newPriceBook(scope, trade.NewRepository(pool), labRepo, nil)}
	r.seedCaches()

	got, ok := r.priceBook.Get(resolve.GemKey(scope.ID(), "Seed Caches Gem", "20/20"))
	if !ok {
		t.Fatal("price book Get ok = false, want the seeded trade price")
	}
	if got.Source != resolve.SourceTrade || got.Chaos != 50 {
		t.Errorf("price book Get = %s %v, want trade 50", got.Source, got.Chaos)
	}
}
