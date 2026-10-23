package trade

import (
	"errors"
	"fmt"
	"net/http"
	"sync"
	"testing"
	"time"
)

// testConfig returns a TradeConfig with sensible test defaults.
func testConfig() TradeConfig {
	return TradeConfig{
		Enabled:           true,
		LeagueName:        "Test",
		CeilingFactor:     0.65,
		LatencyPadding:    100 * time.Millisecond,
		DefaultSearchRate: 5,
		DefaultFetchRate:  5,
		MaxQueueWait:      30 * time.Second,
		CacheMaxEntries:   100,
		UserAgent:         "test-agent",
		SyncWaitBudget:    500 * time.Millisecond,
	}
}

func TestEstimateWait_Fresh(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	wait := rl.EstimateWait("search")
	if wait != 0 {
		t.Errorf("expected zero wait for fresh limiter, got %v", wait)
	}

	wait = rl.EstimateWait("fetch")
	if wait != 0 {
		t.Errorf("expected zero wait for fresh fetch pool, got %v", wait)
	}
}

func TestEstimateWait_WithinBudget(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// With ceiling=0.65 and maxHits=5, effective max = int(5*0.65) = 3.
	// Record 2 hits — still under ceiling.
	rl.Record("search")
	rl.Record("search")

	wait := rl.EstimateWait("search")
	if wait != 0 {
		t.Errorf("expected zero wait with 2 hits under ceiling of 3, got %v", wait)
	}
}

func TestEstimateWait_AtCeiling(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// effective max = int(5*0.65) = 3. Record 3 hits to reach ceiling.
	rl.Record("search")
	rl.Record("search")
	rl.Record("search")

	wait := rl.EstimateWait("search")
	if wait <= 0 {
		t.Errorf("expected positive wait at ceiling, got %v", wait)
	}

	// Wait should be approximately window (5s) + padding (100ms) since slots
	// were just recorded (near-zero elapsed time).
	expected := 5*time.Second + 100*time.Millisecond
	tolerance := 200 * time.Millisecond
	if wait < expected-tolerance || wait > expected+tolerance {
		t.Errorf("expected wait near %v, got %v", expected, wait)
	}
}

func TestEstimateWait_ExpiredSlots(t *testing.T) {
	cfg := testConfig()
	rl := NewRateLimiter(cfg)

	// Directly inject old slots that are already expired.
	rl.mu.Lock()
	pool := rl.pools["search"]
	past := time.Now().Add(-10 * time.Second) // well past the 5s window
	for i := range pool.tiers {
		pool.tiers[i].slots = []time.Time{past, past, past, past, past}
	}
	rl.mu.Unlock()

	wait := rl.EstimateWait("search")
	if wait != 0 {
		t.Errorf("expected zero wait after expired slots are purged, got %v", wait)
	}

	// Verify slots were actually purged.
	rl.mu.Lock()
	slotCount := len(pool.tiers[0].slots)
	rl.mu.Unlock()
	if slotCount != 0 {
		t.Errorf("expected 0 slots after purge, got %d", slotCount)
	}
}

func TestEstimateWait_MultiTier(t *testing.T) {
	cfg := testConfig()
	rl := NewRateLimiter(cfg)

	// Replace search pool with two tiers: a loose tier and a strict tier.
	rl.mu.Lock()
	pool := rl.pools["search"]
	pool.tiers = []Tier{
		{maxHits: 10, window: 10 * time.Second}, // loose: effective = int(10*0.65) = 6
		{maxHits: 3, window: 5 * time.Second},   // strict: effective = int(3*0.65) = 1
	}
	rl.mu.Unlock()

	// Record 1 hit — should hit the strict tier's ceiling (effective max=1).
	rl.Record("search")

	wait := rl.EstimateWait("search")
	if wait <= 0 {
		t.Errorf("expected positive wait when strict tier is at ceiling, got %v", wait)
	}

	// The wait should come from the strict tier: ~5s window + padding.
	if wait < 4*time.Second {
		t.Errorf("expected wait near 5s from strict tier, got %v", wait)
	}
}

