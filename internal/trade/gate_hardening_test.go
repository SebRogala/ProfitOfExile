package trade

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func hardeningClock(g *Gate) *atomic.Int64 {
	clock := &atomic.Int64{}
	clock.Store(time.Date(2026, 10, 23, 12, 0, 0, 0, time.UTC).UnixNano())
	g.now = func() time.Time { return time.Unix(0, clock.Load()) }
	g.limiter.now = g.now
	hardeningQueueTimers(g)
	g.sleep = func(ctx context.Context, d time.Duration) bool {
		if ctx.Err() != nil {
			return false
		}
		clock.Add(int64(d))
		return true
	}
	return clock
}

type hardeningQueueTimer struct {
	delay    time.Duration
	callback func()
	stopped  atomic.Bool
}

func hardeningQueueTimers(g *Gate) *[]*hardeningQueueTimer {
	timers := make([]*hardeningQueueTimer, 0)
	g.afterFunc = func(delay time.Duration, callback func()) func() bool {
		timer := &hardeningQueueTimer{delay: delay, callback: callback}
		timers = append(timers, timer)
		return func() bool { return !timer.stopped.Swap(true) }
	}
	return &timers
}

func hardeningRequest(g *Gate) *GateRequest {
	return &GateRequest{
		Gem: "Arc", Variant: "20/20", RequestID: "hardening",
		Priority: PriorityHigh, SubmittedAt: g.now(), Result: make(chan *GateResponse, 1),
	}
}

func processHardeningRequest(t *testing.T, g *Gate, ctx context.Context, req *GateRequest) *GateResponse {
	t.Helper()
	g.Submit(req)
	select {
	case queued := <-g.HighChan():
		g.process(ctx, queued)
	default:
		t.Fatal("request was not queued; an earlier lookup may still own its in-flight key")
	}
	select {
	case result := <-req.Result:
		return result
	default:
		t.Fatal("processed request did not receive a result")
		return nil
	}
}

func exhaustHardeningPool(g *Gate, name string) {
	g.limiter.SyncFromHeaders(name, http.Header{
		"X-Rate-Limit-Rules":    {"ip"},
		"X-Rate-Limit-Ip":       {"1:60:60"},
		"X-Rate-Limit-Ip-State": {"0:60:0"},
	})
	g.limiter.Record(name)
}

func TestGate_EmptySearchDoesNotWaitForFetchBudget(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	hardeningClock(g)
	exhaustHardeningPool(g, "fetch")
	g.sleep = func(context.Context, time.Duration) bool {
		t.Fatal("empty search waited for an unused fetch budget")
		return false
	}
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil || resp.Data == nil || resp.Data.Total != 0 || resp.Data.Listings == nil {
		t.Fatalf("empty search result = %+v, want a successful checked empty result", resp)
	}
}

func TestGate_CapacityUpdateReopensSearchBeforeOldEstimate(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	start := g.now()
	exhaustHardeningPool(g, "search")
	g.sleep = func(_ context.Context, d time.Duration) bool {
		if d > 250*time.Millisecond {
			t.Fatalf("slept %v instead of rechecking capacity within 250ms", d)
		}
		clock.Add(int64(d))
		g.limiter.SyncFromHeaders("search", http.Header{
			"X-Rate-Limit-Rules":    {"ip"},
			"X-Rate-Limit-Ip":       {"100:60:60"},
			"X-Rate-Limit-Ip-State": {"0:60:0"},
		})
		return true
	}
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil || resp.Data == nil || g.now().Sub(start) > 250*time.Millisecond {
		t.Fatalf("result = %+v at %v, want lookup as soon as capacity reopened", resp, g.now().Sub(start))
	}
}

