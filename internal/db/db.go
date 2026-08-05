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
	// DefaultMaxConns is the pool size each process gets when POE_DB_MAX_CONNS
	// is unset — which, as of POE-154, is the case on both production
	// containers, so this constant is what actually runs.
	//
	// Exported because it is a budget other packages spend against, not just an
	// internal default: internal/server/handlers sizes the trade-submit writer
	// pool as a share of it, and asserts that share in a test. Anything that
	// starts a fixed number of connection-holding goroutines has to do the same,
	// or it silently claims connections this comment's arithmetic already spent.
	//
	// There is a pgbouncer in front of Postgres. It appears nowhere else in
	// this repository — no compose service, no config, no docs — yet it is in
	// the path of every query, which is how the previous value went unnoticed.
	// Measured on the shared instance:
	//
	//	default_pool_size    10
	//	reserve_pool_size     5   (opened after reserve_pool_timeout = 3s)
	//	query_wait_timeout  120s
	//	pool_mode            transaction
	//
	// Server and collector use a byte-identical DATABASE_URL, so they land in
	// the same (user,db) pool: 15 Postgres backends for both processes
	// combined, not 15 each. The old default of 50 told each process it had 50,
	// so two processes advertised 100 clients over 15 backends. That asymmetry
	// was the defect. Exhaustion did not fail in Go where the caller could see
	// it; clients queued inside pgbouncer and were killed 120 seconds later,
	// surfacing as an unexplained hang.
	//
	// 6 makes the Go pool the binding constraint again: 2 x 6 = 12 <= 15, so
	// full saturation of both processes still fits, dipping 2 into the reserve
	// that exists for exactly that burst. The remaining slack absorbs the
	// short-lived processes that open their own pool — `cmd/migrate` on deploy
	// and the operator CLIs run via `docker exec`.
	//
	// It is not smaller because the server's own machinery needs room: one
	// connection is parked for the process-lifetime fence, the delayed
	// recompute holds a second under DataLockKey, and the gem-event handler
	// runs three analysis goroutines concurrently (RunTransfigure, RunQuality,
	// RunV2 -> RunFont -> RunDedication). At 6 that leaves headroom for request
	// serving even mid-recompute; at 5 a recompute could starve requests
	// outright. Note that pgxpool holds a connection per query, not per
	// goroutine, so those numbers are peak concurrent holders rather than
	// reservations.
	//
	// It is not larger because connections are not the ceiling. Production has
	// nproc = 2: 20 concurrent 548 ms queries is 11 CPU-seconds over 2 cores,
	// which is the 6.35 s worst case POE-152 actually measured. More backends
	// against 2 cores buys a longer tail, not more throughput. Queueing in Go
	// is strictly better than queueing in pgbouncer — it is local, it is
	// cancellable by the caller's context (the router sets a 20s request
	// deadline), and the waiters show up in /debug/pprof/goroutine.
	//
	// Per-container tuning is the follow-up, not this constant. The intended
	// production split is POE_DB_MAX_CONNS=9 on the server and 5 on the
	// collector (14 <= 15), giving the server the larger share because it
	// carries both the request path and the analysis pipeline. 6 is the
	// symmetric value that is safe when nobody sets either.
	//
	// Landmine, recorded rather than fixed: pool_mode=transaction does not
	// guarantee that session-scoped state stays on one backend. That includes
	// golang-migrate's session-level pg_advisory_lock and the process-lifetime
	// fences in cmd/server/main.go and cmd/collector/main.go, which assume the
	// session holding the lock is the session that later releases and
	// health-checks it. Not observed to fail. Do not assume it cannot.
	DefaultMaxConns = 6

	maxAllowedConns = 10000
)

// resolveMaxConns returns the desired pgxpool MaxConns, honoring the
// POE_DB_MAX_CONNS env var when present and parseable as a positive int
// within the sane upper bound (maxAllowedConns). Invalid, non-positive,
// or out-of-range values log a WARN and fall back to DefaultMaxConns.
func resolveMaxConns() int {
	v := os.Getenv("POE_DB_MAX_CONNS")
	if v == "" {
		return DefaultMaxConns
	}
	n, err := strconv.Atoi(v)
	if err != nil {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "parse error: "+err.Error(),
			"default", DefaultMaxConns,
		)
		return DefaultMaxConns
	}
	if n <= 0 {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "must be positive",
			"default", DefaultMaxConns,
		)
		return DefaultMaxConns
	}
	if n > maxAllowedConns {
		slog.Warn("db: POE_DB_MAX_CONNS rejected, using default",
			"raw_value", v,
			"reason", "exceeds upper bound",
			"max_allowed", maxAllowedConns,
			"default", DefaultMaxConns,
		)
		return DefaultMaxConns
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
