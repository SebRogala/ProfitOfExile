package collector

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// Compile-time check that Repository satisfies SnapshotStore.
var _ SnapshotStore = (*Repository)(nil)

func TestInsertGemSnapshots_emptySlice(t *testing.T) {
	// Repository.InsertGemSnapshots short-circuits on empty input without touching the pool.
	// We test this by calling with a nil pool — if it tries to use it, it panics.
	// A valid scope is passed so the short-circuit (not scope validation) is what returns.
	repo := &Repository{pool: nil}
	count, err := repo.InsertGemSnapshots(context.Background(), league.Historical("Mirage"), time.Now(), nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestInsertGemSnapshots_unscopedRejected(t *testing.T) {
	// A zero scope must be rejected before any pool use — nil pool proves no SQL ran.
	repo := &Repository{pool: nil}
	_, err := repo.InsertGemSnapshots(context.Background(), league.Scope{}, time.Now(),
		[]GemSnapshot{{Name: "Arc", Variant: "20/20"}})
	if err == nil {
		t.Fatal("expected error for unscoped insert, got nil")
	}
}

func TestInsertCurrencySnapshots_emptySlice(t *testing.T) {
	repo := &Repository{pool: nil}
	count, err := repo.InsertCurrencySnapshots(context.Background(), league.Historical("Mirage"), time.Now(), nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}