func TestGate_QueueDeadlinePreventsSearch(t *testing.T) {
	for _, age := range []time.Duration{30 * time.Second, 31 * time.Second, 0} {
		t.Run(age.String(), func(t *testing.T) {
			var sends atomic.Int32
			server := testGateServer(t, &sends)
			defer server.Close()
			g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
			hardeningClock(g)
			if age == 0 {
				exhaustHardeningPool(g, "search")
			}
			req := hardeningRequest(g)
			req.SubmittedAt = req.SubmittedAt.Add(-age)
			resp := processHardeningRequest(t, g, context.Background(), req)
			if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout after 30s") || sends.Load() != 0 {
				t.Fatalf("result=%+v sends=%d, want queue timeout without a search", resp, sends.Load())
			}
		})
	}
}

func TestGate_CancelledWaitReleasesKeyForResubmit(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	exhaustHardeningPool(g, "search")
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	g.sleep = func(context.Context, time.Duration) bool { cancel(); return false }
	resp := processHardeningRequest(t, g, ctx, hardeningRequest(g))
	if !errors.Is(resp.Error, context.Canceled) {
		t.Fatalf("cancelled request error = %v, want context.Canceled", resp.Error)
	}
	clock.Add(int64(time.Minute))
	resp = processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Arc" {
		t.Fatalf("resubmission result = %+v, want successful lookup", resp)
	}
}

func TestGate_RestrictionBlocksFollowingLookupUntilExpiry(t *testing.T) {
	for _, tc := range []struct {
		name     string
		endpoint string
		status   int
	}{
		{"search429", "search", http.StatusTooManyRequests},
		{"fetch429", "fetch", http.StatusTooManyRequests},
		{"searchBan403", "search", http.StatusForbidden},
		{"fetchBan403", "fetch", http.StatusForbidden},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var sends atomic.Int32
			var rejected atomic.Bool
			var beforeExpiry atomic.Bool
			clock := &atomic.Int64{}
			var expiry atomic.Int64
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				sends.Add(1)
				if rejected.Load() && clock.Load() < expiry.Load() {
					beforeExpiry.Store(true)
				}
				isSearch := strings.Contains(r.URL.Path, "/search/")
				if ((tc.endpoint == "search" && isSearch) || (tc.endpoint == "fetch" && !isSearch)) && !rejected.Swap(true) {
					expiry.Store(clock.Load() + int64(2*time.Second))
					if tc.status == http.StatusTooManyRequests {
						w.Header().Set("Retry-After", "2")
					} else {
						w.Header().Set("X-Rate-Limit-Ip-State", "1:60:2")
					}
					w.WriteHeader(tc.status)
					return
				}
				if isSearch {
					json.NewEncoder(w).Encode(tradeSearchResp{ID: "q", Total: 1, Result: []string{"r"}})
				} else {
					json.NewEncoder(w).Encode(tradeFetchResp{Result: []tradeFetchEntry{{Listing: tradeFetchListing{Price: tradeFetchPrice{Amount: 12, Currency: "chaos"}}}}})
				}
			}))
			defer server.Close()
			g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
			clock = hardeningClock(g)
			resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
			if resp.Error == nil || (tc.status == http.StatusTooManyRequests && !errors.Is(resp.Error, ErrRateLimited)) {
				t.Fatalf("first lookup error=%v, want rejection for status %d", resp.Error, tc.status)
			}
			resp = processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
			if resp.Error != nil || resp.Data == nil || resp.Data.PriceFloor != 12 || beforeExpiry.Load() || clock.Load() < expiry.Load() {
				t.Fatalf("following lookup=%+v earlySend=%v, want 12c result only after restriction expiry", resp, beforeExpiry.Load())
			}
			wantSends := int32(3)
			if tc.endpoint == "fetch" {
				wantSends = 4
			}
			if sends.Load() != wantSends {
				t.Errorf("HTTP sends=%d, want %d", sends.Load(), wantSends)
			}
		})
	}
}

type hardeningPersistedLookup struct {
	result *TradeLookupResult
	source string
}

