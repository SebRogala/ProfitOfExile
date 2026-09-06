//go:build integration

package collector

import (
	"context"
	"math"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// testScope pins collector integration writes/reads to the seeded Mirage league.
// The league-control migration registers Mirage, satisfying the FK on the scoped
// snapshot tables. Cleanups are scoped to it so they never touch another league's
// rows that happen to share a timestamp.
var testScope = league.Historical("Mirage")

func integrationPool(t *testing.T) *pgxpool.Pool {
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

	// TimescaleDB guard: verify required hypertables exist before running tests.
	for _, table := range []string{"gem_snapshots", "currency_snapshots", "item_snapshots"} {
		var exists bool
		if err := pool.QueryRow(ctx,
			"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)", table).
			Scan(&exists); err != nil {
			t.Fatalf("check %s table: %v", table, err)
		}
		if !exists {
			t.Skipf("%s table not found, skipping (TimescaleDB migrations not applied)", table)
		}
	}

	return pool
}

func TestInsertGemSnapshots_roundTrip(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	gems := []GemSnapshot{
		{Name: "Arc", Variant: "20/20", Chaos: 150.50, Listings: 42, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Cleave", Variant: "default", Chaos: 5.25, Listings: 100, IsTransfigured: false, GemColor: "RED"},
		{Name: "Arc of Surging", Variant: "default", Chaos: 320.00, Listings: 8, IsTransfigured: true, GemColor: "BLUE"},
	}

	// Register cleanup before assertions so it runs even on failure.
	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM gem_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems)
	if err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}
	if inserted != 3 {
		t.Errorf("inserted = %d, want 3", inserted)
	}

	// Verify round-trip by querying rows back.
	rows, err := pool.Query(ctx,
		`SELECT name, variant, chaos, listings, is_transfigured, COALESCE(gem_color, '')
		 FROM gem_snapshots WHERE time = $1 ORDER BY name`, snapTime)
	if err != nil {
		t.Fatalf("query gem_snapshots: %v", err)
	}
	defer rows.Close()

	type row struct {
		Name           string
		Variant        string
		Chaos          float64
		Listings       int
		IsTransfigured bool
		GemColor       string
	}
	var got []row
	for rows.Next() {
		var r row
		if err := rows.Scan(&r.Name, &r.Variant, &r.Chaos, &r.Listings, &r.IsTransfigured, &r.GemColor); err != nil {
			t.Fatalf("scan: %v", err)
		}
		got = append(got, r)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("rows iteration: %v", err)
	}

	if len(got) != 3 {
		t.Fatalf("row count = %d, want 3", len(got))
	}

	// Rows ordered by name: Arc, Arc of Surging, Cleave.
	if got[0].Name != "Arc" || got[0].Variant != "20/20" {
		t.Errorf("row 0: got %q/%q, want Arc/20/20", got[0].Name, got[0].Variant)
	}
	if math.Abs(got[0].Chaos-150.50) > 0.01 {
		t.Errorf("row 0 chaos = %v, want 150.50", got[0].Chaos)
	}
	if got[0].Listings != 42 {
		t.Errorf("row 0 listings = %d, want 42", got[0].Listings)
	}
	if got[0].IsTransfigured {
		t.Error("row 0 is_transfigured = true, want false")
	}
	if got[0].GemColor != "BLUE" {
		t.Errorf("row 0 gem_color = %q, want BLUE", got[0].GemColor)
	}

	if got[1].Name != "Arc of Surging" || !got[1].IsTransfigured {
		t.Errorf("row 1: got name=%q transfigured=%v, want Arc of Surging/true", got[1].Name, got[1].IsTransfigured)
	}
	if math.Abs(got[1].Chaos-320.00) > 0.01 {
		t.Errorf("row 1 chaos = %v, want 320.00", got[1].Chaos)
	}

	if got[2].Name != "Cleave" || got[2].GemColor != "RED" {
		t.Errorf("row 2: got name=%q color=%q, want Cleave/RED", got[2].Name, got[2].GemColor)
	}
}

