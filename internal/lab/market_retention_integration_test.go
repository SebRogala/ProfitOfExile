//go:build integration

package lab

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// POE-399: market hypertables keep 21 days. The two endpoints that read the
// longest window — /api/analysis/history (signal history, 14 days) and offering
// timing (fragment medians, 14 days) — must answer the same once everything
// older than 21 days is gone. The SQL already bounds both reads to 14 days, so
// these are regression guards: widening either window past the retention period
// makes the fixtures' old rows count before the delete and not after it.
//
// The delete is a row-level emulation of the retention drop, scoped to the test
// league. run_job / drop_chunks would drop every league's old chunks in the
// shared integration database. That the real job is 21 days is pinned by
// internal/league/tables_integration_test.go.

const marketRetentionDays = 21

// retentionTestLeague registers a league unique to this run and deletes its
// rows from tables when the test ends, before the league itself (LIFO).
func retentionTestLeague(t *testing.T, pool *pgxpool.Pool, prefix string, tables ...string) league.Scope {
	t.Helper()
	id := fmt.Sprintf("%s-%d", prefix, time.Now().UnixNano())
	registerLeague(t, pool, id)
	t.Cleanup(func() {
		for _, table := range tables {
			if _, err := pool.Exec(context.Background(),
				fmt.Sprintf("DELETE FROM %s WHERE league = $1", table), id); err != nil {
				t.Logf("cleanup warning: delete %s rows of league %q: %v", table, id, err)
			}
		}
	})
	return league.Historical(id)
}

// deleteRowsPastRetention removes the league's rows older than the retention
// period from each table and returns how many went.
func deleteRowsPastRetention(t *testing.T, pool *pgxpool.Pool, scope league.Scope, tables ...string) int64 {
	t.Helper()
	var total int64
	for _, table := range tables {
		tag, err := pool.Exec(context.Background(),
			fmt.Sprintf("DELETE FROM %s WHERE league = $1 AND time < NOW() - make_interval(days => $2)", table),
			scope.ID(), marketRetentionDays)
		if err != nil {
			t.Fatalf("delete %s rows past retention: %v", table, err)
		}
		total += tag.RowsAffected()
	}
	return total
}

// seededSignalHistory is what the endpoint serves from a warm cache: a fresh
// cache seeded from SignalHistoryWindow (a warm cache never reads the database
// again, so each read needs its own).
func seededSignalHistory(t *testing.T, ctx context.Context, repo *Repository, scope league.Scope, name, variant string) []SignalChange {
	t.Helper()
	cache := NewCache(scope)
	if err := populateSignalHistory(ctx, repo, cache, scope, nil, nil); err != nil {
		t.Fatalf("seed signal history cache: %v", err)
	}
	return cache.For(scope).SignalHistory(name, variant)
}

