//go:build integration

package main

import (
	"context"
	"os"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
	"profitofexile/internal/league"
)

// newTestPool opens an isolated pool against DATABASE_URL. A second pool yields
// connections that are distinct Postgres sessions — required to simulate a
// competing recompute holding the league data lock from "another process".
func newTestPool(t *testing.T, databaseURL string) *pgxpool.Pool {
	t.Helper()
	pool, err := pgxpool.New(context.Background(), databaseURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool
}

func requireDatabase(t *testing.T) (*pgxpool.Pool, string) {
	t.Helper()
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}
	if err := db.MigrateUp(db.MigrationsFS, databaseURL); err != nil {
		t.Fatalf("apply migrations: %v", err)
	}
	return newTestPool(t, databaseURL), databaseURL
}

// bumpRevision advances runtime_config.revision and restores it on cleanup so
// the mutation does not leak into sibling tests sharing the database.
func bumpRevision(t *testing.T, pool *pgxpool.Pool, delta int64) {
	t.Helper()
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `UPDATE runtime_config SET revision = revision + $1 WHERE singleton = TRUE`, delta); err != nil {
		t.Fatalf("bump revision: %v", err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `UPDATE runtime_config SET revision = revision - $1 WHERE singleton = TRUE`, delta); err != nil {
			t.Logf("restore revision failed: %v", err)
		}
	})
}

// TestPrepareDelayedRecompute_ProceedsWhenLeagueUnchangedAndLockFree proves the
// happy path: the timer's captured scope still matches the active league and no
// peer holds the data lock, so the callback is cleared to run RunV2 and receives
// a live lock it must release.
func TestPrepareDelayedRecompute_ProceedsWhenLeagueUnchangedAndLockFree(t *testing.T) {
	pool, _ := requireDatabase(t)
	ctx := context.Background()

	captured, err := league.Resolve(ctx, pool)
	if err != nil {
		t.Fatalf("resolve captured scope: %v", err)
	}

	lock, skip, err := prepareDelayedRecompute(ctx, pool, captured)
	if err != nil {
		t.Fatalf("prepareDelayedRecompute err = %v, want nil", err)
	}
	if skip != "" {
		t.Fatalf("prepareDelayedRecompute skip = %q, want proceed", skip)
	}
	if lock == nil {
		t.Fatal("prepareDelayedRecompute returned nil lock on the proceed path")
	}
	lock.Release()

	// The lock was genuinely released: a fresh acquire on the same key succeeds.
	reacquired, err := league.AcquireProcessLock(ctx, pool, league.DataLockKey(captured))
	if err != nil {
		t.Fatalf("re-acquire data lock after proceed path: %v", err)
	}
	reacquired.Release()
}

// TestPrepareDelayedRecompute_SkipsWhenLeagueChanged is the load-bearing
// wrong-league guard. The timer captured revision N; the active revision is now
// N+1 (a same-name league bumped mid rolling deploy). Running RunV2 under the
// captured scope would commit stale-league data as current, so the function must
// skip and must NOT hand back a lock. Dropping the id/revision comparison in
// prepareDelayedRecompute makes this return a non-nil lock with an empty skip —
// this test fails in that mutation.
func TestPrepareDelayedRecompute_SkipsWhenLeagueChanged(t *testing.T) {
	pool, _ := requireDatabase(t)
	ctx := context.Background()

	captured, err := league.Resolve(ctx, pool)
	if err != nil {
		t.Fatalf("resolve captured scope: %v", err)
	}

	bumpRevision(t, pool, 1) // active revision drifts past the captured one

	lock, skip, err := prepareDelayedRecompute(ctx, pool, captured)
	if err != nil {
		t.Fatalf("prepareDelayedRecompute err = %v, want a clean skip", err)
	}
	if lock != nil {
		lock.Release()
		t.Fatal("prepareDelayedRecompute proceeded under a changed league; must skip")
	}
	if skip == "" {
		t.Fatal("prepareDelayedRecompute returned no skip reason for a changed league")
	}
}

// TestPrepareDelayedRecompute_SkipsWhenDataLockHeld proves the cross-process
// guard: while a peer session holds the league data lock, the timer must not run
// a second concurrent recompute of the same dataset — it skips without error and
// without returning a lock.
func TestPrepareDelayedRecompute_SkipsWhenDataLockHeld(t *testing.T) {
	pool, databaseURL := requireDatabase(t)
	ctx := context.Background()

	captured, err := league.Resolve(ctx, pool)
	if err != nil {
		t.Fatalf("resolve captured scope: %v", err)
	}

	// A separate session (distinct pool) holds the data lock, standing in for a
	// second server's recompute already in progress.
	peerPool := newTestPool(t, databaseURL)
	held, err := league.AcquireProcessLock(ctx, peerPool, league.DataLockKey(captured))
	if err != nil {
		t.Fatalf("peer AcquireProcessLock: %v", err)
	}
	defer held.Release()

	lock, skip, err := prepareDelayedRecompute(ctx, pool, captured)
	if err != nil {
		t.Fatalf("prepareDelayedRecompute err = %v, want a clean skip", err)
	}
	if lock != nil {
		lock.Release()
		t.Fatal("prepareDelayedRecompute proceeded while the data lock was held; must skip")
	}
	if skip == "" {
		t.Fatal("prepareDelayedRecompute returned no skip reason while the data lock was held")
	}
}