func TestInsertCurrencySnapshots_roundTrip(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	currencies := []CurrencySnapshot{
		{CurrencyID: "divine-orb", Chaos: 220.50, SparklineChange: -2.5},
		{CurrencyID: "exalted-orb", Chaos: 15.75, SparklineChange: 1.2},
	}

	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM currency_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted, err := repo.InsertCurrencySnapshots(ctx, testScope, snapTime, currencies)
	if err != nil {
		t.Fatalf("InsertCurrencySnapshots: %v", err)
	}
	if inserted != 2 {
		t.Errorf("inserted = %d, want 2", inserted)
	}

	// Verify all fields stored correctly.
	rows, err := pool.Query(ctx,
		`SELECT currency_id, chaos, sparkline_change
		 FROM currency_snapshots WHERE time = $1 ORDER BY currency_id`, snapTime)
	if err != nil {
		t.Fatalf("query currency_snapshots: %v", err)
	}
	defer rows.Close()

	type row struct {
		CurrencyID      string
		Chaos           float64
		SparklineChange float64
	}
	var got []row
	for rows.Next() {
		var r row
		if err := rows.Scan(&r.CurrencyID, &r.Chaos, &r.SparklineChange); err != nil {
			t.Fatalf("scan: %v", err)
		}
		got = append(got, r)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("rows iteration: %v", err)
	}

	if len(got) != 2 {
		t.Fatalf("row count = %d, want 2", len(got))
	}

	// Ordered by currency_id: divine-orb, exalted-orb.
	if got[0].CurrencyID != "divine-orb" {
		t.Errorf("row 0 currency_id = %q, want divine-orb", got[0].CurrencyID)
	}
	if math.Abs(got[0].Chaos-220.50) > 0.01 {
		t.Errorf("row 0 chaos = %v, want 220.50", got[0].Chaos)
	}
	if math.Abs(got[0].SparklineChange-(-2.5)) > 0.01 {
		t.Errorf("row 0 sparkline_change = %v, want -2.5", got[0].SparklineChange)
	}

	if got[1].CurrencyID != "exalted-orb" {
		t.Errorf("row 1 currency_id = %q, want exalted-orb", got[1].CurrencyID)
	}
	if math.Abs(got[1].Chaos-15.75) > 0.01 {
		t.Errorf("row 1 chaos = %v, want 15.75", got[1].Chaos)
	}
	if math.Abs(got[1].SparklineChange-1.2) > 0.01 {
		t.Errorf("row 1 sparkline_change = %v, want 1.2", got[1].SparklineChange)
	}
}

func TestLastGemSnapshotTime_afterInsert(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	gems := []GemSnapshot{
		{Name: "Fireball", Variant: "default", Chaos: 10.0, Listings: 50, IsTransfigured: false, GemColor: "BLUE"},
	}

	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM gem_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	_, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems)
	if err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}

	lastTime, err := repo.LastGemSnapshotTime(ctx, testScope)
	if err != nil {
		t.Fatalf("LastGemSnapshotTime: %v", err)
	}

	// lastTime should be >= our snapTime (there may be other snapshots in the DB).
	if lastTime.Before(snapTime) {
		t.Errorf("LastGemSnapshotTime = %v, want >= %v", lastTime, snapTime)
	}
}

func TestInsertGemSnapshots_onConflictDoNothing(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	gems := []GemSnapshot{
		{Name: "Lightning Warp", Variant: "default", Chaos: 3.0, Listings: 20, IsTransfigured: false, GemColor: "BLUE"},
	}

	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM gem_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	// First insert should succeed.
	inserted1, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems)
	if err != nil {
		t.Fatalf("first InsertGemSnapshots: %v", err)
	}
	if inserted1 != 1 {
		t.Errorf("first insert count = %d, want 1", inserted1)
	}

	// Second insert with same PK (time, name, variant) should conflict silently.
	inserted2, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems)
	if err != nil {
		t.Fatalf("second InsertGemSnapshots: %v", err)
	}
	if inserted2 != 0 {
		t.Errorf("second insert count = %d, want 0 (conflict)", inserted2)
	}

	// Verify still only one row.
	var count int
	err = pool.QueryRow(ctx,
		"SELECT COUNT(*) FROM gem_snapshots WHERE time = $1 AND name = $2",
		snapTime, "Lightning Warp").Scan(&count)
	if err != nil {
		t.Fatalf("count query: %v", err)
	}
	if count != 1 {
		t.Errorf("row count = %d, want 1 (no duplicate)", count)
	}
}

