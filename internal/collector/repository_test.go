package collector

import (
	"context"
	"testing"
	"time"
)

// SnapshotStore defines the interface that Repository implements.
// Used by the scheduler and other consumers to decouple from pgxpool.
type SnapshotStore interface {
	LastGemSnapshotTime(ctx context.Context) (time.Time, error)
	InsertGemSnapshots(ctx context.Context, snapTime time.Time, snapshots []GemSnapshot) (int, error)
	InsertCurrencySnapshots(ctx context.Context, snapTime time.Time, snapshots []CurrencySnapshot) (int, error)
	LatestSnapshot(ctx context.Context) (*SnapshotSummary, error)
}

// mockStore implements SnapshotStore with configurable function fields.
type mockStore struct {
	LastGemSnapshotTimeFn    func(ctx context.Context) (time.Time, error)
	InsertGemSnapshotsFn     func(ctx context.Context, snapTime time.Time, snapshots []GemSnapshot) (int, error)
	InsertCurrencySnapshotsFn func(ctx context.Context, snapTime time.Time, snapshots []CurrencySnapshot) (int, error)
	LatestSnapshotFn         func(ctx context.Context) (*SnapshotSummary, error)
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

func TestMockStore_insertGemSnapshots(t *testing.T) {
	snapTime := time.Date(2026, 3, 13, 12, 0, 0, 0, time.UTC)
	gems := []GemSnapshot{
		{Name: "Arc", Variant: "20/20", Chaos: 15.5, Listings: 300},
		{Name: "Cleave", Variant: "default", Chaos: 1.0, Listings: 500, IsTransfigured: false},
	}

	store := &mockStore{
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			if !st.Equal(snapTime) {
				t.Errorf("snapTime = %v, want %v", st, snapTime)
			}
			return len(s), nil
		},
	}

	count, err := store.InsertGemSnapshots(context.Background(), snapTime, gems)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 2 {
		t.Errorf("count = %d, want 2", count)
	}
}

func TestMockStore_insertCurrencySnapshots(t *testing.T) {
	snapTime := time.Date(2026, 3, 13, 12, 0, 0, 0, time.UTC)
	currencies := []CurrencySnapshot{
		{CurrencyID: "divine", Chaos: 210.5, SparklineChange: -2.3},
	}

	store := &mockStore{
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			return len(s), nil
		},
	}

	count, err := store.InsertCurrencySnapshots(context.Background(), snapTime, currencies)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if count != 1 {
		t.Errorf("count = %d, want 1", count)
	}
}

func TestMockStore_duplicateInsert(t *testing.T) {
	// Simulates ON CONFLICT DO NOTHING — second insert returns 0.
	callCount := 0
	store := &mockStore{
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			callCount++
			if callCount == 1 {
				return len(s), nil
			}
			// Duplicate: no rows inserted.
			return 0, nil
		},
	}

	gems := []GemSnapshot{{Name: "Arc", Variant: "20/20", Chaos: 15.5, Listings: 300}}
	snapTime := time.Now().UTC()

	// First insert — all rows inserted.
	count1, err := store.InsertGemSnapshots(context.Background(), snapTime, gems)
	if err != nil {
		t.Fatalf("first insert error: %v", err)
	}
	if count1 != 1 {
		t.Errorf("first insert count = %d, want 1", count1)
	}

	// Second insert — duplicate, zero rows.
	count2, err := store.InsertGemSnapshots(context.Background(), snapTime, gems)
	if err != nil {
		t.Fatalf("second insert error: %v", err)
	}
	if count2 != 0 {
		t.Errorf("second insert count = %d, want 0 (ON CONFLICT DO NOTHING)", count2)
	}
}

func TestMockStore_lastGemSnapshotTime_emptyTable(t *testing.T) {
	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil
		},
	}

	got, err := store.LastGemSnapshotTime(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !got.IsZero() {
		t.Errorf("got %v, want zero time for empty table", got)
	}
}

func TestMockStore_lastGemSnapshotTime_withData(t *testing.T) {
	expected := time.Date(2026, 3, 13, 10, 0, 0, 0, time.UTC)
	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return expected, nil
		},
	}

	got, err := store.LastGemSnapshotTime(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !got.Equal(expected) {
		t.Errorf("got %v, want %v", got, expected)
	}
}