func TestSyncFromHeaders_ParsesTiers(t *testing.T) {
	cfg := testConfig()
	rl := NewRateLimiter(cfg)

	headers := http.Header{}
	headers.Set("X-Rate-Limit-Rules", "account")
	headers.Set("X-Rate-Limit-Account", "5:10:60,15:60:120")
	headers.Set("X-Rate-Limit-Account-State", "0:10:0,0:60:0")

	rl.SyncFromHeaders("search", headers)

	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool := rl.pools["search"]
	if len(pool.tiers) != 2 {
		t.Fatalf("expected 2 tiers after sync, got %d", len(pool.tiers))
	}

	// First tier: 5 req / 10s / 60s penalty.
	if pool.tiers[0].maxHits != 5 {
		t.Errorf("tier 0 maxHits: expected 5, got %d", pool.tiers[0].maxHits)
	}
	if pool.tiers[0].window != 10*time.Second {
		t.Errorf("tier 0 window: expected 10s, got %v", pool.tiers[0].window)
	}
	if pool.tiers[0].penalty != 60*time.Second {
		t.Errorf("tier 0 penalty: expected 60s, got %v", pool.tiers[0].penalty)
	}

	// Second tier: 15 req / 60s / 120s penalty.
	if pool.tiers[1].maxHits != 15 {
		t.Errorf("tier 1 maxHits: expected 15, got %d", pool.tiers[1].maxHits)
	}
	if pool.tiers[1].window != 60*time.Second {
		t.Errorf("tier 1 window: expected 60s, got %v", pool.tiers[1].window)
	}
	if pool.tiers[1].penalty != 120*time.Second {
		t.Errorf("tier 1 penalty: expected 120s, got %v", pool.tiers[1].penalty)
	}
}

func TestSyncFromHeaders_PhantomInjection(t *testing.T) {
	cfg := testConfig()
	rl := NewRateLimiter(cfg)

	headers := http.Header{}
	headers.Set("X-Rate-Limit-Rules", "account")
	headers.Set("X-Rate-Limit-Account", "10:10:60")
	headers.Set("X-Rate-Limit-Account-State", "7:10:0")

	// We have 0 local slots but server says 7 hits. Should inject 7 phantoms.
	rl.SyncFromHeaders("search", headers)

	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool := rl.pools["search"]
	if len(pool.tiers) != 1 {
		t.Fatalf("expected 1 tier, got %d", len(pool.tiers))
	}

	slotCount := len(pool.tiers[0].slots)
	if slotCount != 7 {
		t.Errorf("expected 7 phantom slots, got %d", slotCount)
	}

	// Verify phantom timestamps are within the window and chronologically ordered.
	now := time.Now()
	windowStart := now.Add(-10 * time.Second)
	for i, slot := range pool.tiers[0].slots {
		if slot.Before(windowStart.Add(-time.Second)) || slot.After(now.Add(time.Second)) {
			t.Errorf("phantom slot %d at %v is outside window [%v, %v]", i, slot, windowStart, now)
		}
		if i > 0 && slot.Before(pool.tiers[0].slots[i-1]) {
			t.Errorf("phantom slots not in order: slot %d (%v) before slot %d (%v)",
				i, slot, i-1, pool.tiers[0].slots[i-1])
		}
	}
}

func TestSyncFromHeaders_TierUpdate(t *testing.T) {
	cfg := testConfig()
	rl := NewRateLimiter(cfg)

	// First sync: single tier 5:10:60.
	h1 := http.Header{}
	h1.Set("X-Rate-Limit-Rules", "account")
	h1.Set("X-Rate-Limit-Account", "5:10:60")
	h1.Set("X-Rate-Limit-Account-State", "2:10:0")
	rl.SyncFromHeaders("search", h1)

	rl.mu.Lock()
	if len(rl.pools["search"].tiers) != 1 {
		t.Fatalf("expected 1 tier after first sync, got %d", len(rl.pools["search"].tiers))
	}
	if rl.pools["search"].tiers[0].maxHits != 5 {
		t.Errorf("expected maxHits=5 after first sync, got %d", rl.pools["search"].tiers[0].maxHits)
	}
	rl.mu.Unlock()

	// Second sync: different limits 8:10:30,20:60:120.
	h2 := http.Header{}
	h2.Set("X-Rate-Limit-Rules", "account")
	h2.Set("X-Rate-Limit-Account", "8:10:30,20:60:120")
	h2.Set("X-Rate-Limit-Account-State", "1:10:0,3:60:0")
	rl.SyncFromHeaders("search", h2)

	rl.mu.Lock()
	defer rl.mu.Unlock()

	pool := rl.pools["search"]
	if len(pool.tiers) != 2 {
		t.Fatalf("expected 2 tiers after second sync, got %d", len(pool.tiers))
	}
	if pool.tiers[0].maxHits != 8 {
		t.Errorf("tier 0 maxHits: expected 8, got %d", pool.tiers[0].maxHits)
	}
	if pool.tiers[1].maxHits != 20 {
		t.Errorf("tier 1 maxHits: expected 20, got %d", pool.tiers[1].maxHits)
	}
}