func TestGate_QueuedCallerExpiresDuringActiveFetchWait(t *testing.T) {
	var searches atomic.Int32
	server := testGateServer(t, &searches)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	timers := hardeningQueueTimers(g)
	exhaustHardeningPool(g, "fetch")
	active := hardeningRequest(g)
	queued := hardeningRequest(g)
	queued.Gem, queued.RequestID = "Spark", "queued-spark"
	g.Submit(active)
	g.Submit(queued)
	if len(*timers) != 2 || (*timers)[1].delay != 30*time.Second {
		t.Fatalf("queue timers=%+v, want one 30s deadline per caller", *timers)
	}
	expired := false
	g.sleep = func(_ context.Context, _ time.Duration) bool {
		if !expired {
			expired = true
			clock.Add(int64(30 * time.Second))
			(*timers)[1].callback()
			select {
			case resp := <-queued.Result:
				if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout after 30s") {
					t.Fatalf("queued result=%+v, want queue timeout during the active fetch wait", resp)
				}
			default:
				t.Fatal("queued caller remained unanswered behind an active fetch wait")
			}
			if searches.Load() != 1 {
				t.Fatalf("searches=%d before fetch capacity reopened, want only the active lookup", searches.Load())
			}
		}
		clock.Add(int64(30 * time.Second))
		return true
	}
	g.process(context.Background(), <-g.HighChan())
	select {
	case resp := <-active.Result:
		if resp.Error != nil || resp.Data == nil || resp.Data.PriceFloor != 10 {
			t.Fatalf("active lookup=%+v, want successful running lookup despite queue age", resp)
		}
	default:
		t.Fatal("active lookup did not complete")
	}
	g.process(context.Background(), <-g.HighChan())
	if searches.Load() != 1 {
		t.Errorf("expired queue item sent another search; searches=%d", searches.Load())
	}
}

func TestGate_StartedLookupIgnoresLateQueueCallback(t *testing.T) {
	server := testGateServer(t, nil)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	timers := hardeningQueueTimers(g)
	exhaustHardeningPool(g, "fetch")
	g.sleep = func(_ context.Context, _ time.Duration) bool {
		if len(*timers) != 1 || !(*timers)[0].stopped.Load() {
			t.Fatal("queue timer was not stopped at search start")
		}
		clock.Add(int64(time.Minute))
		// A timer already dispatched before Stop can still invoke its callback.
		(*timers)[0].callback()
		return true
	}
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil || resp.Data == nil || resp.Data.PriceFloor != 10 {
		t.Fatalf("running lookup=%+v, want 10c result rather than a late queue timeout", resp)
	}
}

func TestGate_LaterDedupWaiterSurvivesOlderDeadline(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	pub := &mockPublisher{}
	g := newTestGate(t, server, pub, 30*time.Second)
	clock := hardeningClock(g)
	timers := hardeningQueueTimers(g)
	older := hardeningRequest(g)
	g.Submit(older)
	clock.Add(int64(20 * time.Second))
	later := hardeningRequest(g)
	later.RequestID = "later-dedup"
	g.Submit(later)
	if len(*timers) != 2 || (*timers)[1].delay != 30*time.Second {
		t.Fatalf("dedup timers=%+v, want a separate 30s deadline for the later caller", *timers)
	}
	clock.Add(int64(10 * time.Second))
	(*timers)[0].callback()
	select {
	case resp := <-older.Result:
		if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout after 30s") {
			t.Fatalf("older result=%+v, want queue timeout", resp)
		}
	default:
		t.Fatal("older waiter missed its deadline")
	}
	select {
	case resp := <-later.Result:
		t.Fatalf("later waiter expired at older deadline: %+v", resp)
	default:
	}
	g.process(context.Background(), <-g.HighChan())
	select {
	case resp := <-later.Result:
		if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Arc" || resp.Data.Total != 0 {
			t.Fatalf("later dedup result=%+v, want successful empty lookup", resp)
		}
	default:
		t.Fatal("surviving dedup waiter did not receive its lookup")
	}
	readyForLater := false
	for _, event := range pub.getEvents() {
		var payload struct {
			Type      string             `json:"type"`
			RequestID string             `json:"requestId"`
			Data      *TradeLookupResult `json:"data"`
		}
		if err := json.Unmarshal([]byte(event.Payload), &payload); err != nil {
			t.Fatalf("decode trade event: %v", err)
		}
		if payload.Type != "ready" {
			continue
		}
		if payload.RequestID == older.RequestID {
			t.Fatal("expired older caller received a ready event")
		}
		if payload.RequestID == later.RequestID && payload.Data != nil && payload.Data.Gem == "Arc" && payload.Data.Total == 0 {
			readyForLater = true
		}
	}
	if !readyForLater {
		t.Fatal("surviving dedup caller did not receive its ready event with the empty lookup")
	}
}

