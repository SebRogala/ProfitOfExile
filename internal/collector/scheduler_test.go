package collector

import (
	"context"
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
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

func TestScheduler_startupJitter(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	tests := []struct {
		name      string
		jitterMin time.Duration
		jitterMax time.Duration
		wantZero  bool
	}{
		{
			name:      "JitterMax=0 returns 0",
			jitterMin: 0,
			jitterMax: 0,
			wantZero:  true,
		},
		{
			name:      "JitterMax <= JitterMin returns 0",
			jitterMin: 5 * time.Second,
			jitterMax: 5 * time.Second,
			wantZero:  true,
		},
		{
			name:      "JitterMax < JitterMin returns 0",
			jitterMin: 10 * time.Second,
			jitterMax: 5 * time.Second,
			wantZero:  true,
		},
		{
			name:      "valid range returns value in [JitterMin, JitterMax)",
			jitterMin: 2 * time.Second,
			jitterMax: 7 * time.Second,
			wantZero:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ep := EndpointConfig{
				JitterMin: tt.jitterMin,
				JitterMax: tt.jitterMax,
			}
			got := s.startupJitter(ep)
			if tt.wantZero {
				if got != 0 {
					t.Errorf("startupJitter() = %v, want 0", got)
				}
			} else {
				if got < tt.jitterMin || got >= tt.jitterMax {
					t.Errorf("startupJitter() = %v, want in [%v, %v)", got, tt.jitterMin, tt.jitterMax)
				}
			}
		})
	}
}

func TestScheduler_nilStoreFuncDoesNotPanic(t *testing.T) {
	// Verifies the nil StoreFunc guard in fetchAndStore: the scheduler should
	// complete without panic when StoreFunc is nil.
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
		StoreFunc: nil, // intentionally nil
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
		t.Error("fetch should have been called with nil StoreFunc")
	}
}

func TestScheduler_calculateSleepZeroMaxAge(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		MaxAge:           0, // misconfigured zero MaxAge
		FallbackInterval: 30 * time.Minute,
		MinSleep:         30 * time.Second,
	}

	// With MaxAge=0, sleep = 0 - 0 + 5s = 5s, which is < MinSleep 30s, so clamps to MinSleep.
	got := s.calculateSleep(ep, 0)
	if got != 30*time.Second {
		t.Errorf("calculateSleep(MaxAge=0, age=0) = %v, want %v (MinSleep)", got, 30*time.Second)
	}

	// With MaxAge=0 and age=100, sleep = 0 - 100s + 5s = -95s, clamps to MinSleep.
	got = s.calculateSleep(ep, 100)
	if got != 30*time.Second {
		t.Errorf("calculateSleep(MaxAge=0, age=100) = %v, want %v (MinSleep)", got, 30*time.Second)
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

func TestScheduler_recentCurrencySnapshotSkipsFirstFetch(t *testing.T) {
	recentTime := time.Now().UTC().Add(-5 * time.Minute)
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.CurrencyData), nil
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
		t.Error("currency fetcher should not have been called when snapshot is recent")
	}
}

func TestScheduler_304RetriesUpToMaxThenFallback(t *testing.T) {
	// MaxRetries=3: allows 304 responses at MinSleep while retryCount <= MaxRetries
	// (1..3), then the 4th 304 (retryCount=4 > MaxRetries) resets and sleeps
	// FallbackInterval.
	// With MinSleep=1ms the first retry cycle completes almost immediately, so
	// we assert at least MaxRetries+1 fetches occurred (exhausting one full cycle).
	const maxRetries = 3
	var fetchCount int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           50 * time.Millisecond,
		FallbackInterval: 50 * time.Millisecond,
		MaxRetries:       maxRetries,
		MinSleep:         1 * time.Millisecond,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCount, 1)
			return &FetchResult{NotModified: true, ETag: `"abc"`, Age: 0}, nil
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
	time.Sleep(100 * time.Millisecond)
	cancel()
	<-done

	// At minimum one full retry cycle (MaxRetries+1 fetches) must have completed.
	count := atomic.LoadInt32(&fetchCount)
	if count <= maxRetries {
		t.Errorf("expected >%d fetch calls to exhaust one retry cycle, got %d", maxRetries, count)
	}
}