func TestRecord_PoolIndependence(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// Record 3 hits to search (reaches ceiling), none to fetch.
	rl.Record("search")
	rl.Record("search")
	rl.Record("search")

	searchWait := rl.EstimateWait("search")
	fetchWait := rl.EstimateWait("fetch")

	if searchWait <= 0 {
		t.Errorf("expected positive wait for search pool at ceiling, got %v", searchWait)
	}
	if fetchWait != 0 {
		t.Errorf("expected zero wait for untouched fetch pool, got %v", fetchWait)
	}
}

func TestConcurrentAccess(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	var wg sync.WaitGroup
	goroutines := 50

	// Half the goroutines record, half estimate wait — concurrently.
	for i := 0; i < goroutines; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			pool := "search"
			if n%3 == 0 {
				pool = "fetch"
			}
			if n%2 == 0 {
				rl.Record(pool)
			} else {
				rl.EstimateWait(pool)
			}
		}(i)
	}

	// Also run SyncFromHeaders concurrently.
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			headers := http.Header{}
			headers.Set("X-Rate-Limit-Rules", "account")
			headers.Set("X-Rate-Limit-Account", "5:10:60")
			headers.Set("X-Rate-Limit-Account-State", "2:10:0")
			rl.SyncFromHeaders("search", headers)
		}()
	}

	wg.Wait()
	// If we get here without a race detector complaint, the test passes.
}

func TestEstimateWait_UnknownPool(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	wait := rl.EstimateWait("nonexistent")
	if wait != 0 {
		t.Errorf("expected zero wait for unknown pool, got %v", wait)
	}
}

func TestRecord_UnknownPool(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// Must not panic.
	rl.Record("nonexistent")
}

func TestSyncFromHeaders_EmptyRules(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// No X-Rate-Limit-Rules header — should be a no-op.
	rl.SyncFromHeaders("search", http.Header{})

	rl.mu.Lock()
	defer rl.mu.Unlock()

	// Pool should still have original single tier.
	pool := rl.pools["search"]
	if len(pool.tiers) != 1 {
		t.Errorf("expected 1 tier unchanged, got %d", len(pool.tiers))
	}
}

func TestSyncFromHeaders_MismatchedLimitState(t *testing.T) {
	rl := NewRateLimiter(testConfig())

	// 2 limit tiers but only 1 state tier — should skip.
	headers := http.Header{}
	headers.Set("X-Rate-Limit-Rules", "account")
	headers.Set("X-Rate-Limit-Account", "5:10:60,15:60:120")
	headers.Set("X-Rate-Limit-Account-State", "2:10:0")
	rl.SyncFromHeaders("search", headers)

	rl.mu.Lock()
	defer rl.mu.Unlock()

	// Should remain unchanged.
	pool := rl.pools["search"]
	if pool.tiers[0].maxHits != 5 {
		t.Errorf("expected original maxHits=5 unchanged, got %d", pool.tiers[0].maxHits)
	}
}

// fixedLimiterClock is set before any goroutine uses the limiter. These tests
// advance it synchronously; concurrent production calls use time.Now.
func fixedLimiterClock(rl *RateLimiter) *time.Time {
	now := time.Date(2026, 10, 14, 12, 0, 0, 0, time.UTC)
	rl.now = func() time.Time { return now }
	return &now
}

func TestObserveResponse_RetryAfterBlocksBothPools(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("Retry-After", "120")
	rl.ObserveResponse("search", headers, ErrRateLimited)
	for _, pool := range []string{"search", "fetch"} {
		if got := rl.EstimateWait(pool); got != 120*time.Second {
			t.Errorf("%s may send before Retry-After: wait = %v, want 2m", pool, got)
		}
	}
}

