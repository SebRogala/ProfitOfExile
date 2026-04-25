package server

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/trade"
)

// newTickTestGate builds a Gate suitable for HandleTradeTick assertions.
// We never call gate.Run — tests inspect the low-priority channel directly
// via gate.LowChan() to confirm whether a submission landed.
func newTickTestGate() *trade.Gate {
	cfg := trade.TradeConfig{
		LeagueName:        "Mirage",
		UserAgent:         "ProfitOfExile/test",
		CeilingFactor:     0.65,
		DefaultSearchRate: 5,
		DefaultFetchRate:  5,
		MaxQueueWait:      30 * time.Second,
		CacheMaxEntries:   10,
	}
	limiter := trade.NewRateLimiter(cfg)
	client := trade.NewClient(cfg)
	cache := trade.NewTradeCache(cfg.CacheMaxEntries)
	return trade.NewGate(cfg, limiter, client, nil, cache, func() float64 { return 200.0 }, nil)
}

// seedStaleEntry inserts a fake stale cache entry so OldestStale can find it.
// The entry's FetchedAt is set deep in the past so any minAge filter passes.
func seedStaleEntry(c *trade.TradeCache, gem, variant string) {
	c.Set(trade.CacheKey(gem, variant), &trade.TradeLookupResult{
		Gem:       gem,
		Variant:   variant,
		FetchedAt: time.Now().Add(-1 * time.Hour),
	})
}

// drainLow does a non-blocking read on the gate's low channel.
// Returns the request (or nil) plus a flag for whether one was queued.
func drainLow(gate *trade.Gate) (*trade.GateRequest, bool) {
	select {
	case req := <-gate.LowChan():
		return req, true
	default:
		return nil, false
	}
}

func TestHandleTradeTick_InvalidJSON_NoSubmit(t *testing.T) {
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	HandleTradeTick(context.Background(), gate, cache, labCache, []byte("not json"))

	if _, ok := drainLow(gate); ok {
		t.Fatal("invalid payload must not enqueue a refresh")
	}
}

func TestHandleTradeTick_UnknownMinTier_NoSubmit(t *testing.T) {
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	// Even with a stale candidate present, an unknown minTier must short-circuit.
	seedStaleEntry(cache, "Arc of Surging", "20/20")

	HandleTradeTick(context.Background(), gate, cache, labCache,
		[]byte(`{"variant":"20/20","minTier":"NOPE","minAge":"1m"}`))

	if _, ok := drainLow(gate); ok {
		t.Fatal("unknown minTier must not enqueue a refresh")
	}
}

func TestHandleTradeTick_EmptyCache_NoSubmit(t *testing.T) {
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	HandleTradeTick(context.Background(), gate, cache, labCache,
		[]byte(`{"variant":"20/20","minAge":"1m"}`))

	if _, ok := drainLow(gate); ok {
		t.Fatal("empty cache must not enqueue a refresh")
	}
}

func TestHandleTradeTick_StaleTierMatch_Submits(t *testing.T) {
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	// Cache one stale transfigured gem matching variant 20/20.
	seedStaleEntry(cache, "Arc of Surging", "20/20")

	// Lab cache must list that gem at >= MID-HIGH for the tier filter to pick it.
	labCache.SetGemSignals([]lab.GemSignal{
		{Name: "Arc of Surging", Variant: "20/20", Tier: "HIGH"},
	})

	// HandleTradeTick blocks inside trade.SubmitRefresh until req.Result is
	// written. Run it in a goroutine and intercept the request, then send a
	// fake error response back to unblock the helper.
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)

	done := make(chan struct{})
	go func() {
		HandleTradeTick(ctx, gate, cache, labCache,
			[]byte(`{"variant":"20/20","minTier":"MID-HIGH","minAge":"1m"}`))
		close(done)
	}()

	select {
	case req := <-gate.LowChan():
		if req.Gem != "Arc of Surging" {
			t.Errorf("Gem = %q, want %q", req.Gem, "Arc of Surging")
		}
		if req.Variant != "20/20" {
			t.Errorf("Variant = %q, want %q", req.Variant, "20/20")
		}
		if req.Priority != trade.PriorityLow {
			t.Errorf("Priority = %v, want PriorityLow", req.Priority)
		}
		// Unblock the SubmitRefresh goroutine so it doesn't leak.
		req.Result <- &trade.GateResponse{Error: context.Canceled}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for tick submission")
	}

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("HandleTradeTick did not return after delivering response")
	}
}