func TestScheduler_200SleepFromAgeHeader(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		MaxAge:           30 * time.Minute,
		FallbackInterval: 35 * time.Minute,
		MinSleep:         30 * time.Second,
	}

	// Age=600s (10 min), expected sleep = 30m - 10m + 5s = 20m5s
	got := s.calculateSleep(ep, 600)
	want := 20*time.Minute + 5*time.Second
	if got != want {
		t.Errorf("calculateSleep(age=600) = %v, want %v", got, want)
	}
}

func TestScheduler_missingAgeHeaderFallsBackToMaxAge(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		MaxAge:           30 * time.Minute,
		FallbackInterval: 35 * time.Minute,
		MinSleep:         30 * time.Second,
	}

	// Age=0 (missing age header), sleep = MaxAge - 0 + 5s = 30m5s
	got := s.calculateSleep(ep, 0)
	want := 30*time.Minute + 5*time.Second
	if got != want {
		t.Errorf("calculateSleep(age=0) = %v, want %v", got, want)
	}
}

func TestScheduler_ageExceedsMaxAgeClampsToMinSleep(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MinSleep:         30 * time.Second,
	}

	// Age=2100s (35 min) exceeds MaxAge 30 min.
	// sleep = 30m - 35m + 5s = -4m55s -> clamped to MinSleep
	got := s.calculateSleep(ep, 2100)
	if got != 30*time.Second {
		t.Errorf("calculateSleep(age=2100) = %v, want %v (MinSleep)", got, 30*time.Second)
	}
}

func TestScheduler_negativeCalculatedSleepClampsToMinSleep(t *testing.T) {
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		MaxAge:           10 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MinSleep:         30 * time.Second,
	}

	// Age=3600s (60 min), sleep = 10m - 60m + 5s = -49m55s -> clamped to MinSleep
	got := s.calculateSleep(ep, 3600)
	if got != 30*time.Second {
		t.Errorf("calculateSleep(age=3600) = %v, want %v (MinSleep)", got, 30*time.Second)
	}
}

func TestScheduler_contextCancellationStopsAllGoroutines(t *testing.T) {
	var gemStarted, currStarted int32

	gemEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&gemStarted, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	currEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&currStarted, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{gemEp, currEp}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()

	select {
	case err := <-done:
		if err != nil {
			t.Errorf("Run returned error = %v, want nil", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Run did not stop within 2s after context cancellation — goroutine leak suspected")
	}

	// Verify both goroutines actually ran.
	if atomic.LoadInt32(&gemStarted) == 0 {
		t.Error("gem endpoint goroutine did not run before cancellation")
	}
	if atomic.LoadInt32(&currStarted) == 0 {
		t.Error("currency endpoint goroutine did not run before cancellation")
	}
}

func TestScheduler_perSourceSemaphoreShared(t *testing.T) {
	// Both endpoints share source "ninja". Verify they share the same
	// semaphore channel and both can execute.
	var gemFetched, currFetched int32

	gemEp := EndpointConfig{
		Name:   EndpointNinjaGems,
		Source: "ninja",
		MaxAge: 30 * time.Minute, FallbackInterval: time.Hour,
		MaxRetries: 3, MinSleep: 30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&gemFetched, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	currEp := EndpointConfig{
		Name:   EndpointNinjaCurrency,
		Source: "ninja",
		MaxAge: 30 * time.Minute, FallbackInterval: time.Hour,
		MaxRetries: 3, MinSleep: 30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&currFetched, 1)
			return &FetchResult{}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{gemEp, currEp}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&gemFetched) == 0 {
		t.Error("gem endpoint should have fetched")
	}
	if atomic.LoadInt32(&currFetched) == 0 {
		t.Error("currency endpoint should have fetched")
	}

	// Verify shared semaphore: one semaphore for both endpoints with source "ninja".
	if _, ok := s.semaphores["ninja"]; !ok {
		t.Error("expected a shared semaphore for source 'ninja'")
	}
	if len(s.semaphores) != 1 {
		t.Errorf("expected 1 semaphore (shared source), got %d", len(s.semaphores))
	}
}

func TestScheduler_oneEndpointFailureDoesNotAffectOther(t *testing.T) {
	var currencyStored int32

	failingEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return nil, errors.New("network timeout")
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	workingEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&currencyStored, 1)
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{failingEp, workingEp}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&currencyStored) == 0 {
		t.Error("currency endpoint should have stored data despite gem endpoint failure")
	}
}