func TestInsertCurrencySnapshots_onConflictDoNothing(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	currencies := []CurrencySnapshot{
		{CurrencyID: "chaos-orb", Chaos: 1.0, SparklineChange: 0.0},
	}

	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM currency_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted1, err := repo.InsertCurrencySnapshots(ctx, testScope, snapTime, currencies)
	if err != nil {
		t.Fatalf("first InsertCurrencySnapshots: %v", err)
	}
	if inserted1 != 1 {
		t.Errorf("first insert count = %d, want 1", inserted1)
	}

	inserted2, err := repo.InsertCurrencySnapshots(ctx, testScope, snapTime, currencies)
	if err != nil {
		t.Fatalf("second InsertCurrencySnapshots: %v", err)
	}
	if inserted2 != 0 {
		t.Errorf("second insert count = %d, want 0 (conflict)", inserted2)
	}

	var count int
	err = pool.QueryRow(ctx,
		"SELECT COUNT(*) FROM currency_snapshots WHERE time = $1 AND currency_id = $2",
		snapTime, "chaos-orb").Scan(&count)
	if err != nil {
		t.Fatalf("count query: %v", err)
	}
	if count != 1 {
		t.Errorf("row count = %d, want 1 (no duplicate)", count)
	}
}

func TestLatestSnapshot_emptyTables(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	// Use a unique time far in the past to avoid interference with other test data.
	// We test the COALESCE fallback by querying when there are potentially no rows.
	// The LatestSnapshot method uses COALESCE(MAX(time), '1970-01-01'::timestamptz),
	// which should return the epoch time when no rows exist at all (or at least
	// not fail with an error).
	summary, err := repo.LatestSnapshot(ctx, testScope)
	if err != nil {
		t.Fatalf("LatestSnapshot on potentially empty tables: %v", err)
	}

	// Summary should be non-nil and have valid (possibly zero-ish) values.
	if summary == nil {
		t.Fatal("expected non-nil SnapshotSummary, got nil")
	}

	// Gem and currency counts should be >= 0 (there may be data from other tests).
	if summary.GemCount < 0 {
		t.Errorf("GemCount = %d, want >= 0", summary.GemCount)
	}
	if summary.CurrencyCount < 0 {
		t.Errorf("CurrencyCount = %d, want >= 0", summary.CurrencyCount)
	}
}