func TestGate_ResubmitBeforeExpiredQueueItemConsumedUsesReservedFlight(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	timers := hardeningQueueTimers(g)
	expired := hardeningRequest(g)
	g.Submit(expired)
	clock.Add(int64(30 * time.Second))
	(*timers)[0].callback()
	select {
	case resp := <-expired.Result:
		if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout after 30s") {
			t.Fatalf("original caller=%+v, want queue timeout", resp)
		}
	default:
		t.Fatal("original caller did not expire")
	}
	replacement := hardeningRequest(g)
	replacement.RequestID = "replacement"
	g.Submit(replacement)
	select {
	case queued := <-g.HighChan():
		if queued != expired {
			t.Fatal("replacement displaced the original reserved queue item")
		}
		g.process(context.Background(), queued)
	default:
		t.Fatal("original reserved queue item disappeared")
	}
	select {
	case resp := <-replacement.Result:
		if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Arc" || resp.Data.Total != 0 {
			t.Fatalf("replacement=%+v, want successful lookup through the original queue item", resp)
		}
	default:
		t.Fatal("original queue item did not complete the replacement caller")
	}
	select {
	case <-g.HighChan():
		t.Fatal("replacement created an extra queue item for the reserved flight")
	default:
	}
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Arc" {
		t.Fatalf("future submission=%+v, want successful lookup after reserved flight completion", resp)
	}
}

type hardeningPersister chan hardeningPersistedLookup

func (p hardeningPersister) InsertTradeLookup(_ context.Context, _ league.Scope, result *TradeLookupResult, source string) error {
	p <- hardeningPersistedLookup{result: result, source: source}
	return nil
}

func TestGate_EmptyLookupPersistsServerGateProvenance(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	hardeningClock(g)
	persisted := make(hardeningPersister, 1)
	g.repo = persisted
	g.persistAsync = func(f func()) { f() }
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error != nil {
		t.Fatalf("lookup failed: %v", resp.Error)
	}
	select {
	case got := <-persisted:
		if got.source != "server-gate" || got.result != resp.Data || got.result.FetchedAt.IsZero() || got.result.Listings == nil || len(got.result.Listings) != 0 {
			t.Fatalf("persisted=%+v result=%+v, want stamped empty server-gate result", got, got.result)
		}
	default:
		t.Fatal("successful empty lookup was not persisted")
	}
}

