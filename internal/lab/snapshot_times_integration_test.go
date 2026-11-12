//go:build integration

package lab

import (
	"context"
	"fmt"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestLatestTwoSnapshotTimes_ReturnsNewestAndPreviousDistinctTimes(t *testing.T) {
	pool := labIntegrationPool(t)
	id := fmt.Sprintf("snaptimes-%d", time.Now().UnixNano())
	registerLeague(t, pool, id)
	cleanupLeagueSnapshots(t, pool, id)
	newest := time.Date(2099, 5, 1, 12, 0, 0, 0, time.UTC)
	older := newest.Add(-40 * time.Minute)
	seedGemSnapshot(t, pool, id, newest, "Snap Gem A", "20/20", true, false, 10, 5, "BLUE")
	seedGemSnapshot(t, pool, id, newest, "Snap Gem B", "20/20", true, false, 10, 5, "BLUE")
	seedGemSnapshot(t, pool, id, older, "Snap Gem A", "20/20", true, false, 10, 5, "BLUE")

	last, prev, err := NewRepository(pool).LatestTwoSnapshotTimes(context.Background(), league.Historical(id))
	if err != nil {
		t.Fatalf("LatestTwoSnapshotTimes: %v", err)
	}
	if !last.Equal(newest) || !prev.Equal(older) {
		t.Fatalf("(last, prev) = (%v, %v), want (%v, %v)", last, prev, newest, older)
	}
}

func TestLatestTwoSnapshotTimes_NoRowsReturnsAnError(t *testing.T) {
	pool := labIntegrationPool(t)
	id := fmt.Sprintf("snaptimes-empty-%d", time.Now().UnixNano())
	registerLeague(t, pool, id)

	_, _, err := NewRepository(pool).LatestTwoSnapshotTimes(context.Background(), league.Historical(id))
	if err == nil {
		t.Fatal("LatestTwoSnapshotTimes on a league without snapshots returned no error")
	}
}
