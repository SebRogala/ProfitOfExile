package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"regexp"
	"time"

	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// tradeLookupPersister is the persistence seam TradeSubmit writes through.
// *trade.Repository is the production implementation; the interface exists so
// the bounded writer below can be exercised without a database.
type tradeLookupPersister interface {
	InsertTradeLookup(ctx context.Context, scope league.Scope, result *trade.TradeLookupResult, source string) error
}

// submitWriterConfig bounds the asynchronous persist path behind
// POST /api/trade/submit.
//
// Before this bound, the handler fired one detached goroutine per request and
// answered 204 immediately, so the HTTP layer had no way to shed load — it had
// already replied. Submits arrive one per desktop trade lookup and up to three
// per compare, so the write rate scales with the user count.
//
// The trade against a full queue is deliberate and is the point of the design:
//
//   - Not a silent drop. These rows feed the shared trade cache and the
//     trade_lookups history other users read; losing one with no signal anywhere
//     is the outcome worth avoiding.
//   - Brief block first, then 503. A saturated queue holds the request for
//     enqueueWait before giving up, which is real backpressure — the caller's
//     connection stays occupied, so load propagates back instead of piling up in
//     server memory. Only when that window expires does the request shed with a
//     503 + Retry-After, which is a signal the client and the logs both see.
//   - The 503 is honest but partial: the shared LRU cache was already updated
//     synchronously before enqueueing, so a shed submit still enriches other
//     users' responses. What is refused is the durable history row.
//
// # SIZES, AND WHAT THE WORKER COUNT IS ACTUALLY BOUNDED BY
//
// Not throughput. At the measured 0.93 ms per INSERT one worker is ~1,075
// inserts/s, already three orders of magnitude above the expected rate, so every
// worker past the first buys nothing a queue of 256 does not already absorb.
//
// The bound is the connection pool. These goroutines hold a pool connection for
// the length of an insert, and they are the only work in the process whose
// latency nobody is waiting on — the caller already has its 204. So they take
// the smaller share: at most half of what the process-lifetime fence leaves of
// db.DefaultMaxConns, which is 2 at the current default of 6. Asserted in
// trade_submit_pool_share_test.go, because this number and that one landed on
// the same day in separate commits and drifted — the comment here claimed "4 of
// the pool's 50" for four months after the pool became 6.
//
// The second worker is not throughput either, it is liveness: insertTimeout is
// 5 s, and with a single worker one stuck insert stops the queue draining for
// that whole window. Two means a stall still leaves a drain path.
//
// This is a share of the DEFAULT pool. POE_DB_MAX_CONNS can set the real pool
// lower — 3 is the floor cmd/server/main.go enforces — and at that floor these
// two workers plus the fence can hold everything for the ~1 ms of an insert.
// That is added latency, not a wedge: a worker releases after each row rather
// than holding across a wait.
//
// What does NOT hold, and used to be claimed here: that request-path queries
// "can never be starved by writes". Nothing reserves connections for requests.
// Writes contend like everything else and the loser queues in Go, which is the
// failure mode internal/db deliberately chose over queueing in pgbouncer.
//
// A 256-slot queue absorbs a full simultaneous burst from ~50 users compared
// three-at-a-time without ever reaching the shed path.
type submitWriterConfig struct {
	workers       int
	queue         int
	enqueueWait   time.Duration
	insertTimeout time.Duration
}

var defaultSubmitWriterConfig = submitWriterConfig{
	workers:       2,
	queue:         256,
	enqueueWait:   250 * time.Millisecond,
	insertTimeout: 5 * time.Second,
}

// submitWriter persists trade lookups on a fixed pool of workers fed by a
// bounded queue.
type submitWriter struct {
	jobs chan *trade.TradeLookupResult
	cfg  submitWriterConfig
}

// newSubmitWriter starts cfg.workers goroutines. They run for the life of the
// process, which is the same lifetime the detached goroutines they replace had;
// the difference is that there are now a fixed number of them. In-flight queued
// rows are still lost on shutdown, exactly as before.
func newSubmitWriter(p tradeLookupPersister, scope league.Scope, cfg submitWriterConfig) *submitWriter {
	sw := &submitWriter{
		jobs: make(chan *trade.TradeLookupResult, cfg.queue),
		cfg:  cfg,
	}
	for i := 0; i < cfg.workers; i++ {
		go sw.run(p, scope)
	}
	return sw
}

// run drains the queue. Each insert gets its own timeout context derived from
// Background, not from the request: the write deliberately outlives the 204.
func (sw *submitWriter) run(p tradeLookupPersister, scope league.Scope) {
	for result := range sw.jobs {
		ctx, cancel := context.WithTimeout(context.Background(), sw.cfg.insertTimeout)
		if err := p.InsertTradeLookup(ctx, scope, result, "desktop"); err != nil {
			slog.Warn("trade submit: persist lookup failed", "gem", result.Gem, "variant", result.Variant, "error", err)
		}
		cancel()
	}
}

