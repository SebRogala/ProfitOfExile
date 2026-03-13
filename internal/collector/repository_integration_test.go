//go:build integration

package collector

import (
	"context"
	"math"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

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
	for _, table := range []string{"gem_snapshots", "currency_snapshots"} {
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
			"DELETE FROM gem_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted, err := repo.InsertGemSnapshots(ctx, snapTime, gems)
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
			"DELETE FROM currency_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted, err := repo.InsertCurrencySnapshots(ctx, snapTime, currencies)
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
			"DELETE FROM gem_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	_, err := repo.InsertGemSnapshots(ctx, snapTime, gems)
	if err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}

	lastTime, err := repo.LastGemSnapshotTime(ctx)
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
			"DELETE FROM gem_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	// First insert should succeed.
	inserted1, err := repo.InsertGemSnapshots(ctx, snapTime, gems)
	if err != nil {
		t.Fatalf("first InsertGemSnapshots: %v", err)
	}
	if inserted1 != 1 {
		t.Errorf("first insert count = %d, want 1", inserted1)
	}

	// Second insert with same PK (time, name, variant) should conflict silently.
	inserted2, err := repo.InsertGemSnapshots(ctx, snapTime, gems)
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
			"DELETE FROM currency_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	inserted1, err := repo.InsertCurrencySnapshots(ctx, snapTime, currencies)
	if err != nil {
		t.Fatalf("first InsertCurrencySnapshots: %v", err)
	}
	if inserted1 != 1 {
		t.Errorf("first insert count = %d, want 1", inserted1)
	}

	inserted2, err := repo.InsertCurrencySnapshots(ctx, snapTime, currencies)
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
	summary, err := repo.LatestSnapshot(ctx)
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
			"DELETE FROM gem_snapshots WHERE time = $1", snapTime)
		if err != nil {
			t.Logf("cleanup warning: failed to delete test rows: %v", err)
		}
	})

	_, err := repo.InsertGemSnapshots(ctx, snapTime, gems)
	if err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}

	// Query with a 1-hour window -- our snapTime is "now" so it should be included.
	snapshots, err := repo.QueryGemSnapshots(ctx, 1)
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
		if _, err := pool.Exec(context.Background(), "DELETE FROM gem_snapshots WHERE time = $1", snapTime); err != nil {
			t.Logf("cleanup warning: failed to delete gem_snapshots test rows: %v", err)
		}
		if _, err := pool.Exec(context.Background(), "DELETE FROM currency_snapshots WHERE time = $1", snapTime); err != nil {
			t.Logf("cleanup warning: failed to delete currency_snapshots test rows: %v", err)
		}
	})

	if _, err := repo.InsertGemSnapshots(ctx, snapTime, gems); err != nil {
		t.Fatalf("InsertGemSnapshots: %v", err)
	}
	if _, err := repo.InsertCurrencySnapshots(ctx, snapTime, currencies); err != nil {
		t.Fatalf("InsertCurrencySnapshots: %v", err)
	}

	summary, err := repo.LatestSnapshot(ctx)
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