func TestScheduler_upsertDiscoveriesCalledOnlyForGems(t *testing.T) {
	// Verifies the scheduler completes without crashing when a resolver is
	// present. UpsertDiscoveries will return an error (no database) but the
	// error is logged non-fatally. Note: this test does not verify that
	// UpsertDiscoveries is called exclusively for gems -- that would require a
	// spy/mock on the resolver. It only asserts both endpoints store data.
	resolver := newTestResolver(map[string]string{"Arc": "BLUE"})

	var gemStoreCalled, currStoreCalled int32

	gemEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "20/20", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&gemStoreCalled, 1)
			return 1, nil
		},
	}

	currEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&currStoreCalled, 1)
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{gemEp, currEp}, resolver, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(100 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&gemStoreCalled) == 0 {
		t.Error("gem endpoint store should have been called")
	}
	if atomic.LoadInt32(&currStoreCalled) == 0 {
		t.Error("currency endpoint store should have been called")
	}
}

func TestScheduler_etagPropagatedAcrossFetchCycles(t *testing.T) {
	// Verifies the core cache-aware polling contract: a 200 response with
	// ETag="abc" causes the next FetchFunc call to receive etag="abc".
	var mu sync.Mutex
	var receivedEtags []string
	callCount := 0

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           50 * time.Millisecond,
		FallbackInterval: 50 * time.Millisecond,
		MaxRetries:       3,
		MinSleep:         1 * time.Millisecond,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			mu.Lock()
			receivedEtags = append(receivedEtags, etag)
			callCount++
			c := callCount
			mu.Unlock()

			if c == 1 {
				// First call: return 200 with ETag.
				return &FetchResult{
					GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
					ETag:    `"abc"`,
					Age:     0,
				}, nil
			}
			// Subsequent calls: return 304 to keep looping quickly.
			return &FetchResult{NotModified: true, ETag: `"abc"`, Age: 0}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.GemData), nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(150 * time.Millisecond)
	cancel()
	<-done

	mu.Lock()
	defer mu.Unlock()

	if len(receivedEtags) < 2 {
		t.Fatalf("expected at least 2 fetch calls, got %d", len(receivedEtags))
	}
	if receivedEtags[0] != "" {
		t.Errorf("first fetch etag = %q, want empty string", receivedEtags[0])
	}
	if receivedEtags[1] != `"abc"` {
		t.Errorf("second fetch etag = %q, want %q (ETag should propagate)", receivedEtags[1], `"abc"`)
	}
}

func TestScheduler_retryCountResetsOn200AfterMultiple304s(t *testing.T) {
	// Verifies that retryCount resets to 0 on a 200 response, so subsequent
	// 304s get the full retry budget again instead of exhausting immediately.
	var mu sync.Mutex
	callCount := 0

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           50 * time.Millisecond,
		FallbackInterval: 50 * time.Millisecond,
		MaxRetries:       2,
		MinSleep:         1 * time.Millisecond,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			mu.Lock()
			callCount++
			c := callCount
			mu.Unlock()

			// Calls 1-2: 304, call 3: 200, calls 4+: 304
			switch {
			case c <= 2:
				return &FetchResult{NotModified: true, ETag: `"v1"`, Age: 0}, nil
			case c == 3:
				return &FetchResult{
					GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
					ETag:    `"v2"`,
					Age:     0,
				}, nil
			default:
				return &FetchResult{NotModified: true, ETag: `"v2"`, Age: 0}, nil
			}
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.GemData), nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(200 * time.Millisecond)
	cancel()
	<-done

	mu.Lock()
	defer mu.Unlock()

	// With MaxRetries=2: calls 1,2 are 304 (retryCount 1,2), call 3 is 200
	// (retryCount resets to 0), calls 4,5 are 304 (retryCount 1,2), call 6
	// would exhaust. If reset did NOT happen, call 4 would already exhaust
	// (retryCount would be 3 > MaxRetries=2) and we would get a long sleep.
	// With MinSleep=1ms and 200ms window, we should get at least 5 calls.
	if callCount < 5 {
		t.Errorf("expected at least 5 fetch calls (proving retry reset), got %d", callCount)
	}
}

