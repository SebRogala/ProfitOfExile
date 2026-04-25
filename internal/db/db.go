package db

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"strconv"

	"github.com/jackc/pgx/v5/pgxpool"
)

const defaultMaxConns = 50

// resolveMaxConns returns the desired pgxpool MaxConns, honoring the
// POE_DB_MAX_CONNS env var when present and parseable as a positive int.
// Invalid or non-positive values silently fall back to defaultMaxConns.
func resolveMaxConns() int {
	v := os.Getenv("POE_DB_MAX_CONNS")
	if v == "" {
		return defaultMaxConns
	}
	n, err := strconv.Atoi(v)
	if err != nil || n <= 0 {
		return defaultMaxConns
	}
	return n
}

// NewPool creates a PostgreSQL connection pool from the given database URL.
// It parses the URL, creates the pool, and pings the database to verify
// connectivity before returning.
func NewPool(ctx context.Context, databaseURL string) (*pgxpool.Pool, error) {
	config, err := pgxpool.ParseConfig(databaseURL)
	if err != nil {
		return nil, fmt.Errorf("db: parse config: %w", err)
	}

	config.MaxConns = int32(resolveMaxConns())
	config.MinConns = 2

	pool, err := pgxpool.NewWithConfig(ctx, config)
	if err != nil {
		return nil, fmt.Errorf("db: connect: %w", err)
	}

	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("db: ping: %w", err)
	}

	slog.Info("db: pool configured",
		"max_conns", config.MaxConns,
		"min_conns", config.MinConns,
	)

	return pool, nil
}
