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

func TestLatestLookups_readsLegacyGateDesktopSources(t *testing.T) {
	pool := tradeIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)
	const leagueID = "POE-297-trade-sources"
	registerTradeLeague(t, pool, leagueID)
	scope := league.Historical(leagueID)

	for i, source := range []string{"user", "server-gate", "desktop"} {
		t.Run(source, func(t *testing.T) {
			gem := "POE297 Source " + source
			cleanupTradeGem(t, pool, gem)
			price := float64(100 + i)
			row := BuildResult(gem, "20/20", leagueID,
				SearchResponse{Total: 1}, []TradeListingDetail{{Price: price, Currency: "chaos"}}, 200)
			if err := repo.InsertTradeLookup(ctx, scope, row, source); err != nil {
				t.Fatalf("InsertTradeLookup source %q: %v", source, err)
			}
			var storedSource string
			if err := pool.QueryRow(ctx,
				`SELECT source FROM trade_lookups WHERE league = $1 AND gem = $2 AND variant = $3`,
				leagueID, gem, row.Variant).Scan(&storedSource); err != nil {
				t.Fatalf("read stored source: %v", err)
			}
			if storedSource != source {
				t.Fatalf("fixture source = %q, want %q", storedSource, source)
			}
			results, err := repo.LatestLookups(ctx, scope, 24)
			if err != nil {
				t.Fatalf("LatestLookups: %v", err)
			}
			for _, got := range results {
				if got.Gem == gem && got.Variant == row.Variant {
					if got.PriceFloor != price || len(got.Listings) != 1 || got.Listings[0].ChaosPrice != price {
						t.Fatalf("source %q result = %+v, want one %gc listing", source, got, price)
					}
					return
				}
			}
			t.Fatalf("LatestLookups omitted source %q", source)
		})
	}
}

func TestLatestLookups_emptyResultReplacesStalePriceOnWarm(t *testing.T) {
	pool := tradeIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)
	const leagueID = "POE-297-trade-empty"
	const gem = "POE297 Empty Restart Gem"
	const variant = "20/20"
	registerTradeLeague(t, pool, leagueID)
	cleanupTradeGem(t, pool, gem)
	scope := league.Historical(leagueID)
	old := BuildResult(gem, variant, leagueID,
		SearchResponse{Total: 1}, []TradeListingDetail{{Price: 42, Currency: "chaos"}}, 200)
	old.FetchedAt = time.Now().Add(-time.Hour)
	if err := repo.InsertTradeLookup(ctx, scope, old, "user"); err != nil {
		t.Fatalf("insert stale price: %v", err)
	}
	empty := BuildResult(gem, variant, leagueID, SearchResponse{}, []TradeListingDetail{}, 200)
	if err := repo.InsertTradeLookup(ctx, scope, empty, "server-gate"); err != nil {
		t.Fatalf("insert empty check: %v", err)
	}
	results, err := repo.LatestLookups(ctx, scope, 24)
	if err != nil {
		t.Fatalf("LatestLookups: %v", err)
	}
	cache := NewTradeCache(10, scope)
	cache.Warm(results)
	got, ok := cache.Get(CacheKey(gem, variant))
	if !ok {
		t.Fatal("warm cache lost the empty lookup")
	}
	if got.Listings == nil || len(got.Listings) != 0 || got.Total != 0 || got.PriceFloor != 0 || got.MedianTop10 != 0 {
		t.Fatalf("warm result = %+v, want the empty check instead of the stale 42c quote", got)
	}
	// PostgreSQL stores microsecond precision; the stamp still identifies the
	// latest check and keeps the picker from immediately refreshing it again.
	if got.FetchedAt.Sub(empty.FetchedAt).Abs() > time.Microsecond {
		t.Errorf("warm fetchedAt = %v, want latest check %v", got.FetchedAt, empty.FetchedAt)
	}
	if key, ok := cache.OldestStale(5*time.Minute, nil); ok {
		t.Errorf("OldestStale = %q, want no fresh empty check eligible", key)
	}
}

func TestKnownGem_exactRawMembership(t *testing.T) {
	pool := tradeIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)
	const leagueA, leagueB = "POE-297-known-A", "POE-297-known-B"
	registerTradeLeague(t, pool, leagueA)
	registerTradeLeague(t, pool, leagueB)
	const support = "POE297 Known Support"
	const heist = "POE297 Known of Trarthus"
	const base = "POE297 Snapshot Base"
	const corrupted = "POE297 Vaal Arc (Arc of Surging)"
	const foreign = "POE297 Foreign of Surging"
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM gem_snapshots WHERE name = ANY($1)`, []string{base, corrupted, foreign}); err != nil {
			t.Logf("cleanup gem snapshots: %v", err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM gem_colors WHERE name = ANY($1)`, []string{support, heist}); err != nil {
			t.Logf("cleanup gem colors: %v", err)
		}
	})
	if _, err := pool.Exec(ctx,
		`INSERT INTO gem_colors (name, color) VALUES ($1, 'BLUE'), ($2, 'GREEN')`, support, heist); err != nil {
		t.Fatalf("seed raw gem colors: %v", err)
	}
	stamp := time.Now().UTC().Truncate(time.Second)
	for _, row := range []struct {
		league       string
		name         string
		variant      string
		corrupted    bool
		transfigured bool
	}{
		{leagueA, base, "1", false, false},
		{leagueA, corrupted, "21/20c", true, true},
		{leagueA, corrupted, "21/23c", true, true},
		{leagueB, foreign, "20/20", false, true},
	} {
		if _, err := pool.Exec(ctx,
			`INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, is_transfigured)
			 VALUES ($1, $2, $3, $4, $5, $6)`, row.league, stamp, row.name, row.variant, row.corrupted, row.transfigured); err != nil {
			t.Fatalf("seed raw snapshot %q: %v", row.name, err)
		}
	}
	for _, tc := range []struct {
		name   string
		league string
		gem    string
		want   bool
	}{
		{"support-from-colors", leagueA, support, true},
		{"heist-from-colors", leagueA, heist, true},
		{"shared-colors-in-other-league", leagueB, support, true},
		{"base-from-snapshot", leagueA, base, true},
		{"corrupted-transfigured-from-snapshot", leagueA, corrupted, true},
		{"foreign-snapshot-in-own-league", leagueB, foreign, true},
		{"foreign-snapshot-excluded", leagueA, foreign, false},
		{"case-is-exact", leagueA, "POE297 Known support", false},
		{"substring-is-not-membership", leagueA, "POE297 Known", false},
		{"unknown-name", leagueA, "POE297 Unknown of Junk", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			got, err := repo.KnownGem(ctx, league.Historical(tc.league), tc.gem)
			if err != nil || got != tc.want {
				t.Fatalf("KnownGem(%q, %q) = %v, %v; want %v, nil", tc.league, tc.gem, got, err, tc.want)
			}
		})
	}
}

func TestKnownGem_rejectsInvalidScopeBeforeQuery(t *testing.T) {
	known, err := NewRepository(nil).KnownGem(context.Background(), league.Scope{}, "Arc")
	if err == nil || known {
		t.Fatalf("KnownGem with invalid scope = %v, %v; want false and validation error", known, err)
	}
}
