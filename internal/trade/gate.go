package trade

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"profitofexile/internal/mercure"
)

const mercureTradeTopic = "poe/trade/results"

// Gate is the core orchestration component for trade API lookups. It receives
// GateRequests on two priority channels (high for interactive, low for
// background), deduplicates in-flight lookups for the same gem+variant, enforces
// rate limits, and publishes Mercure events for wait/ready/error state changes.
type Gate struct {
	high     chan *GateRequest // buffered: 10 — interactive lookups
	low      chan *GateRequest // buffered: 50 — background scans
	limiter  *RateLimiter
	client   *Client
	mercure  mercure.Publisher
	cache    *TradeCache
	inflight map[string][]*GateRequest
	mu       sync.Mutex
	maxWait  time.Duration
}

// NewGate creates a Gate wired to the given dependencies.
func NewGate(cfg TradeConfig, limiter *RateLimiter, client *Client, pub mercure.Publisher, cache *TradeCache) *Gate {
	return &Gate{
		high:     make(chan *GateRequest, 10),
		low:      make(chan *GateRequest, 50),
		limiter:  limiter,
		client:   client,
		mercure:  pub,
		cache:    cache,
		inflight: make(map[string][]*GateRequest),
		maxWait:  cfg.MaxQueueWait,
	}
}

// Submit enqueues a request into the gate. If an identical gem+variant lookup
// is already in flight, the request is attached to the existing fan-out list
// so only one API call is made and all waiters receive the result.
func (g *Gate) Submit(req *GateRequest) {
	key := CacheKey(req.Gem, req.Variant)

	g.mu.Lock()
	if existing, ok := g.inflight[key]; ok {
		// Dedup: attach to existing in-flight lookup.
		g.inflight[key] = append(existing, req)
		g.mu.Unlock()
		return
	}
	// First request for this key — register and dispatch.
	g.inflight[key] = []*GateRequest{req}
	g.mu.Unlock()

	switch req.Priority {
	case PriorityHigh:
		g.high <- req
	default:
		g.low <- req
	}
}

// HighChan returns the high-priority request channel. Intended for testing
// only — allows test code to intercept submitted requests before Run processes
// them.
func (g *Gate) HighChan() <-chan *GateRequest { return g.high }

// Run is the main processing loop. It blocks until ctx is cancelled. High
// priority requests are always drained before low priority ones via nested
// select with a default fallback.
func (g *Gate) Run(ctx context.Context) {
	for {
		select {
		case <-ctx.Done():
			return
		case req := <-g.high:
			g.process(ctx, req)
		default:
			select {
			case <-ctx.Done():
				return
			case req := <-g.high:
				g.process(ctx, req)
			case req := <-g.low:
				g.process(ctx, req)
			}
		}
	}
}

// process executes the two-phase search+fetch for a single request, respecting
// rate limits, publishing Mercure events, and delivering results to all
// fan-out waiters.
func (g *Gate) process(ctx context.Context, req *GateRequest) {
	key := CacheKey(req.Gem, req.Variant)

	// Check queue age — reject if the request has been waiting too long.
	if time.Since(req.SubmittedAt) > g.maxWait {
		err := fmt.Errorf("trade: queue timeout after %s", g.maxWait)
		g.deliverError(key, err)
		g.publishError(ctx, req, err.Error())
		return
	}

	// Estimate wait across both pools, take the worst case.
	searchWait := g.limiter.EstimateWait("search")
	fetchWait := g.limiter.EstimateWait("fetch")
	totalWait := searchWait
	if fetchWait > totalWait {
		totalWait = fetchWait
	}

	// If we need to wait, publish a Mercure event and sleep.
	if totalWait > 0 {
		g.publishWait(ctx, req, totalWait)
		if !g.sleepWithContext(ctx, totalWait) {
			return // context cancelled during sleep
		}
	}

	// Re-check search pool before firing (wait may have been approximate).
	if w := g.limiter.EstimateWait("search"); w > 0 {
		if !g.sleepWithContext(ctx, w) {
			return
		}
	}

	// Phase 1: Search.
	searchResp, searchHeaders, err := g.client.Search(ctx, req.Gem)
	if err != nil {
		g.deliverError(key, fmt.Errorf("trade search: %w", err))
		g.publishError(ctx, req, err.Error())
		return
	}
	g.limiter.SyncFromHeaders("search", searchHeaders)
	g.limiter.Record("search")

	// Check fetch pool wait before phase 2.
	if w := g.limiter.EstimateWait("fetch"); w > 0 {
		if !g.sleepWithContext(ctx, w) {
			return
		}
	}

	// Phase 2: Fetch listing details.
	listings, fetchHeaders, err := g.client.Fetch(ctx, searchResp.QueryID, searchResp.IDs)
	if err != nil {
		g.deliverError(key, fmt.Errorf("trade fetch: %w", err))
		g.publishError(ctx, req, err.Error())
		return
	}
	g.limiter.SyncFromHeaders("fetch", fetchHeaders)
	g.limiter.Record("fetch")

	// Build result, cache it, deliver to all fan-out waiters.
	result := BuildResult(req.Gem, req.Variant, *searchResp, listings)
	g.cache.Set(key, result)
	g.deliverResult(key, result)
	g.publishReady(ctx, req, result)
}

// deliverResult sends the successful result to all in-flight waiters for the
// given key and removes the key from the inflight map.
func (g *Gate) deliverResult(key string, result *TradeLookupResult) {
	g.mu.Lock()
	waiters := g.inflight[key]
	delete(g.inflight, key)
	g.mu.Unlock()

	resp := &GateResponse{Data: result}
	for _, w := range waiters {
		select {
		case w.Result <- resp:
		default:
			// Channel full or nobody listening — skip.
		}
	}
}

// deliverError sends the error to all in-flight waiters for the given key and
// removes the key from the inflight map.
func (g *Gate) deliverError(key string, err error) {
	g.mu.Lock()
	waiters := g.inflight[key]
	delete(g.inflight, key)
	g.mu.Unlock()

	resp := &GateResponse{Error: err}
	for _, w := range waiters {
		select {
		case w.Result <- resp:
		default:
		}
	}
}

// sleepWithContext pauses for the given duration but returns immediately (false)
// if the context is cancelled.
func (g *Gate) sleepWithContext(ctx context.Context, d time.Duration) bool {
	timer := time.NewTimer(d)
	defer timer.Stop()

	select {
	case <-timer.C:
		return true
	case <-ctx.Done():
		return false
	}
}

// --- Mercure event helpers ---
// All events are published to the single topic "poe/trade/results".

func (g *Gate) publishWait(ctx context.Context, req *GateRequest, wait time.Duration) {
	payload, _ := json.Marshal(map[string]interface{}{
		"type":        "waiting",
		"requestId":   req.RequestID,
		"gem":         req.Gem,
		"waitSeconds": int(wait.Seconds() + 0.5), // round to nearest second
	})
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish wait event", "error", err)
	}
}

func (g *Gate) publishReady(ctx context.Context, req *GateRequest, result *TradeLookupResult) {
	payload, _ := json.Marshal(map[string]interface{}{
		"type":      "ready",
		"requestId": req.RequestID,
		"data":      result,
	})
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish ready event", "error", err)
	}
}

func (g *Gate) publishError(ctx context.Context, req *GateRequest, message string) {
	payload, _ := json.Marshal(map[string]interface{}{
		"type":      "error",
		"requestId": req.RequestID,
		"gem":       req.Gem,
		"message":   message,
	})
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish error event", "error", err)
	}
}
