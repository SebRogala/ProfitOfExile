//go:build integration

package lab

import (
	"context"
	"sync"
	"testing"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// statementCounter counts every SQL statement the pool sends, BEGIN and
// COMMIT included.
type statementCounter struct {
	mu    sync.Mutex
	count int
}

func (c *statementCounter) TraceQueryStart(ctx context.Context, _ *pgx.Conn, _ pgx.TraceQueryStartData) context.Context {
	c.mu.Lock()
	c.count++
	c.mu.Unlock()
	return ctx
}

func (c *statementCounter) TraceQueryEnd(context.Context, *pgx.Conn, pgx.TraceQueryEndData) {}

func (c *statementCounter) reset() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	n := c.count
	c.count = 0
	return n
}

func countingPool(t *testing.T, dbURL string) (*pgxpool.Pool, *statementCounter) {
	t.Helper()
	cfg, err := pgxpool.ParseConfig(dbURL)
	if err != nil {
		t.Fatalf("parse pool config: %v", err)
	}
	counter := &statementCounter{}
	cfg.ConnConfig.Tracer = counter
	pool, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatalf("open counting pool: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool, counter
}