func TestQueryGemSnapshots_roundTrip(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	gems := []GemSnapshot{
		{Name: "Spark", Variant: "20/20", Chaos: 45.75, Listings: 120, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Spark of Nova", Variant: "default", Chaos: 200.00, Listings: 15, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Detonate Dead", Variant: "default", Chaos: 2.50, Listings: 300, IsTransfigured: false, GemColor: "RED"},
	}

	t.Cleanup(func() {
		_, err := pool.Exec(context.Background(),
			"DELETE FROM gem_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	_, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems)
	if err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}

	// Query with a 1-hour window -- our snapTime is "now" so it should be included.
	snapshots, err := repo.QueryGemSnapshots(ctx, testScope, 1)
	if err != nil {
		t.Fatalf("QueryGemSnapshots: %v", err)
	}

	// Find our test gems in the results (there may be data from other tests).
	found := map[string]GemSnapshot{}
	for _, s := range snapshots {
		if s.Time.Equal(snapTime) {
			found[s.Name] = s
		}
	}

	if len(found) != 3 {
		t.Fatalf("found %d test gems at snapTime, want 3", len(found))
	}

	// Verify field mapping for Spark.
	spark, ok := found["Spark"]
	if !ok {
		t.Fatal("Spark not found in results")
	}
	if spark.Variant != "20/20" {
		t.Errorf("Spark Variant = %q, want %q", spark.Variant, "20/20")
	}
	if math.Abs(spark.Chaos-45.75) > 0.01 {
		t.Errorf("Spark Chaos = %v, want 45.75", spark.Chaos)
	}
	if spark.Listings != 120 {
		t.Errorf("Spark Listings = %d, want 120", spark.Listings)
	}
	if spark.IsTransfigured {
		t.Error("Spark IsTransfigured = true, want false")
	}
	if spark.GemColor != "BLUE" {
		t.Errorf("Spark GemColor = %q, want %q", spark.GemColor, "BLUE")
	}

	// Verify transfigured gem.
	sparkNova, ok := found["Spark of Nova"]
	if !ok {
		t.Fatal("Spark of Nova not found in results")
	}
	if !sparkNova.IsTransfigured {
		t.Error("Spark of Nova IsTransfigured = false, want true")
	}
	if math.Abs(sparkNova.Chaos-200.00) > 0.01 {
		t.Errorf("Spark of Nova Chaos = %v, want 200.00", sparkNova.Chaos)
	}

	// Verify third gem.
	dd, ok := found["Detonate Dead"]
	if !ok {
		t.Fatal("Detonate Dead not found in results")
	}
	if dd.GemColor != "RED" {
		t.Errorf("Detonate Dead GemColor = %q, want %q", dd.GemColor, "RED")
	}
}

func TestLatestSnapshot_afterInserts(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)

	gems := []GemSnapshot{
		{Name: "Frostbolt", Variant: "default", Chaos: 8.0, Listings: 30, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Ice Nova", Variant: "default", Chaos: 12.0, Listings: 25, IsTransfigured: false, GemColor: "BLUE"},
	}
	currencies := []CurrencySnapshot{
		{CurrencyID: "mirror-of-kalandra", Chaos: 50000.0, SparklineChange: 0.5},
	}

	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), "DELETE FROM gem_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime); err != nil {
			t.Logf("cleanup warning: failed to delete gem_snapshots test rows: %v", err)
		}
		if _, err := pool.Exec(context.Background(), "DELETE FROM currency_snapshots WHERE league = $1 AND time = $2", testScope.ID(), snapTime); err != nil {
			t.Logf("cleanup warning: failed to delete currency_snapshots test rows: %v", err)
		}
	})

	if _, err := repo.InsertGemSnapshots(ctx, testScope, snapTime, gems); err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}
	if _, err := repo.InsertCurrencySnapshots(ctx, testScope, snapTime, currencies); err != nil {
		t.Fatalf("InsertCurrencySnapshots: %v", err)
	}

	summary, err := repo.LatestSnapshot(ctx, testScope)
	if err != nil {
		t.Fatalf("LatestSnapshot: %v", err)
	}

	// The latest gem snapshot time should be at least our snapTime.
	if summary.LastGemTime.Before(snapTime) {
		t.Errorf("LastGemTime = %v, want >= %v", summary.LastGemTime, snapTime)
	}
	// Gem count is for the latest snapshot batch — if our snapTime is the latest,
	// it should be at least 2.
	if summary.LastGemTime.Equal(snapTime) && summary.GemCount != 2 {
		t.Errorf("GemCount = %d, want 2 (when our snapshot is latest)", summary.GemCount)
	}

	if summary.LastCurrencyTime.Before(snapTime) {
		t.Errorf("LastCurrencyTime = %v, want >= %v", summary.LastCurrencyTime, snapTime)
	}
	if summary.LastCurrencyTime.Equal(snapTime) && summary.CurrencyCount != 1 {
		t.Errorf("CurrencyCount = %d, want 1 (when our snapshot is latest)", summary.CurrencyCount)
	}
}

// itemSnapshot builds an ItemSnapshot with every stored field populated so a
// round-trip assertion can catch a crossed column.
func itemSnapshot(category string, ninjaID int64, name string) ItemSnapshot {
	return ItemSnapshot{
		Category:        category,
		NinjaID:         ninjaID,
		DetailsID:       "details-" + name,
		Name:            name,
		Variant:         "Fire",
		Links:           6,
		Chaos:           844.6,
		Divine:          2.5,
		Exalted:         361.1,
		Listings:        2553,
		SampleCount:     399,
		StackSize:       10,
		Icon:            "https://web.poecdn.com/gen/image/" + name + ".png",
		ItemClass:       5,
		ItemType:        "Ring",
		BaseType:        "Chronicle of Atzoatl",
		LevelRequired:   49,
		SparklineChange: 40.77,
	}
}

