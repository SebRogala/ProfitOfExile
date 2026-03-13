package collector

import (
	"context"
	"testing"
	"time"
)

// Compile-time check that Repository satisfies SnapshotStore.
var _ SnapshotStore = (*Repository)(nil)

func TestInsertGemSnapshots_emptySlice(t *testing.T) {
	// Repository.InsertGemSnapshots short-circuits on empty input without touching the pool.
	// We test this by calling with a nil pool — if it tries to use it, it panics.
	repo := &Repository{pool: nil}
	count, err := repo.InsertGemSnapshots(context.Background(), time.Now(), nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestInsertCurrencySnapshots_emptySlice(t *testing.T) {
	repo := &Repository{pool: nil}
	count, err := repo.InsertCurrencySnapshots(context.Background(), time.Now(), nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}
