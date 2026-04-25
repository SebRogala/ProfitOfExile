package db

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"strconv"

	"github.com/jackc/pgx/v5/pgxpool"
)

const (
	defaultMaxConns = 50
	maxAllowedConns = 10000
)

// resolveMaxConns returns the desired pgxpool MaxConns, honoring the
// POE_DB_MAX_CONNS env var when present and parseable as a positive int
// within the sane upper bound (maxAllowedConns). Invalid, non-positive,
// or out-of-range values log a WARN and fall back to defaultMaxConns.
func resolveMaxConns() int {
	v := os.Getenv("POE_DB_MAX_CONNS")
	if v == "" {
		return defaultMaxConns
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "parse error: "+err.Error(),
			"default", defaultMaxConns,
		)
		return defaultMaxConns
	}
	if n <= 0 {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "must be positive",
			"default", defaultMaxConns,
		)
		return defaultMaxConns
	}
	if n > maxAllowedConns {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "exceeds upper bound",
			"max_allowed", maxAllowedConns,
			"default", defaultMaxConns,
		)
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
	// Clamp MinConns to MaxConns to avoid pgxpool boot failure when
	// POE_DB_MAX_CONNS=1 (MinConns > MaxConns is rejected by pgxpool).
	minConns := int32(2)
	if config.MaxConns < minConns {
		minConns = config.MaxConns
	}
	config.MinConns = minConns

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