func TestGate_FailedLookupAllowsNextStaleKeyRefresh(t *testing.T) {
	for _, tc := range []struct {
		name       string
		endpoint   string
		status     int
		retryDelay time.Duration
	}{
		{"search400", "search", http.StatusBadRequest, 6 * time.Hour},
		{"fetch404", "fetch", http.StatusNotFound, 6 * time.Hour},
		{"search422", "search", http.StatusUnprocessableEntity, 6 * time.Hour},
		{"fetch403", "fetch", http.StatusForbidden, 5 * time.Minute},
		{"search500", "search", http.StatusInternalServerError, 5 * time.Minute},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if strings.Contains(r.URL.Path, "/fetch/") {
					w.WriteHeader(tc.status)
					return
				}
				var query struct {
					Query struct {
						Type string `json:"type"`
					} `json:"query"`
				}
				if err := json.NewDecoder(r.Body).Decode(&query); err != nil {
					t.Errorf("decode search: %v", err)
					w.WriteHeader(http.StatusBadRequest)
					return
				}
				if query.Query.Type == "Arc" {
					if tc.endpoint == "search" {
						w.WriteHeader(tc.status)
						return
					}
					json.NewEncoder(w).Encode(tradeSearchResp{ID: "arc", Total: 1, Result: []string{"r"}})
					return
				}
				json.NewEncoder(w).Encode(tradeSearchResp{ID: "spark", Total: 0, Result: []string{}})
			}))
			defer server.Close()
			g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
			clock := hardeningClock(g)
			g.cache.now = g.now
			persisted := make(hardeningPersister, 1)
			g.repo = persisted
			g.persistAsync = func(f func()) { f() }
			failedKey, nextKey := CacheKey("Arc", "20/20"), CacheKey("Spark", "20/20")
			lastSuccessfulAt := g.now().Add(-2 * time.Hour)
			lastSuccess := &TradeLookupResult{Gem: "Arc", Variant: "20/20", Total: 1,
				PriceFloor: 42, MedianTop10: 42, FetchedAt: lastSuccessfulAt,
				Listings: []TradeListingDetail{{ChaosPrice: 42}}}
			g.cache.Set(failedKey, lastSuccess)
			g.cache.Set(nextKey, &TradeLookupResult{Gem: "Spark", Variant: "20/20", FetchedAt: g.now().Add(-time.Hour)})
			if key, ok := g.cache.OldestStale(5*time.Minute, nil); !ok || key != failedKey {
				t.Fatalf("initial stale key=%q found=%v, want Arc", key, ok)
			}
			resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
			if resp.Error == nil || resp.Data != nil {
				t.Fatalf("failed lookup=%+v, want error without an empty-market result", resp)
			}
			// Set derives MedianTop3 on a copy; failure must preserve the cached
			// quote's values, not the address of the supplied result.
			if got, ok := g.cache.Get(failedKey); !ok || got.Gem != "Arc" || got.Variant != "20/20" ||
				got.PriceFloor != 42 || got.MedianTop3 != 42 || got.MedianTop10 != 42 || got.Total != 1 ||
				len(got.Listings) != 1 || got.Listings[0].ChaosPrice != 42 || !got.FetchedAt.Equal(lastSuccessfulAt) {
				t.Fatalf("failed lookup replaced prior successful quote: %+v", got)
			}
			select {
			case got := <-persisted:
				t.Fatalf("failed lookup persisted a price row: %+v", got)
			default:
			}
			// Advance one minute; this also expires the global pause in fetch403.
			clock.Add(int64(time.Minute))
			key, ok := g.cache.OldestStale(5*time.Minute, nil)
			if !ok || key != nextKey {
				t.Fatalf("next stale key=%q found=%v, want Spark instead of failed Arc", key, ok)
			}
			next := hardeningRequest(g)
			next.Gem, next.Variant = ParseCacheKey(key)
			resp = processHardeningRequest(t, g, context.Background(), next)
			if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Spark" || resp.Data.Total != 0 {
				t.Fatalf("next stale refresh=%+v, want successful Spark lookup", resp)
			}
			if key, ok := g.cache.OldestStale(5*time.Minute, nil); ok {
				t.Fatalf("stale key=%q, want none while Arc is delayed and Spark is fresh", key)
			}
			clock.Add(int64(tc.retryDelay - time.Minute - time.Nanosecond))
			if key, ok := g.cache.OldestStale(5*time.Minute, func(key string, _ *TradeLookupResult) bool {
				return key == failedKey
			}); ok {
				t.Fatalf("failed key=%q retried before its %v delay expired", key, tc.retryDelay)
			}
			clock.Add(int64(time.Nanosecond))
			if key, ok := g.cache.OldestStale(5*time.Minute, nil); !ok || key != failedKey {
				t.Fatalf("retry at delay expiry=%q found=%v, want Arc", key, ok)
			}
		})
	}
}

