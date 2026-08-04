package handlers

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

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// persistCall records the arguments one InsertTradeLookup call received.
type persistCall struct {
	scope  league.Scope
	result *trade.TradeLookupResult
	source string
}

// blockingPersister stands in for *trade.Repository. Every insert parks until
// release is closed, which lets a test hold all workers busy and observe how
// many of them can be in flight at once.
type blockingPersister struct {
	release chan struct{}
	entered chan struct{}

	inFlight  atomic.Int64
	maxInFlgt atomic.Int64
	persisted atomic.Int64

	mu    sync.Mutex
	calls []persistCall
}

func newBlockingPersister() *blockingPersister {
	return &blockingPersister{
		release: make(chan struct{}),
		entered: make(chan struct{}, 1024),
	}
}

func (p *blockingPersister) InsertTradeLookup(ctx context.Context, scope league.Scope, result *trade.TradeLookupResult, source string) error {
	n := p.inFlight.Add(1)
	for {
		peak := p.maxInFlgt.Load()
		if n <= peak || p.maxInFlgt.CompareAndSwap(peak, n) {
			break
		}
	}
	p.mu.Lock()
	p.calls = append(p.calls, persistCall{scope: scope, result: result, source: source})
	p.mu.Unlock()

	select {
	case p.entered <- struct{}{}:
	default:
	}

	<-p.release
	p.inFlight.Add(-1)
	p.persisted.Add(1)
	return nil
}

// awaitEntered blocks until n inserts have started, so a test can be sure the
// workers are occupied before it measures anything.
func (p *blockingPersister) awaitEntered(t *testing.T, n int) {
	t.Helper()
	for i := 0; i < n; i++ {
		select {
		case <-p.entered:
		case <-time.After(3 * time.Second):
			t.Fatalf("only %d of %d inserts started within 3s", i, n)
		}
	}
}

// awaitPersisted blocks until n inserts have finished.
func (p *blockingPersister) awaitPersisted(t *testing.T, n int64) {
	t.Helper()
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		if p.persisted.Load() >= n {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("persisted = %d after 3s, want %d", p.persisted.Load(), n)
}

// boundedSubmitRouter wires the submit route with an injected persister and
// explicit writer bounds.
func boundedSubmitRouter(cache *trade.TradeCache, p tradeLookupPersister, cfg submitWriterConfig) http.Handler {
	r := chi.NewRouter()
	r.Post("/api/trade/submit", tradeSubmit(cache, p, league.Historical("Mirage"), cfg))
	return r
}

func postSubmit(router http.Handler, gem, variant string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(validSubmitBody(gem, variant)))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)
	return w
}

// The bound is the whole point of the change: however many submits arrive at
// once, no more than cfg.workers inserts may be in flight. Restore the previous
// `go func()`-per-request form and the observed peak becomes the request count.
func TestTradeSubmit_ConcurrentSubmitsNeverExceedWorkerBound(t *testing.T) {
	const workers = 3
	const submits = 32

	p := newBlockingPersister()
	cache := trade.NewTradeCache(64, league.Historical("Mirage"))
	router := boundedSubmitRouter(cache, p, submitWriterConfig{
		workers:       workers,
		queue:         submits,
		enqueueWait:   2 * time.Second,
		insertTimeout: 5 * time.Second,
	})

	var wg sync.WaitGroup
	codes := make([]int, submits)
	for i := 0; i < submits; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			codes[i] = postSubmit(router, "Gem "+string(rune('A'+i)), "21/20").Code
		}(i)
	}
	wg.Wait()

	for i, code := range codes {
		if code != http.StatusNoContent {
			t.Fatalf("submit %d: status = %d, want %d (the queue was sized to hold them all)", i, code, http.StatusNoContent)
		}
	}

	// Every worker is now parked inside an insert; nothing else can start.
	p.awaitEntered(t, workers)
	if got := p.inFlight.Load(); got != workers {
		t.Fatalf("in-flight inserts = %d, want exactly %d — the pool is not the bound", got, workers)
	}

	close(p.release)
	p.awaitPersisted(t, submits)

	if peak := p.maxInFlgt.Load(); peak > workers {
		t.Errorf("peak concurrent inserts = %d, want at most %d", peak, workers)
	}
	if got := p.persisted.Load(); got != submits {
		t.Errorf("persisted = %d, want %d — queued submits must not be dropped", got, submits)
	}
}

// A full queue sheds with 503 + Retry-After rather than dropping the row
// silently, and says so in the body. Replace the shed path with a bare 204 and
// the caller loses the only signal that its history row was refused.
func TestTradeSubmit_SaturatedQueueSheds503WithRetryAfter(t *testing.T) {
	p := newBlockingPersister()
	defer close(p.release)

	cache := trade.NewTradeCache(64, league.Historical("Mirage"))
	router := boundedSubmitRouter(cache, p, submitWriterConfig{
		workers:       1,
		queue:         1,
		enqueueWait:   50 * time.Millisecond,
		insertTimeout: 5 * time.Second,
	})

	// Occupy the single worker, then the single queue slot.
	if code := postSubmit(router, "Occupies Worker", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("first submit status = %d, want 204", code)
	}
	p.awaitEntered(t, 1)
	if code := postSubmit(router, "Occupies Queue", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("second submit status = %d, want 204", code)
	}

	w := postSubmit(router, "Shed Submit", "21/20")

	if w.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d once workers and queue are both full", w.Code, http.StatusServiceUnavailable)
	}
	if got := w.Header().Get("Retry-After"); got != "1" {
		t.Errorf("Retry-After = %q, want %q", got, "1")
	}
	var body map[string]string
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode 503 body: %v", err)
	}
	if !strings.Contains(body["error"], "saturated") {
		t.Errorf("error = %q, want it to name the saturation", body["error"])
	}
}