// registerItemLeague inserts a second league so league scoping can be asserted
// against a real neighbour rather than an absence. Registered before the row
// cleanups it must outlive (t.Cleanup is LIFO).
func registerItemLeague(t *testing.T, pool *pgxpool.Pool, id string) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
}

// cleanupItemSnapshots removes every item_snapshots row at the given time,
// across leagues, so a failed assertion cannot leak rows into a later run.
func cleanupItemSnapshots(t *testing.T, pool *pgxpool.Pool, times ...time.Time) {
	t.Helper()
	t.Cleanup(func() {
		for _, tm := range times {
			if _, err := pool.Exec(context.Background(),
				"DELETE FROM item_snapshots WHERE time = $1", tm); err != nil {
				t.Logf("cleanup warning: delete item_snapshots at %v: %v", tm, err)
			}
		}
	})
}

func TestInsertItemSnapshots_roundTrip(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, snapTime)

	want := itemSnapshot("IncursionTemple", 108490, "Locus of Corruption (Tier 3)")

	inserted, err := repo.InsertItemSnapshots(ctx, testScope, snapTime, []ItemSnapshot{want})
	if err != nil {
		t.Fatalf("InsertItemSnapshots: %v", err)
	}
	if inserted != 1 {
		t.Fatalf("inserted = %d, want 1", inserted)
	}

	var got ItemSnapshot
	err = pool.QueryRow(ctx,
		`SELECT category, ninja_id, details_id, name, variant, links,
		        chaos, divine, exalted, listings, sample_count, stack_size,
		        icon, item_class, item_type, base_type, level_required, sparkline_change
		 FROM item_snapshots WHERE league = $1 AND time = $2`, testScope.ID(), snapTime,
	).Scan(&got.Category, &got.NinjaID, &got.DetailsID, &got.Name, &got.Variant, &got.Links,
		&got.Chaos, &got.Divine, &got.Exalted, &got.Listings, &got.SampleCount, &got.StackSize,
		&got.Icon, &got.ItemClass, &got.ItemType, &got.BaseType, &got.LevelRequired,
		&got.SparklineChange)
	if err != nil {
		t.Fatalf("read back item snapshot: %v", err)
	}

	if got != want {
		t.Errorf("stored row = %+v,\nwant %+v", got, want)
	}
}

func TestInsertItemSnapshots_emptyCategoryIsRejected(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, snapTime)

	unstamped := itemSnapshot("", 108490, "Locus of Corruption (Tier 3)")
	stamped := itemSnapshot("Vial", 42, "Sacrifice at Dusk")

	inserted, err := repo.InsertItemSnapshots(ctx, testScope, snapTime, []ItemSnapshot{stamped, unstamped})
	if err == nil {
		t.Fatal("expected an error for a snapshot with no category, got nil")
	}
	if inserted != 0 {
		t.Errorf("inserted = %d, want 0", inserted)
	}

	// The batch must not be half-applied: category is part of the primary key,
	// so the whole write is refused before any row is queued.
	var rows int
	if err := pool.QueryRow(ctx,
		"SELECT count(*) FROM item_snapshots WHERE time = $1", snapTime).Scan(&rows); err != nil {
		t.Fatalf("count rows: %v", err)
	}
	if rows != 0 {
		t.Errorf("stored rows = %d, want 0", rows)
	}
}

func TestInsertItemSnapshots_repeatedNinjaIDInOneTickStoresOnce(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, snapTime)

	first := itemSnapshot("UniqueAccessory", 7747, "Precursor's Emblem")
	repeat := itemSnapshot("UniqueAccessory", 7747, "Precursor's Emblem")
	repeat.Chaos = 1

	inserted, err := repo.InsertItemSnapshots(ctx, testScope, snapTime, []ItemSnapshot{first, repeat})
	if err != nil {
		t.Fatalf("InsertItemSnapshots: %v", err)
	}
	if inserted != 1 {
		t.Errorf("inserted = %d, want 1 (the repeat conflicts on the primary key)", inserted)
	}

	var chaos float64
	if err := pool.QueryRow(ctx,
		"SELECT chaos FROM item_snapshots WHERE league = $1 AND time = $2 AND ninja_id = 7747",
		testScope.ID(), snapTime).Scan(&chaos); err != nil {
		t.Fatalf("read back stored row: %v", err)
	}
	if chaos != first.Chaos {
		t.Errorf("stored chaos = %v, want %v (the first row wins, the repeat is dropped)", chaos, first.Chaos)
	}
}

