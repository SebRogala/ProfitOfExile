//go:build integration

package league

import (
	"context"
	"errors"
	"hash/fnv"
	"os"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
)

// testLockKey derives a deterministic, test-unique advisory-lock key from the
// running test's name. These tests exercise the ProcessLock mechanism, not any
// particular production fence, so they must NOT key on ServerLockKey /
// RuntimeLockKey / DataLockKey / AdministrationLockKey: a live dev server holds
// ServerLockKey on the shared dev DB, which would make an acquire here fail with
// ErrLockHeld. A name-derived key cannot alias any live process fence (distinct
// hash prefix from lockKey), so these pass even while the dev server is up.
func testLockKey(t *testing.T) int64 {
	t.Helper()
	h := fnv.New64a()
	_, _ = h.Write([]byte("profitofexile/league/advisory_integration_test/"))
	_, _ = h.Write([]byte(t.Name()))
	return int64(h.Sum64() & (1<<63 - 1))
}

// newTestPool opens an isolated pool against DATABASE_URL. Two calls yield two
// pools whose connections are distinct Postgres sessions, which is what an
// advisory-lock fence between separate processes actually relies on.
func newTestPool(t *testing.T, databaseURL string) *pgxpool.Pool {
	t.Helper()
	pool, err := pgxpool.New(context.Background(), databaseURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool
}

func requireDatabase(t *testing.T) string {
	t.Helper()
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}
	if err := db.MigrateUp(db.MigrationsFS, databaseURL); err != nil {
		t.Fatalf("apply migrations: %v", err)
	}
	return databaseURL
}

// TestProcessLockExcludesSecondSessionUntilReleased is the load-bearing fence
// test. It proves a held lock excludes a second session and that release hands
// ownership over. It fails if AcquireProcessLock is rewritten to run the lock
// through pool.Exec: that returns the connection to the pool immediately,
// releasing the lock, so poolB would wrongly acquire the key while poolA still
// believes it holds it.
func TestProcessLockExcludesSecondSessionUntilReleased(t *testing.T) {
	requireDatabase(t)
	ctx := context.Background()

	key := testLockKey(t)

	poolA := newTestPool(t, os.Getenv("DATABASE_URL"))
	poolB := newTestPool(t, os.Getenv("DATABASE_URL"))

	lockA, err := AcquireProcessLock(ctx, poolA, key)
	if err != nil {
		t.Fatalf("poolA AcquireProcessLock: %v", err)
	}

	// While A holds the key, B must be refused with the distinguishable
	// sentinel — not blocked, not a generic error.
	if _, err := AcquireProcessLock(ctx, poolB, key); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("poolB AcquireProcessLock while held = %v, want ErrLockHeld", err)
	}

	lockA.Release()

	// After A releases, B must be able to take the same key.
	lockB, err := AcquireProcessLock(ctx, poolB, key)
	if err != nil {
		t.Fatalf("poolB AcquireProcessLock after release = %v, want success", err)
	}
	lockB.Release()
}

// TestReleaseIsIdempotent proves a second Release is a no-op rather than a
// double free of the pooled connection.
func TestReleaseIsIdempotent(t *testing.T) {
	requireDatabase(t)
	ctx := context.Background()

	key := testLockKey(t)
	pool := newTestPool(t, os.Getenv("DATABASE_URL"))
	lock, err := AcquireProcessLock(ctx, pool, key)
	if err != nil {
		t.Fatalf("AcquireProcessLock: %v", err)
	}

	lock.Release()
	lock.Release() // must not panic or double-release the connection

	// The key is free again, provable by re-acquiring it on the same pool.
	reacquired, err := AcquireProcessLock(ctx, pool, key)
	if err != nil {
		t.Fatalf("re-acquire after double Release: %v", err)
	}
	reacquired.Release()
}

// TestCheckHeldReportsLivenessAndRelease proves CheckHeld succeeds while the
// lock is held, errors once the underlying connection is closed (the case that
// silently transfers the Postgres lock to a peer), and reports ErrLockReleased
// after a deliberate Release.
func TestCheckHeldReportsLivenessAndRelease(t *testing.T) {
	requireDatabase(t)
	ctx := context.Background()

	pool := newTestPool(t, os.Getenv("DATABASE_URL"))
	lock, err := AcquireProcessLock(ctx, pool, testLockKey(t))
	if err != nil {
		t.Fatalf("AcquireProcessLock: %v", err)
	}

	if err := lock.CheckHeld(ctx); err != nil {
		t.Fatalf("CheckHeld while held = %v, want nil", err)
	}

	// Close the underlying connection out from under the lock. Postgres
	// auto-releases the advisory lock when the session ends, so CheckHeld must
	// surface an error the caller can fail health on.
	if err := lock.conn.Conn().Close(ctx); err != nil {
		t.Fatalf("close underlying conn: %v", err)
	}
	if err := lock.CheckHeld(ctx); err == nil {
		t.Fatal("CheckHeld after conn close = nil, want error")
	}

	lock.Release()
	if err := lock.CheckHeld(ctx); !errors.Is(err, ErrLockReleased) {
		t.Fatalf("CheckHeld after Release = %v, want ErrLockReleased", err)
	}
}
