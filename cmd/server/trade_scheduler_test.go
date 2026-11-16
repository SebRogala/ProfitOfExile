package main

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/server"
	"profitofexile/internal/trade"
)

type schedulerInputs struct {
	gems             []lab.GemPrice
	rows             []trade.TradeLookupResult
	stamp            time.Time
	gemErr, checkErr error
}

func (i *schedulerInputs) LatestGemPrices(context.Context, league.Scope) ([]lab.GemPrice, time.Time, error) {
	return i.gems, i.stamp, i.gemErr
}
func (i *schedulerInputs) LookupsAsOf(context.Context, league.Scope, time.Time) ([]trade.TradeLookupResult, error) {
	return i.rows, i.checkErr
}

func schedulerGem(name, variant string) lab.GemPrice {
	return lab.GemPrice{Name: name, Variant: variant, Chaos: 1, IsTransfigured: true, GemColor: "GREEN", Listings: 1}
}

func schedulerHarness(t *testing.T, input *schedulerInputs, responses ...http.HandlerFunc) (*tradeScheduler, *trade.TradeCache, *trade.RateLimiter, <-chan string) {
	t.Helper()
	requests := make(chan string, 100)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var body struct {
			Query struct {
				Type string `json:"type"`
				Term string `json:"term"`
			} `json:"query"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		if body.Query.Term != "" {
			body.Query.Type = body.Query.Term
		}
		requests <- body.Query.Type
		if len(responses) > 0 {
			responses[0](w, r)
			return
		}
		w.Write([]byte(`{"id":"test","result":[],"total":0}`))
	}))
	t.Cleanup(upstream.Close)
	scope := league.Historical("Test")
	cfg := trade.TradeConfig{LeagueName: scope.ID(), CeilingFactor: 1, DefaultSearchRate: 1000, DefaultFetchRate: 1000, MaxQueueWait: time.Second}
	cache := trade.NewTradeCache(100, scope)
	limiter := trade.NewRateLimiter(cfg)
	client := trade.NewClient(cfg)
	client.SetBaseURL(upstream.URL)
	gate := trade.NewGate(cfg, scope, limiter, client, nil, cache, func() float64 { return 200 }, nil)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { defer close(done); gate.Run(ctx) }()
	t.Cleanup(func() { cancel(); <-done })
	s := newTradeScheduler(scope, gate, nil, input, input, func() resolve.Rate { return resolve.Rate{Chaos: 200} })
	return s, cache, limiter, requests
}

func expectSchedulerRequest(t *testing.T, requests <-chan string, want string) {
	t.Helper()
	select {
	case got := <-requests:
		if got != want {
			t.Fatalf("selected %q, want %q", got, want)
		}
	case <-time.After(time.Second):
		t.Fatalf("missing lookup for %q", want)
	}
}
func noSchedulerRequest(t *testing.T, requests <-chan string) {
	t.Helper()
	select {
	case got := <-requests:
		t.Fatalf("unexpected lookup %q", got)
	default:
	}
}

func TestTradeScheduler_MissingCacheCoversFourMarkets(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now}
	for _, variant := range []string{"1", "1/20", "20", "20/20"} {
		input.gems = append(input.gems, schedulerGem("Siege Ballista of Splintering", variant))
	}
	s, cache, _, requests := schedulerHarness(t, input)
	for range 4 {
		server.HandleTradeTick(context.Background(), s.scope, []byte(`{"league":"Test","variant":"20/20","minTier":"MID"}`), s.Tick)
		expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
	}
	for _, variant := range []string{"1", "1/20", "20", "20/20"} {
		if r, ok := cache.Get(trade.CacheKey("Siege Ballista of Splintering", variant)); !ok || r.Variant != variant {
			t.Fatalf("market %s not checked", variant)
		}
	}
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	if got := s.queue.Stats(time.Now()); got.NeverChecked != 0 {
		t.Fatalf("completion lag lost checks: %+v", got)
	}
}

func TestTradeScheduler_ChaoticSelectable(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now(), gems: []lab.GemPrice{schedulerGem("Siege Ballista of Splintering", "20")}}
	input.gems[0].Chaos = 5000
	for i, name := range []string{"A of Test", "B of Test", "C of Test", "D of Test", "E of Test", "F of Test", "G of Test", "H of Test", "I of Test"} {
		g := schedulerGem(name, "20")
		g.Chaos = float64(200 - i*15)
		g.Listings = 20
		input.gems = append(input.gems, g)
	}
	if got := lab.ComputeGemClassification(input.gems).Gems[lab.GemClassificationKey{Name: input.gems[0].Name, Variant: "20"}].Tier; got != "CHAOTIC" {
		t.Fatalf("fixture tier %q", got)
	}
	s, _, _, requests := schedulerHarness(t, input)
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
}

func TestTradeScheduler_BootFailureRecoversOnTick(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now(), gems: []lab.GemPrice{schedulerGem("Siege Ballista of Splintering", "1")}, gemErr: errors.New("read failed")}
	s, _, _, requests := schedulerHarness(t, input)
	s.Seed(context.Background())
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	if stats := s.queue.Stats(time.Now()); stats.Demand != 0 {
		t.Fatalf("failed input published: %+v", stats)
	}
	input.gemErr = nil
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
}

func TestTradeScheduler_ReadFailurePreservesOverdue(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now, gems: []lab.GemPrice{schedulerGem("Siege Ballista of Splintering", "1")},
		rows: []trade.TradeLookupResult{{Gem: "Siege Ballista of Splintering", Variant: "1", FetchedAt: now.Add(-25 * time.Hour)}}}
	s, _, _, requests := schedulerHarness(t, input)
	s.Seed(context.Background())
	input.checkErr = errors.New("check read failed")
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	stats := s.queue.Stats(now)
	if stats.Demand != 1 || stats.Overdue != 1 || stats.NeverChecked != 0 {
		t.Fatalf("read failure erased checks: %+v", stats)
	}
	input.checkErr = nil
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
}

func TestTradeScheduler_DesktopAliasAfterBoot(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now, gems: []lab.GemPrice{schedulerGem("A of Test", "1"), schedulerGem("B of Test", "1")}}
	s, _, _, requests := schedulerHarness(t, input)
	s.Seed(context.Background())
	input.rows = []trade.TradeLookupResult{{Gem: "A of Test", Variant: "1/0", FetchedAt: now}}
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "B of Test")
}

func TestTradeScheduler_UnrelatedHistoryCannotBlockDemand(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now, gems: []lab.GemPrice{schedulerGem("Siege Ballista of Splintering", "1")},
		rows: []trade.TradeLookupResult{
			{Gem: trade.FixedTargetLens60Outcome, Variant: trade.FixedTargetVariantFont, FetchedAt: now},
			{Gem: "Other of Test", Variant: "21/20c", FetchedAt: now},
			{Gem: "Other", Variant: "bad", FetchedAt: now},
		}}
	s, _, _, requests := schedulerHarness(t, input)
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
}

func TestTradeScheduler_EmptySnapshotReplacesDemand(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now(), gems: []lab.GemPrice{schedulerGem("Old of Test", "1")}}
	s, _, _, requests := schedulerHarness(t, input)
	s.Seed(context.Background())
	input.gems = nil
	input.stamp = time.Time{}
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	if stats := s.queue.Stats(time.Now()); stats.Demand != 0 {
		t.Fatalf("retained empty demand: %+v", stats)
	}
	input.stamp = time.Now()
	input.gems = []lab.GemPrice{schedulerGem("New of Test", "20")}
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "New of Test")
}

func TestTradeScheduler_LimiterPausePreservesOverdue(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now, gems: []lab.GemPrice{schedulerGem("Old of Test", "1")},
		rows: []trade.TradeLookupResult{{Gem: "Old of Test", Variant: "1", FetchedAt: now.Add(-7 * time.Hour)}}}
	s, _, limiter, requests := schedulerHarness(t, input)
	s.Seed(context.Background())
	limiter.ObserveResponse("search", http.Header{"Retry-After": []string{"60"}}, trade.ErrRateLimited)
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	if stats := s.queue.Stats(now); stats.Overdue != 1 || stats.NeverChecked != 0 {
		t.Fatalf("pause erased overdue: %+v", stats)
	}
}

func TestTradeScheduler_FixedTargetConsumesOneTick(t *testing.T) {
	now := time.Now()
	input := &schedulerInputs{stamp: now, gems: []lab.GemPrice{schedulerGem("Siege Ballista of Splintering", "1")}}
	s, cache, _, requests := schedulerHarness(t, input)
	targets := trade.FixedTargets()
	cache.Set(targets[1].Key(), &trade.TradeLookupResult{Gem: targets[1].Gem, Variant: targets[1].Variant, FetchedAt: now})
	s.fixed = trade.NewFixedTargetScheduler(cache, s.scope)
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Facetor's Lens")
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "Siege Ballista of Splintering")
}

func TestTradeScheduler_FailedInputDoesNotClaimFixedTarget(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now(), gemErr: errors.New("offline")}
	s, cache, _, requests := schedulerHarness(t, input)
	s.fixed = trade.NewFixedTargetScheduler(cache, s.scope)
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	target, ok := s.fixed.Claim(time.Now())
	if !ok || !strings.Contains(target.Gem, "outcome") {
		t.Fatalf("failed input consumed fixed claim: %+v %v", target, ok)
	}
}

func TestTradeScheduler_FailureDefersWithoutSuccessfulCheck(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now(), gems: []lab.GemPrice{schedulerGem("A of Test", "1"), schedulerGem("B of Test", "1")}}
	attempt := 0
	s, _, _, requests := schedulerHarness(t, input, func(w http.ResponseWriter, r *http.Request) {
		attempt++
		if attempt == 1 {
			w.WriteHeader(500)
			return
		}
		w.Write([]byte(`{"id":"test","result":[],"total":0}`))
	})
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "A of Test")
	s.Tick(context.Background())
	expectSchedulerRequest(t, requests, "B of Test")
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	if stats := s.queue.Stats(time.Now()); stats.NeverChecked != 1 {
		t.Fatalf("failed lookup counted: %+v", stats)
	}
}

func TestTradeScheduler_PreflightDoesNotClaimFixedTarget(t *testing.T) {
	input := &schedulerInputs{stamp: time.Now()}
	s, cache, limiter, requests := schedulerHarness(t, input)
	s.fixed = trade.NewFixedTargetScheduler(cache, s.scope)
	limiter.ObserveResponse("fetch", http.Header{"Retry-After": []string{"60"}}, trade.ErrRateLimited)
	s.Tick(context.Background())
	noSchedulerRequest(t, requests)
	target, ok := s.fixed.Claim(time.Now())
	if !ok || target.Gem != trade.FixedTargetLens60Outcome {
		t.Fatalf("preflight consumed claim: %+v %v", target, ok)
	}
}