func TestGate_QueueTimeoutDuringRestrictionDefersKeyPastExpiry(t *testing.T) {
	for _, tc := range []struct {
		name     string
		viaTimer bool
		delay    time.Duration
	}{
		{"dequeued", false, 0},
		{"queued-timer", true, 30 * time.Second},
		{"timer-at-pause-expiry", true, 30 * time.Minute},
		{"timer-after-pause-expiry", true, 40 * time.Minute},
		{"dequeued-after-pause-expiry", false, 40 * time.Minute},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var searches atomic.Int32
			server := testGateServer(t, &searches)
			defer server.Close()
			g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
			clock := hardeningClock(g)
			timers := hardeningQueueTimers(g)
			g.cache.now = g.now
			failedKey, nextKey := CacheKey("Arc", "20/20"), CacheKey("Spark", "20/20")
			lastSuccessAt := g.now().Add(-2 * time.Hour)
			g.cache.Set(failedKey, &TradeLookupResult{Gem: "Arc", Variant: "20/20", FetchedAt: lastSuccessAt})
			g.cache.Set(nextKey, &TradeLookupResult{Gem: "Spark", Variant: "20/20", FetchedAt: g.now().Add(-time.Hour)})
			until := g.now().Add(30 * time.Minute)
			g.limiter.ObserveResponse("search", http.Header{"Retry-After": {"1800"}}, ErrRateLimited)
			req := hardeningRequest(g)
			g.Submit(req)
			clock.Add(int64(tc.delay))
			if tc.viaTimer {
				(*timers)[0].callback()
			}
			g.process(context.Background(), <-g.HighChan())
			var resp *GateResponse
			select {
			case resp = <-req.Result:
			default:
				t.Fatal("queued caller did not receive its timeout")
			}
			if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout") || searches.Load() != 0 {
				t.Fatalf("response=%+v searches=%d, want queue timeout without sending", resp, searches.Load())
			}
			if got, ok := g.cache.Get(failedKey); !ok || !got.FetchedAt.Equal(lastSuccessAt) {
				t.Fatalf("queue timeout changed successful check timestamp: %+v", got)
			}
			// The cooldown starts after the later of restriction expiry and
			// timeout processing, including a callback delayed past expiry.
			resumeAt := until
			if g.now().After(resumeAt) {
				resumeAt = g.now()
			}
			cooldownEnds := resumeAt.Add(5 * time.Minute)
			clock.Store(resumeAt.UnixNano())
			if key, ok := g.cache.OldestStale(5*time.Minute, nil); !ok || key != nextKey {
				t.Fatalf("first stale key after restriction=%q found=%v, want Spark", key, ok)
			}
			clock.Store(cooldownEnds.Add(-time.Nanosecond).UnixNano())
			if key, ok := g.cache.OldestStale(5*time.Minute, func(key string, _ *TradeLookupResult) bool {
				return key == failedKey
			}); ok {
				t.Fatalf("timed-out key=%q became eligible before post-pause cooldown expired", key)
			}
			clock.Add(int64(time.Nanosecond))
			if key, ok := g.cache.OldestStale(5*time.Minute, nil); !ok || key != failedKey {
				t.Fatalf("stale key at cooldown expiry=%q found=%v, want Arc", key, ok)
			}
		})
	}
}

