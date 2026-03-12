//go:build integration

package db

import (
	"context"
	"os"
	"testing"
	"time"
)

func TestNewPoolIntegration(t *testing.T) {
	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := NewPool(ctx, dbURL)
	if err != nil {
		t.Fatalf("NewPool: %v", err)
	}
	defer pool.Close()

	// Verify the pool can execute a simple query.
	var result int
	err = pool.QueryRow(ctx, "SELECT 1").Scan(&result)
	if err != nil {
		t.Fatalf("query after connect: %v", err)
	}
	if result != 1 {
		t.Errorf("SELECT 1 = %d, want 1", result)
	}
}
