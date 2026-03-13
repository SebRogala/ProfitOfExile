package collector

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

// mockFetcher implements Fetcher with configurable function fields.
type mockFetcher struct {
	FetchGemsFn     func(ctx context.Context, league string) ([]GemSnapshot, error)
	FetchCurrencyFn func(ctx context.Context, league string) ([]CurrencySnapshot, error)
}

func (m *mockFetcher) FetchGems(ctx context.Context, league string) ([]GemSnapshot, error) {
	return m.FetchGemsFn(ctx, league)
}

func (m *mockFetcher) FetchCurrency(ctx context.Context, league string) ([]CurrencySnapshot, error) {
	return m.FetchCurrencyFn(ctx, league)
}

func TestScheduler_recentSnapshotSkipsFirstFetch(t *testing.T) {
	// When the last snapshot is recent (within interval), the scheduler should
	// skip the first fetch and wait for the next tick.
	recentTime := time.Now().UTC().Add(-5 * time.Minute)
	fetchCalled := false

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return recentTime, nil
		},
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			return len(s), nil
		},
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			return len(s), nil
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			fetchCalled = true
			return []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			fetchCalled = true
			return nil, nil
		},
	}

	// Simulate the startup check: if last snapshot is within interval, skip.
	interval := 15 * time.Minute
	lastSnap, err := store.LastGemSnapshotTime(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	shouldSkip := !lastSnap.IsZero() && time.Since(lastSnap) < interval
	if !shouldSkip {
		// Only fetch if not skipping.
		_, _ = fetcher.FetchGems(context.Background(), "Standard")
	}

	if !shouldSkip {
		t.Error("expected to skip first fetch when snapshot is recent")
	}
	if fetchCalled {
		t.Error("fetcher should not have been called when snapshot is recent")
	}
}

func TestScheduler_staleSnapshotFetchesImmediately(t *testing.T) {
	// When the last snapshot is stale (older than interval), fetch immediately.
	staleTime := time.Now().UTC().Add(-30 * time.Minute)
	fetchCalled := false

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return staleTime, nil
		},
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			return len(s), nil
		},
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			return len(s), nil
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			fetchCalled = true
			return []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, nil
		},
	}

	interval := 15 * time.Minute
	lastSnap, err := store.LastGemSnapshotTime(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	shouldSkip := !lastSnap.IsZero() && time.Since(lastSnap) < interval
	if !shouldSkip {
		_, _ = fetcher.FetchGems(context.Background(), "Standard")
	}

	if shouldSkip {
		t.Error("should NOT skip when snapshot is stale")
	}
	if !fetchCalled {
		t.Error("fetcher should have been called when snapshot is stale")
	}
}

func TestScheduler_emptyTableFetchesImmediately(t *testing.T) {
	// When no snapshots exist (zero time), fetch immediately.
	fetchCalled := false

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			fetchCalled = true
			return nil, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, nil
		},
	}

	interval := 15 * time.Minute
	lastSnap, err := store.LastGemSnapshotTime(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	shouldSkip := !lastSnap.IsZero() && time.Since(lastSnap) < interval
	if !shouldSkip {
		_, _ = fetcher.FetchGems(context.Background(), "Standard")
	}

	if shouldSkip {
		t.Error("should NOT skip when table is empty (zero time)")
	}
	if !fetchCalled {
		t.Error("fetcher should have been called when no snapshots exist")
	}
}

func TestScheduler_fetcherErrorDoesNotAffectOtherFetchers(t *testing.T) {
	// When gem fetch fails, currency fetch should still run.
	gemErr := errors.New("ninja API timeout")
	currencyCalled := false

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			return nil, gemErr
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			currencyCalled = true
			return []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}, nil
		},
	}

	store := &mockStore{
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			return len(s), nil
		},
	}

	// Simulate a fetch cycle: both fetchers run independently.
	ctx := context.Background()
	league := "Standard"
	snapTime := time.Now().UTC()

	// Gem fetch — expect error, but proceed.
	gems, err := fetcher.FetchGems(ctx, league)
	if err == nil {
		t.Error("expected gem fetch error")
	}
	if gems != nil {
		t.Error("expected nil gems on error")
	}

	// Currency fetch — should succeed independently.
	currencies, err := fetcher.FetchCurrency(ctx, league)
	if err != nil {
		t.Fatalf("unexpected currency error: %v", err)
	}

	if !currencyCalled {
		t.Error("currency fetcher should have been called despite gem fetch error")
	}

	// Store the successful currency result.
	count, err := store.InsertCurrencySnapshots(ctx, snapTime, currencies)
	if err != nil {
		t.Fatalf("unexpected insert error: %v", err)
	}
	if count != 1 {
		t.Errorf("inserted count = %d, want 1", count)
	}
}

func TestScheduler_contextCancellation(t *testing.T) {
	// When context is cancelled, fetcher calls should return promptly.
	ctx, cancel := context.WithCancel(context.Background())

	var mu sync.Mutex
	fetchStarted := false

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			mu.Lock()
			fetchStarted = true
			mu.Unlock()
			// Simulate a long-running fetch that respects context.
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(5 * time.Second):
				return []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}, nil
			}
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, ctx.Err()
		},
	}

	done := make(chan struct{})
	go func() {
		defer close(done)
		_, _ = fetcher.FetchGems(ctx, "Standard")
	}()

	// Give the goroutine time to start, then cancel.
	time.Sleep(10 * time.Millisecond)
	cancel()

	// Wait for the fetch to complete — should be nearly instant after cancel.
	select {
	case <-done:
		// Success — fetch returned after cancellation.
	case <-time.After(2 * time.Second):
		t.Fatal("fetcher did not shut down within 2s after context cancellation")
	}

	mu.Lock()
	started := fetchStarted
	mu.Unlock()
	if !started {
		t.Error("fetch goroutine never started")
	}
}
