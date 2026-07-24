//go:build integration

package trade

import (
	"context"
	"math"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// These integration tests cover the scoped trade_lookups repository — the only
// persistence path that warms the startup TradeCache (LatestLookups). An
// unscoped predicate here leaks the previous league's lookups into the new
// league's cache, invisible to the cache's per-league panic guard because the
// rows arrive already stripped of their league. Every assertion is designed to
// fail if the `league = $1` predicate is dropped from the read.

func tradeIntegrationPool(t *testing.T) *pgxpool.Pool {
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

	t.Cleanup(func() { pool.Close() })

	// Guard: the scoped trade_lookups schema (POE-120) plus the league registry
	// must be present, otherwise the isolation assertions are meaningless.
	for _, table := range []string{"leagues", "trade_lookups"} {
		var exists bool
		if err := pool.QueryRow(ctx,
			"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)", table).
			Scan(&exists); err != nil {
			t.Fatalf("check %s table: %v", table, err)
		}
		if !exists {
			t.Skipf("%s table not found, skipping (league-scope migrations not applied)", table)
		}
	}

	return pool
}

// registerTradeLeague inserts a league so scoped rows can satisfy the
// trade_lookups league FK. Registered before row cleanup so it runs LAST (LIFO),
// after the referencing lookup rows are gone.
func registerTradeLeague(t *testing.T, pool *pgxpool.Pool, id string) {
	t.Helper()
	_, err := pool.Exec(context.Background(),
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id)
	if err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
}

// cleanupTradeGem deletes every trade_lookups row for the named gem across all
// leagues. Registered after registerTradeLeague so it runs first (LIFO), clearing
// the FK references before the leagues are removed.
func cleanupTradeGem(t *testing.T, pool *pgxpool.Pool, gem string) {
	t.Helper()
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(),
			`DELETE FROM trade_lookups WHERE gem = $1`, gem); err != nil {
			t.Logf("cleanup warning: delete trade_lookups for gem %q: %v", gem, err)
		}
	})
}

// LatestLookups is scoped per league. Seeding the SAME gem+variant under two
// leagues with distinguishing price_floor values, then reading under league A,
// must return A's value only.
//
// League B's row is at a STRICTLY LATER time than A's. LatestLookups uses
// `DISTINCT ON (gem, variant) ... ORDER BY gem, variant, time DESC`, so if the
// `league = $1` predicate is dropped the DISTINCT ON collapses both leagues'
// rows to the single latest one — league B's — and the read returns B's value.
// Asserting the VALUE (not merely the row count) is what makes that leak
// observable, since the leaked result is still exactly one row for this gem.
func TestLatestLookups_returnsOnlyScopedLeague(t *testing.T) {
	pool := tradeIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-trade-A", "POE-120-trade-B"
	registerTradeLeague(t, pool, leagueA)
	registerTradeLeague(t, pool, leagueB)

	const gem = "POE120 Trade Isolation Gem"
	const variant = "20/20"
	cleanupTradeGem(t, pool, gem)

	// Far-future times keep these rows outside LatestLookups' NOW()-window floor
	// only from below; both satisfy `time > NOW() - interval`. B is strictly later
	// than A so the unscoped-read mutation deterministically surfaces B's row.
	tA := time.Date(2099, 2, 1, 0, 0, 0, 0, time.UTC)
	tB := tA.Add(time.Hour)

	rowA := &TradeLookupResult{Gem: gem, Variant: variant, Total: 10, PriceFloor: 111, FetchedAt: tA}
	rowB := &TradeLookupResult{Gem: gem, Variant: variant, Total: 20, PriceFloor: 222, FetchedAt: tB}
	if err := repo.InsertTradeLookup(ctx, league.Historical(leagueA), rowA, "user"); err != nil {
		t.Fatalf("InsertTradeLookup league A: %v", err)
	}
	if err := repo.InsertTradeLookup(ctx, league.Historical(leagueB), rowB, "user"); err != nil {
		t.Fatalf("InsertTradeLookup league B: %v", err)
	}

	res, err := repo.LatestLookups(ctx, league.Historical(leagueA), 24)
	if err != nil {
		t.Fatalf("LatestLookups: %v", err)
	}

	var matches []TradeLookupResult
	for _, r := range res {
		if r.Gem == gem {
			matches = append(matches, r)
		}
	}
	if len(matches) != 1 {
		t.Fatalf("row count for gem %q = %d, want 1 (only league %q's lookup)", gem, len(matches), leagueA)
	}
	if math.Abs(matches[0].PriceFloor-111) > 0.01 {
		t.Errorf("priceFloor = %v, want 111 (league %q's lookup); 222 means the read pulled league %q's later row", matches[0].PriceFloor, leagueA, leagueB)
	}
	if !matches[0].FetchedAt.Equal(tA) {
		t.Errorf("fetchedAt = %v, want %v (league %q's lookup)", matches[0].FetchedAt, tA, leagueA)
	}
}

// InsertTradeLookup stores under the scope's league. Writing under a non-Mirage
// league and reading the stored `league` column back catches a writer that
// ignores scope and hardcodes the pre-POE-120 'Mirage' default.
func TestInsertTradeLookup_storesScopeLeague(t *testing.T) {
	pool := tradeIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-trade-save"
	registerTradeLeague(t, pool, leagueID)

	const gem = "POE120 Trade Save Gem"
	const variant = "20/20"
	cleanupTradeGem(t, pool, gem)

	tm := time.Date(2099, 2, 2, 0, 0, 0, 0, time.UTC)
	row := &TradeLookupResult{Gem: gem, Variant: variant, Total: 5, PriceFloor: 42, FetchedAt: tm}
	if err := repo.InsertTradeLookup(ctx, league.Historical(leagueID), row, "user"); err != nil {
		t.Fatalf("InsertTradeLookup: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM trade_lookups WHERE time = $1 AND gem = $2 AND variant = $3`,
		tm, gem, variant).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}
