package trade

import (
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