// enqueue hands result to a worker, reporting false when the queue stayed full
// for the whole enqueueWait window. The caller turns that into a 503 rather than
// dropping the row unseen.
func (sw *submitWriter) enqueue(ctx context.Context, result *trade.TradeLookupResult) bool {
	select {
	case sw.jobs <- result:
		return true
	default:
	}

	// Queue full: hold the request briefly so the pressure is felt upstream.
	timer := time.NewTimer(sw.cfg.enqueueWait)
	defer timer.Stop()
	select {
	case sw.jobs <- result:
		return true
	case <-timer.C:
		return false
	case <-ctx.Done():
		return false
	}
}

// TradeSubmit handles POST /api/trade/submit. It accepts a TradeLookupResult
// from the desktop app (which queries GGG directly), stores it in the shared
// LRU cache so CompareAnalysis can enrich responses, and optionally persists
// to the database for historical tracking.
func TradeSubmit(cache *trade.TradeCache, repo *trade.Repository, scope league.Scope, dictionary *lab.Cache, checker tradeGemNameChecker) http.HandlerFunc {
	// The nil check has to happen on the concrete type: a typed-nil
	// *trade.Repository stored in an interface produces a non-nil interface.
	var persister tradeLookupPersister
	if repo != nil {
		persister = repo
	}
	return tradeSubmit(cache, persister, scope, defaultSubmitWriterConfig, tradeSubmitOptions{dictionary: dictionary, checker: checker})
}

// contributionMaxAge matches HandleTradeTick's current default stale window.
// A contributed timestamp already eligible for refresh is reset to receipt
// time rather than
// letting a client choose the oldest key; recent genuine timestamps are retained.
const contributionMaxAge = 5 * time.Minute

type tradeSubmitOptions struct {
	dictionary *lab.Cache
	checker    tradeGemNameChecker
	now        func() time.Time
}

// tradeSubmit keeps the bounded writer lifecycle separate from HTTP validation.
// Missing validation options use the same cold-dictionary startup policy.
func tradeSubmit(cache *trade.TradeCache, persister tradeLookupPersister, scope league.Scope, cfg submitWriterConfig, options ...tradeSubmitOptions) http.HandlerFunc {
	var enqueue func(context.Context, *trade.TradeLookupResult) bool
	if persister != nil {
		writer := newSubmitWriter(persister, scope, cfg)
		enqueue = func(ctx context.Context, result *trade.TradeLookupResult) bool {
			accepted := writer.enqueue(ctx, result)
			if !accepted {
				slog.Warn("trade submit: persist queue saturated, shedding load",
					"gem", result.Gem, "variant", result.Variant,
					"queue", cfg.queue, "workers", cfg.workers)
			}
			return accepted
		}
	}
	var opts tradeSubmitOptions
	if len(options) > 0 {
		opts = options[0]
	}
	return tradeSubmitHandler(cache, scope, enqueue, opts)
}

func tradeSubmitHandler(cache *trade.TradeCache, scope league.Scope, enqueue func(context.Context, *trade.TradeLookupResult) bool, opts tradeSubmitOptions) http.HandlerFunc {
	if opts.now == nil {
		opts.now = time.Now
	}
	return func(w http.ResponseWriter, r *http.Request) {
		receivedAt := opts.now()
		r.Body = http.MaxBytesReader(w, r.Body, 65536)

		var result trade.TradeLookupResult
		if err := json.NewDecoder(r.Body).Decode(&result); err != nil {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON body"})
			return
		}

		if !validateTradeInput(w, r, scope, opts.dictionary, opts.checker, result.Gem, result.Variant) {
			return
		}
		if result.FetchedAt.After(receivedAt) || !result.FetchedAt.After(receivedAt.Add(-contributionMaxAge)) {
			result.FetchedAt = receivedAt
		}

		// Cap listings to max 20 and validate price floor.
		if len(result.Listings) > 20 {
			result.Listings = result.Listings[:20]
		}
		if result.PriceFloor < 0 {
			result.PriceFloor = 0
		}

		key := trade.CacheKey(result.Gem, result.Variant)

		if cache != nil {
			cache.For(scope).Set(key, &result)
		}

		// Queued after the cache Set, so a shed submit still enriches other
		// users' responses — only the durable history row is refused.
		if enqueue != nil && !enqueue(r.Context(), &result) {
			w.Header().Set("Content-Type", "application/json")
			w.Header().Set("Retry-After", "1")
			w.WriteHeader(http.StatusServiceUnavailable)
			json.NewEncoder(w).Encode(map[string]string{"error": "trade persistence saturated, retry"})
			return
		}

		w.WriteHeader(http.StatusNoContent)
	}
}

// tradeSubmitter is the lookup boundary; rejected HTTP inputs never reach it.
type tradeSubmitter interface {
	Submit(*trade.GateRequest)
}

type tradeGemNameChecker func(context.Context, league.Scope, string) (bool, error)