func TestLastItemSnapshotTime_readsOneCategoryOnly(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	older := time.Now().UTC().Add(-2 * time.Hour).Truncate(time.Microsecond)
	newer := time.Now().UTC().Add(-1 * time.Hour).Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, older, newer)

	if _, err := repo.InsertItemSnapshots(ctx, testScope, older,
		[]ItemSnapshot{itemSnapshot("IncursionTemple", 108490, "Locus of Corruption (Tier 3)")}); err != nil {
		t.Fatalf("insert older IncursionTemple tick: %v", err)
	}
	if _, err := repo.InsertItemSnapshots(ctx, testScope, newer,
		[]ItemSnapshot{itemSnapshot("Vial", 42, "Sacrifice at Dusk")}); err != nil {
		t.Fatalf("insert newer Vial tick: %v", err)
	}

	// IncursionTemple must report its own older tick, not the table maximum —
	// otherwise a category that just started collecting would be considered
	// fresh because a sibling wrote a moment ago.
	gotTemple, err := repo.LastItemSnapshotTime(ctx, testScope, "IncursionTemple")
	if err != nil {
		t.Fatalf("LastItemSnapshotTime IncursionTemple: %v", err)
	}
	if !gotTemple.Equal(older) {
		t.Errorf("IncursionTemple last time = %v, want %v", gotTemple, older)
	}

	gotVial, err := repo.LastItemSnapshotTime(ctx, testScope, "Vial")
	if err != nil {
		t.Fatalf("LastItemSnapshotTime Vial: %v", err)
	}
	if !gotVial.Equal(newer) {
		t.Errorf("Vial last time = %v, want %v", gotVial, newer)
	}
}

func TestLastItemSnapshotTime_ignoresAnotherLeaguesRows(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	const otherLeagueID = "POE-254-Other-League"
	registerItemLeague(t, pool, otherLeagueID)
	otherScope := league.Historical(otherLeagueID)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, snapTime)

	if _, err := repo.InsertItemSnapshots(ctx, otherScope, snapTime,
		[]ItemSnapshot{itemSnapshot("Vial", 42, "Sacrifice at Dusk")}); err != nil {
		t.Fatalf("insert into the other league: %v", err)
	}

	got, err := repo.LastItemSnapshotTime(ctx, testScope, "Vial")
	if err != nil {
		t.Fatalf("LastItemSnapshotTime: %v", err)
	}
	if !got.IsZero() {
		t.Errorf("last time = %v, want the zero time — %s has no Vial rows, only %s does",
			got, testScope.ID(), otherLeagueID)
	}
}

func TestInsertItemSnapshots_writesUnderTheGivenLeague(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	const otherLeagueID = "POE-254-Write-League"
	registerItemLeague(t, pool, otherLeagueID)
	otherScope := league.Historical(otherLeagueID)

	snapTime := time.Now().UTC().Truncate(time.Microsecond)
	cleanupItemSnapshots(t, pool, snapTime)

	if _, err := repo.InsertItemSnapshots(ctx, otherScope, snapTime,
		[]ItemSnapshot{itemSnapshot("UniqueFlask", 900, "Rumi's Concoction")}); err != nil {
		t.Fatalf("InsertItemSnapshots: %v", err)
	}

	var storedLeague string
	if err := pool.QueryRow(ctx,
		"SELECT league FROM item_snapshots WHERE time = $1 AND ninja_id = 900", snapTime).Scan(&storedLeague); err != nil {
		t.Fatalf("read back stored league: %v", err)
	}
	if storedLeague != otherLeagueID {
		t.Errorf("stored league = %q, want %q", storedLeague, otherLeagueID)
	}
}
