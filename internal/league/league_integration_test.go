//go:build integration

package league

import (
	"context"
	"os"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
)

func TestResolveReadsMirageBootstrap(t *testing.T) {
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}
	if err := db.MigrateUp(db.MigrationsFS, databaseURL); err != nil {
		t.Fatalf("apply migrations: %v", err)
	}

	pool, err := pgxpool.New(context.Background(), databaseURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	defer pool.Close()

	scope, err := Resolve(context.Background(), pool)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if got, want := scope.ID(), "Mirage"; got != want {
		t.Errorf("scope ID = %q, want %q", got, want)
	}
	if got, want := scope.Revision(), int64(1); got != want {
		t.Errorf("scope revision = %d, want %d", got, want)
	}
}