func TestScheduler_mercurePublishFiresPerEndpoint(t *testing.T) {
	type publishEvent struct {
		Topic    string
		Endpoint string
		Inserted float64 // JSON numbers decode as float64
	}

	var mu sync.Mutex
	var events []publishEvent

	mercureServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err == nil {
			topic := r.FormValue("topic")
			data := r.FormValue("data")
			var payload map[string]any
			if err := json.Unmarshal([]byte(data), &payload); err == nil {
				ep, _ := payload["endpoint"].(string)
				inserted, _ := payload["inserted"].(float64)
				mu.Lock()
				events = append(events, publishEvent{
					Topic:    topic,
					Endpoint: ep,
					Inserted: inserted,
				})
				mu.Unlock()
			}
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer mercureServer.Close()

	gemEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 1, nil
		},
	}

	currEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{gemEp, currEp}, nil, "Standard", mercureServer.URL, "test-secret", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(200 * time.Millisecond)
	cancel()
	<-done

	mu.Lock()
	defer mu.Unlock()

	hasGems := false
	hasCurrency := false
	for _, ev := range events {
		if ev.Endpoint == EndpointNinjaGems {
			hasGems = true
			if ev.Topic != "poe/collector/gems" {
				t.Errorf("gems topic = %q, want %q", ev.Topic, "poe/collector/gems")
			}
			if ev.Inserted != 1 {
				t.Errorf("gems inserted = %v, want 1", ev.Inserted)
			}
		}
		if ev.Endpoint == EndpointNinjaCurrency {
			hasCurrency = true
			if ev.Topic != "poe/collector/currency" {
				t.Errorf("currency topic = %q, want %q", ev.Topic, "poe/collector/currency")
			}
			if ev.Inserted != 1 {
				t.Errorf("currency inserted = %v, want 1", ev.Inserted)
			}
		}
	}
	if !hasGems {
		t.Error("expected Mercure publish for gems endpoint")
	}
	if !hasCurrency {
		t.Error("expected Mercure publish for currency endpoint")
	}
}

func TestNewScheduler_emptyNameReturnsError(t *testing.T) {
	ep := EndpointConfig{
		Name:             "", // empty
		Source:           "ninja",
		FetchFunc:        func(ctx context.Context, league string, etag string) (*FetchResult, error) { return nil, nil },
		FallbackInterval: 30 * time.Minute,
	}

	_, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err == nil {
		t.Fatal("expected error for empty Name, got nil")
	}
	if !strings.Contains(err.Error(), "empty Name") {
		t.Errorf("error = %q, want it to mention 'empty Name'", err.Error())
	}
}

func TestNewScheduler_nilFetchFuncReturnsError(t *testing.T) {
	ep := EndpointConfig{
		Name:             "test-endpoint",
		Source:           "ninja",
		FetchFunc:        nil, // nil
		FallbackInterval: 30 * time.Minute,
	}

	_, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err == nil {
		t.Fatal("expected error for nil FetchFunc, got nil")
	}
	if !strings.Contains(err.Error(), "nil FetchFunc") {
		t.Errorf("error = %q, want it to mention 'nil FetchFunc'", err.Error())
	}
}