func TestGate_QueueTimeoutWithoutRestrictionKeepsStaleEligibility(t *testing.T) {
	server := emptySearchServer(t, 0)
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	hardeningClock(g)
	g.cache.now = g.now
	key := CacheKey("Arc", "20/20")
	g.cache.Set(key, &TradeLookupResult{Gem: "Arc", Variant: "20/20", FetchedAt: g.now().Add(-time.Hour)})
	exhaustHardeningPool(g, "search")
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout") {
		t.Fatalf("response=%+v, want timeout waiting for local pool capacity", resp)
	}
	if got, ok := g.cache.OldestStale(5*time.Minute, nil); !ok || got != key {
		t.Fatalf("local-capacity timeout changed stale eligibility: key=%q found=%v", got, ok)
	}
}

func TestGate_BadKeyIsNotFirstPickAfterFloodPause(t *testing.T) {
	var searches atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		searches.Add(1)
		var body struct {
			Query struct {
				Type string `json:"type"`
			} `json:"query"`
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Errorf("decode query: %v", err)
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		if body.Query.Type == "Arc" {
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		json.NewEncoder(w).Encode(tradeSearchResp{ID: "spark", Total: 0, Result: []string{}})
	}))
	defer server.Close()
	g := newTestGate(t, server, &mockPublisher{}, 30*time.Second)
	clock := hardeningClock(g)
	g.cache.now = g.now
	badKey, goodKey := CacheKey("Arc", "20/20"), CacheKey("Spark", "20/20")
	g.cache.Set(badKey, &TradeLookupResult{Gem: "Arc", Variant: "20/20", FetchedAt: g.now().Add(-2 * time.Hour)})
	g.cache.Set(goodKey, &TradeLookupResult{Gem: "Spark", Variant: "20/20", FetchedAt: g.now().Add(-time.Hour)})
	// A known long rule reproduces a pause longer than the transient key delay.
	g.limiter.SyncFromHeaders("search", http.Header{
		"X-Rate-Limit-Rules":    {"ip"},
		"X-Rate-Limit-Ip":       {"100:1800:1800"},
		"X-Rate-Limit-Ip-State": {"0:1800:0"},
	})
	resp := processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error == nil || !g.limiter.BlockedUntil().IsZero() {
		t.Fatalf("single bad gem response=%+v global deadline=%v, want key-local rejection", resp, g.limiter.BlockedUntil())
	}
	for i := 1; i < invalidRequestThreshold; i++ {
		g.limiter.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: http.StatusBadRequest})
	}
	until := g.limiter.BlockedUntil()
	if !until.Equal(g.now().Add(30 * time.Minute)) {
		t.Fatalf("flood restriction=%v, want known-rule fallback of 30m", until)
	}
	// An explicit retry can still queue during the pause. Its timeout must
	// preserve the rejected key's longer six-hour delay.
	resp = processHardeningRequest(t, g, context.Background(), hardeningRequest(g))
	if resp.Error == nil || !strings.Contains(resp.Error.Error(), "queue timeout") || searches.Load() != 1 {
		t.Fatalf("retry during flood pause=%+v searches=%d, want timeout without sending", resp, searches.Load())
	}
	clock.Store(until.UnixNano())
	key, ok := g.cache.OldestStale(5*time.Minute, nil)
	if !ok || key != goodKey {
		t.Fatalf("first key after flood pause=%q found=%v, want Spark instead of rejected Arc", key, ok)
	}
	next := hardeningRequest(g)
	next.Gem, next.Variant = ParseCacheKey(key)
	resp = processHardeningRequest(t, g, context.Background(), next)
	if resp.Error != nil || resp.Data == nil || resp.Data.Gem != "Spark" || searches.Load() != 2 {
		t.Fatalf("next refresh=%+v searches=%d, want one bad Arc attempt followed by successful Spark", resp, searches.Load())
	}
	clock.Store(until.Add(5 * time.Minute).UnixNano())
	if key, ok := g.cache.OldestStale(5*time.Minute, func(key string, _ *TradeLookupResult) bool {
		return key == badKey
	}); ok {
		t.Fatalf("queued timeout shortened the six-hour rejection delay: eligible key=%q", key)
	}
}