func TestObserveResponse_RestrictionExpiryReopensBothPools(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("Retry-After", "120")
	rl.ObserveResponse("search", headers, ErrRateLimited)
	*now = now.Add(120 * time.Second)
	for _, pool := range []string{"search", "fetch"} {
		if got := rl.EstimateWait(pool); got != 0 {
			t.Errorf("%s did not resume at restriction expiry: wait = %v", pool, got)
		}
	}
}

func TestObserveResponse_HTTPDateRestriction(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("Retry-After", now.Add(95*time.Second).Format(http.TimeFormat))
	rl.ObserveResponse("fetch", headers, ErrRateLimited)
	if got := rl.EstimateWait("search"); got != 95*time.Second {
		t.Errorf("HTTP-date wait = %v, want 95s", got)
	}
}

func TestObserveResponse_RestrictionWithoutTierDefinitions(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("X-Rate-Limit-Rules", "account, ip")
	headers.Set("X-Rate-Limit-Account", "5:10:60")
	headers.Set("X-Rate-Limit-Account-State", "0:10:0")
	// Missing IP limit definition must not hide the invalid-request ban.
	headers.Set("X-Rate-Limit-Ip-State", "0:60:1616")
	headers.Set("Retry-After", "10")
	rl.ObserveResponse("fetch", headers, ErrRateLimited)
	if got := rl.EstimateWait("search"); got != 1616*time.Second {
		t.Errorf("second rule ban wait = %v, want 1616s", got)
	}
}

func TestSyncFromHeaders_UnannouncedStateBlocksSends(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("X-Rate-Limit-Ip-State", "0:60:45")
	rl.SyncFromHeaders("search", headers)
	if got := rl.EstimateWait("fetch"); got != 45*time.Second {
		t.Errorf("restriction without Rules header wait = %v, want 45s", got)
	}
}

func TestObserveResponse_ShorterRestrictionKeepsOriginalDeadline(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("Retry-After", "120")
	rl.ObserveResponse("search", headers, ErrRateLimited)
	*now = now.Add(30 * time.Second)
	headers.Set("Retry-After", "10")
	rl.ObserveResponse("fetch", headers, ErrRateLimited)
	if got := rl.EstimateWait("search"); got != 90*time.Second {
		t.Errorf("shorter response changed active deadline: wait = %v, want 90s", got)
	}
}

func TestObserveResponse_InvalidRetryAfterUsesFiniteFallback(t *testing.T) {
	for _, value := range []string{"", "broken", "-1", "0", "9223372036854775807", "Wed, 01 Jan 2020 00:00:00 GMT"} {
		t.Run(value, func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			now := fixedLimiterClock(rl)
			headers := http.Header{}
			headers.Set("Retry-After", value)
			rl.ObserveResponse("search", headers, ErrRateLimited)
			if got := rl.EstimateWait("fetch"); got != time.Minute {
				t.Fatalf("unusable Retry-After %q: wait = %v, want fallback 1m", value, got)
			}
			*now = now.Add(time.Minute)
			if got := rl.EstimateWait("fetch"); got != 0 {
				t.Errorf("fallback did not expire: wait = %v", got)
			}
		})
	}
}

func TestObserveResponse_SuccessWithoutRestrictionLeavesBudgetOpen(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("Retry-After", "invalid")
	headers.Set("X-Rate-Limit-Ip-State", "-1:10:60")
	rl.ObserveResponse("search", headers, nil)
	if got := rl.EstimateWait("fetch"); got != 0 {
		t.Errorf("invalid restriction blocked unrelated pool: wait = %v", got)
	}
}

func TestSyncFromHeaders_AllRulesConstrainBudget(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	headers := http.Header{}
	headers.Set("X-Rate-Limit-Rules", "account, ip")
	headers.Set("X-Rate-Limit-Account", "100:10:60")
	headers.Set("X-Rate-Limit-Account-State", "0:10:0")
	headers.Set("X-Rate-Limit-Ip", "1:60:120")
	headers.Set("X-Rate-Limit-Ip-State", "0:60:0")
	rl.SyncFromHeaders("search", headers)
	rl.Record("search")
	if got := rl.EstimateWait("search"); got != 60*time.Second+100*time.Millisecond {
		t.Errorf("second rule did not constrain send: wait = %v, want 1m0.1s", got)
	}
}

