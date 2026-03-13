package collector

import (
	"context"
	"errors"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// newFailingMercureServer creates an httptest server that always returns 500
// for Mercure publish requests.
func newFailingMercureServer(t *testing.T) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
}

func TestScheduler_recentSnapshotSkipsFirstFetch(t *testing.T) {
	recentTime := time.Now().UTC().Add(-5 * time.Minute)
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.GemData), nil
		},
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			return recentTime, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) != 0 {
		t.Error("fetcher should not have been called when snapshot is recent")
	}
}

func TestScheduler_staleSnapshotFetchesImmediately(t *testing.T) {
	staleTime := time.Now().UTC().Add(-60 * time.Minute)
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.GemData), nil
		},
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			return staleTime, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) == 0 {
		t.Error("fetcher should have been called immediately when snapshot is stale")
	}
}

func TestScheduler_emptyTableFetchesImmediately(t *testing.T) {
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, nil // zero time = no data
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) == 0 {
		t.Error("fetcher should have been called when no snapshots exist")
	}
}

func TestScheduler_fetchErrorSleepsFallbackInterval(t *testing.T) {
	var fetchCount int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCount, 1)
			return nil, errors.New("ninja API timeout")
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCount) != 1 {
		t.Errorf("fetch count = %d, want 1 (should fetch once then sleep on error)", atomic.LoadInt32(&fetchCount))
	}
}

func TestScheduler_leaguePassedToFetchFunc(t *testing.T) {
	var receivedLeague string

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			receivedLeague = league
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Mirage", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if receivedLeague != "Mirage" {
		t.Errorf("FetchFunc received league = %q, want %q", receivedLeague, "Mirage")
	}
}

func TestScheduler_mercureFailureIsNonFatal(t *testing.T) {
	var storeCount int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&storeCount, 1)
			return len(result.GemData), nil
		},
	}

	mercureServer := newFailingMercureServer(t)
	defer mercureServer.Close()

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", mercureServer.URL, "test-secret", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&storeCount) == 0 {
		t.Error("store should have been called despite Mercure failure")
	}
}

func TestNewScheduler_emptyEndpointsReturnsError(t *testing.T) {
	_, err := NewScheduler([]EndpointConfig{}, nil, "Standard", "", "", slog.Default())
	if err == nil {
		t.Fatal("expected error for empty endpoints, got nil")
	}
	if !strings.Contains(err.Error(), "at least one endpoint") {
		t.Errorf("error = %q, want it to mention 'at least one endpoint'", err.Error())
	}
}

func TestNewScheduler_nilEndpointsReturnsError(t *testing.T) {
	_, err := NewScheduler(nil, nil, "Standard", "", "", slog.Default())
	if err == nil {
		t.Fatal("expected error for nil endpoints, got nil")
	}
	if !strings.Contains(err.Error(), "at least one endpoint") {
		t.Errorf("error = %q, want it to mention 'at least one endpoint'", err.Error())
	}
}

func TestScheduler_contextCancellationStopsRun(t *testing.T) {
	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			return time.Now().UTC(), nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

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

func TestScheduler_multipleEndpointsRunConcurrently(t *testing.T) {
	var gemFetchCount, currencyFetchCount int32

	gemEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&gemFetchCount, 1)
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 1, nil
		},
	}

	currencyEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&currencyFetchCount, 1)
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{gemEp, currencyEp}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&gemFetchCount) == 0 {
		t.Error("gem endpoint should have been fetched")
	}
	if atomic.LoadInt32(&currencyFetchCount) == 0 {
		t.Error("currency endpoint should have been fetched")
	}
}

func TestScheduler_stalenessCheckErrorFetchesImmediately(t *testing.T) {
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			return time.Time{}, errors.New("connection refused")
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) == 0 {
		t.Error("fetcher should have been called immediately when StalenessFunc returns an error")
	}
}

func TestScheduler_nilStalenessFuncFetchesImmediately(t *testing.T) {
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) == 0 {
		t.Error("fetcher should have been called when StalenessFunc is nil")
	}
}

func TestScheduler_calculateSleep(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	tests := []struct {
		name       string
		ep         EndpointConfig
		ageSeconds int
		want       time.Duration
	}{
		{
			name: "fresh response sleeps near MaxAge, capped by FallbackInterval",
			ep: EndpointConfig{
				MaxAge:           30 * time.Minute,
				FallbackInterval: 35 * time.Minute, // enough room for MaxAge + 5s buffer
				MinSleep:         30 * time.Second,
			},
			ageSeconds: 0,
			want:       30*time.Minute + 5*time.Second,
		},
		{
			name: "aged response sleeps shorter",
			ep: EndpointConfig{
				MaxAge:           30 * time.Minute,
				FallbackInterval: 30 * time.Minute,
				MinSleep:         30 * time.Second,
			},
			ageSeconds: 1500,
			want:       5*time.Minute + 5*time.Second,
		},
		{
			name: "stale response clamps to MinSleep",
			ep: EndpointConfig{
				MaxAge:           30 * time.Minute,
				FallbackInterval: 30 * time.Minute,
				MinSleep:         30 * time.Second,
			},
			ageSeconds: 2000,
			want:       30 * time.Second,
		},
		{
			name: "sleep capped at FallbackInterval",
			ep: EndpointConfig{
				MaxAge:           60 * time.Minute,
				FallbackInterval: 30 * time.Minute,
				MinSleep:         30 * time.Second,
			},
			ageSeconds: 0,
			want:       30 * time.Minute, // capped by FallbackInterval
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := s.calculateSleep(tt.ep, tt.ageSeconds)
			if got != tt.want {
				t.Errorf("calculateSleep() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestScheduler_storeErrorDoesNotCrash(t *testing.T) {
	var fetchCount int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCount, 1)
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, errors.New("disk full")
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCount) == 0 {
		t.Error("fetch should have been called even though store will fail")
	}
}
