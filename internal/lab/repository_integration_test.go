//go:build integration

package lab

import (
	"context"
	"fmt"
	"math"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// These tests are written against the POST-POE-120 scoped repository signatures
// (`Method(ctx, scope league.Scope, ...existingArgs)`). Until Chunk 2 threads
// `league.Scope` through internal/lab/repository.go they DO NOT COMPILE — that is
// the intended red step for test-first development. Each test's contract is
// cross-league isolation: a read returns only the requested league's rows, a
// write stores under the scope's league, and a scoped delete leaves other
// leagues untouched. Every assertion is designed to fail if the league predicate
// is removed from (or points at the wrong league in) the query it covers.

func labIntegrationPool(t *testing.T) *pgxpool.Pool {
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

	// Guard: the scoped-league schema (POE-119) must be present, otherwise these
	// isolation tests are meaningless.
	for _, table := range []string{"leagues", "gem_snapshots", "gem_features", "gem_signals"} {
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

// registerLeague inserts a league into the registry so scoped rows can reference
// it (the FK on every scoped table requires this). Cleanup deletes the league;
// it is registered first so it runs LAST (LIFO), after row cleanups have removed
// the referencing rows.
func registerLeague(t *testing.T, pool *pgxpool.Pool, id string) {
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

// cleanupAtTime deletes rows at the given snapshot time from each named table
// across ALL leagues. Registered after registerLeague so it runs first (LIFO),
// clearing FK references before the leagues themselves are removed.
func cleanupAtTime(t *testing.T, pool *pgxpool.Pool, tm time.Time, tables ...string) {
	t.Helper()
	t.Cleanup(func() {
		for _, table := range tables {
			if _, err := pool.Exec(context.Background(),
				fmt.Sprintf("DELETE FROM %s WHERE time = $1", table), tm); err != nil {
				t.Logf("cleanup warning: delete %s at %v: %v", table, tm, err)
			}
		}
	})
}

// seedGemSnapshot inserts one raw gem_snapshots row under an explicit league.
// Raw SQL keeps the gem_snapshots read tests independent of any writer.
func seedGemSnapshot(t *testing.T, pool *pgxpool.Pool, leagueID string, tm time.Time,
	name, variant string, isTransfigured, isCorrupted bool, chaos float64, listings int, color string) {
	t.Helper()
	_, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots
			(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`,
		leagueID, tm, name, variant, isCorrupted, isTransfigured, chaos, listings, color)
	if err != nil {
		t.Fatalf("seed gem_snapshot (league %q, %s): %v", leagueID, name, err)
	}
}

func valuesClose(a, b float64) bool { return math.Abs(a-b) < 0.01 }

// futureTime returns a far-future snapshot time unique per test day. Future
// timestamps guarantee this test's rows are the MAX(time) the LatestX queries
// select, isolating them from real data and from other tests' rows.
func futureTime(day int) time.Time {
	return time.Date(2099, 1, day, 0, 0, 0, 0, time.UTC)
}

// ---------------------------------------------------------------------------
// Read isolation — gem_snapshots-backed methods (seeded via raw SQL)
// ---------------------------------------------------------------------------

func TestLatestGemPrices_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-lgp-A", "POE-120-lgp-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(2)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// Same identity in both leagues, different chaos so the returned value
	// distinguishes the scoped league from the other one.
	seedGemSnapshot(t, pool, leagueA, tm, "POE120 Price Gem", "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueB, tm, "POE120 Price Gem", "20/20", true, false, 222, 20, "BLUE")

	gems, snapTime, err := repo.LatestGemPrices(ctx, league.Historical(leagueA))
	if err != nil {
		t.Fatalf("LatestGemPrices: %v", err)
	}
	if !snapTime.Equal(tm) {
		t.Errorf("snapshot time = %v, want %v", snapTime, tm)
	}
	if len(gems) != 1 {
		t.Fatalf("gem count = %d, want 1 (only league %q); an unscoped query returns both leagues", len(gems), leagueA)
	}
	if !valuesClose(gems[0].Chaos, 111) {
		t.Errorf("chaos = %v, want 111 (league %q's row); 222 means the query read league %q", gems[0].Chaos, leagueA, leagueB)
	}
}

func TestGemPriceHistoryByVariant_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-gphv-A", "POE-120-gphv-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(3)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	const name = "POE120 History Gem"
	// Transfigured, non-corrupted, chaos > 5, variant in the analysis set.
	seedGemSnapshot(t, pool, leagueA, tm, name, "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueB, tm, name, "20/20", true, false, 222, 20, "BLUE")

	hist, err := repo.GemPriceHistoryByVariant(ctx, league.Historical(leagueA), "", 24)
	if err != nil {
		t.Fatalf("GemPriceHistoryByVariant: %v", err)
	}

	var got *GemPriceHistory
	for i := range hist {
		if hist[i].Name == name {
			got = &hist[i]
			break
		}
	}
	if got == nil {
		t.Fatalf("no history entry for %q", name)
	}
	// The result groups by (name, variant), NOT league. An unscoped query folds
	// both leagues' points into this one entry, so a point count of 2 (or a
	// value of 222) is the cross-league leak.
	if len(got.Points) != 1 {
		t.Fatalf("point count = %d, want 1 (only league %q)", len(got.Points), leagueA)
	}
	if !valuesClose(got.Points[0].Chaos, 111) {
		t.Errorf("chaos = %v, want 111 (league %q); 222 means the query read league %q", got.Points[0].Chaos, leagueA, leagueB)
	}
}

func TestSparklineData_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-spark-A", "POE-120-spark-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(4)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	const name = "POE120 Spark Gem"
	seedGemSnapshot(t, pool, leagueA, tm, name, "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueB, tm, name, "20/20", true, false, 222, 20, "BLUE")

	data, err := repo.SparklineData(ctx, league.Historical(leagueA), []string{name}, "", 24)
	if err != nil {
		t.Fatalf("SparklineData: %v", err)
	}
	points, ok := data[name]
	if !ok {
		t.Fatalf("no sparkline points for %q", name)
	}
	if len(points) != 1 {
		t.Fatalf("point count = %d, want 1 (only league %q)", len(points), leagueA)
	}
	if !valuesClose(points[0].Price, 111) {
		t.Errorf("price = %v, want 111 (league %q); 222 means the query read league %q", points[0].Price, leagueA, leagueB)
	}
}

// ---------------------------------------------------------------------------
// SparklineWindow — the population query behind the sparkline cache (POE-133).
//
// It must NOT inherit the filters GemPriceHistoryByVariant applies (chaos floor,
// Trarthus exclusion, transfigured-only), because SparklineData never applied
// them and the cache has to reproduce the served content exactly. It must apply
// the filters that DO exist: league, the four-variant allowlist for the main
// map, and the corrupted split.
// ---------------------------------------------------------------------------

// spwBounds is the read bound for the filter tests: a window wide enough that
// every seeded row lands in the window half, so those tests observe the filters
// alone rather than the window/tail split. The bounded-cold-read tests below set
// their own bounds.
func spwBounds(hours int) SparklineBounds {
	return SparklineBounds{WindowHours: hours, TailPoints: SparklineTailPoints, LookbackHours: hours}
}

func TestSparklineWindow_returnsBaseGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-base"
	registerLeague(t, pool, leagueID)

	tm := futureTime(30)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// is_transfigured = false: TrendAnalysis charts base gems, so a query that
	// filters on is_transfigured (as GemPriceHistoryByVariant does) loses them.
	const name = "POE133 Base Only Gem"
	seedGemSnapshot(t, pool, leagueID, tm, name, "20/20", false, false, 111, 10, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	points, ok := series[sparklineKey{name: name, variant: "20/20"}]
	if !ok {
		t.Fatalf("no series for base gem %q — an is_transfigured = true filter drops it and blanks the TrendAnalysis base sparkline", name)
	}
	if len(points) != 1 {
		t.Fatalf("point count = %d, want 1", len(points))
	}
	if !valuesClose(points[0].Price, 111) {
		t.Errorf("price = %v, want 111", points[0].Price)
	}
}

func TestSparklineWindow_returnsGemsBelowTheChaosFloor(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-cheap"
	registerLeague(t, pool, leagueID)

	tm := futureTime(31)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// 3 chaos is under GemPriceHistoryByVariant's `chaos > 5` floor (POE-134).
	// SparklineData has never applied it, so the cache must not either.
	const name = "POE133 Cheap Gem"
	seedGemSnapshot(t, pool, leagueID, tm, name, "20/20", true, false, 3, 10, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	points, ok := series[sparklineKey{name: name, variant: "20/20"}]
	if !ok {
		t.Fatalf("no series for %q priced at 3 chaos — the query inherited the `chaos > 5` floor", name)
	}
	if !valuesClose(points[0].Price, 3) {
		t.Errorf("price = %v, want 3", points[0].Price)
	}
}

func TestSparklineWindow_returnsTrarthusGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-trarthus"
	registerLeague(t, pool, leagueID)

	tm := futureTime(32)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// GemPriceHistoryByVariant excludes `name NOT LIKE '%Trarthus%'`;
	// SparklineData does not, so these gems keep their sparklines.
	const name = "POE133 Trarthus Gem"
	seedGemSnapshot(t, pool, leagueID, tm, name, "20/20", true, false, 111, 10, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	if _, ok := series[sparklineKey{name: name, variant: "20/20"}]; !ok {
		t.Fatalf("no series for %q — the query inherited the Trarthus exclusion", name)
	}
}

func TestSparklineWindow_putsCorruptedRowsInTheCorruptedMap(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-corrupt"
	registerLeague(t, pool, leagueID)

	tm := futureTime(33)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// Dedication serves corrupted 21/23c series from a separate map; mixing them
	// into the main map would show corrupted prices on the collective sparkline.
	const name = "POE133 Corrupted Gem"
	seedGemSnapshot(t, pool, leagueID, tm, name, "21/23c", true, true, 111, 10, "BLUE")

	series, corrupted, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	key := sparklineKey{name: name, variant: "21/23c"}
	points, ok := corrupted[key]
	if !ok {
		t.Fatalf("no corrupted series for %q at 21/23c", name)
	}
	if !valuesClose(points[0].Price, 111) {
		t.Errorf("corrupted price = %v, want 111", points[0].Price)
	}
	if _, leaked := series[key]; leaked {
		t.Errorf("corrupted row also landed in the non-corrupted map — the is_corrupted split is not applied")
	}
}

func TestSparklineWindow_excludesVariantsOutsideTheServedSet(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-variant"
	registerLeague(t, pool, leagueID)

	tm := futureTime(34)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// The collector writes poe.ninja's variant verbatim ("default" when empty),
	// so an unfiltered query opens the key space to variants nothing serves.
	const name = "POE133 Variant Gem"
	seedGemSnapshot(t, pool, leagueID, tm, name, "default", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueID, tm, name, "20", true, false, 222, 20, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	if _, ok := series[sparklineKey{name: name, variant: "default"}]; ok {
		t.Errorf("variant %q is outside the served set {1, 1/20, 20, 20/20} but was cached", "default")
	}
	if _, ok := series[sparklineKey{name: name, variant: "20"}]; !ok {
		t.Errorf("variant %q is in the served set but is missing", "20")
	}
}

func TestSparklineWindow_returnsOnlyRowsNewerThanSince(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-since"
	registerLeague(t, pool, leagueID)

	older := futureTime(35)
	newer := older.Add(time.Hour)
	cleanupAtTime(t, pool, older, "gem_snapshots")
	cleanupAtTime(t, pool, newer, "gem_snapshots")

	// The incremental population path passes its high-water mark as `since`;
	// re-reading already-merged rows would duplicate points on every tick.
	const name = "POE133 Since Gem"
	seedGemSnapshot(t, pool, leagueID, older, name, "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueID, newer, name, "20/20", true, false, 222, 20, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), older, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	points := series[sparklineKey{name: name, variant: "20/20"}]
	if len(points) != 1 {
		t.Fatalf("point count = %d, want 1 (only the row after `since`); 2 means `since` was ignored", len(points))
	}
	if !valuesClose(points[0].Price, 222) {
		t.Errorf("price = %v, want 222 (the newer row); 111 is the row at `since`, which is not strictly newer", points[0].Price)
	}
}

func TestSparklineWindow_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-133-spw-A", "POE-133-spw-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(36)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	const name = "POE133 Scoped Gem"
	seedGemSnapshot(t, pool, leagueA, tm, name, "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueB, tm, name, "20/20", true, false, 222, 20, "BLUE")

	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueA), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}
	// The map keys on (name, variant), not league, so an unscoped query folds
	// both leagues' points into this one series.
	points := series[sparklineKey{name: name, variant: "20/20"}]
	if len(points) != 1 {
		t.Fatalf("point count = %d, want 1 (only league %q)", len(points), leagueA)
	}
	if !valuesClose(points[0].Price, 111) {
		t.Errorf("price = %v, want 111 (league %q); 222 means the query read league %q", points[0].Price, leagueA, leagueB)
	}
}

func TestSparklineWindow_returnsEmptyMapsWhenNothingMatches(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	// A league registered but never collected — the shape a fresh league has for
	// its first tick. Population must treat it as "nothing yet", not an error.
	leagueID := "POE-133-spw-empty"
	registerLeague(t, pool, leagueID)

	series, corrupted, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, spwBounds(24))
	if err != nil {
		t.Fatalf("SparklineWindow on an empty league: %v", err)
	}
	if series == nil || corrupted == nil {
		t.Fatalf("maps = (%v, %v), want non-nil empties; a nil map forces every caller to nil-check", series == nil, corrupted == nil)
	}
	if len(series) != 0 || len(corrupted) != 0 {
		t.Errorf("series/corrupted sizes = %d/%d, want 0/0 for a league with no snapshots", len(series), len(corrupted))
	}
}

// ---------------------------------------------------------------------------
// SparklineWindow — the bounded cold read.
//
// The tests above seed far-future instants so every row lands inside any window;
// these seed relative to NOW() so the window/tail split is observable. A cold
// read must return the served window in full plus only a short per-series tail
// beyond it, never the flat lookback: that read is roughly fourteen times the
// rows for the same cache contents, held live for the whole merge, at the one
// moment the analysis pass is loading its own history.
// ---------------------------------------------------------------------------

// cleanupLeagueSnapshots deletes every gem_snapshots row for one league.
// cleanupAtTime keys on an exact instant across ALL leagues, which is safe for
// year-2099 seeds but not for seeds relative to NOW().
func cleanupLeagueSnapshots(t *testing.T, pool *pgxpool.Pool, leagueID string) {
	t.Helper()
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(),
			`DELETE FROM gem_snapshots WHERE league = $1`, leagueID); err != nil {
			t.Logf("cleanup warning: delete gem_snapshots for league %q: %v", leagueID, err)
		}
	})
}

// sparkAges returns how many hours before now each returned point sits, rounded
// to the nearest hour, so an assertion names ages rather than instants.
func sparkAges(t *testing.T, now time.Time, points []SparklinePoint) []int {
	t.Helper()
	out := make([]int, len(points))
	for i, p := range points {
		at, err := time.Parse(time.RFC3339, p.Time)
		if err != nil {
			t.Fatalf("point %d has unparsable time %q: %v", i, p.Time, err)
		}
		out[i] = int(now.Sub(at).Round(time.Hour) / time.Hour)
	}
	return out
}

func sameInts(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestSparklineWindow_coldReadKeepsTheWindowAndOnlyATailBeyondIt(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-cold-bounded"
	registerLeague(t, pool, leagueID)
	cleanupLeagueSnapshots(t, pool, leagueID)

	const name = "POE133 Bounded Gem"
	now := time.Now().UTC().Truncate(time.Second)
	// Three rows inside the 12-hour window, five older ones inside the lookback.
	for _, ago := range []int{1, 5, 11, 20, 30, 40, 50, 60} {
		seedGemSnapshot(t, pool, leagueID, now.Add(-time.Duration(ago)*time.Hour),
			name, "20/20", true, false, float64(100+ago), 10, "BLUE")
	}

	bounds := SparklineBounds{WindowHours: 12, TailPoints: 4, LookbackHours: 72}
	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, bounds)
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}

	// The three in-window rows, plus the tail extension to four points: the 20h
	// row is the fourth-newest. 30h and older are what an unbounded read adds.
	want := []int{20, 11, 5, 1}
	got := sparkAges(t, now, series[sparklineKey{name: name, variant: "20/20"}])
	if !sameInts(got, want) {
		t.Fatalf("point ages (hours before now) = %v, want %v; the extra rows mean the cold read fetched the flat %d-hour lookback",
			got, want, bounds.LookbackHours)
	}
}

func TestSparklineWindow_coldReadStillReturnsATailForASeriesWithNoInWindowRows(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-cold-tail"
	registerLeague(t, pool, leagueID)
	cleanupLeagueSnapshots(t, pool, leagueID)

	// A gem that stopped appearing in snapshots: nothing inside the window, so a
	// window-only read blanks its sparkline instead of showing its last shape.
	const name = "POE133 Delisted Gem"
	now := time.Now().UTC().Truncate(time.Second)
	for _, ago := range []int{20, 24, 28, 32, 36} {
		seedGemSnapshot(t, pool, leagueID, now.Add(-time.Duration(ago)*time.Hour),
			name, "20/20", true, false, float64(100+ago), 10, "BLUE")
	}

	bounds := SparklineBounds{WindowHours: 12, TailPoints: 4, LookbackHours: 72}
	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, bounds)
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}

	want := []int{32, 28, 24, 20}
	got := sparkAges(t, now, series[sparklineKey{name: name, variant: "20/20"}])
	if !sameInts(got, want) {
		t.Fatalf("point ages (hours before now) = %v, want the newest %d rows %v",
			got, bounds.TailPoints, want)
	}
}

func TestSparklineWindow_coldReadDropsSeriesOlderThanTheLookback(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-133-spw-cold-lookback"
	registerLeague(t, pool, leagueID)
	cleanupLeagueSnapshots(t, pool, leagueID)

	// Past the lookback a flat line from last week is worse than nothing, so the
	// tail must not reach these rows even though the series has fewer than
	// TailPoints inside it.
	const name = "POE133 Ancient Gem"
	now := time.Now().UTC().Truncate(time.Second)
	for _, ago := range []int{80, 100} {
		seedGemSnapshot(t, pool, leagueID, now.Add(-time.Duration(ago)*time.Hour),
			name, "20/20", true, false, float64(100+ago), 10, "BLUE")
	}

	bounds := SparklineBounds{WindowHours: 12, TailPoints: 4, LookbackHours: 72}
	series, _, err := repo.SparklineWindow(ctx, league.Historical(leagueID), time.Time{}, bounds)
	if err != nil {
		t.Fatalf("SparklineWindow: %v", err)
	}

	if points, ok := series[sparklineKey{name: name, variant: "20/20"}]; ok {
		t.Errorf("series has %d points %v, want no series at all — every row is older than the %d-hour lookback",
			len(points), sparkAges(t, now, points), bounds.LookbackHours)
	}
}

func TestGemNamesAutocomplete_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-auto-A", "POE-120-auto-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(5)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// DISTINCT names so DISTINCT-name collapse can't hide a leak: the query
	// returns names, and identical names would fold to one row regardless of
	// scoping. Different names in each league make the leak observable.
	const nameA = "POE120 Alpha Marker"
	const nameB = "POE120 Beta Marker"
	seedGemSnapshot(t, pool, leagueA, tm, nameA, "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueB, tm, nameB, "20/20", true, false, 222, 20, "BLUE")

	names, err := repo.GemNamesAutocomplete(ctx, league.Historical(leagueA), "Marker", 100)
	if err != nil {
		t.Fatalf("GemNamesAutocomplete: %v", err)
	}
	if len(names) != 1 {
		t.Fatalf("name count = %d, want 1 (only league %q); %v", len(names), leagueA, names)
	}
	if names[0] != nameA {
		t.Errorf("name = %q, want %q; %q means the query read league %q", names[0], nameA, nameB, leagueB)
	}
}

func TestGemNamesAutocomplete_whitespaceQueryReturnsEveryName(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-128-blank-query"
	registerLeague(t, pool, leagueID)

	tm := futureTime(6)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// The desktop OCR path sends q=" " to mean "give me the whole dictionary".
	// Building the SQL with zero ILIKE conditions emits "AND ORDER BY" — a syntax
	// error that surfaces as a 500 whenever the in-memory cache is cold.
	seedGemSnapshot(t, pool, leagueID, tm, "POE128 Blank Alpha", "20/20", true, false, 111, 10, "BLUE")
	seedGemSnapshot(t, pool, leagueID, tm, "POE128 Blank Beta", "20/20", true, false, 222, 20, "BLUE")

	names, err := repo.GemNamesAutocomplete(ctx, league.Historical(leagueID), " ", 100)
	if err != nil {
		t.Fatalf("GemNamesAutocomplete with a whitespace query: %v", err)
	}
	if len(names) != 2 {
		t.Fatalf("name count = %d, want 2 (both seeded names); %v", len(names), names)
	}
}

func TestGemNameDictionary_ignoresLeagueAndMarketData(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-128-dictionary"
	registerLeague(t, pool, leagueID)

	// gem_colors is static game data: no league column, no prices. Seed a
	// transfigured/base pair that appears in NO snapshot, so a dictionary built
	// from market tables could not return it.
	const base = "POE128 Dictionary Base"
	const transfigured = "POE128 Dictionary Base of Testing"
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(),
			`DELETE FROM gem_colors WHERE name = ANY($1)`, []string{base, transfigured}); err != nil {
			t.Logf("cleanup gem_colors: %v", err)
		}
	})
	if _, err := pool.Exec(ctx,
		`INSERT INTO gem_colors (name, color) VALUES ($1, 'BLUE'), ($2, 'BLUE')
		 ON CONFLICT (name) DO NOTHING`, base, transfigured); err != nil {
		t.Fatalf("seed gem_colors: %v", err)
	}

	names, err := repo.GemNameDictionary(ctx, league.Historical(leagueID), true)
	if err != nil {
		t.Fatalf("GemNameDictionary: %v", err)
	}

	var sawTransfigured, sawBase bool
	for _, n := range names {
		switch n {
		case transfigured:
			sawTransfigured = true
		case base:
			sawBase = true
		}
	}
	if !sawTransfigured {
		t.Errorf("%q missing — an unpriced gem must still be recognisable by OCR", transfigured)
	}
	if sawBase {
		t.Errorf("%q returned as transfigured — it is the base gem", base)
	}
}

func TestGemNameDictionary_includesLeagueNamesMissingFromGemColors(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-128-dict-superset"
	registerLeague(t, pool, leagueID)

	tm := futureTime(7)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// gem_colors is collector-fed (gemcolor.Resolver upserts what it prices), so a
	// gem can be listed on the market before it lands there. The dictionary must
	// stay a superset of the market-scoped endpoint it replaced, or swapping the
	// desktop over to it loses names OCR could previously match.
	const onlyInSnapshots = "POE128 Snapshot Only of Testing"
	seedGemSnapshot(t, pool, leagueID, tm, onlyInSnapshots, "20/20", true, false, 100, 5, "BLUE")

	var inColors bool
	if err := pool.QueryRow(ctx,
		`SELECT EXISTS (SELECT 1 FROM gem_colors WHERE name = $1)`, onlyInSnapshots).Scan(&inColors); err != nil {
		t.Fatalf("check gem_colors: %v", err)
	}
	if inColors {
		t.Fatalf("%q unexpectedly present in gem_colors — the test cannot prove the union", onlyInSnapshots)
	}

	names, err := repo.GemNameDictionary(ctx, league.Historical(leagueID), true)
	if err != nil {
		t.Fatalf("GemNameDictionary: %v", err)
	}

	for _, n := range names {
		if n == onlyInSnapshots {
			return
		}
	}
	t.Errorf("%q missing — a gem the market lists but gem_colors has not recorded is unmatchable by OCR", onlyInSnapshots)
}

// assertNotInGemColors fails the test when name is already in gem_colors. The
// dictionary unions gem_colors with the league's snapshot names, so a name
// present in both proves nothing about which half produced it.
func assertNotInGemColors(t *testing.T, pool *pgxpool.Pool, name string) {
	t.Helper()
	var inColors bool
	if err := pool.QueryRow(context.Background(),
		`SELECT EXISTS (SELECT 1 FROM gem_colors WHERE name = $1)`, name).Scan(&inColors); err != nil {
		t.Fatalf("check gem_colors: %v", err)
	}
	if inColors {
		t.Fatalf("%q unexpectedly present in gem_colors — the test cannot attribute it to the snapshot half", name)
	}
}

func containsName(names []string, want string) bool {
	for _, n := range names {
		if n == want {
			return true
		}
	}
	return false
}

func TestGemNameDictionary_skillPoolExcludesSupportGemsSeenOnlyInSnapshots(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-144-dict-support"
	registerLeague(t, pool, leagueID)

	tm := futureTime(8)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// poe.ninja's type=SkillGem feed carries support gems, so they land in
	// gem_snapshots. Neither the Font nor Dedication can hand out a support gem, so
	// every one of these is a false-positive candidate for the desktop OCR matcher.
	const supportOnlyInSnapshots = "POE144 Snapshot Only Support"
	seedGemSnapshot(t, pool, leagueID, tm, supportOnlyInSnapshots, "20/20", false, false, 100, 5, "BLUE")
	assertNotInGemColors(t, pool, supportOnlyInSnapshots)

	// Positive control for the absence assertion below. Same league, same
	// snapshot time, differing from the support name only in the " Support"
	// suffix — so if the snapshot half stops arriving in the skill dictionary at
	// all (league scoping broken, wrong is_transfigured predicate, an over-strip
	// that drops everything), this fails first and the absence assertion is not
	// credited as a pass.
	const controlOnlyInSnapshots = "POE144 Snapshot Only Control"
	seedGemSnapshot(t, pool, leagueID, tm, controlOnlyInSnapshots, "20/20", false, false, 100, 5, "BLUE")
	assertNotInGemColors(t, pool, controlOnlyInSnapshots)

	names, err := repo.GemNameDictionary(ctx, league.Historical(leagueID), false)
	if err != nil {
		t.Fatalf("GemNameDictionary: %v", err)
	}

	if !containsName(names, controlOnlyInSnapshots) {
		t.Fatalf("control %q missing — the snapshot half did not reach the skill dictionary, so the exclusion below proves nothing", controlOnlyInSnapshots)
	}

	if containsName(names, supportOnlyInSnapshots) {
		t.Errorf("%q present in the skill dictionary — a support gem is never a Font or Dedication outcome", supportOnlyInSnapshots)
	}
}

func TestGemNameDictionary_skillPoolKeepsNonSupportNamesMissingFromGemColors(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-144-dict-union"
	registerLeague(t, pool, leagueID)

	tm := futureTime(9)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// The union exists so a gem the market prices before gem_colors records it is
	// still OCR-matchable. This name has no " of ", so it is the plain
	// non-support case: it catches an inverted isSupportGem predicate, and any
	// over-strip that drops the snapshot half out of the skill pool.
	//
	// It does NOT catch the classification hazard — swapping the strip for
	// FilterGemDictionary keeps this name, because extractBaseName returns it
	// unchanged and it classifies non-transfigured. The classification hazard is
	// guarded on the transfigured half, by
	// TestGemNameDictionary_includesLeagueNamesMissingFromGemColors, which seeds a
	// " of " name flagged transfigured.
	const onlyInSnapshots = "POE144 Snapshot Only Skill"
	seedGemSnapshot(t, pool, leagueID, tm, onlyInSnapshots, "20/20", false, false, 100, 5, "BLUE")
	assertNotInGemColors(t, pool, onlyInSnapshots)

	names, err := repo.GemNameDictionary(ctx, league.Historical(leagueID), false)
	if err != nil {
		t.Fatalf("GemNameDictionary: %v", err)
	}

	if !containsName(names, onlyInSnapshots) {
		t.Errorf("%q missing — a gem the market lists but gem_colors has not recorded is unmatchable by OCR", onlyInSnapshots)
	}
}

func TestGemNameDictionary_transfiguredPoolIsNotSupportFiltered(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-144-dict-transfigured"
	registerLeague(t, pool, leagueID)

	tm := futureTime(10)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	// The transfigured half is trusted straight from the market — is_transfigured
	// comes from poe.ninja's alt_ discriminator, not from a name rule — and the
	// support strip is gated on the skill pool alone. A " Support" name flagged
	// transfigured is the one shape that distinguishes a gated strip from an
	// unconditional one, so it is what this test seeds.
	const transfiguredSupport = "POE144 Transfigured Support"
	seedGemSnapshot(t, pool, leagueID, tm, transfiguredSupport, "20/20", true, false, 100, 5, "BLUE")
	assertNotInGemColors(t, pool, transfiguredSupport)

	names, err := repo.GemNameDictionary(ctx, league.Historical(leagueID), true)
	if err != nil {
		t.Fatalf("GemNameDictionary: %v", err)
	}

	if !containsName(names, transfiguredSupport) {
		t.Errorf("%q missing from the transfigured dictionary — the support strip must not reach this pool", transfiguredSupport)
	}
}

// ---------------------------------------------------------------------------
// Read isolation — result tables (seeded via the scoped Save* writers)
// ---------------------------------------------------------------------------

func TestLatestFontResults_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-font-A", "POE-120-font-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(6)
	cleanupAtTime(t, pool, tm, "font_snapshots")

	base := FontResult{Time: tm, Color: "RED", Variant: "20/20", Mode: "safe", LiquidityRisk: "LOW"}
	rowA := base
	rowA.Profit = 111
	rowB := base
	rowB.Profit = 222
	if _, err := repo.SaveFontResults(ctx, league.Historical(leagueA), []FontResult{rowA}); err != nil {
		t.Fatalf("SaveFontResults league A: %v", err)
	}
	if _, err := repo.SaveFontResults(ctx, league.Historical(leagueB), []FontResult{rowB}); err != nil {
		t.Fatalf("SaveFontResults league B: %v", err)
	}

	res, err := repo.LatestFontResults(ctx, league.Historical(leagueA), "", "", 100)
	if err != nil {
		t.Fatalf("LatestFontResults: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if !valuesClose(res[0].Profit, 111) {
		t.Errorf("profit = %v, want 111 (league %q); 222 means the query read league %q", res[0].Profit, leagueA, leagueB)
	}
}

func TestLatestQualityResults_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-qual-A", "POE-120-qual-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(7)
	cleanupAtTime(t, pool, tm, "quality_results")

	base := QualityResult{Time: tm, Name: "POE120 Quality Gem", Level: 20, Confidence: "OK"}
	rowA := base
	rowA.BuyPrice = 111
	rowB := base
	rowB.BuyPrice = 222
	if _, err := repo.SaveQualityResults(ctx, league.Historical(leagueA), []QualityResult{rowA}); err != nil {
		t.Fatalf("SaveQualityResults league A: %v", err)
	}
	if _, err := repo.SaveQualityResults(ctx, league.Historical(leagueB), []QualityResult{rowB}); err != nil {
		t.Fatalf("SaveQualityResults league B: %v", err)
	}

	res, err := repo.LatestQualityResults(ctx, league.Historical(leagueA), "", 100)
	if err != nil {
		t.Fatalf("LatestQualityResults: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if !valuesClose(res[0].BuyPrice, 111) {
		t.Errorf("buy_price = %v, want 111 (league %q); 222 means the query read league %q", res[0].BuyPrice, leagueA, leagueB)
	}
}

func TestLatestGemFeatures_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-feat-A", "POE-120-feat-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(8)
	cleanupAtTime(t, pool, tm, "gem_features")

	base := GemFeature{Time: tm, Name: "POE120 Feature Gem", Variant: "20/20", Tier: "LOW", MarketRegime: "TEMPORAL"}
	rowA := base
	rowA.Chaos = 111
	rowB := base
	rowB.Chaos = 222
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueA), []GemFeature{rowA}); err != nil {
		t.Fatalf("SaveGemFeatures league A: %v", err)
	}
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueB), []GemFeature{rowB}); err != nil {
		t.Fatalf("SaveGemFeatures league B: %v", err)
	}

	res, err := repo.LatestGemFeatures(ctx, league.Historical(leagueA), "", "", 100)
	if err != nil {
		t.Fatalf("LatestGemFeatures: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if !valuesClose(res[0].Chaos, 111) {
		t.Errorf("chaos = %v, want 111 (league %q); 222 means the query read league %q", res[0].Chaos, leagueA, leagueB)
	}
}

func TestLatestGemSignals_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-sig-A", "POE-120-sig-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(9)
	cleanupAtTime(t, pool, tm, "gem_signals")

	base := GemSignal{
		Time: tm, Name: "POE120 Signal Gem", Variant: "20/20",
		Signal: "STABLE", WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW",
	}
	rowA := base
	rowA.Confidence = 111
	rowB := base
	rowB.Confidence = 222
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueA), []GemSignal{rowA}); err != nil {
		t.Fatalf("SaveGemSignals league A: %v", err)
	}
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueB), []GemSignal{rowB}); err != nil {
		t.Fatalf("SaveGemSignals league B: %v", err)
	}

	res, err := repo.LatestGemSignals(ctx, league.Historical(leagueA), "", "", 100)
	if err != nil {
		t.Fatalf("LatestGemSignals: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if res[0].Confidence != 111 {
		t.Errorf("confidence = %d, want 111 (league %q); 222 means the query read league %q", res[0].Confidence, leagueA, leagueB)
	}
}

func TestLatestDedicationResults_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-ded-A", "POE-120-ded-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(10)
	cleanupAtTime(t, pool, tm, "dedication_snapshots")

	base := DedicationResult{Time: tm, Color: "RED", GemType: "skill", Mode: "safe", LiquidityRisk: "LOW"}
	rowA := base
	rowA.Profit = 111
	rowB := base
	rowB.Profit = 222
	if _, err := repo.SaveDedicationResults(ctx, league.Historical(leagueA), []DedicationResult{rowA}); err != nil {
		t.Fatalf("SaveDedicationResults league A: %v", err)
	}
	if _, err := repo.SaveDedicationResults(ctx, league.Historical(leagueB), []DedicationResult{rowB}); err != nil {
		t.Fatalf("SaveDedicationResults league B: %v", err)
	}

	res, err := repo.LatestDedicationResults(ctx, league.Historical(leagueA), "", "", "", 100)
	if err != nil {
		t.Fatalf("LatestDedicationResults: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if !valuesClose(res[0].Profit, 111) {
		t.Errorf("profit = %v, want 111 (league %q); 222 means the query read league %q", res[0].Profit, leagueA, leagueB)
	}
}

func TestLatestTransfigureResults_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-trans-A", "POE-120-trans-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(11)
	cleanupAtTime(t, pool, tm, "transfigure_results")

	base := TransfigureResult{
		Time: tm, BaseName: "POE120 Base", TransfiguredName: "POE120 Transfigured",
		Variant: "20/20", Confidence: "OK", GemColor: "BLUE",
	}
	rowA := base
	rowA.ROI = 111
	rowB := base
	rowB.ROI = 222
	if _, err := repo.SaveTransfigureResults(ctx, league.Historical(leagueA), []TransfigureResult{rowA}); err != nil {
		t.Fatalf("SaveTransfigureResults league A: %v", err)
	}
	if _, err := repo.SaveTransfigureResults(ctx, league.Historical(leagueB), []TransfigureResult{rowB}); err != nil {
		t.Fatalf("SaveTransfigureResults league B: %v", err)
	}

	res, err := repo.LatestTransfigureResults(ctx, league.Historical(leagueA), "", 100)
	if err != nil {
		t.Fatalf("LatestTransfigureResults: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (only league %q)", len(res), leagueA)
	}
	if !valuesClose(res[0].ROI, 111) {
		t.Errorf("roi = %v, want 111 (league %q); 222 means the query read league %q", res[0].ROI, leagueA, leagueB)
	}
}

// ---------------------------------------------------------------------------
// SignalHistory — stronger gate than row identity.
//
// SignalHistory LEFT JOINs gem_signals onto gem_features on (time, name, variant).
// A correctly-scoped WHERE with an UNSCOPED join returns the right signal rows
// but sources the joined feature columns from whichever league's gem_features
// row also matches the key. Seeding both leagues at the SAME key with DIFFERENT
// feature values makes that leak observable: the returned velocity/price/listing
// columns must be league A's, and there must be exactly one row (an unscoped join
// fans out to both leagues' feature rows).
// ---------------------------------------------------------------------------

func TestSignalHistory_joinsScopedFeaturesOnly(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-sighist-A", "POE-120-sighist-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(12)
	cleanupAtTime(t, pool, tm, "gem_signals", "gem_features")

	const name = "POE120 SignalHistory Gem"
	const variant = "20/20"

	signal := GemSignal{
		Time: tm, Name: name, Variant: variant,
		Signal: "STABLE", WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW",
	}
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueA), []GemSignal{signal}); err != nil {
		t.Fatalf("SaveGemSignals league A: %v", err)
	}
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueB), []GemSignal{signal}); err != nil {
		t.Fatalf("SaveGemSignals league B: %v", err)
	}

	// Distinct feature values per league at the SAME key.
	featureA := GemFeature{
		Time: tm, Name: name, Variant: variant, Tier: "LOW", MarketRegime: "TEMPORAL",
		VelLongPrice: 1.5, VelLongListing: 2.5, Chaos: 111, Listings: 11,
	}
	featureB := GemFeature{
		Time: tm, Name: name, Variant: variant, Tier: "LOW", MarketRegime: "TEMPORAL",
		VelLongPrice: 9.5, VelLongListing: 8.5, Chaos: 222, Listings: 22,
	}
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueA), []GemFeature{featureA}); err != nil {
		t.Fatalf("SaveGemFeatures league A: %v", err)
	}
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueB), []GemFeature{featureB}); err != nil {
		t.Fatalf("SaveGemFeatures league B: %v", err)
	}

	changes, err := repo.SignalHistory(ctx, league.Historical(leagueA), name, variant, 100)
	if err != nil {
		t.Fatalf("SignalHistory: %v", err)
	}
	if len(changes) != 1 {
		t.Fatalf("change count = %d, want 1; >1 means the signal WHERE or the feature JOIN is unscoped and fanned out across leagues", len(changes))
	}
	c := changes[0]
	if !valuesClose(c.PriceVel, 1.5) {
		t.Errorf("priceVel = %v, want 1.5 (league %q features); 9.5 means the JOIN pulled league %q's gem_features row", c.PriceVel, leagueA, leagueB)
	}
	if !valuesClose(c.ListVel, 2.5) {
		t.Errorf("listVel = %v, want 2.5 (league %q features); 8.5 means the JOIN pulled league %q's gem_features row", c.ListVel, leagueA, leagueB)
	}
	if !valuesClose(c.Price, 111) {
		t.Errorf("price = %v, want 111 (league %q features); 222 means the JOIN pulled league %q's gem_features row", c.Price, leagueA, leagueB)
	}
	if c.Listings != 11 {
		t.Errorf("listings = %d, want 11 (league %q features); 22 means the JOIN pulled league %q's gem_features row", c.Listings, leagueA, leagueB)
	}
}

// ---------------------------------------------------------------------------
// DeleteV2ForSnapshot — a scoped delete must not touch other leagues.
//
// DeleteV2ForSnapshot interpolates table names into `DELETE ... WHERE time = $1`;
// a scoped signature compiles while the DELETE still wipes every league at that
// timestamp. Seed two leagues at one time across the deleted tables, delete for
// league A, and assert league B's rows survive.
// ---------------------------------------------------------------------------

func TestDeleteV2ForSnapshot_leavesOtherLeagueIntact(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-del-A", "POE-120-del-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tm := futureTime(13)
	cleanupAtTime(t, pool, tm, "gem_features", "gem_signals", "font_snapshots")

	feature := GemFeature{Time: tm, Name: "POE120 Del Gem", Variant: "20/20", Tier: "LOW", MarketRegime: "TEMPORAL"}
	signal := GemSignal{Time: tm, Name: "POE120 Del Gem", Variant: "20/20", Signal: "STABLE", WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW"}
	font := FontResult{Time: tm, Color: "RED", Variant: "20/20", Mode: "safe", LiquidityRisk: "LOW"}

	for _, leagueID := range []string{leagueA, leagueB} {
		scope := league.Historical(leagueID)
		if _, err := repo.SaveGemFeatures(ctx, scope, []GemFeature{feature}); err != nil {
			t.Fatalf("SaveGemFeatures %q: %v", leagueID, err)
		}
		if _, err := repo.SaveGemSignals(ctx, scope, []GemSignal{signal}); err != nil {
			t.Fatalf("SaveGemSignals %q: %v", leagueID, err)
		}
		if _, err := repo.SaveFontResults(ctx, scope, []FontResult{font}); err != nil {
			t.Fatalf("SaveFontResults %q: %v", leagueID, err)
		}
	}

	if err := repo.DeleteV2ForSnapshot(ctx, league.Historical(leagueA), tm); err != nil {
		t.Fatalf("DeleteV2ForSnapshot: %v", err)
	}

	countAtLeague := func(table, leagueID string) int {
		t.Helper()
		var n int
		if err := pool.QueryRow(ctx,
			fmt.Sprintf("SELECT count(*) FROM %s WHERE time = $1 AND league = $2", table), tm, leagueID).Scan(&n); err != nil {
			t.Fatalf("count %s for %q: %v", table, leagueID, err)
		}
		return n
	}

	for _, table := range []string{"gem_features", "gem_signals", "font_snapshots"} {
		if got := countAtLeague(table, leagueA); got != 0 {
			t.Errorf("%s league %q rows = %d, want 0 (scoped delete should remove them)", table, leagueA, got)
		}
		if got := countAtLeague(table, leagueB); got != 1 {
			t.Errorf("%s league %q rows = %d, want 1 (scoped delete must NOT touch other leagues)", table, leagueB, got)
		}
	}
}

// ---------------------------------------------------------------------------
// Save* writers store the scope's league.
//
// Each test writes under a NON-Mirage league and reads the stored `league` back.
// Using a non-Mirage scope catches a writer that ignores scope and hardcodes
// 'Mirage' (the pre-POE-120 implicit default) — such a writer would store the
// wrong league and fail the assertion, where a Mirage-scoped test would pass
// falsely.
// ---------------------------------------------------------------------------

func TestSaveTransfigureResults_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-trans"
	registerLeague(t, pool, leagueID)
	tm := futureTime(14)
	cleanupAtTime(t, pool, tm, "transfigure_results")

	row := TransfigureResult{Time: tm, BaseName: "B", TransfiguredName: "POE120 Save Trans", Variant: "20/20", Confidence: "OK"}
	if _, err := repo.SaveTransfigureResults(ctx, league.Historical(leagueID), []TransfigureResult{row}); err != nil {
		t.Fatalf("SaveTransfigureResults: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM transfigure_results WHERE time = $1 AND transfigured_name = $2 AND variant = $3`,
		tm, row.TransfiguredName, row.Variant).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveFontResults_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-font"
	registerLeague(t, pool, leagueID)
	tm := futureTime(15)
	cleanupAtTime(t, pool, tm, "font_snapshots")

	row := FontResult{Time: tm, Color: "RED", Variant: "20/20", Mode: "safe", LiquidityRisk: "LOW"}
	if _, err := repo.SaveFontResults(ctx, league.Historical(leagueID), []FontResult{row}); err != nil {
		t.Fatalf("SaveFontResults: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM font_snapshots WHERE time = $1 AND color = $2 AND variant = $3 AND mode = $4`,
		tm, row.Color, row.Variant, row.Mode).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveQualityResults_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-qual"
	registerLeague(t, pool, leagueID)
	tm := futureTime(16)
	cleanupAtTime(t, pool, tm, "quality_results")

	row := QualityResult{Time: tm, Name: "POE120 Save Quality", Level: 20, Confidence: "OK"}
	if _, err := repo.SaveQualityResults(ctx, league.Historical(leagueID), []QualityResult{row}); err != nil {
		t.Fatalf("SaveQualityResults: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM quality_results WHERE time = $1 AND name = $2 AND level = $3`,
		tm, row.Name, row.Level).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveGemFeatures_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-feat"
	registerLeague(t, pool, leagueID)
	tm := futureTime(17)
	cleanupAtTime(t, pool, tm, "gem_features")

	row := GemFeature{Time: tm, Name: "POE120 Save Feature", Variant: "20/20", Tier: "LOW", MarketRegime: "TEMPORAL"}
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueID), []GemFeature{row}); err != nil {
		t.Fatalf("SaveGemFeatures: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM gem_features WHERE time = $1 AND name = $2 AND variant = $3`,
		tm, row.Name, row.Variant).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveGemSignals_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-sig"
	registerLeague(t, pool, leagueID)
	tm := futureTime(18)
	cleanupAtTime(t, pool, tm, "gem_signals")

	row := GemSignal{Time: tm, Name: "POE120 Save Signal", Variant: "20/20", Signal: "STABLE", WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW"}
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueID), []GemSignal{row}); err != nil {
		t.Fatalf("SaveGemSignals: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM gem_signals WHERE time = $1 AND name = $2 AND variant = $3`,
		tm, row.Name, row.Variant).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveDedicationResults_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-ded"
	registerLeague(t, pool, leagueID)
	tm := futureTime(19)
	cleanupAtTime(t, pool, tm, "dedication_snapshots")

	row := DedicationResult{Time: tm, Color: "RED", GemType: "skill", Mode: "safe", LiquidityRisk: "LOW"}
	if _, err := repo.SaveDedicationResults(ctx, league.Historical(leagueID), []DedicationResult{row}); err != nil {
		t.Fatalf("SaveDedicationResults: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM dedication_snapshots WHERE time = $1 AND color = $2 AND gem_type = $3 AND mode = $4`,
		tm, row.Color, row.GemType, row.Mode).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveMarketContext_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-120-save-mc"
	registerLeague(t, pool, leagueID)
	tm := futureTime(20)
	cleanupAtTime(t, pool, tm, "market_context")

	mc := MarketContext{
		Time:               tm,
		PricePercentiles:   map[string]float64{"P50": 100},
		ListingPercentiles: map[string]float64{"P50": 10},
		TierBoundaries:     TierBoundaries{Boundaries: []float64{100, 50}},
		HourlyBias:         make([]float64, 24),
		HourlyVolatility:   make([]float64, 24),
		HourlyActivity:     make([]float64, 24),
		WeekdayBias:        make([]float64, 7),
		WeekdayVolatility:  make([]float64, 7),
		WeekdayActivity:    make([]float64, 7),
		TemporalMode:       "none",
	}
	if err := repo.SaveMarketContext(ctx, league.Historical(leagueID), mc); err != nil {
		t.Fatalf("SaveMarketContext: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM market_context WHERE time = $1`, tm).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

// ---------------------------------------------------------------------------
// Inner MAX(time) league scoping.
//
// The LatestX methods select `WHERE league = $1 AND time = (SELECT MAX(time)
// FROM <table> WHERE league = $1)`. The `_returnsOnlyScopedLeague` tests above
// seed both leagues at the SAME timestamp, so they cannot observe whether the
// INNER MAX subquery carries `WHERE league = $1`: with equal timestamps the
// scoped and unscoped MAX return the same value.
//
// These tests close that gap. League A's latest row is at tA; league B's row is
// at tB = tA+1h, STRICTLY LATER. Under the correct (scoped) inner MAX, league
// A's MAX is tA and the read returns A's row. If the inner MAX loses its league
// predicate it resolves to tB (the global max), the outer `league=A AND time=tB`
// matches nothing, and the read returns EMPTY — which these tests fail on.
//
// Far-future timestamps guarantee tB is the global MAX(time) across every
// league, so the unscoped-MAX mutation deterministically resolves to tB rather
// than to unrelated production data.
// ---------------------------------------------------------------------------

func TestLatestGemFeatures_innerMaxScopedToLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-featmax-A", "POE-120-featmax-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tA := futureTime(21)
	tB := tA.Add(time.Hour) // strictly later than league A's latest row
	cleanupAtTime(t, pool, tA, "gem_features")
	cleanupAtTime(t, pool, tB, "gem_features")

	base := GemFeature{Name: "POE120 InnerMax Feature", Variant: "20/20", Tier: "LOW", MarketRegime: "TEMPORAL"}
	rowA := base
	rowA.Time = tA
	rowA.Chaos = 111
	rowB := base
	rowB.Time = tB
	rowB.Chaos = 222
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueA), []GemFeature{rowA}); err != nil {
		t.Fatalf("SaveGemFeatures league A: %v", err)
	}
	if _, err := repo.SaveGemFeatures(ctx, league.Historical(leagueB), []GemFeature{rowB}); err != nil {
		t.Fatalf("SaveGemFeatures league B: %v", err)
	}

	res, err := repo.LatestGemFeatures(ctx, league.Historical(leagueA), "", "", 100)
	if err != nil {
		t.Fatalf("LatestGemFeatures: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (league %q's row at its own MAX time tA); empty means the inner MAX(time) subquery is unscoped and resolved to league %q's later timestamp tB", len(res), leagueA, leagueB)
	}
	if !res[0].Time.Equal(tA) {
		t.Errorf("row time = %v, want %v (league %q's latest)", res[0].Time, tA, leagueA)
	}
	if !valuesClose(res[0].Chaos, 111) {
		t.Errorf("chaos = %v, want 111 (league %q's row); 222 means the read pulled league %q's later row", res[0].Chaos, leagueA, leagueB)
	}
}

func TestLatestGemSignals_innerMaxScopedToLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueA, leagueB := "POE-120-sigmax-A", "POE-120-sigmax-B"
	registerLeague(t, pool, leagueA)
	registerLeague(t, pool, leagueB)

	tA := futureTime(22)
	tB := tA.Add(time.Hour) // strictly later than league A's latest row
	cleanupAtTime(t, pool, tA, "gem_signals")
	cleanupAtTime(t, pool, tB, "gem_signals")

	base := GemSignal{
		Name: "POE120 InnerMax Signal", Variant: "20/20",
		Signal: "STABLE", WindowSignal: "CLOSED", SellabilityLabel: "MODERATE", Tier: "LOW",
	}
	rowA := base
	rowA.Time = tA
	rowA.Confidence = 111
	rowB := base
	rowB.Time = tB
	rowB.Confidence = 222
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueA), []GemSignal{rowA}); err != nil {
		t.Fatalf("SaveGemSignals league A: %v", err)
	}
	if _, err := repo.SaveGemSignals(ctx, league.Historical(leagueB), []GemSignal{rowB}); err != nil {
		t.Fatalf("SaveGemSignals league B: %v", err)
	}

	res, err := repo.LatestGemSignals(ctx, league.Historical(leagueA), "", "", 100)
	if err != nil {
		t.Fatalf("LatestGemSignals: %v", err)
	}
	if len(res) != 1 {
		t.Fatalf("result count = %d, want 1 (league %q's row at its own MAX time tA); empty means the inner MAX(time) subquery is unscoped and resolved to league %q's later timestamp tB", len(res), leagueA, leagueB)
	}
	if !res[0].Time.Equal(tA) {
		t.Errorf("row time = %v, want %v (league %q's latest)", res[0].Time, tA, leagueA)
	}
	if res[0].Confidence != 111 {
		t.Errorf("confidence = %d, want 111 (league %q's row); 222 means the read pulled league %q's later row", res[0].Confidence, leagueA, leagueB)
	}
}
