//go:build integration

package main

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/price/book"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/trade"
)

func newSchedulerForSeed(scope league.Scope) (*book.Scheduler, error) {
	return book.NewScheduler(scope, lab.GemPricePolicy(), 5*time.Minute)
}

func TestTradeSchedulerSeed_DurableAliasesScopeOldChecks(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)
	other := seedCachesLeague(t, pool)
	ctx := context.Background()
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, "DELETE FROM trade_lookups WHERE league = $1 OR league = $2", scope.ID(), other.ID()); err != nil {
			t.Error(err)
		}
	})
	now := time.Now().UTC().Truncate(time.Second)
	for _, name := range []string{"A of Test", "B of Test"} {
		if _, err := pool.Exec(ctx, `INSERT INTO gem_snapshots (league,time,name,variant,is_corrupted,is_transfigured,chaos,listings,gem_color) VALUES ($1,$2,$3,'1',false,true,1,1,'GREEN')`, scope.ID(), now, name); err != nil {
			t.Fatal(err)
		}
	}
	repo := trade.NewRepository(pool)
	for _, row := range []struct {
		name, variant string
		age           time.Duration
	}{
		{"A of Test", "1", 30 * time.Hour}, {"A of Test", "1/0", time.Minute}, {"B of Test", "1", 25 * time.Hour},
		{trade.FixedTargetLens60Outcome, trade.FixedTargetVariantFont, time.Minute},
	} {
		if err := repo.InsertTradeLookup(ctx, scope, &trade.TradeLookupResult{Gem: row.name, Variant: row.variant, FetchedAt: now.Add(-row.age), Listings: []trade.TradeListingDetail{}}, "desktop-native"); err != nil {
			t.Fatal(err)
		}
	}
	if err := repo.InsertTradeLookup(ctx, other, &trade.TradeLookupResult{Gem: "B of Test", Variant: "1", FetchedAt: now, Listings: []trade.TradeListingDetail{}}, "desktop-native"); err != nil {
		t.Fatal(err)
	}
	// The real repository readers replace the harness inputs; HTTP remains local.
	s, _, _, requests := schedulerHarness(t, &schedulerInputs{})
	s.scope = scope
	queue, err := newSchedulerForSeed(scope)
	if err != nil {
		t.Fatal(err)
	}
	s.queue = queue
	s.gems = lab.NewRepository(pool)
	s.checks = repo
	s.now = func() time.Time { return now }
	s.Seed(ctx)
	stats := s.queue.Stats(now)
	if stats.Demand != 2 || stats.NeverChecked != 0 || stats.Overdue != 1 {
		t.Fatalf("durable seed: %+v", stats)
	}
	d, ok := s.queue.Take(now, resolve.Rate{Chaos: 200})
	if !ok || d.Key.Name != "B of Test" || d.Key.Variant != "1" || d.Key.League != scope.ID() {
		t.Fatalf("durable selected %+v %v", d.Key, ok)
	}
	noSchedulerRequest(t, requests)
}

func TestTradeSchedulerSeed_NoSnapshot(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)
	s, _, _, _ := schedulerHarness(t, &schedulerInputs{})
	s.scope = scope
	q, err := newSchedulerForSeed(scope)
	if err != nil {
		t.Fatal(err)
	}
	s.queue = q
	s.gems = lab.NewRepository(pool)
	s.checks = trade.NewRepository(pool)
	s.Seed(context.Background())
	if stats := s.queue.Stats(time.Now()); stats.Demand != 0 {
		t.Fatalf("no snapshot demand: %+v", stats)
	}
	if d, ok := s.queue.Take(time.Now(), resolve.Rate{}); ok {
		t.Fatalf("no snapshot selected %+v", d.Key)
	}
}

func TestTradeSchedulerSeed_MalformedListingsRemainSuccessfulCheck(t *testing.T) {
	for _, listings := range []string{`{}`, `[]`} {
		t.Run(listings, func(t *testing.T) {
			pool, _ := requireDatabase(t)
			scope := seedCachesLeague(t, pool)
			ctx := context.Background()
			now := time.Now().UTC().Truncate(time.Second)
			t.Cleanup(func() {
				if _, err := pool.Exec(ctx, "DELETE FROM trade_lookups WHERE league=$1", scope.ID()); err != nil {
					t.Error(err)
				}
			})
			if _, err := pool.Exec(ctx, `INSERT INTO gem_snapshots (league,time,name,variant,is_transfigured,chaos,listings,gem_color) VALUES ($1,$2,'Siege Ballista of Splintering','1',true,1,1,'GREEN')`, scope.ID(), now); err != nil {
				t.Fatal(err)
			}
			if _, err := pool.Exec(ctx, `INSERT INTO trade_lookups (league,time,gem,variant,listings) VALUES ($1,$2,'Siege Ballista of Splintering','1',$3::jsonb)`, scope.ID(), now, listings); err != nil {
				t.Fatal(err)
			}
			s, _, _, _ := schedulerHarness(t, &schedulerInputs{})
			s.scope = scope
			q, err := newSchedulerForSeed(scope)
			if err != nil {
				t.Fatal(err)
			}
			s.queue = q
			s.gems = lab.NewRepository(pool)
			s.checks = trade.NewRepository(pool)
			s.Seed(ctx)
			if stats := s.queue.Stats(now); stats.Demand != 1 || stats.NeverChecked != 0 {
				t.Fatalf("empty/malformed check lost: %+v", stats)
			}
			if d, ok := s.queue.Take(now, resolve.Rate{}); ok {
				t.Fatalf("fresh empty selected %+v", d.Key)
			}
		})
	}
}