func TestSignalHistory_SameAnswerOnceRowsOlderThanRetentionAreDropped(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	repo := NewRepository(pool)
	scope := retentionTestLeague(t, pool, "POE-399-sighist", "gem_signals", "gem_features")

	const name = "POE399 Retention Gem"
	const variant = "20/20"
	now := time.Now().UTC().Truncate(time.Second)

	// Inside the 14-day window, plus rows past 21 days whose signal and price
	// would show up in the answer if either read reached them.
	type row struct {
		age    time.Duration
		signal string
		chaos  float64
	}
	rows := []row{
		{24 * time.Hour, "RISING", 110},
		{6 * 24 * time.Hour, "STABLE", 120},
		{13 * 24 * time.Hour, "FALLING", 130},
		{23 * 24 * time.Hour, "OLD-23D", 9999},
		{35 * 24 * time.Hour, "OLD-35D", 9998},
	}
	for _, r := range rows {
		tm := now.Add(-r.age)
		if _, err := repo.SaveGemSignals(ctx, scope, []GemSignal{{
			Time: tm, Name: name, Variant: variant,
			Signal: r.signal, WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW",
		}}); err != nil {
			t.Fatalf("SaveGemSignals at %v: %v", tm, err)
		}
		if _, err := repo.SaveGemFeatures(ctx, scope, []GemFeature{{
			Time: tm, Name: name, Variant: variant, Tier: "LOW", MarketRegime: "TEMPORAL",
			VelLongPrice: 1.5, VelLongListing: 2.5, Chaos: r.chaos, Listings: 7,
		}}); err != nil {
			t.Fatalf("SaveGemFeatures at %v: %v", tm, err)
		}
	}

	coldBefore, err := repo.SignalHistory(ctx, scope, name, variant, SignalHistoryDepth)
	if err != nil {
		t.Fatalf("SignalHistory before: %v", err)
	}
	warmBefore := seededSignalHistory(t, ctx, repo, scope, name, variant)
	for label, got := range map[string][]SignalChange{"cold": coldBefore, "warm": warmBefore} {
		if len(got) != 3 || got[0].Signal != "RISING" || got[0].Price != 110 {
			t.Fatalf("%s history before the delete = %+v, want the three in-window transitions newest first (RISING at 110 first)", label, got)
		}
	}

	if n := deleteRowsPastRetention(t, pool, scope, "gem_signals", "gem_features"); n != 4 {
		t.Fatalf("deleted %d rows past %d days, want 4 (two signals, two features); the fixture is not exercising the drop", n, marketRetentionDays)
	}

	coldAfter, err := repo.SignalHistory(ctx, scope, name, variant, SignalHistoryDepth)
	if err != nil {
		t.Fatalf("SignalHistory after: %v", err)
	}
	if !reflect.DeepEqual(coldBefore, coldAfter) {
		t.Errorf("cold history changed after dropping rows past %d days:\nbefore %+v\nafter  %+v", marketRetentionDays, coldBefore, coldAfter)
	}
	warmAfter := seededSignalHistory(t, ctx, repo, scope, name, variant)
	if !reflect.DeepEqual(warmBefore, warmAfter) {
		t.Errorf("seeded-cache history changed after dropping rows past %d days:\nbefore %+v\nafter  %+v", marketRetentionDays, warmBefore, warmAfter)
	}
}

func TestOfferingTimings_SameAnswerOnceRowsOlderThanRetentionAreDropped(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()
	repo := NewRepository(pool)
	scope := retentionTestLeague(t, pool, "POE-399-offering", "fragment_snapshots")

	const fragmentID = "offer-gift"
	now := time.Now().UTC().Truncate(time.Second)
	insert := func(at time.Time, chaos float64) {
		if _, err := pool.Exec(ctx,
			`INSERT INTO fragment_snapshots (league, time, fragment_id, chaos) VALUES ($1, $2, $3, $4)`,
			scope.ID(), at, fragmentID, chaos); err != nil {
			t.Fatalf("seed fragment at %v: %v", at, err)
		}
	}

	// Eight hours of the day on each of days 1..13 back: 13 rows per hour (gate
	// >= 3) and at least 8 per weekday (gate >= 5) on every weekday. The extra
	// 30 minutes keeps every row clear of the 3- and 14-day boundaries while the
	// two computations run. Prices vary by hour and day so the medians mean
	// something; the rows past 21 days are priced far above them, so including
	// them would move every median they touch.
	for d := 1; d <= 13; d++ {
		for k := 0; k < 8; k++ {
			at := now.Add(-time.Duration(d)*24*time.Hour - time.Duration(k)*3*time.Hour - 30*time.Minute)
			insert(at, float64(100+10*k+d))
		}
	}
	for _, d := range []int{23, 35} {
		for k := 0; k < 8; k++ {
			insert(now.Add(-time.Duration(d)*24*time.Hour-time.Duration(k)*3*time.Hour-30*time.Minute), 5000)
		}
	}

	before := repo.OfferingTimings(ctx, scope)
	if len(before) != 1 {
		t.Fatalf("offerings before the delete = %+v, want exactly the seeded %s", before, fragmentID)
	}
	ot := before[0]
	if len(ot.HourlyMedians) != 8 || len(ot.CheapHours) == 0 || len(ot.ExpensiveHours) == 0 ||
		len(ot.CheapDays) == 0 || len(ot.ExpensiveDays) == 0 {
		t.Fatalf("offering timing before the delete = %+v, want 8 hourly medians and populated cheap/expensive hours and days", ot)
	}

	if n := deleteRowsPastRetention(t, pool, scope, "fragment_snapshots"); n != 16 {
		t.Fatalf("deleted %d rows past %d days, want 16; the fixture is not exercising the drop", n, marketRetentionDays)
	}

	after := repo.OfferingTimings(ctx, scope)
	if !reflect.DeepEqual(before, after) {
		t.Errorf("offering timings changed after dropping rows past %d days:\nbefore %+v\nafter  %+v", marketRetentionDays, before, after)
	}
}
