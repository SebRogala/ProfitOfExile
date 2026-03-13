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

func TestMigrationUpDownReversibility(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()

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
