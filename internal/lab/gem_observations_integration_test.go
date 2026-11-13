//go:build integration

package lab

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestGemPricesAsOf_readsTheNewestSnapshotAtOrBeforeAsOf(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-307-gpa-1"
	registerLeague(t, pool, id)
	earlier, later := futureTime(81).Add(1*time.Hour), futureTime(81).Add(3*time.Hour)
	cleanupAtTime(t, pool, earlier, "gem_snapshots")
	cleanupAtTime(t, pool, later, "gem_snapshots")
	seedGemSnapshot(t, pool, id, earlier, "Spark of Nova", "20/20", true, false, 10, 5, "")
	seedGemSnapshot(t, pool, id, later, "Spark of Nova", "20/20", true, false, 30, 5, "")

	gems, snap, err := NewRepository(pool).GemPricesAsOf(context.Background(), league.Historical(id), futureTime(81).Add(2*time.Hour))
	if err != nil {
		t.Fatalf("GemPricesAsOf: %v", err)
	}

	if !snap.Equal(earlier) || len(gems) != 1 || !valuesClose(gems[0].Chaos, 10) {
		t.Errorf("snapshot %v gems %+v, want %v with one 10c line", snap, gems, earlier)
	}
}

func TestGemPricesAsOf_ignoresOtherLeagues(t *testing.T) {
	pool := labIntegrationPool(t)
	idA, idB := "POE-307-gpa-2A", "POE-307-gpa-2B"
	registerLeague(t, pool, idA)
	registerLeague(t, pool, idB)
	tA, tB := futureTime(82).Add(1*time.Hour), futureTime(82).Add(2*time.Hour)
	cleanupAtTime(t, pool, tA, "gem_snapshots")
	cleanupAtTime(t, pool, tB, "gem_snapshots")
	seedGemSnapshot(t, pool, idA, tA, "Spark of Nova", "20/20", true, false, 10, 5, "")
	seedGemSnapshot(t, pool, idB, tB, "Spark of Nova", "20/20", true, false, 99, 5, "")

	gems, snap, err := NewRepository(pool).GemPricesAsOf(context.Background(), league.Historical(idA), futureTime(82).Add(3*time.Hour))
	if err != nil {
		t.Fatalf("GemPricesAsOf: %v", err)
	}

	if !snap.Equal(tA) || len(gems) != 1 || !valuesClose(gems[0].Chaos, 10) {
		t.Errorf("snapshot %v gems %+v, want league A's %v with one 10c line", snap, gems, tA)
	}
}

func TestGemPricesAsOf_noSnapshotIsEmptyWithoutError(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-307-gpa-3"
	registerLeague(t, pool, id)
	tm := futureTime(83).Add(2 * time.Hour)
	cleanupAtTime(t, pool, tm, "gem_snapshots")
	seedGemSnapshot(t, pool, id, tm, "Spark of Nova", "20/20", true, false, 10, 5, "")

	gems, snap, err := NewRepository(pool).GemPricesAsOf(context.Background(), league.Historical(id), futureTime(83).Add(1*time.Hour))

	if err != nil || gems != nil || !snap.IsZero() {
		t.Errorf("GemPricesAsOf = %+v, %v, %v; want nil, zero time, nil", gems, snap, err)
	}
}
