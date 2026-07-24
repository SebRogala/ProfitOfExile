package league

import (
	"context"
	"errors"
	"fmt"
	"sync"

	"github.com/jackc/pgx/v5/pgxpool"
)

// ErrLockHeld reports that another session already owns the advisory lock. A
// process fence turns this into a refusal of readiness rather than a fatal
// error: the peer that holds the lock is the legitimate owner.
var ErrLockHeld = errors.New("league advisory lock is held by another session")

// ErrLockReleased reports a liveness check against a lock that the local
// process already released. It distinguishes a deliberate release from a
// dropped connection, which surfaces as a query error instead.
var ErrLockReleased = errors.New("league advisory lock was already released")

// ProcessLock is a session-level Postgres advisory lock held for the lifetime
// of a process fence. It owns a dedicated pooled connection: a session advisory
// lock is bound to the connection that took it, so the connection must stay out
// of pool rotation until Release. Running the lock through pool.Exec instead
// would return the connection to the pool immediately and release the lock,
// making the fence silently inert.
type ProcessLock struct {
	conn *pgxpool.Conn
	key  int64

	mu       sync.Mutex
	released bool
}

// AcquireProcessLock takes a non-blocking session advisory lock on key using a
// dedicated connection held for the lock's lifetime. It returns ErrLockHeld
// when another session already owns the key, so a boot fence fails fast instead
// of blocking. The caller owns the returned ProcessLock and must call Release.
func AcquireProcessLock(ctx context.Context, pool *pgxpool.Pool, key int64) (*ProcessLock, error) {
	conn, err := pool.Acquire(ctx)
	if err != nil {
		return nil, fmt.Errorf("acquire advisory lock connection: %w", err)
	}

	var acquired bool
	if err := conn.QueryRow(ctx, `SELECT pg_try_advisory_lock($1)`, key).Scan(&acquired); err != nil {
		conn.Release()
		return nil, fmt.Errorf("try advisory lock: %w", err)
	}
	if !acquired {
		conn.Release()
		return nil, ErrLockHeld
	}

	return &ProcessLock{conn: conn, key: key}, nil
}

// CheckHeld verifies the held connection is still alive by round-tripping a
// trivial query on it. A dropped connection auto-releases the Postgres lock,
// letting a peer silently acquire it, so callers poll this on their health path
// and fail readiness when it errors. It returns ErrLockReleased after Release.
func (l *ProcessLock) CheckHeld(ctx context.Context) error {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.released {
		return ErrLockReleased
	}

	var one int
	if err := l.conn.QueryRow(ctx, `SELECT 1`).Scan(&one); err != nil {
		return fmt.Errorf("advisory lock liveness check: %w", err)
	}
	return nil
}

// Release unlocks the advisory lock and returns the dedicated connection to the
// pool. It is idempotent and safe to call more than once. The unlock runs
// best-effort: a dropped connection has already auto-released the lock in
// Postgres, so a failing unlock is not an error worth propagating.
func (l *ProcessLock) Release() {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.released {
		return
	}
	l.released = true

	// Use a background context: Release is cleanup and must run even when the
	// caller's context is already cancelled (the common shutdown path).
	_, _ = l.conn.Exec(context.Background(), `SELECT pg_advisory_unlock($1)`, l.key)
	l.conn.Release()
}