func TestEstimateWait_OverCeilingWaitsForEnoughSlots(t *testing.T) {
	cfg := testConfig()
	cfg.CeilingFactor = 1
	cfg.DefaultSearchRate = 2
	cfg.LatencyPadding = 0
	rl := NewRateLimiter(cfg)
	now := fixedLimiterClock(rl)
	rl.Record("search")
	*now = now.Add(time.Second)
	rl.Record("search")
	*now = now.Add(time.Second)
	rl.Record("search")
	// Three active slots need two expirations to fall below the ceiling of 2.
	if got := rl.EstimateWait("search"); got != 4*time.Second {
		t.Fatalf("over-ceiling wait = %v, want 4s until second slot expires", got)
	}
	*now = now.Add(4 * time.Second)
	if got := rl.EstimateWait("search"); got != 0 {
		t.Errorf("budget should reopen at second expiry: wait = %v", got)
	}
}

func TestEstimateWait_PaddingRemainsUntilSendDeadline(t *testing.T) {
	cfg := testConfig()
	cfg.DefaultSearchRate = 1
	rl := NewRateLimiter(cfg)
	now := fixedLimiterClock(rl)
	rl.Record("search")
	*now = now.Add(5 * time.Second)
	if got := rl.EstimateWait("search"); got != cfg.LatencyPadding {
		t.Fatalf("padding lost at window boundary: wait = %v, want %v", got, cfg.LatencyPadding)
	}
	*now = now.Add(cfg.LatencyPadding)
	if got := rl.EstimateWait("search"); got != 0 {
		t.Errorf("send blocked at padded expiry: wait = %v", got)
	}
}

func TestObserveResponse_HeaderlessAccessRestrictionBlocksBothPools(t *testing.T) {
	for _, status := range []int{401, 403, 429} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			fixedLimiterClock(rl)
			err := fmt.Errorf("lookup failed: %w", &HTTPStatusError{StatusCode: status, Operation: "search"})
			rl.ObserveResponse("search", nil, err)
			for _, pool := range []string{"search", "fetch"} {
				if got := rl.EstimateWait(pool); got != time.Minute {
					t.Errorf("headerless status %d: %s wait = %v, want 1m", status, pool, got)
				}
			}
		})
	}
}

func TestObserveResponse_Headerless403FallbackExpires(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	rl.ObserveResponse("fetch", nil, &HTTPStatusError{StatusCode: http.StatusForbidden, Operation: "fetch"})
	*now = now.Add(time.Minute)
	for _, pool := range []string{"search", "fetch"} {
		if got := rl.EstimateWait(pool); got != 0 {
			t.Errorf("headerless 403 fallback did not reopen %s: wait = %v", pool, got)
		}
	}
}

func TestObserveResponse_Non4xxErrorDoesNotRestrictBothPools(t *testing.T) {
	for _, err := range []error{
		&HTTPStatusError{StatusCode: 399, Operation: "search"},
		&HTTPStatusError{StatusCode: 500, Operation: "search"},
		errors.New("transport failed"),
	} {
		t.Run(err.Error(), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			fixedLimiterClock(rl)
			rl.ObserveResponse("search", nil, err)
			for _, pool := range []string{"search", "fetch"} {
				if got := rl.EstimateWait(pool); got != 0 {
					t.Errorf("error %v incorrectly restricted %s: wait = %v", err, pool, got)
				}
			}
		})
	}
}

func TestObserveResponse_FallbackUsesLongerTierWindow(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	rl.SyncFromHeaders("search", http.Header{
		"X-Rate-Limit-Rules":    {"ip"},
		"X-Rate-Limit-Ip":       {"100:120:20"},
		"X-Rate-Limit-Ip-State": {"0:120:0"},
	})
	rl.ObserveResponse("search", nil, ErrRateLimited)
	if got := rl.EstimateWait("fetch"); got != 2*time.Minute {
		t.Errorf("fallback wait = %v, want 2m from known search window", got)
	}
}

func TestObserveResponse_FallbackUsesLongerTierPenalty(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	rl.SyncFromHeaders("search", http.Header{
		"X-Rate-Limit-Rules":    {"ip"},
		"X-Rate-Limit-Ip":       {"100:5:120"},
		"X-Rate-Limit-Ip-State": {"0:5:0"},
	})
	rl.ObserveResponse("search", nil, ErrRateLimited)
	if got := rl.EstimateWait("fetch"); got != 2*time.Minute {
		t.Errorf("fallback wait = %v, want 2m from known search penalty", got)
	}
}

