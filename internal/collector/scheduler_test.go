package collector

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"net/http/httptest"
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

// newTestScheduler builds a Scheduler with a short interval suitable for tests.
// Panics on invalid configuration since test inputs should always be valid.
func newTestScheduler(store SnapshotStore, fetcher Fetcher, interval time.Duration) *Scheduler {
	s, err := NewScheduler(
		store,
		[]Fetcher{fetcher},
		nil, // no gem color resolver in unit tests
		interval,
		"Standard",
		"", // no Mercure in unit tests
		"",
		slog.Default(),
	)
	if err != nil {
		panic("newTestScheduler: " + err.Error())
	}
	return s
}

// newFailingMercureServer creates an httptest server that always returns 500
// for Mercure publish requests. Used to verify that Mercure failures are non-fatal.
func newFailingMercureServer(t *testing.T) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
}

func TestScheduler_recentSnapshotSkipsFirstFetch(t *testing.T) {
	// When the last snapshot is recent (within interval), Run should skip the
	// first collect call and wait for the next tick.
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
			return nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s := newTestScheduler(store, fetcher, 15*time.Minute)

	// Run briefly then cancel — enough time for the startup check, not a full tick.
	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if fetchCalled {
		t.Error("fetcher should not have been called when snapshot is recent")
	}
}

func TestScheduler_staleSnapshotFetchesImmediately(t *testing.T) {
	// When the last snapshot is older than the interval, Run should call collect
	// immediately on startup.
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

	ctx, cancel := context.WithCancel(context.Background())
	s := newTestScheduler(store, fetcher, 15*time.Minute)

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if !fetchCalled {
		t.Error("fetcher should have been called immediately when snapshot is stale")
	}
}

func TestScheduler_emptyTableFetchesImmediately(t *testing.T) {
	// When no snapshots exist (zero time returned), Run should fetch immediately.
	fetchCalled := false

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil
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
			return nil, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s := newTestScheduler(store, fetcher, 15*time.Minute)

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if !fetchCalled {
		t.Error("fetcher should have been called when no snapshots exist")
	}
}

func TestScheduler_gemFetchErrorDoesNotBlockCurrencyFetch(t *testing.T) {
	// When gem fetch fails, currency fetch should still run and be stored.
	gemErr := errors.New("ninja API timeout")
	currencyInserted := 0

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil // force immediate collect
		},
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			return len(s), nil
		},
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			currencyInserted += len(s)
			return len(s), nil
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			return nil, gemErr
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s := newTestScheduler(store, fetcher, 15*time.Minute)

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if currencyInserted != 1 {
		t.Errorf("currency inserted = %d, want 1 (gem error should not block currency)", currencyInserted)
	}
}

func TestScheduler_leaguePassedToFetchers(t *testing.T) {
	// Verify that the scheduler passes the configured league to fetchers.
	var receivedGemLeague, receivedCurrencyLeague string

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil // force immediate collect
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
			receivedGemLeague = league
			return nil, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			receivedCurrencyLeague = league
			return nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler(store, []Fetcher{fetcher}, nil, 15*time.Minute, "Mirage", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if receivedGemLeague != "Mirage" {
		t.Errorf("FetchGems received league = %q, want %q", receivedGemLeague, "Mirage")
	}
	if receivedCurrencyLeague != "Mirage" {
		t.Errorf("FetchCurrency received league = %q, want %q", receivedCurrencyLeague, "Mirage")
	}
}

func TestScheduler_mercureFailureIsNonFatal(t *testing.T) {
	// A Mercure publish failure should not prevent the collect cycle from
	// completing successfully or affect snapshot insertion.
	gemsInserted := 0

	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil // force immediate collect
		},
		InsertGemSnapshotsFn: func(ctx context.Context, st time.Time, s []GemSnapshot) (int, error) {
			gemsInserted += len(s)
			return len(s), nil
		},
		InsertCurrencySnapshotsFn: func(ctx context.Context, st time.Time, s []CurrencySnapshot) (int, error) {
			return len(s), nil
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			return []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	// Point Mercure at a server that always returns 500.
	mercureServer := newFailingMercureServer(t)
	defer mercureServer.Close()

	s, err := NewScheduler(store, []Fetcher{fetcher}, nil, 15*time.Minute, "Standard",
		mercureServer.URL, "test-secret", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(20 * time.Millisecond)
	cancel()
	<-done

	if gemsInserted != 1 {
		t.Errorf("gems inserted = %d, want 1 (Mercure failure should not block snapshot insertion)", gemsInserted)
	}
}

func TestScheduler_contextCancellationStopsRun(t *testing.T) {
	// Run should return nil promptly when ctx is cancelled.
	store := &mockStore{
		LastGemSnapshotTimeFn: func(ctx context.Context) (time.Time, error) {
			return time.Now().UTC(), nil // recent — skip first collect
		},
	}

	fetcher := &mockFetcher{
		FetchGemsFn: func(ctx context.Context, league string) ([]GemSnapshot, error) {
			return nil, nil
		},
		FetchCurrencyFn: func(ctx context.Context, league string) ([]CurrencySnapshot, error) {
			return nil, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s := newTestScheduler(store, fetcher, time.Hour) // long interval — won't tick

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()

	cancel()

	select {
	case err := <-done:
		if err != nil {
			t.Errorf("Run returned error = %v, want nil", err)
		}
	case <-time.After(500 * time.Millisecond):
		t.Fatal("Run did not stop within 500ms after context cancellation")
	}
}
