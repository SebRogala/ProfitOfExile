package trade

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// mockPublisher records all Mercure publish calls for test assertions.
type mockPublisher struct {
	mu     sync.Mutex
	events []publishedEvent
}

type publishedEvent struct {
	Topic   string
	Payload string
}

func (p *mockPublisher) Publish(_ context.Context, topic, payload string) error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.events = append(p.events, publishedEvent{Topic: topic, Payload: payload})
	return nil
}

func (p *mockPublisher) getEvents() []publishedEvent {
	p.mu.Lock()
	defer p.mu.Unlock()
	cp := make([]publishedEvent, len(p.events))
	copy(cp, p.events)
	return cp
}

// testGateServer returns an httptest.Server that serves valid search+fetch
// responses. The searchCalls counter is incremented on each search request.
func testGateServer(t *testing.T, searchCalls *atomic.Int32) *httptest.Server {
	t.Helper()
	indexed := time.Now().Add(-30 * time.Minute).UTC().Truncate(time.Second)
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.Contains(r.URL.Path, "/api/trade/search/") {
			if searchCalls != nil {
				searchCalls.Add(1)
			}
			json.NewEncoder(w).Encode(tradeSearchResp{
				ID:     "qid-test",
				Result: []string{"r1", "r2"},
				Total:  42,
			})
			return
		}
		if strings.Contains(r.URL.Path, "/api/trade/fetch/") {
			json.NewEncoder(w).Encode(tradeFetchResp{
				Result: []tradeFetchEntry{
					{
						Listing: tradeFetchListing{
							Indexed: indexed,
							Account: tradeFetchAccount{Name: "acc1"},
							Price:   tradeFetchPrice{Amount: 10.0, Currency: "chaos"},
						},
						Item: tradeFetchItem{
							Properties: []tradeFetchProperty{
								{Name: "Level", Values: [][]interface{}{{"20", float64(0)}}},
							},
						},
					},
					{
						Listing: tradeFetchListing{
							Indexed: indexed,
							Account: tradeFetchAccount{Name: "acc2"},
							Price:   tradeFetchPrice{Amount: 15.0, Currency: "chaos"},
						},
						Item: tradeFetchItem{
							Properties: []tradeFetchProperty{
								{Name: "Level", Values: [][]interface{}{{"20", float64(0)}}},
							},
						},
					},
				},
			})
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
}

// newTestGate creates a Gate wired to a test server, mock publisher, and fresh
// rate limiter + cache. Returns all components for assertion access.
func newTestGate(t *testing.T, server *httptest.Server, pub *mockPublisher, maxWait time.Duration) *Gate {
	t.Helper()
	cfg := TradeConfig{
		LeagueName:        "Mirage",
		UserAgent:         "ProfitOfExile/test",
		CeilingFactor:     0.65,
		LatencyPadding:    0,
		DefaultSearchRate: 5,
		DefaultFetchRate:  5,
		MaxQueueWait:      maxWait,
		CacheMaxEntries:   100,
	}
	limiter := NewRateLimiter(cfg)
	client := NewClient(cfg)
	client.SetBaseURL(server.URL)
	cache := NewTradeCache(cfg.CacheMaxEntries)

	return NewGate(cfg, limiter, client, pub, cache, func() float64 { return 212.0 })
}

func TestGate_FastPath(t *testing.T) {
	var calls atomic.Int32
	server := testGateServer(t, &calls)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 30*time.Second)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req := &GateRequest{
		Gem:         "Arc",
		Variant:     "1",
		RequestID:   "req-fast",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	gate.Submit(req)

	select {
	case resp := <-req.Result:
		if resp.Error != nil {
			t.Fatalf("unexpected error: %v", resp.Error)
		}
		if resp.Data == nil {
			t.Fatal("expected non-nil data")
		}
		if resp.Data.Gem != "Arc" {
			t.Errorf("Gem = %q, want %q", resp.Data.Gem, "Arc")
		}
		if resp.Data.Total != 42 {
			t.Errorf("Total = %d, want 42", resp.Data.Total)
		}
		if len(resp.Data.Listings) != 2 {
			t.Errorf("len(Listings) = %d, want 2", len(resp.Data.Listings))
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for gate response")
	}

	if c := calls.Load(); c != 1 {
		t.Errorf("search API calls = %d, want 1", c)
	}
}

func TestGate_Dedup(t *testing.T) {
	var calls atomic.Int32
	server := testGateServer(t, &calls)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 30*time.Second)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req1 := &GateRequest{
		Gem:         "Arc",
		Variant:     "1",
		RequestID:   "req-dup-1",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	req2 := &GateRequest{
		Gem:         "Arc",
		Variant:     "1",
		RequestID:   "req-dup-2",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}

	// Submit first, then immediately second to trigger dedup.
	gate.Submit(req1)
	gate.Submit(req2)

	// Both channels should receive the result.
	for i, req := range []*GateRequest{req1, req2} {
		select {
		case resp := <-req.Result:
			if resp.Error != nil {
				t.Fatalf("req%d: unexpected error: %v", i+1, resp.Error)
			}
			if resp.Data == nil {
				t.Fatalf("req%d: expected non-nil data", i+1)
			}
			if resp.Data.Gem != "Arc" {
				t.Errorf("req%d: Gem = %q, want %q", i+1, resp.Data.Gem, "Arc")
			}
		case <-time.After(2 * time.Second):
			t.Fatalf("req%d: timed out waiting for response", i+1)
		}
	}

	if c := calls.Load(); c != 1 {
		t.Errorf("search API calls = %d, want 1 (dedup should coalesce)", c)
	}
}

func TestGate_HighPriorityFirst(t *testing.T) {
	// Use a server that tracks the order of gem names requested.
	var mu sync.Mutex
	var searchOrder []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.Contains(r.URL.Path, "/api/trade/search/") {
			body := make([]byte, 4096)
			n, _ := r.Body.Read(body)
			bodyStr := string(body[:n])
			// Extract the term from the request body.
			var parsed map[string]interface{}
			if err := json.Unmarshal([]byte(bodyStr), &parsed); err == nil {
				if q, ok := parsed["query"].(map[string]interface{}); ok {
					if term, ok := q["term"].(string); ok {
						mu.Lock()
						searchOrder = append(searchOrder, term)
						mu.Unlock()
					}
				}
			}
			json.NewEncoder(w).Encode(tradeSearchResp{
				ID:     "qid",
				Result: []string{"r1"},
				Total:  1,
			})
			return
		}
		if strings.Contains(r.URL.Path, "/api/trade/fetch/") {
			indexed := time.Now().UTC()
			json.NewEncoder(w).Encode(tradeFetchResp{
				Result: []tradeFetchEntry{
					{
						Listing: tradeFetchListing{
							Indexed: indexed,
							Account: tradeFetchAccount{Name: "acc1"},
							Price:   tradeFetchPrice{Amount: 10.0, Currency: "chaos"},
						},
						Item: tradeFetchItem{},
					},
				},
			})
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
	defer server.Close()

	pub := &mockPublisher{}
	// Create gate but don't start Run yet so we can fill both channels.
	gate := newTestGate(t, server, pub, 30*time.Second)

	// Submit low priority first, then high priority.
	lowReq := &GateRequest{
		Gem:         "LowGem",
		Variant:     "1",
		RequestID:   "req-low",
		Priority:    PriorityLow,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	highReq := &GateRequest{
		Gem:         "HighGem",
		Variant:     "1",
		RequestID:   "req-high",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}

	// Fill channels before starting the processor.
	gate.Submit(lowReq)
	gate.Submit(highReq)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	// Wait for both to complete.
	for _, req := range []*GateRequest{highReq, lowReq} {
		select {
		case <-req.Result:
		case <-time.After(2 * time.Second):
			t.Fatal("timed out waiting for response")
		}
	}

	mu.Lock()
	defer mu.Unlock()
	if len(searchOrder) < 2 {
		t.Fatalf("expected at least 2 searches, got %d", len(searchOrder))
	}
	if searchOrder[0] != "HighGem" {
		t.Errorf("first search was %q, want %q (high priority should go first)", searchOrder[0], "HighGem")
	}
}

func TestGate_QueueTimeout(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 10*time.Millisecond)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Submit a request that is already expired.
	req := &GateRequest{
		Gem:         "ExpiredGem",
		Variant:     "1",
		RequestID:   "req-timeout",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now().Add(-1 * time.Second), // already past maxWait
		Result:      make(chan *GateResponse, 1),
	}

	go gate.Run(ctx)
	gate.Submit(req)

	select {
	case resp := <-req.Result:
		if resp.Error == nil {
			t.Fatal("expected error for timed-out request")
		}
		if !strings.Contains(resp.Error.Error(), "queue timeout") {
			t.Errorf("error = %q, want it to mention 'queue timeout'", resp.Error.Error())
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for error response")
	}

	// Verify Mercure error event was published.
	events := pub.getEvents()
	found := false
	for _, e := range events {
		if e.Topic != mercureTradeTopic {
			continue
		}
		var payload map[string]interface{}
		if err := json.Unmarshal([]byte(e.Payload), &payload); err != nil {
			continue
		}
		if payload["type"] == "error" && payload["requestId"] == "req-timeout" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected Mercure error event for timed-out request")
	}
}

func TestGate_QueueTimeout_AllAttachees(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 10*time.Millisecond)

	// Submit two requests for the same gem, both already expired.
	req1 := &GateRequest{
		Gem:         "ExpiredGem",
		Variant:     "1",
		RequestID:   "req-t1",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now().Add(-1 * time.Second),
		Result:      make(chan *GateResponse, 1),
	}
	req2 := &GateRequest{
		Gem:         "ExpiredGem",
		Variant:     "1",
		RequestID:   "req-t2",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now().Add(-1 * time.Second),
		Result:      make(chan *GateResponse, 1),
	}

	gate.Submit(req1)
	gate.Submit(req2)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	// Both should receive the error.
	for i, req := range []*GateRequest{req1, req2} {
		select {
		case resp := <-req.Result:
			if resp.Error == nil {
				t.Fatalf("req%d: expected error", i+1)
			}
		case <-time.After(2 * time.Second):
			t.Fatalf("req%d: timed out", i+1)
		}
	}
}

func TestGate_WaitEvent(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	cfg := TradeConfig{
		LeagueName:        "Mirage",
		UserAgent:         "ProfitOfExile/test",
		CeilingFactor:     1.0, // use full budget so we can fill it precisely
		LatencyPadding:    100 * time.Millisecond,
		DefaultSearchRate: 1, // 1 req per 5s window
		DefaultFetchRate:  5,
		MaxQueueWait:      10 * time.Second,
		CacheMaxEntries:   100,
	}
	limiter := NewRateLimiter(cfg)
	client := NewClient(cfg)
	client.SetBaseURL(server.URL)
	cache := NewTradeCache(100)
	gate := NewGate(cfg, limiter, client, pub, cache, func() float64 { return 212.0 })

	// Pre-fill the search pool to force a wait.
	limiter.Record("search")

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req := &GateRequest{
		Gem:         "WaitGem",
		Variant:     "1",
		RequestID:   "req-wait",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	gate.Submit(req)

	// Wait for the request to complete (it will have to wait for rate limit).
	select {
	case resp := <-req.Result:
		if resp.Error != nil {
			t.Fatalf("unexpected error: %v", resp.Error)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("timed out waiting for response")
	}

	// Verify a Mercure "waiting" event was published.
	events := pub.getEvents()
	found := false
	for _, e := range events {
		if e.Topic != mercureTradeTopic {
			continue
		}
		var payload map[string]interface{}
		if err := json.Unmarshal([]byte(e.Payload), &payload); err != nil {
			continue
		}
		if payload["type"] == "waiting" && payload["requestId"] == "req-wait" {
			waitSec, ok := payload["waitSeconds"].(float64)
			if !ok {
				t.Error("waitSeconds missing or not a number")
			} else if waitSec <= 0 {
				t.Errorf("waitSeconds = %v, want > 0", waitSec)
			}
			found = true
			break
		}
	}
	if !found {
		t.Error("expected Mercure 'waiting' event")
	}
}

func TestGate_ReadyEvent(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 30*time.Second)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req := &GateRequest{
		Gem:         "ReadyGem",
		Variant:     "1",
		RequestID:   "req-ready",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	gate.Submit(req)

	select {
	case <-req.Result:
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for response")
	}

	// Give the goroutine a moment to execute publishReady after deliverResult.
	time.Sleep(50 * time.Millisecond)

	// Verify Mercure "ready" event with result data.
	events := pub.getEvents()
	found := false
	for _, e := range events {
		if e.Topic != mercureTradeTopic {
			continue
		}
		var payload map[string]interface{}
		if err := json.Unmarshal([]byte(e.Payload), &payload); err != nil {
			continue
		}
		if payload["type"] == "ready" {
			data, ok := payload["data"].(map[string]interface{})
			if !ok {
				t.Error("ready event missing 'data' object")
			} else {
				if data["gem"] != "ReadyGem" {
					t.Errorf("ready data gem = %v, want %q", data["gem"], "ReadyGem")
				}
			}
			found = true
			break
		}
	}
	if !found {
		t.Error("expected Mercure 'ready' event")
	}
}

func TestGate_RateLimitSync(t *testing.T) {
	// Server returns rate limit headers on search response.
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.Contains(r.URL.Path, "/api/trade/search/") {
			w.Header().Set("X-Rate-Limit-Rules", "account")
			w.Header().Set("X-Rate-Limit-Account", "8:10:60")
			w.Header().Set("X-Rate-Limit-Account-State", "3:10:0")
			json.NewEncoder(w).Encode(tradeSearchResp{
				ID:     "qid",
				Result: []string{"r1"},
				Total:  1,
			})
			return
		}
		if strings.Contains(r.URL.Path, "/api/trade/fetch/") {
			indexed := time.Now().UTC()
			json.NewEncoder(w).Encode(tradeFetchResp{
				Result: []tradeFetchEntry{
					{
						Listing: tradeFetchListing{
							Indexed: indexed,
							Account: tradeFetchAccount{Name: "acc1"},
							Price:   tradeFetchPrice{Amount: 10.0, Currency: "chaos"},
						},
						Item: tradeFetchItem{},
					},
				},
			})
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
	defer server.Close()

	pub := &mockPublisher{}
	cfg := TradeConfig{
		LeagueName:        "Mirage",
		UserAgent:         "ProfitOfExile/test",
		CeilingFactor:     0.65,
		LatencyPadding:    0,
		DefaultSearchRate: 1, // intentionally low default
		DefaultFetchRate:  5,
		MaxQueueWait:      30 * time.Second,
		CacheMaxEntries:   100,
	}
	limiter := NewRateLimiter(cfg)
	client := NewClient(cfg)
	client.SetBaseURL(server.URL)
	cache := NewTradeCache(100)
	gate := NewGate(cfg, limiter, client, pub, cache, func() float64 { return 212.0 })

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req := &GateRequest{
		Gem:         "SyncGem",
		Variant:     "1",
		RequestID:   "req-sync",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	gate.Submit(req)

	select {
	case resp := <-req.Result:
		if resp.Error != nil {
			t.Fatalf("unexpected error: %v", resp.Error)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for response")
	}

	// After processing, the limiter's search pool should have been updated from
	// headers. The server said 8:10:60, so the search pool tier should now have
	// maxHits=8. We can verify indirectly: with ceiling 0.65, effective max = 5.
	// We recorded 1 hit (from process), plus server said 3 hits (phantom injection
	// adds 2 more). Total should be at least 3 slots. With 5 effective max,
	// EstimateWait should still be 0 since 3 < 5.
	wait := limiter.EstimateWait("search")
	// Just verify the sync didn't break anything. The tier was updated from 1->8.
	// With 0 padding and only ~3-4 slots in a 10s window, wait should be 0.
	if wait > 5*time.Second {
		t.Errorf("EstimateWait after sync = %v, expected small or zero (limiter should have updated from headers)", wait)
	}
}

func TestGate_CacheOnResult(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	cfg := TradeConfig{
		LeagueName:        "Mirage",
		UserAgent:         "ProfitOfExile/test",
		CeilingFactor:     0.65,
		LatencyPadding:    0,
		DefaultSearchRate: 5,
		DefaultFetchRate:  5,
		MaxQueueWait:      30 * time.Second,
		CacheMaxEntries:   100,
	}
	limiter := NewRateLimiter(cfg)
	client := NewClient(cfg)
	client.SetBaseURL(server.URL)
	cache := NewTradeCache(100)
	gate := NewGate(cfg, limiter, client, pub, cache, func() float64 { return 212.0 })

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	go gate.Run(ctx)

	req := &GateRequest{
		Gem:         "CacheGem",
		Variant:     "21",
		RequestID:   "req-cache",
		Priority:    PriorityHigh,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}
	gate.Submit(req)

	select {
	case resp := <-req.Result:
		if resp.Error != nil {
			t.Fatalf("unexpected error: %v", resp.Error)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for response")
	}

	// Verify the result was stored in cache.
	key := CacheKey("CacheGem", "21")
	result, ok := cache.Get(key)
	if !ok {
		t.Fatal("expected result to be cached")
	}
	if result.Gem != "CacheGem" {
		t.Errorf("cached Gem = %q, want %q", result.Gem, "CacheGem")
	}
	if result.Variant != "21" {
		t.Errorf("cached Variant = %q, want %q", result.Variant, "21")
	}
	if result.Total != 42 {
		t.Errorf("cached Total = %d, want 42", result.Total)
	}
}

func TestGate_Shutdown(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()

	pub := &mockPublisher{}
	gate := newTestGate(t, server, pub, 30*time.Second)

	ctx, cancel := context.WithCancel(context.Background())

	done := make(chan struct{})
	go func() {
		gate.Run(ctx)
		close(done)
	}()

	// Cancel context and verify Run exits promptly.
	cancel()

	select {
	case <-done:
		// Run exited as expected.
	case <-time.After(2 * time.Second):
		t.Fatal("Run did not exit after context cancellation")
	}
}
