//go:build integration

package migrations_test

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
	"github.com/jackc/pgx/v5/pgxpool"
)

func testSetup(t *testing.T) (*pgxpool.Pool, *migrate.Migrate) {
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

	// golang-migrate uses lib/pq which requires explicit sslmode.
	migrateURL := dbURL
	if !strings.Contains(migrateURL, "sslmode=") {
		if strings.Contains(migrateURL, "?") {
			migrateURL += "&sslmode=disable"
		} else {
			migrateURL += "?sslmode=disable"
		}
	}

	m, err := migrate.New("file://.", migrateURL)
	if err != nil {
		pool.Close()
		t.Fatalf("create migrate instance: %v", err)
	}

	return pool, m
}

// requireTimescaleDB checks that TimescaleDB extension is available and skips
// the test if it is not installed on the database server.
func requireTimescaleDB(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()

	ctx := context.Background()
	var available bool
	err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb')").
		Scan(&available)
	if err != nil {
		t.Fatalf("check timescaledb availability: %v", err)
	}
	if !available {
		t.Skip("TimescaleDB not available, skipping test")
	}
}

func TestMigrationUpDownReversibility(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()

	// Migrations beyond 20260312100000 require TimescaleDB.
	requireTimescaleDB(t, pool)

	ctx := context.Background()

	// Apply all migrations.
	if err := m.Up(); err != nil && err != migrate.ErrNoChange {
		t.Fatalf("migrate up: %v", err)
	}

	// Verify the strategies table exists.
	var exists bool
	err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'strategies')").
		Scan(&exists)
	if err != nil {
		t.Fatalf("check table existence: %v", err)
	}
	if !exists {
		t.Fatal("strategies table should exist after up migration")
	}

	// Verify the index exists.
	err = pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_strategies_league')").
		Scan(&exists)
	if err != nil {
		t.Fatalf("check index existence: %v", err)
	}
	if !exists {
		t.Fatal("idx_strategies_league index should exist after up migration")
	}

	// Verify the trigger exists.
	err = pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.triggers WHERE trigger_name = 'trg_strategies_updated_at')").
		Scan(&exists)
	if err != nil {
		t.Fatalf("check trigger existence: %v", err)
	}
	if !exists {
		t.Fatal("trg_strategies_updated_at trigger should exist after up migration")
	}

	// Roll back all migrations.
	if err := m.Down(); err != nil && err != migrate.ErrNoChange {
		t.Fatalf("migrate down: %v", err)
	}

	// Verify the strategies table no longer exists.
	err = pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'strategies')").
		Scan(&exists)
	if err != nil {
		t.Fatalf("check table after down: %v", err)
	}
	if exists {
		t.Fatal("strategies table should not exist after down migration")
	}

	// Re-apply to leave DB in clean state for other tests.
	if err := m.Up(); err != nil && err != migrate.ErrNoChange {
		t.Fatalf("migrate up (cleanup): %v", err)
	}
}

