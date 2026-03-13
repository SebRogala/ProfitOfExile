package collector

import (
	"context"
	"testing"
	"time"
)

// mockStore implements SnapshotStore with configurable function fields.
// Used by scheduler tests to inject controlled behavior without a database.
type mockStore struct {
	LastGemSnapshotTimeFn     func(ctx context.Context) (time.Time, error)
	InsertGemSnapshotsFn      func(ctx context.Context, snapTime time.Time, snapshots []GemSnapshot) (int, error)
	InsertCurrencySnapshotsFn func(ctx context.Context, snapTime time.Time, snapshots []CurrencySnapshot) (int, error)
	LatestSnapshotFn          func(ctx context.Context) (*SnapshotSummary, error)
}

func (m *mockStore) LastGemSnapshotTime(ctx context.Context) (time.Time, error) {
	return m.LastGemSnapshotTimeFn(ctx)
}

func (m *mockStore) InsertGemSnapshots(ctx context.Context, snapTime time.Time, snapshots []GemSnapshot) (int, error) {
	return m.InsertGemSnapshotsFn(ctx, snapTime, snapshots)
}

func (m *mockStore) InsertCurrencySnapshots(ctx context.Context, snapTime time.Time, snapshots []CurrencySnapshot) (int, error) {
	return m.InsertCurrencySnapshotsFn(ctx, snapTime, snapshots)
}

func (m *mockStore) LatestSnapshot(ctx context.Context) (*SnapshotSummary, error) {
	return m.LatestSnapshotFn(ctx)
}

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
