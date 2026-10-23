package trade

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/mercure"
)

const mercureTradeTopic = "poe/trade/results"

const (
	failedLookupRetryDelay  = 5 * time.Minute
	invalidLookupRetryDelay = 6 * time.Hour
)

var errNoQueuedWaiters = errors.New("trade: no queued waiters")

// Gate is the core orchestration component for trade API lookups. It receives
// GateRequests on two priority channels (high for interactive, low for
// background), deduplicates in-flight lookups for the same gem+variant, enforces
// rate limits, and publishes Mercure events for wait/ready/error state changes.
// DivineRateFunc returns the current divine→chaos exchange rate.
// Injected at construction so the gate doesn't need direct DB access.
type DivineRateFunc func() float64

// lookupWriter is the gate's persistence boundary; Repository implements it.
type lookupWriter interface {
	InsertTradeLookup(context.Context, league.Scope, *TradeLookupResult, string) error
}

type Gate struct {
	high         chan *GateRequest // buffered: 10 — interactive lookups
	low          chan *GateRequest // buffered: 50 — background scans
	limiter      *RateLimiter
	client       *Client
	mercure      mercure.Publisher
	cache        *TradeCache
	repo         lookupWriter
	scope        league.Scope
	inflight     map[string][]*GateRequest
	started      map[string]bool
	queueStops   map[*GateRequest]func() bool
	afterFunc    func(time.Duration, func()) func() bool
	mu           sync.Mutex
	maxWait      time.Duration
	divineRate   DivineRateFunc
	now          func() time.Time
	sleep        func(context.Context, time.Duration) bool
	persistAsync func(func())
}

// NewGate creates a Gate wired to the given dependencies. The scope is fixed at
// construction: the gate's background persistence writes every lookup under it.
func NewGate(cfg TradeConfig, scope league.Scope, limiter *RateLimiter, client *Client, pub mercure.Publisher, cache *TradeCache, divRate DivineRateFunc, repo *Repository) *Gate {
	g := &Gate{
		high:       make(chan *GateRequest, 10),
		low:        make(chan *GateRequest, 50),
		limiter:    limiter,
		client:     client,
		mercure:    pub,
		cache:      cache,
		scope:      scope,
		inflight:   make(map[string][]*GateRequest),
		started:    make(map[string]bool),
		queueStops: make(map[*GateRequest]func() bool),
		afterFunc: func(d time.Duration, f func()) func() bool {
			return time.AfterFunc(d, f).Stop
		},
		maxWait:      cfg.MaxQueueWait,
		divineRate:   divRate,
		now:          time.Now,
		persistAsync: func(f func()) { go f() },
	}
	if repo != nil {
		g.repo = repo
	}
	g.sleep = g.sleepWithContext
	return g
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
		if !g.started[key] {
			g.armQueueTimeoutLocked(key, req)
		}
		g.mu.Unlock()
		return
	}
	// First request for this key — register and dispatch.
	g.inflight[key] = []*GateRequest{req}
	g.armQueueTimeoutLocked(key, req)
	g.mu.Unlock()

	ch := g.low
	if req.Priority == PriorityHigh {
		ch = g.high
	}
	select {
	case ch <- req:
	default:
		// Queue full — reject immediately.
		g.deliverError(key, fmt.Errorf("trade: queue full, try again later"))
	}
}

// HighChan returns the high-priority request channel. Intended for testing
// only — allows test code to intercept submitted requests before Run processes
// them.
func (g *Gate) HighChan() <-chan *GateRequest { return g.high }

// LowChan returns the low-priority request channel. Intended for testing only
// — symmetric counterpart to HighChan, used to assert that background-priority
// submissions (e.g. tick-driven refreshes) reached the queue.
func (g *Gate) LowChan() <-chan *GateRequest { return g.low }