func TestNewScheduler_nonPositiveFallbackIntervalReturnsError(t *testing.T) {
	tests := []struct {
		name             string
		fallbackInterval time.Duration
	}{
		{name: "zero FallbackInterval", fallbackInterval: 0},
		{name: "negative FallbackInterval", fallbackInterval: -5 * time.Minute},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ep := EndpointConfig{
				Name:             "test-endpoint",
				Source:           "ninja",
				FetchFunc:        func(ctx context.Context, league string, etag string) (*FetchResult, error) { return nil, nil },
				FallbackInterval: tt.fallbackInterval,
			}

			_, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
			if err == nil {
				t.Fatal("expected error for non-positive FallbackInterval, got nil")
			}
			if !strings.Contains(err.Error(), "non-positive FallbackInterval") {
				t.Errorf("error = %q, want it to mention 'non-positive FallbackInterval'", err.Error())
			}
		})
	}
}

func TestScheduler_invalidFetchResultDoesNotCallStore(t *testing.T) {
	// When FetchFunc returns a FetchResult that fails Validate() (both GemData
	// and CurrencyData populated), StoreFunc must NOT be called and the
	// scheduler should continue running (sleep FallbackInterval, not crash).
	var storeCalled int32
	var fetchCount int32

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           50 * time.Millisecond,
		FallbackInterval: 50 * time.Millisecond,
		MaxRetries:       3,
		MinSleep:         1 * time.Millisecond,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCount, 1)
			// Return an invalid result: both data fields populated.
			return &FetchResult{
				GemData:      []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
				CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}},
			}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&storeCalled, 1)
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
	time.Sleep(150 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&storeCalled) != 0 {
		t.Error("StoreFunc should NOT be called when FetchResult.Validate() fails")
	}
	if atomic.LoadInt32(&fetchCount) == 0 {
		t.Error("FetchFunc should have been called at least once")
	}
}

func TestScheduler_emptySourceNoSemaphore(t *testing.T) {
	// An endpoint with Source="" should run without panic and no semaphore
	// should be created for it.
	var fetchCalled int32

	ep := EndpointConfig{
		Name:             "custom-endpoint",
		Source:           "", // empty source
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			atomic.AddInt32(&fetchCalled, 1)
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	// Verify no semaphore was created for empty source.
	if len(s.semaphores) != 0 {
		t.Errorf("expected 0 semaphores for empty source, got %d", len(s.semaphores))
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(50 * time.Millisecond)
	cancel()
	<-done

	if atomic.LoadInt32(&fetchCalled) == 0 {
		t.Error("endpoint with empty Source should still execute fetches")
	}
}

func TestScheduler_storeErrorReturnsMinSleepNotFallback(t *testing.T) {
	// When StoreFunc returns an error, fetchAndStore must return ep.MinSleep
	// (retry sooner), not ep.FallbackInterval. This ensures transient DB errors
	// trigger a quick retry rather than a long wait.
	s := &Scheduler{
		logger:     slog.Default(),
		semaphores: map[string]chan struct{}{},
	}

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "",
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, errors.New("connection refused")
		},
	}

	state := &endpointState{}
	got := s.fetchAndStore(context.Background(), ep, state)

	if got != ep.MinSleep {
		t.Errorf("fetchAndStore on store error = %v, want %v (MinSleep)", got, ep.MinSleep)
	}
	// Ensure it is NOT FallbackInterval.
	if got == ep.FallbackInterval {
		t.Errorf("fetchAndStore on store error should return MinSleep, not FallbackInterval (%v)", ep.FallbackInterval)
	}
}

func TestScheduler_checkStalenessMinSleepFloor(t *testing.T) {
	// When the remaining freshness window is very small (computed sleep <
	// MinSleep), checkStaleness must clamp up to MinSleep.
	s := &Scheduler{logger: slog.Default()}

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		MaxAge:           30 * time.Minute,
		FallbackInterval: 30 * time.Minute,
		MinSleep:         30 * time.Second,
		StalenessFunc: func(ctx context.Context) (time.Time, error) {
			// Data is 29m55s old: remaining = 30m - 29m55s = 5s, plus 5s buffer = 10s.
			// 10s < MinSleep 30s, so should clamp to 30s.
			return time.Now().UTC().Add(-29*time.Minute - 55*time.Second), nil
		},
	}

	got := s.checkStaleness(context.Background(), ep)
	if got < ep.MinSleep {
		t.Errorf("checkStaleness = %v, want >= %v (MinSleep floor)", got, ep.MinSleep)
	}
}