// The server query builder accepts decimal level[/quality], without the c
// suffix used by corrupted snapshot markets. Retain level-only legacy aliases.
var tradeVariantPattern = regexp.MustCompile(`^[1-9][0-9]?(?:/(?:0|[1-9][0-9]?))?$`)

// KnownTradeGem uses the OCR dictionary as a positive cache only: it excludes
// support and Heist-only gems. Raw membership supplies those names on a miss.
// Cold/empty caches or an absent checker permit lookup so startup does not
// disable trading. Query errors are returned, so cancellation cannot bypass
// validation once the dictionary is warm.
func KnownTradeGem(ctx context.Context, dictionary *lab.Cache, checker tradeGemNameChecker, scope league.Scope, name string) (bool, error) {
	if dictionary == nil {
		return true, nil
	}
	c := dictionary.For(scope)
	skills, skillsWarm := c.GemDictionary(false)
	transfigured, transfiguredWarm := c.GemDictionary(true)
	if !skillsWarm || !transfiguredWarm || len(skills)+len(transfigured) == 0 {
		return true, nil
	}
	for _, names := range [][]string{skills, transfigured} {
		for _, known := range names {
			if name == known {
				return true, nil
			}
		}
	}
	if checker == nil {
		return true, nil
	}
	return checker(ctx, scope, name)
}

// validateTradeInput is shared by lookup and contribution: neither route may
// let rejected names/variants reach the gate, shared cache or persisted history.
func validateTradeInput(w http.ResponseWriter, r *http.Request, scope league.Scope, dictionary *lab.Cache, checker tradeGemNameChecker, gem, variant string) bool {
	if gem == "" || variant == "" {
		jsonError(w, http.StatusBadRequest, "gem and variant are required")
		return false
	}
	if len(gem) > 200 || len(variant) > 20 {
		jsonError(w, http.StatusBadRequest, "gem or variant too long")
		return false
	}
	if !tradeVariantPattern.MatchString(variant) {
		jsonError(w, http.StatusBadRequest, "invalid variant: expected level or level/quality")
		return false
	}
	known, err := KnownTradeGem(r.Context(), dictionary, checker, scope, gem)
	if err != nil {
		slog.Warn("trade: gem dictionary query failed", "error", err)
		jsonError(w, http.StatusServiceUnavailable, "gem dictionary unavailable")
		return false
	}
	if !known {
		jsonError(w, http.StatusBadRequest, "unknown gem name")
		return false
	}
	return true
}

// tradeLookupRequest is the expected JSON body for POST /api/trade/lookup.
type tradeLookupRequest struct {
	Gem       string `json:"gem"`
	Variant   string `json:"variant"`
	Force     bool   `json:"force"`
	CacheOnly bool   `json:"cacheOnly"`
}

// TradeLookup validates POST /api/trade/lookup before checking the LRU cache
// (unless force=true), then submits a GateRequest and either returns the result
// synchronously (if it arrives within syncTimeout) or responds with 202 and a
// requestId for the client to await via Mercure.
func TradeLookup(gate tradeSubmitter, cache *trade.TradeCache, scope league.Scope, syncTimeout time.Duration, dictionary *lab.Cache, checker tradeGemNameChecker) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var body tradeLookupRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON body"})
			return
		}

		if !validateTradeInput(w, r, scope, dictionary, checker, body.Gem, body.Variant) {
			return
		}

		// Cache-first path (skip when force-refreshing).
		if !body.Force && cache != nil {
			if result, ok := cache.For(scope).Get(trade.CacheKey(body.Gem, body.Variant)); ok {
				w.Header().Set("Content-Type", "application/json")
				if err := json.NewEncoder(w).Encode(result); err != nil {
					slog.Error("trade lookup: encode cached response", "error", err)
				}
				return
			}
		}

		// CacheOnly mode: return 204 on cache miss — no GGG API request.
		if body.CacheOnly {
			w.WriteHeader(http.StatusNoContent)
			return
		}

		requestID := middleware.GetReqID(r.Context())

		req := &trade.GateRequest{
			Gem:         body.Gem,
			Variant:     body.Variant,
			RequestID:   requestID,
			Priority:    trade.PriorityHigh,
			SubmittedAt: time.Now(),
			Result:      make(chan *trade.GateResponse, 1),
		}

		gate.Submit(req)

		// Wait up to syncTimeout for a fast-path response.
		select {
		case res := <-req.Result:
			if res.Error != nil {
				slog.Warn("trade lookup: gate error", "error", res.Error, "gem", body.Gem, "variant", body.Variant)
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadGateway)
				json.NewEncoder(w).Encode(map[string]string{"error": "trade lookup failed"})
				return
			}
			w.Header().Set("Content-Type", "application/json")
			if err := json.NewEncoder(w).Encode(res.Data); err != nil {
				slog.Error("trade lookup: encode response", "error", err)
			}
		case <-time.After(syncTimeout):
			// Result not yet available — tell the client to listen on Mercure.
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusAccepted)
			json.NewEncoder(w).Encode(map[string]string{"requestId": requestID})
		}
	}
}