// Run is the main processing loop. It blocks until ctx is cancelled. High
// priority requests are always drained before low priority ones via nested
// select with a default fallback.
func (g *Gate) Run(ctx context.Context) {
	defer func() {
		g.mu.Lock()
		keys := make([]string, 0, len(g.inflight))
		for key := range g.inflight {
			keys = append(keys, key)
		}
		g.mu.Unlock()
		for _, key := range keys {
			g.deliverError(key, ctx.Err())
		}
	}()
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

	// Search need not wait for fetch capacity: an empty search uses no fetch.
	// The queue deadline includes limiter waiting, up to the first send.
	if err := g.waitForPool(ctx, req, "search", true); err != nil {
		if errors.Is(err, errNoQueuedWaiters) {
			return
		}
		g.failLookup(ctx, key, err)
		return
	}

	// Phase 1: Search.
	slog.Info("trade gate: searching", "gem", req.Gem, "requestId", req.RequestID)
	g.limiter.Record("search")
	searchResp, searchHeaders, err := g.client.Search(ctx, req.Gem, req.Variant)
	g.limiter.ObserveResponse("search", searchHeaders, err)
	if err != nil {
		slog.Warn("trade gate: search failed", "gem", req.Gem, "error", err)
		g.failUpstreamLookup(ctx, key, fmt.Errorf("trade search: %w", err))
		return
	}
	slog.Info("trade gate: search ok", "gem", req.Gem, "total", searchResp.Total, "ids", len(searchResp.IDs))

	// No listings: skip the fetch but still cache a fresh, empty result.
	// Erroring here left the old cache entry's FetchedAt untouched, so the
	// background refresh (OldestStale) re-picked the same key every tick.
	if len(searchResp.IDs) == 0 {
		slog.Info("trade gate: no listings found, skipping fetch", "gem", req.Gem)
		result := BuildResult(req.Gem, req.Variant, g.client.leagueName, *searchResp, []TradeListingDetail{}, g.divineRate())
		g.completeLookup(ctx, req, key, result)
		return
	}

	if err := g.waitForPool(ctx, req, "fetch", false); err != nil {
		g.failLookup(ctx, key, err)
		return
	}

	// Phase 2: Fetch listing details.
	g.limiter.Record("fetch")
	listings, fetchHeaders, err := g.client.Fetch(ctx, searchResp.QueryID, searchResp.IDs)
	g.limiter.ObserveResponse("fetch", fetchHeaders, err)
	if err != nil {
		slog.Warn("trade gate: fetch failed", "gem", req.Gem, "error", err)
		g.failUpstreamLookup(ctx, key, fmt.Errorf("trade fetch: %w", err))
		return
	}

	result := BuildResult(req.Gem, req.Variant, g.client.leagueName, *searchResp, listings, g.divineRate())
	g.completeLookup(ctx, req, key, result)
}

// waitForPool rechecks only the endpoint about to send. Bounded waits let
// header updates reopen capacity early and make cancellation/deadlines observable.
// Once search has started, MaxQueueWait no longer limits the running lookup.
func (g *Gate) waitForPool(ctx context.Context, req *GateRequest, pool string, queued bool) error {
	announced := false
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		remaining := time.Duration(0)
		if queued {
			var ok bool
			remaining, ok = g.queueRemaining(ctx, req)
			if !ok {
				return errNoQueuedWaiters
			}
		}
		wait := g.limiter.EstimateWait(pool)
		if wait <= 0 {
			if queued {
				// Fence timer callbacks against the transition to running.
				g.mu.Lock()
				key := CacheKey(req.Gem, req.Variant)
				if len(g.inflight[key]) == 0 {
					delete(g.inflight, key)
					g.mu.Unlock()
					return errNoQueuedWaiters
				}
				expired := false
				for _, waiter := range g.inflight[key] {
					if !g.now().Before(waiter.SubmittedAt.Add(g.maxWait)) {
						expired = true
						break
					}
				}
				if expired {
					g.mu.Unlock()
					continue
				}
				g.started[key] = true
				g.stopQueueTimersLocked(key)
				g.mu.Unlock()
			}
			return nil
		}
		if !announced {
			for _, waiter := range g.waitersSnapshot(CacheKey(req.Gem, req.Variant)) {
				g.publishWait(ctx, waiter, wait)
			}
			announced = true
		}
		if wait > 250*time.Millisecond {
			wait = 250 * time.Millisecond
		}
		if queued && wait > remaining {
			wait = remaining
		}
		if !g.sleep(ctx, wait) {
			return ctx.Err()
		}
	}
}