func TestTimescaleDBMigrations(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()

	requireTimescaleDB(t, pool)

	ctx := context.Background()

	// Apply all migrations.
	if err := m.Up(); err != nil && err != migrate.ErrNoChange {
		t.Fatalf("migrate up: %v", err)
	}

	t.Run("hypertables exist", func(t *testing.T) {
		expectedHypertables := []string{
			"gem_snapshots",
			"font_snapshots",
			"exchange_snapshots",
			"gcp_snapshots",
		}

		for _, name := range expectedHypertables {
			var exists bool
			err := pool.QueryRow(ctx,
				"SELECT EXISTS (SELECT 1 FROM timescaledb_information.hypertables WHERE hypertable_name = $1)",
				name).Scan(&exists)
			if err != nil {
				t.Fatalf("check hypertable %q: %v", name, err)
			}
			if !exists {
				t.Errorf("hypertable %q should exist after migration", name)
			}
		}
	})

	t.Run("gem_colors seeded", func(t *testing.T) {
		var count int
		err := pool.QueryRow(ctx, "SELECT count(*) FROM gem_colors").Scan(&count)
		if err != nil {
			t.Fatalf("count gem_colors: %v", err)
		}
		if count < 700 {
			t.Errorf("gem_colors count = %d, want >= 700 (seed has 750 entries)", count)
		}
	})

	t.Run("gem_colors known entries", func(t *testing.T) {
		knownGems := map[string]string{
			"Arc":             "BLUE",
			"Cleave":          "RED",
			"Rain of Arrows":  "GREEN",
		}

		for gem, wantColor := range knownGems {
			var color string
			err := pool.QueryRow(ctx, "SELECT color FROM gem_colors WHERE name = $1", gem).Scan(&color)
			if err != nil {
				t.Errorf("query gem %q: %v", gem, err)
				continue
			}
			if color != wantColor {
				t.Errorf("gem %q color = %q, want %q", gem, color, wantColor)
			}
		}
	})

	t.Run("continuous aggregates exist", func(t *testing.T) {
		expectedAggregates := []string{
			"gem_snapshots_hourly",
			"gem_snapshots_daily",
		}

		for _, name := range expectedAggregates {
			var exists bool
			err := pool.QueryRow(ctx,
				"SELECT EXISTS (SELECT 1 FROM timescaledb_information.continuous_aggregates WHERE view_name = $1)",
				name).Scan(&exists)
			if err != nil {
				t.Fatalf("check continuous aggregate %q: %v", name, err)
			}
			if !exists {
				t.Errorf("continuous aggregate %q should exist after migration", name)
			}
		}
	})

	t.Run("compression policies exist", func(t *testing.T) {
		compressedTables := []string{
			"gem_snapshots",
			"font_snapshots",
			"exchange_snapshots",
			"gcp_snapshots",
		}

		for _, name := range compressedTables {
			var exists bool
			err := pool.QueryRow(ctx,
				`SELECT EXISTS (
					SELECT 1 FROM timescaledb_information.jobs
					WHERE proc_name = 'policy_compression'
					AND hypertable_name = $1
				)`, name).Scan(&exists)
			if err != nil {
				t.Fatalf("check compression policy for %q: %v", name, err)
			}
			if !exists {
				t.Errorf("compression policy for %q should exist after migration", name)
			}
		}
	})

	t.Run("retention policies exist", func(t *testing.T) {
		retainedTables := []string{
			"gem_snapshots",
			"font_snapshots",
			"exchange_snapshots",
			"gcp_snapshots",
		}

		for _, name := range retainedTables {
			var exists bool
			err := pool.QueryRow(ctx,
				`SELECT EXISTS (
					SELECT 1 FROM timescaledb_information.jobs
					WHERE proc_name = 'policy_retention'
					AND hypertable_name = $1
				)`, name).Scan(&exists)
			if err != nil {
				t.Fatalf("check retention policy for %q: %v", name, err)
			}
			if !exists {
				t.Errorf("retention policy for %q should exist after migration", name)
			}
		}
	})

	t.Run("full down migration reversibility", func(t *testing.T) {
		// Roll back ALL migrations — verifies correct ordering:
		// continuous aggregates first, then policies, then hypertables, then extension.
		if err := m.Down(); err != nil && err != migrate.ErrNoChange {
			t.Fatalf("migrate down: %v", err)
		}

		// Verify hypertables are gone.
		var count int
		err := pool.QueryRow(ctx,
			"SELECT count(*) FROM information_schema.tables WHERE table_name IN ('gem_snapshots', 'font_snapshots', 'exchange_snapshots', 'gcp_snapshots', 'gem_colors')").
			Scan(&count)
		if err != nil {
			t.Fatalf("check tables after down: %v", err)
		}
		if count != 0 {
			t.Errorf("expected 0 price tables after full down, got %d", count)
		}

		// Re-apply all migrations to leave DB in clean state.
		if err := m.Up(); err != nil && err != migrate.ErrNoChange {
			t.Fatalf("migrate up (restore): %v", err)
		}
	})
}
