//go:build integration

package league

import (
	"context"
	"errors"
	"hash/fnv"
	"os"
	"testing"
	"time"

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

// TestAcquireProcessLockWaitTimesOutWhileHeldWholeWindow proves the bounded
// retry gives up with ErrLockHeld — not success, not a block — when the key
// stays held for the entire maxWait window. This is the genuine-double-boot
// case: a second concurrent process must be excluded, and the fence must return
// so the caller can exit non-zero rather than hang forever.
func TestAcquireProcessLockWaitTimesOutWhileHeldWholeWindow(t *testing.T) {
	requireDatabase(t)
	ctx := context.Background()

	key := testLockKey(t)
	holderPool := newTestPool(t, os.Getenv("DATABASE_URL"))
	waiterPool := newTestPool(t, os.Getenv("DATABASE_URL"))

	held, err := AcquireProcessLock(ctx, holderPool, key)
	if err != nil {
		t.Fatalf("holder AcquireProcessLock: %v", err)
	}
	defer held.Release()

	const maxWait = 1 * time.Second
	start := time.Now()
	lock, err := AcquireProcessLockWait(ctx, waiterPool, key, maxWait)
	elapsed := time.Since(start)

	if !errors.Is(err, ErrLockHeld) {
		if lock != nil {
			lock.Release()
		}
		t.Fatalf("AcquireProcessLockWait while held whole window = %v, want ErrLockHeld", err)
	}
	if lock != nil {
		lock.Release()
		t.Fatal("AcquireProcessLockWait returned a lock on timeout; the key was never free")
	}
	// It must actually have waited, not returned instantly — otherwise it is a
	// fail-fast acquire wearing the wait signature. Allow slack below maxWait
	// because the loop stops one retry interval short of overshooting.
	if elapsed < maxWait-lockRetryInterval-100*time.Millisecond {
		t.Fatalf("AcquireProcessLockWait returned after %s, want it to wait near %s", elapsed, maxWait)
	}
}

// TestAcquireProcessLockWaitAcquiresWhenHolderReleasesMidWindow is the
// load-bearing deploy-handoff test. The incumbent holds the fence, then releases
// it partway through the new process's wait window; the new process must retry
// past the initial ErrLockHeld and acquire once the key frees. A fail-fast
// acquire (or a wait variant that does not actually retry) would return
// ErrLockHeld on the first miss and crashloop the redeploy.
func TestAcquireProcessLockWaitAcquiresWhenHolderReleasesMidWindow(t *testing.T) {
	requireDatabase(t)
	ctx := context.Background()

	key := testLockKey(t)
	holderPool := newTestPool(t, os.Getenv("DATABASE_URL"))
	waiterPool := newTestPool(t, os.Getenv("DATABASE_URL"))

	held, err := AcquireProcessLock(ctx, holderPool, key)
	if err != nil {
		t.Fatalf("holder AcquireProcessLock: %v", err)
	}

	// Simulate the old instance finishing its shutdown partway through the new
	// instance's boot wait.
	go func() {
		time.Sleep(300 * time.Millisecond)
		held.Release()
	}()

	lock, err := AcquireProcessLockWait(ctx, waiterPool, key, 3*time.Second)
	if err != nil {
		t.Fatalf("AcquireProcessLockWait after mid-window release = %v, want success", err)
	}
	if lock == nil {
		t.Fatal("AcquireProcessLockWait returned nil lock with nil error")
	}
	lock.Release()
}

// TestAcquireProcessLockWaitHonorsContextCancellation proves a cancelled context
// aborts the wait promptly with ctx.Err() instead of running out the full
// maxWait — a shutdown signal during boot must not be swallowed by the retry.
func TestAcquireProcessLockWaitHonorsContextCancellation(t *testing.T) {
	requireDatabase(t)

	key := testLockKey(t)
	holderPool := newTestPool(t, os.Getenv("DATABASE_URL"))
	waiterPool := newTestPool(t, os.Getenv("DATABASE_URL"))

	held, err := AcquireProcessLock(context.Background(), holderPool, key)
	if err != nil {
		t.Fatalf("holder AcquireProcessLock: %v", err)
	}
	defer held.Release()

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(200 * time.Millisecond)
		cancel()
	}()

	start := time.Now()
	lock, err := AcquireProcessLockWait(ctx, waiterPool, key, 10*time.Second)
	elapsed := time.Since(start)

	if lock != nil {
		lock.Release()
		t.Fatal("AcquireProcessLockWait acquired despite cancellation; key was held throughout")
	}
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("AcquireProcessLockWait on cancel = %v, want context.Canceled", err)
	}
	if elapsed > 2*time.Second {
		t.Fatalf("AcquireProcessLockWait took %s to observe cancellation, want prompt return", elapsed)
	}
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