// armQueueTimeoutLocked gives each queued caller its own deadline, even while
// another lookup is in HTTP or waiting through a long fetch restriction.
// An empty flight remains reserved until its queue item is consumed, so a
// timer cannot leave an old item that later deletes a replacement flight.
func (g *Gate) armQueueTimeoutLocked(key string, req *GateRequest) {
	g.queueStops[req] = g.afterFunc(req.SubmittedAt.Add(g.maxWait).Sub(g.now()), func() {
		g.expireQueuedWaiter(context.Background(), key, req)
	})
}

func (g *Gate) expireQueuedWaiter(ctx context.Context, key string, req *GateRequest) {
	g.mu.Lock()
	stop, queued := g.queueStops[req]
	if !queued {
		g.mu.Unlock()
		return
	}
	stop()
	delete(g.queueStops, req)
	// A queue timeout overlapping a global pause must not immediately feed
	// the same key back into the picker. Retain expired restriction evidence
	// too: timer callbacks or dequeue processing can run after the pause ends.
	if until := g.limiter.restrictionDeadlineAfter(req.SubmittedAt); !until.IsZero() {
		retryAfter := g.now().Add(failedLookupRetryDelay)
		if afterPause := until.Add(failedLookupRetryDelay); afterPause.After(retryAfter) {
			retryAfter = afterPause
		}
		g.cache.For(g.scope).RecordFailure(key, retryAfter)
	}
	waiters := g.inflight[key]
	for i, waiter := range waiters {
		if waiter == req {
			g.inflight[key] = append(waiters[:i], waiters[i+1:]...)
			break
		}
	}
	g.mu.Unlock()
	err := fmt.Errorf("trade: queue timeout after %s", g.maxWait)
	select {
	case req.Result <- &GateResponse{Error: err}:
	default:
	}
	g.publishError(ctx, req, err.Error())
}

// queueRemaining also expires by the injected clock, keeping deadline checks
// deterministic and preventing a delayed timer callback from permitting a send.
func (g *Gate) queueRemaining(ctx context.Context, req *GateRequest) (time.Duration, bool) {
	key := CacheKey(req.Gem, req.Variant)
	for {
		g.mu.Lock()
		waiters := g.inflight[key]
		if len(waiters) == 0 {
			delete(g.inflight, key)
			delete(g.started, key)
			g.mu.Unlock()
			return 0, false
		}
		now := g.now()
		remaining := waiters[0].SubmittedAt.Add(g.maxWait).Sub(now)
		expired := (*GateRequest)(nil)
		for _, waiter := range waiters {
			d := waiter.SubmittedAt.Add(g.maxWait).Sub(now)
			if d <= 0 {
				expired = waiter
				break
			}
			if d < remaining {
				remaining = d
			}
		}
		g.mu.Unlock()
		if expired == nil {
			return remaining, true
		}
		g.expireQueuedWaiter(ctx, key, expired)
	}
}

func (g *Gate) stopQueueTimersLocked(key string) {
	for _, waiter := range g.inflight[key] {
		if stop, ok := g.queueStops[waiter]; ok {
			stop()
			delete(g.queueStops, waiter)
		}
	}
}

// completeLookup caches the result, persists it to the DB in the background,
// delivers it to all fan-out waiters, and publishes the ready event.
func (g *Gate) completeLookup(ctx context.Context, req *GateRequest, key string, result *TradeLookupResult) {
	result.FetchedAt = g.now()
	g.cache.For(g.scope).Set(key, result)
	if g.repo != nil {
		g.persistAsync(func() {
			if err := g.repo.InsertTradeLookup(context.Background(), g.scope, result, "server-gate"); err != nil {
				slog.Warn("trade gate: persist lookup failed", "gem", req.Gem, "error", err)
			}
		})
	}
	for _, waiter := range g.deliverResult(key, result) {
		g.publishReady(ctx, waiter, result)
	}
	slog.Info("trade gate: lookup complete", "gem", req.Gem, "total", result.Total, "priceFloor", result.PriceFloor, "listings", len(result.Listings))
}