func TestObserveResponse_FallbackCapsAtThirtyMinutes(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	rl.SyncFromHeaders("search", http.Header{
		"X-Rate-Limit-Rules":    {"ip"},
		"X-Rate-Limit-Ip":       {"100:7200:7200"},
		"X-Rate-Limit-Ip-State": {"0:7200:0"},
	})
	rl.ObserveResponse("search", nil, ErrRateLimited)
	if got := rl.EstimateWait("fetch"); got != 30*time.Minute {
		t.Errorf("fallback wait = %v, want 30m cap despite known 2h window/penalty", got)
	}
}

func TestObserveResponse_SingleRequestSpecific4xxDoesNotBlockBothPools(t *testing.T) {
	for _, status := range []int{400, 404, 422, 499} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			fixedLimiterClock(rl)
			rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: status, Operation: "search"})
			for _, pool := range []string{"search", "fetch"} {
				if got := rl.EstimateWait(pool); got != 0 {
					t.Errorf("single status %d restricted %s: wait = %v", status, pool, got)
				}
			}
		})
	}
}

func TestObserveResponse_FifthInvalidRequestTripsFloodBreaker(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	for i := 0; i < 5; i++ {
		rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 400, Operation: "search"})
		want := time.Duration(0)
		if i == 4 {
			want = time.Minute
		}
		for _, pool := range []string{"search", "fetch"} {
			if got := rl.EstimateWait(pool); got != want {
				t.Errorf("invalid request %d: %s wait = %v, want %v", i+1, pool, got, want)
			}
		}
		*now = now.Add(time.Minute)
	}
}

func TestObserveResponse_SpreadInvalidRequestsDoNotTripFloodBreaker(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	for i := 0; i < 5; i++ {
		rl.ObserveResponse("fetch", nil, &HTTPStatusError{StatusCode: 404, Operation: "fetch"})
		if got := rl.EstimateWait("search"); got != 0 {
			t.Fatalf("invalid request %d spread 11m apart triggered wait %v", i+1, got)
		}
		*now = now.Add(11 * time.Minute)
	}
}

func TestObserveResponse_FloodWindowBoundaryExcludesOldRequests(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	for i := 0; i < 4; i++ {
		rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 422, Operation: "search"})
	}
	*now = now.Add(10 * time.Minute)
	rl.ObserveResponse("fetch", nil, &HTTPStatusError{StatusCode: 404, Operation: "fetch"})
	if got := rl.EstimateWait("search"); got != 0 {
		t.Errorf("requests exactly 10m old counted toward breaker: wait = %v", got)
	}
}

func TestObserveResponse_FloodWindowJustInsideBoundaryTripsBreaker(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	for i := 0; i < 4; i++ {
		rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 400, Operation: "search"})
	}
	*now = now.Add(10*time.Minute - time.Nanosecond)
	rl.ObserveResponse("fetch", nil, &HTTPStatusError{StatusCode: 404, Operation: "fetch"})
	if got := rl.EstimateWait("search"); got != time.Minute {
		t.Errorf("five requests just within 10m: wait = %v, want 1m", got)
	}
}

func TestObserveResponse_InvalidFloodUsesSlidingWindow(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	invalid := &HTTPStatusError{StatusCode: 400, Operation: "search"}
	rl.ObserveResponse("search", nil, invalid)
	*now = now.Add(5 * time.Minute)
	for i := 0; i < 3; i++ {
		rl.ObserveResponse("search", nil, invalid)
	}
	*now = now.Add(5 * time.Minute)
	rl.ObserveResponse("search", nil, invalid)
	if got := rl.EstimateWait("fetch"); got != 0 {
		t.Fatalf("first expired event was still counted: wait = %v", got)
	}
	*now = now.Add(time.Nanosecond)
	rl.ObserveResponse("search", nil, invalid)
	if got := rl.EstimateWait("fetch"); got != time.Minute {
		t.Errorf("five surviving events across initial window boundary: wait = %v, want 1m", got)
	}
}

func TestObserveResponse_InvalidFloodCountsHeaderlessAccessRefusals(t *testing.T) {
	for _, status := range []int{401, 403} {
		t.Run(fmt.Sprint(status), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			now := fixedLimiterClock(rl)
			rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: status, Operation: "search"})
			*now = now.Add(time.Minute)
			for i := 0; i < 3; i++ {
				rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 400, Operation: "search"})
			}
			if got := rl.EstimateWait("fetch"); got != 0 {
				t.Fatalf("four invalid requests triggered breaker: wait = %v", got)
			}
			rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 400, Operation: "search"})
			if got := rl.EstimateWait("fetch"); got != time.Minute {
				t.Errorf("headerless %d did not contribute to fifth invalid request: wait = %v, want 1m", status, got)
			}
		})
	}
}

