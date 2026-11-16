package trade

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func runBackground(t *testing.T, g *Gate) (context.Context, context.CancelFunc) {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); g.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	return ctx, cancel
}
func backgroundRequest(context.Context) (*GateRequest, error) {
	return &GateRequest{Gem: "Arc of Surging", Variant: "1", RequestID: "background"}, nil
}

func TestBackgroundRefresh_PileupShedDuringUpstream(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	var searches atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if searches.Add(1) == 1 {
			close(entered)
		}
		select {
		case <-release:
		case <-r.Context().Done():
			return
		}
		w.Write([]byte(`{"id":"test","result":[],"total":0}`))
	}))
	defer upstream.Close()
	g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
	ctx, _ := runBackground(t, g)
	b := NewBackgroundRefresh(g)
	first := make(chan string, 1)
	go func() { first <- b.Run(ctx, backgroundRequest, nil) }()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("first search missing")
	}
	for range 50 {
		if outcome := b.Run(ctx, func(context.Context) (*GateRequest, error) {
			t.Error("overlap reached reconciliation")
			return nil, nil
		}, nil); outcome != "busy" {
			t.Fatalf("overlap: %s", outcome)
		}
	}
	close(release)
	select {
	case outcome := <-first:
		if outcome != "success" {
			t.Fatal(outcome)
		}
	case <-time.After(time.Second):
		t.Fatal("first did not complete")
	}
	if searches.Load() != 1 {
		t.Fatalf("pileup sent %d searches", searches.Load())
	}
	if got, ok := g.cache.Get(CacheKey("Arc of Surging", "1")); !ok || got.FetchedAt.IsZero() {
		t.Fatal("successful empty check missing")
	}
}

func TestBackgroundRefresh_ForegroundPreflightMakesNoClaim(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		close(entered)
		select {
		case <-release:
		case <-r.Context().Done():
			return
		}
		w.Write([]byte(`{"id":"test","result":[],"total":0}`))
	}))
	defer upstream.Close()
	g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
	ctx, _ := runBackground(t, g)
	req := &GateRequest{Gem: "Foreground", Variant: "1", Priority: PriorityHigh, SubmittedAt: time.Now(), Result: make(chan *GateResponse, 1)}
	g.Submit(req)
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("foreground missing")
	}
	b := NewBackgroundRefresh(g)
	outcome := b.Run(ctx, func(context.Context) (*GateRequest, error) { t.Error("busy gate claimed"); return nil, nil }, nil)
	close(release)
	if outcome != "foreground" {
		t.Fatalf("outcome %s", outcome)
	}
	select {
	case r := <-req.Result:
		if r.Error != nil {
			t.Fatal(r.Error)
		}
	case <-time.After(time.Second):
		t.Fatal("foreground did not finish")
	}
}

func TestBackgroundRefresh_RatePreflightMakesNoClaim(t *testing.T) {
	for _, pool := range []string{"search", "fetch"} {
		t.Run(pool, func(t *testing.T) {
			var calls atomic.Int32
			upstream := testGateServer(t, &calls)
			defer upstream.Close()
			g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
			for range 5 {
				g.limiter.Record(pool)
			}
			ctx, _ := runBackground(t, g)
			outcome := NewBackgroundRefresh(g).Run(ctx, func(context.Context) (*GateRequest, error) { t.Error("limited tick claimed"); return nil, nil }, nil)
			if outcome != "limited" || calls.Load() != 0 {
				t.Fatalf("outcome=%s searches=%d", outcome, calls.Load())
			}
		})
	}
}

func TestBackgroundRefresh_CancellationStopsOwnedLookup(t *testing.T) {
	entered := make(chan struct{})
	release := make(chan struct{})
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		close(entered)
		select {
		case <-r.Context().Done():
		case <-release:
		}
	}))
	defer upstream.Close()
	defer close(release)
	g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
	ctx, cancel := runBackground(t, g)
	result := make(chan string, 1)
	go func() {
		result <- NewBackgroundRefresh(g).Run(ctx, backgroundRequest, func(_ *GateRequest, r *GateResponse, _ time.Time) {
			if !errors.Is(r.Error, context.Canceled) {
				t.Errorf("cancellation error %v", r.Error)
			}
		})
	}()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("search missing")
	}
	cancel()
	select {
	case <-result:
	case <-time.After(time.Second):
		t.Fatal("cancellation did not release work")
	}
}

func TestBackgroundRefresh_FailureUsesExistingRetryPolicy(t *testing.T) {
	for _, tc := range []struct {
		name   string
		status int
		delay  time.Duration
	}{
		{"transient", 500, 5 * time.Minute}, {"invalid", 400, 6 * time.Hour}, {"restricted", 429, 5 * time.Minute},
	} {
		t.Run(tc.name, func(t *testing.T) {
			upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(tc.status); w.Write([]byte("upstream")) }))
			defer upstream.Close()
			g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
			now := time.Now()
			g.now = func() time.Time { return now }
			ctx, _ := runBackground(t, g)
			var retry time.Time
			outcome := NewBackgroundRefresh(g).Run(ctx, backgroundRequest, func(_ *GateRequest, r *GateResponse, until time.Time) {
				if r.Error == nil || r.Data != nil {
					t.Errorf("failure became success: %+v", r)
				}
				retry = until
			})
			if outcome != "failed" || retry.Before(now.Add(tc.delay)) {
				t.Fatalf("outcome %s retry %s want >= %s", outcome, retry, now.Add(tc.delay))
			}
			if _, ok := g.cache.Get(CacheKey("Arc of Surging", "1")); ok {
				t.Fatal("failure manufactured check")
			}
		})
	}
}

func TestBackgroundRefresh_FetchFailureIsNotCheck(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost {
			json.NewEncoder(w).Encode(tradeSearchResp{ID: "test", Result: []string{"one"}, Total: 1})
			return
		}
		w.WriteHeader(500)
	}))
	defer upstream.Close()
	g := newTestGate(t, upstream, &mockPublisher{}, time.Second)
	ctx, _ := runBackground(t, g)
	outcome := NewBackgroundRefresh(g).Run(ctx, backgroundRequest, func(_ *GateRequest, r *GateResponse, _ time.Time) {
		if r.Error == nil || r.Data != nil {
			t.Errorf("failed fetch is successful: %+v", r)
		}
	})
	if outcome != "failed" {
		t.Fatal(outcome)
	}
	if _, ok := g.cache.Get(CacheKey("Arc of Surging", "1")); ok {
		t.Fatal("failed fetch stored check")
	}
}