func TestScheduler_304DoesNotOverwriteLastETag(t *testing.T) {
	// Verifies that a 304 response with a different ETag value does NOT
	// overwrite lastETag. Call 1 returns 200 with ETag "v1", call 2 returns
	// 304 with ETag "v2", call 3 must still receive etag="v1".
	var mu sync.Mutex
	var receivedEtags []string
	callCount := 0

	ep := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           50 * time.Millisecond,
		FallbackInterval: 50 * time.Millisecond,
		MaxRetries:       5,
		MinSleep:         1 * time.Millisecond,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			mu.Lock()
			receivedEtags = append(receivedEtags, etag)
			callCount++
			c := callCount
			mu.Unlock()

			switch c {
			case 1:
				// 200 with ETag "v1".
				return &FetchResult{
					GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
					ETag:    `"v1"`,
					Age:     0,
				}, nil
			case 2:
				// 304 with a DIFFERENT ETag "v2" -- must NOT overwrite lastETag.
				return &FetchResult{NotModified: true, ETag: `"v2"`, Age: 0}, nil
			default:
				// Subsequent: 304 to keep looping.
				return &FetchResult{NotModified: true, Age: 0}, nil
			}
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return len(result.GemData), nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{ep}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(200 * time.Millisecond)
	cancel()
	<-done

	mu.Lock()
	defer mu.Unlock()

	if len(receivedEtags) < 3 {
		t.Fatalf("expected at least 3 fetch calls, got %d", len(receivedEtags))
	}
	if receivedEtags[0] != "" {
		t.Errorf("call 1 etag = %q, want empty", receivedEtags[0])
	}
	if receivedEtags[1] != `"v1"` {
		t.Errorf("call 2 etag = %q, want %q", receivedEtags[1], `"v1"`)
	}
	// Call 3 must still have "v1", NOT "v2" from the 304 response.
	if receivedEtags[2] != `"v1"` {
		t.Errorf("call 3 etag = %q, want %q (304 must not overwrite lastETag)", receivedEtags[2], `"v1"`)
	}
}

func TestScheduler_panicInFetchFuncDoesNotCrashOtherEndpoints(t *testing.T) {
	// When one endpoint's FetchFunc panics, the recover handler should catch
	// it so the other endpoint continues running and Run() returns nil.
	var currencyStored int32

	panickingEp := EndpointConfig{
		Name:             EndpointNinjaGems,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			panic("simulated FetchFunc panic")
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			return 0, nil
		},
	}

	workingEp := EndpointConfig{
		Name:             EndpointNinjaCurrency,
		Source:           "ninja",
		MaxAge:           30 * time.Minute,
		FallbackInterval: time.Hour,
		MaxRetries:       3,
		MinSleep:         30 * time.Second,
		FetchFunc: func(ctx context.Context, league string, etag string) (*FetchResult, error) {
			return &FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}}, nil
		},
		StoreFunc: func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error) {
			atomic.AddInt32(&currencyStored, 1)
			return 1, nil
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	s, err := NewScheduler([]EndpointConfig{panickingEp, workingEp}, nil, "Standard", "", "", slog.Default())
	if err != nil {
		t.Fatalf("NewScheduler: %v", err)
	}

	done := make(chan error, 1)
	go func() { done <- s.Run(ctx) }()
	time.Sleep(150 * time.Millisecond)
	cancel()

	select {
	case err := <-done:
		if err != nil {
			t.Errorf("Run returned error = %v, want nil", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Run did not stop within 2s after context cancellation")
	}

	if atomic.LoadInt32(&currencyStored) == 0 {
		t.Error("currency endpoint should have stored data despite gem endpoint panic")
	}
}