// Shedding refuses the durable row, not the shared cache entry — the cache Set
// runs before the enqueue precisely so other users still get the enrichment.
// Move the Set after the shed path and this fails.
func TestTradeSubmit_ShedSubmitStillReachesSharedCache(t *testing.T) {
	p := newBlockingPersister()
	defer close(p.release)

	cache := trade.NewTradeCache(64, league.Historical("Mirage"))
	router := boundedSubmitRouter(cache, p, submitWriterConfig{
		workers:       1,
		queue:         1,
		enqueueWait:   50 * time.Millisecond,
		insertTimeout: 5 * time.Second,
	})

	if code := postSubmit(router, "Occupies Worker", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("first submit status = %d, want 204", code)
	}
	p.awaitEntered(t, 1)
	if code := postSubmit(router, "Occupies Queue", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("second submit status = %d, want 204", code)
	}

	if code := postSubmit(router, "Shed Submit", "21/20").Code; code != http.StatusServiceUnavailable {
		t.Fatalf("third submit status = %d, want 503", code)
	}

	cached, ok := cache.Get(trade.CacheKey("Shed Submit", "21/20"))
	if !ok {
		t.Fatal("shed submit is missing from the shared cache — the 503 refused more than the history row")
	}
	if cached.PriceFloor != 42.5 {
		t.Errorf("cached PriceFloor = %v, want 42.5", cached.PriceFloor)
	}
}

// A client that has already gone away must not hold a request slot for the whole
// enqueueWait window. With a 30s wait configured, returning at all proves the
// cancelled context is what released it.
func TestTradeSubmit_CancelledRequestShedsImmediatelyWhenQueueIsFull(t *testing.T) {
	p := newBlockingPersister()
	defer close(p.release)

	cache := trade.NewTradeCache(64, league.Historical("Mirage"))
	router := boundedSubmitRouter(cache, p, submitWriterConfig{
		workers:       1,
		queue:         1,
		enqueueWait:   30 * time.Second,
		insertTimeout: 5 * time.Second,
	})

	if code := postSubmit(router, "Occupies Worker", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("first submit status = %d, want 204", code)
	}
	p.awaitEntered(t, 1)
	if code := postSubmit(router, "Occupies Queue", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("second submit status = %d, want 204", code)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit",
		strings.NewReader(validSubmitBody("Abandoned Submit", "21/20"))).WithContext(ctx)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	start := time.Now()
	router.ServeHTTP(w, req)
	elapsed := time.Since(start)

	if w.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusServiceUnavailable)
	}
	if elapsed > 5*time.Second {
		t.Errorf("took %v, want the cancelled context to release it well before the 30s enqueue wait", elapsed)
	}
}

// The persist path itself had no coverage: production always passed a live
// *trade.Repository and every test passed nil. Cross the scope and source
// arguments, or hand the worker a copy of the wrong result, and this fails.
func TestTradeSubmit_PersistsResultWithRequestScopeAndDesktopSource(t *testing.T) {
	p := newBlockingPersister()
	close(p.release) // never block; inserts run straight through

	cache := trade.NewTradeCache(64, league.Historical("Mirage"))
	router := boundedSubmitRouter(cache, p, submitWriterConfig{
		workers:       2,
		queue:         8,
		enqueueWait:   time.Second,
		insertTimeout: 5 * time.Second,
	})

	if code := postSubmit(router, "Vaal Grace of Phasing", "21/20").Code; code != http.StatusNoContent {
		t.Fatalf("status = %d, want 204", code)
	}
	p.awaitPersisted(t, 1)

	p.mu.Lock()
	defer p.mu.Unlock()
	if len(p.calls) != 1 {
		t.Fatalf("insert calls = %d, want 1", len(p.calls))
	}
	call := p.calls[0]
	if call.source != "desktop" {
		t.Errorf("source = %q, want %q", call.source, "desktop")
	}
	if call.scope.ID() != league.Historical("Mirage").ID() {
		t.Errorf("scope league = %q, want %q", call.scope.ID(), league.Historical("Mirage").ID())
	}
	if call.result.Gem != "Vaal Grace of Phasing" {
		t.Errorf("persisted Gem = %q, want %q", call.result.Gem, "Vaal Grace of Phasing")
	}
	if call.result.Variant != "21/20" {
		t.Errorf("persisted Variant = %q, want %q", call.result.Variant, "21/20")
	}
	if call.result.PriceFloor != 42.5 {
		t.Errorf("persisted PriceFloor = %v, want 42.5", call.result.PriceFloor)
	}
}
