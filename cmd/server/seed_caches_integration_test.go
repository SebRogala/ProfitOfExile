//go:build integration

package main

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// seedCachesLeague registers a league unique to the test and deletes its
// snapshots and the league row on cleanup.
func seedCachesLeague(t *testing.T, pool *pgxpool.Pool) league.Scope {
	t.Helper()
	id := fmt.Sprintf("seedcaches-%d", time.Now().UnixNano())
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		ctx := context.Background()
		if _, err := pool.Exec(ctx, `DELETE FROM gem_snapshots WHERE league = $1`, id); err != nil {
			t.Logf("cleanup warning: delete gem_snapshots for league %q: %v", id, err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
	return league.Historical(id)
}

func seedCachesSnapshot(t *testing.T, pool *pgxpool.Pool, scope league.Scope, tm time.Time) {
	t.Helper()
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots
			(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`,
		scope.ID(), tm, "Seed Caches Gem", "20/20", false, true, 10.0, 5, "BLUE"); err != nil {
		t.Fatalf("seed gem_snapshot at %v: %v", tm, err)
	}
}

func runSeedCaches(t *testing.T, pool *pgxpool.Pool, scope league.Scope) time.Time {
	t.Helper()
	labCache := lab.NewCache(scope)
	r := &recomputeRuntime{ctx: context.Background(), scope: scope, pool: pool, labCache: labCache,
		latestSnapshotTimes: lab.NewRepository(pool).LatestTwoSnapshotTimes}
	r.seedCaches()
	return labCache.For(scope).NextFetch()
}

func TestSeedCaches_NextFetchIsLastSnapshotPlusTheObservedInterval(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)
	last := time.Date(2099, 3, 1, 12, 0, 0, 0, time.UTC)
	seedCachesSnapshot(t, pool, scope, last.Add(-45*time.Minute))
	seedCachesSnapshot(t, pool, scope, last)

	got := runSeedCaches(t, pool, scope)

	if want := last.Add(45 * time.Minute); !got.Equal(want) {
		t.Fatalf("NextFetch = %v, want last snapshot + 45m = %v", got, want)
	}
}

func TestSeedCaches_SingleSnapshotFallsBackToThirtyMinutes(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)
	last := time.Date(2099, 3, 2, 12, 0, 0, 0, time.UTC)
	seedCachesSnapshot(t, pool, scope, last)

	got := runSeedCaches(t, pool, scope)

	if want := last.Add(30 * time.Minute); !got.Equal(want) {
		t.Fatalf("NextFetch = %v, want last snapshot + 30m fallback = %v", got, want)
	}
}

func TestSeedCaches_NoSnapshotsLeavesNextFetchUnset(t *testing.T) {
	pool, _ := requireDatabase(t)
	scope := seedCachesLeague(t, pool)

	got := runSeedCaches(t, pool, scope)

	if !got.IsZero() {
		t.Fatalf("NextFetch = %v, want zero time when the league has no snapshots", got)
	}
}