func TestObserveResponse_InvalidFloodExcludes429(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	rl.ObserveResponse("search", nil, ErrRateLimited)
	*now = now.Add(time.Minute)
	for i := 0; i < 4; i++ {
		rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 400, Operation: "search"})
	}
	if got := rl.EstimateWait("fetch"); got != 0 {
		t.Errorf("429 counted toward non-429 flood threshold: wait = %v", got)
	}
}

func TestObserveResponse_InvalidFloodExcludesExplicitRestrictions(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	invalid := &HTTPStatusError{StatusCode: 400, Operation: "search"}
	headers := http.Header{}
	headers.Set("Retry-After", "1")
	rl.ObserveResponse("search", headers, invalid)
	*now = now.Add(time.Second)
	for i := 0; i < 4; i++ {
		rl.ObserveResponse("search", nil, invalid)
	}
	if got := rl.EstimateWait("fetch"); got != 0 {
		t.Errorf("explicitly restricted response counted toward headerless flood: wait = %v", got)
	}
}

func TestObserveResponse_InvalidFloodExcludesNon4xxErrors(t *testing.T) {
	for _, err := range []error{nil, errors.New("transport failed"), &HTTPStatusError{StatusCode: 399}, &HTTPStatusError{StatusCode: 500}} {
		t.Run(fmt.Sprint(err), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			fixedLimiterClock(rl)
			for i := 0; i < 5; i++ {
				rl.ObserveResponse("search", nil, err)
			}
			if got := rl.EstimateWait("fetch"); got != 0 {
				t.Errorf("non-4xx responses triggered invalid-request flood: wait = %v", got)
			}
		})
	}
}

func TestBlockedUntil_ReportsGlobalRestrictionDeadline(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 403})
	if got, want := rl.BlockedUntil(), now.Add(time.Minute); !got.Equal(want) {
		t.Errorf("global restriction deadline = %v, want %v", got, want)
	}
}

func TestBlockedUntil_ExpiredRestrictionReturnsZero(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 403})
	*now = now.Add(time.Minute)
	if got := rl.BlockedUntil(); !got.IsZero() {
		t.Errorf("expired restriction deadline = %v, want zero", got)
	}
}

func TestBlockedUntil_LocalBudgetDoesNotReportGlobalRestriction(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	fixedLimiterClock(rl)
	for i := 0; i < 3; i++ {
		rl.Record("search")
	}
	if got := rl.BlockedUntil(); !got.IsZero() {
		t.Errorf("local search budget reported global restriction deadline = %v", got)
	}
}

func TestRestrictionDeadlineAfter_RetainsExpiredOverlappingRestriction(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	submittedAt := *now
	rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 403})
	deadline := now.Add(time.Minute)
	*now = now.Add(2 * time.Minute)
	if got := rl.restrictionDeadlineAfter(submittedAt); !got.Equal(deadline) {
		t.Errorf("expired overlapping restriction deadline = %v, want %v", got, deadline)
	}
}

func TestRestrictionDeadlineAfter_ExcludesDeadlinesAtOrBeforeSubmission(t *testing.T) {
	for _, offset := range []time.Duration{0, time.Nanosecond} {
		t.Run(offset.String(), func(t *testing.T) {
			rl := NewRateLimiter(testConfig())
			now := fixedLimiterClock(rl)
			rl.ObserveResponse("search", nil, &HTTPStatusError{StatusCode: 403})
			submittedAt := now.Add(time.Minute + offset)
			*now = now.Add(2 * time.Minute)
			if got := rl.restrictionDeadlineAfter(submittedAt); !got.IsZero() {
				t.Errorf("restriction predating submission returned deadline = %v, want zero", got)
			}
		})
	}
}

func TestRestrictionDeadlineAfter_NoRestrictionReturnsZero(t *testing.T) {
	rl := NewRateLimiter(testConfig())
	now := fixedLimiterClock(rl)
	if got := rl.restrictionDeadlineAfter(*now); !got.IsZero() {
		t.Errorf("unrestricted limiter returned deadline = %v, want zero", got)
	}
}