// deliverResult sends the successful result to all in-flight waiters for the
// given key and removes the key from the inflight map.
func (g *Gate) deliverResult(key string, result *TradeLookupResult) []*GateRequest {
	g.mu.Lock()
	waiters := g.inflight[key]
	g.stopQueueTimersLocked(key)
	delete(g.started, key)
	delete(g.inflight, key)
	g.mu.Unlock()

	resp := &GateResponse{Data: result}
	for _, w := range waiters {
		select {
		case w.Result <- resp:
		default:
			slog.Warn("trade gate: result delivery skipped (channel full/closed)", "key", key, "requestId", w.RequestID)
		}
	}
	return waiters
}

// deliverError sends the error to all in-flight waiters for the given key and
// removes the key from the inflight map.
func (g *Gate) deliverError(key string, err error) []*GateRequest {
	g.mu.Lock()
	waiters := g.inflight[key]
	g.stopQueueTimersLocked(key)
	delete(g.started, key)
	delete(g.inflight, key)
	g.mu.Unlock()

	resp := &GateResponse{Error: err}
	for _, w := range waiters {
		select {
		case w.Result <- resp:
		default:
			slog.Warn("trade gate: error delivery skipped (channel full/closed)", "key", key, "requestId", w.RequestID)
		}
	}
	return waiters
}

func (g *Gate) waitersSnapshot(key string) []*GateRequest {
	g.mu.Lock()
	defer g.mu.Unlock()
	return append([]*GateRequest(nil), g.inflight[key]...)
}

// Failed upstream calls retain the previous successful quote and postpone only
// stale refresh selection. Request-scoped 4xx rejections get six hours: retrying
// an unchanged query every five minutes can repeat the same rejection. This is
// local policy, not an upstream limit; an explicit successful lookup clears it.
// Global restriction statuses and transient failures retain the five-minute delay.
func (g *Gate) failUpstreamLookup(ctx context.Context, key string, err error) {
	if ctx.Err() == nil {
		delay := failedLookupRetryDelay
		if isInvalidRequestError(err) && !isGlobalRestrictionError(err) {
			delay = invalidLookupRetryDelay
		}
		g.cache.For(g.scope).RecordFailure(key, g.now().Add(delay))
	}
	g.failLookup(ctx, key, err)
}

func (g *Gate) failLookup(ctx context.Context, key string, err error) {
	for _, waiter := range g.deliverError(key, err) {
		g.publishError(ctx, waiter, err.Error())
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
	if g.mercure == nil {
		return
	}
	payload, err := json.Marshal(map[string]interface{}{
		"type":        "waiting",
		"requestId":   req.RequestID,
		"gem":         req.Gem,
		"waitSeconds": int(wait.Seconds() + 0.5),
	})
	if err != nil {
		slog.Error("trade gate: marshal wait event", "error", err, "gem", req.Gem)
		return
	}
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish wait event", "error", err)
	}
}

func (g *Gate) publishReady(ctx context.Context, req *GateRequest, result *TradeLookupResult) {
	if g.mercure == nil {
		return
	}
	payload, err := json.Marshal(map[string]interface{}{
		"type":      "ready",
		"requestId": req.RequestID,
		"data":      result,
	})
	if err != nil {
		slog.Error("trade gate: marshal ready event", "error", err, "gem", req.Gem)
		return
	}
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish ready event", "error", err)
	}
}

func (g *Gate) publishError(ctx context.Context, req *GateRequest, message string) {
	if g.mercure == nil {
		return
	}
	payload, err := json.Marshal(map[string]interface{}{
		"type":      "error",
		"requestId": req.RequestID,
		"gem":       req.Gem,
		"message":   message,
	})
	if err != nil {
		slog.Error("trade gate: marshal error event", "error", err, "gem", req.Gem)
		return
	}
	if err := g.mercure.Publish(ctx, mercureTradeTopic, string(payload)); err != nil {
		slog.Warn("trade gate: publish error event", "error", err)
	}
}