// --- Additional coverage for branches not exercised above ---

func TestHandleTradeTick_BaseGemFilter_NoSubmit(t *testing.T) {
	// Non-transfigured gems (no " of " in the name) must never be picked.
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	// Seed a stale base gem — should be ignored.
	seedStaleEntry(cache, "Arc", "20/20")

	HandleTradeTick(context.Background(), gate, cache, labCache,
		[]byte(`{"variant":"20/20","minAge":"1m"}`))

	if _, ok := drainLow(gate); ok {
		t.Fatal("base gem (no transfigured suffix) must not be refreshed")
	}
}

func TestHandleTradeTick_EmptyTierSet_NoSubmit(t *testing.T) {
	// minTier is set but no signal for the variant qualifies — the early return
	// at "len(tierSet) == 0" must fire without consulting the cache predicate.
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	// Stale entry that WOULD match if tier filter passed — proves we returned
	// early instead of falling through.
	seedStaleEntry(cache, "Arc of Surging", "20/20")

	// Signals exist but for a different variant, so tierSet ends up empty.
	labCache.SetGemSignals([]lab.GemSignal{
		{Name: "Arc of Surging", Variant: "1/0", Tier: "HIGH"},
	})

	HandleTradeTick(context.Background(), gate, cache, labCache,
		[]byte(`{"variant":"20/20","minTier":"MID-HIGH","minAge":"1m"}`))

	if _, ok := drainLow(gate); ok {
		t.Fatal("empty tierSet must short-circuit before cache lookup")
	}
}

func TestHandleTradeTick_InvalidMinAge_NoSubmit(t *testing.T) {
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	seedStaleEntry(cache, "Arc of Surging", "20/20")

	HandleTradeTick(context.Background(), gate, cache, labCache,
		[]byte(`{"variant":"20/20","minAge":"notaduration"}`))

	if _, ok := drainLow(gate); ok {
		t.Fatal("invalid minAge must not enqueue a refresh")
	}
}

func TestHandleTradeTick_DefaultsToTwentyTwenty_Submits(t *testing.T) {
	// Payload omits variant; handler must default to "20/20".
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	seedStaleEntry(cache, "Arc of Surging", "20/20")

	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)

	done := make(chan struct{})
	go func() {
		HandleTradeTick(ctx, gate, cache, labCache, []byte(`{"minAge":"1m"}`))
		close(done)
	}()

	select {
	case req := <-gate.LowChan():
		if req.Variant != "20/20" {
			t.Errorf("Variant = %q, want %q (default)", req.Variant, "20/20")
		}
		req.Result <- &trade.GateResponse{Error: context.Canceled}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for default-variant submission")
	}

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("HandleTradeTick did not return")
	}
}

func TestHandleTradeTick_NoTierFilter_PicksAnyTransfigured(t *testing.T) {
	// minTier omitted: the tierSet branch is skipped entirely; any stale
	// transfigured gem for the variant is picked.
	gate := newTickTestGate()
	cache := trade.NewTradeCache(10)
	labCache := lab.NewCache()

	seedStaleEntry(cache, "Frost Blades of Katabasis", "20/20")
	// Deliberately leave labCache empty — proves the tierSet==nil path doesn't
	// consult signals.

	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)

	done := make(chan struct{})
	go func() {
		HandleTradeTick(ctx, gate, cache, labCache,
			[]byte(`{"variant":"20/20","minAge":"1m"}`))
		close(done)
	}()

	select {
	case req := <-gate.LowChan():
		if req.Gem != "Frost Blades of Katabasis" {
			t.Errorf("Gem = %q, want %q", req.Gem, "Frost Blades of Katabasis")
		}
		req.Result <- &trade.GateResponse{Error: context.Canceled}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for no-tier-filter submission")
	}

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("HandleTradeTick did not return")
	}
}
