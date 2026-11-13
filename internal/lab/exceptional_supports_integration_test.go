//go:build integration

package lab

import (
	"context"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

func seedExceptionalSupport(t *testing.T, pool *pgxpool.Pool, leagueID string, day int, name, variant string, corrupted bool, chaos float64) {
	t.Helper()
	seedGemSnapshot(t, pool, leagueID, futureTime(day), name, variant, false, corrupted, chaos, 10, "")
}

func readExceptionalSupports(t *testing.T, pool *pgxpool.Pool, leagueID string) ExceptionalSupportSnapshot {
	t.Helper()
	snap, err := NewRepository(pool).ExceptionalSupportPrices(context.Background(), league.Historical(leagueID))
	if err != nil {
		t.Fatalf("ExceptionalSupportPrices(%s): %v", leagueID, err)
	}
	return snap
}

func TestExceptionalSupportPrices_ignoresCorruptedRows(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-1"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(61), "gem_snapshots")
	seedExceptionalSupport(t, pool, id, 61, "Enhance Support", "1", false, 9)
	seedExceptionalSupport(t, pool, id, 61, "Enhance Support", "1", true, 50)
	seedExceptionalSupport(t, pool, id, 61, "Enlighten Support", "1", false, 185)
	seedExceptionalSupport(t, pool, id, 61, "Empower Support", "1", false, 43.2)

	got, ok := readExceptionalSupports(t, pool, id).Chaos["Enhance Support"]

	if !ok || !valuesClose(got, 9) {
		t.Errorf("Enhance chaos = %v (present %v), want 9", got, ok)
	}
}

func TestExceptionalSupportPrices_ignoresOtherLevels(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-2"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(62), "gem_snapshots")
	seedExceptionalSupport(t, pool, id, 62, "Enhance Support", "1", false, 9)
	seedExceptionalSupport(t, pool, id, 62, "Enhance Support", "4", false, 70)
	seedExceptionalSupport(t, pool, id, 62, "Enlighten Support", "1", false, 185)
	seedExceptionalSupport(t, pool, id, 62, "Empower Support", "1", false, 43.2)

	got, ok := readExceptionalSupports(t, pool, id).Chaos["Enhance Support"]

	if !ok || !valuesClose(got, 9) {
		t.Errorf("Enhance chaos = %v (present %v), want 9", got, ok)
	}
}

func TestExceptionalSupportPrices_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	idA, idB := "POE-328-esp-3A", "POE-328-esp-3B"
	registerLeague(t, pool, idA)
	registerLeague(t, pool, idB)
	cleanupAtTime(t, pool, futureTime(63), "gem_snapshots")
	seedExceptionalSupport(t, pool, idA, 63, "Enhance Support", "1", false, 9)
	seedExceptionalSupport(t, pool, idA, 63, "Enlighten Support", "1", false, 185)
	seedExceptionalSupport(t, pool, idA, 63, "Empower Support", "1", false, 43.2)
	seedExceptionalSupport(t, pool, idB, 63, "Enlighten Support", "1", false, 222)

	got, ok := readExceptionalSupports(t, pool, idA).Chaos["Enlighten Support"]

	if !ok || !valuesClose(got, 185) {
		t.Errorf("league A Enlighten chaos = %v (present %v), want 185", got, ok)
	}
}

func TestExceptionalSupportPrices_readsTheLatestSnapshot(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-4"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(64), "gem_snapshots")
	cleanupAtTime(t, pool, futureTime(65), "gem_snapshots")
	for _, day := range []int{64, 65} {
		seedExceptionalSupport(t, pool, id, day, "Enhance Support", "1", false, 9)
		seedExceptionalSupport(t, pool, id, day, "Enlighten Support", "1", false, 185)
	}
	seedExceptionalSupport(t, pool, id, 64, "Empower Support", "1", false, 30)
	seedExceptionalSupport(t, pool, id, 65, "Empower Support", "1", false, 43.2)

	snap := readExceptionalSupports(t, pool, id)

	if got := snap.Chaos["Empower Support"]; !valuesClose(got, 43.2) || !snap.AsOf.Equal(futureTime(65)) {
		t.Errorf("Empower chaos = %v asOf %v, want 43.2 at %v", got, snap.AsOf, futureTime(65))
	}
}

func TestExceptionalSupportPrices_gemAbsentFromTheLatestSnapshotHasNoPrice(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-5"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(66), "gem_snapshots")
	cleanupAtTime(t, pool, futureTime(67), "gem_snapshots")
	seedExceptionalSupport(t, pool, id, 66, "Enlighten Support", "1", false, 185)
	seedExceptionalSupport(t, pool, id, 67, "Enhance Support", "1", false, 9)

	if got, ok := readExceptionalSupports(t, pool, id).Chaos["Enlighten Support"]; ok {
		t.Errorf("Enlighten chaos = %v from an older snapshot, want no price", got)
	}
}

func TestExceptionalSupportPrices_nullChaosHasNoPrice(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-6"
	registerLeague(t, pool, id)
	cleanupAtTime(t, pool, futureTime(61), "gem_snapshots")
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		VALUES ($1, $2, 'Enhance Support', '1', false, false, NULL, 10, '')`, id, futureTime(61)); err != nil {
		t.Fatalf("seed null-chaos row: %v", err)
	}
	seedExceptionalSupport(t, pool, id, 61, "Enlighten Support", "1", false, 185)

	if got, ok := readExceptionalSupports(t, pool, id).Chaos["Enhance Support"]; ok {
		t.Errorf("Enhance chaos = %v, want no price for a NULL chaos row", got)
	}
}

func TestExceptionalSupportPrices_leagueWithoutSnapshotsReturnsZeroTime(t *testing.T) {
	pool := labIntegrationPool(t)
	id := "POE-328-esp-7"
	registerLeague(t, pool, id)

	snap, err := NewRepository(pool).ExceptionalSupportPrices(context.Background(), league.Historical(id))

	if err != nil || !snap.AsOf.IsZero() || len(snap.Chaos) != 0 {
		t.Errorf("got (%v, %v, err %v), want zero snapshot and nil error", snap.AsOf, snap.Chaos, err)
	}
}
