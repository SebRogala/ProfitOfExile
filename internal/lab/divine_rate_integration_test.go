//go:build integration

package lab

import (
	"context"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

func seedNinjaDivine(t *testing.T, pool *pgxpool.Pool, leagueID string, at time.Time, chaos float64) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO currency_snapshots (league, time, currency_id, chaos) VALUES ($1, $2, 'divine', $3)`,
		leagueID, at, chaos); err != nil {
		t.Fatalf("seed divine snapshot %s @ %v: %v", leagueID, at, err)
	}
}

func readNinjaDivine(t *testing.T, pool *pgxpool.Pool, leagueID string) (float64, time.Time) {
	t.Helper()
	chaos, at, err := NewRepository(pool).LatestNinjaDivinePrice(context.Background(), league.Historical(leagueID))
	if err != nil {
		t.Fatalf("LatestNinjaDivinePrice(%s): %v", leagueID, err)
	}
	return chaos, at
}

func TestLatestNinjaDivinePrice_readsTheNewestSnapshotWithItsTime(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-339-ndp-1"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(71), "currency_snapshots")
	cleanupAtTime(t, pool, futureTime(72), "currency_snapshots")
	// Newest first: insertion order must not be what picks the answer.
	seedNinjaDivine(t, pool, id, futureTime(72), 180)
	seedNinjaDivine(t, pool, id, futureTime(71), 170)

	chaos, at := readNinjaDivine(t, pool, id)

	if !valuesClose(chaos, 180) || !at.Equal(futureTime(72)) {
		t.Errorf("got (%v, %v), want (180, %v)", chaos, at, futureTime(72))
	}
}

func TestLatestNinjaDivinePrice_ignoresOtherLeagues(t *testing.T) {
	pool := labIntegrationPool(t)
	id, other := "POE-339-ndp-2", "POE-339-ndp-2-other"
	registerLeague(t, pool, id)
	registerLeague(t, pool, other)
	cleanupAtTime(t, pool, futureTime(73), "currency_snapshots")
	cleanupAtTime(t, pool, futureTime(74), "currency_snapshots")
	seedNinjaDivine(t, pool, id, futureTime(73), 180)
	seedNinjaDivine(t, pool, other, futureTime(74), 999)

	chaos, _ := readNinjaDivine(t, pool, id)

	if !valuesClose(chaos, 180) {
		t.Errorf("chaos = %v, want 180 — 999 is the other league's newer row", chaos)
	}
}

func TestLatestNinjaDivinePrice_noSnapshotIsZeroWithoutError(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-339-ndp-3"
	registerLeague(t, pool, id)

	chaos, at := readNinjaDivine(t, pool, id)

	if chaos != 0 || !at.IsZero() {
		t.Errorf("got (%v, %v), want (0, zero time)", chaos, at)
	}
}
