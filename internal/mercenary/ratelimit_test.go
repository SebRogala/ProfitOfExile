package mercenary

import (
	"testing"
	"time"
)

// withClock replaces the limiter's clock so refill can be observed without
// sleeping.
func withClock(l *RateLimiter, at *time.Time) *RateLimiter {
	l.now = func() time.Time { return *at }
	return l
}

func TestRateLimiter_SpendsExactlyTheBudgetBeforeRefusing(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(3, time.Minute, 8), &at)

	for i := 1; i <= 3; i++ {
		if ok, _ := limiter.Allow("device-a"); !ok {
			t.Fatalf("request %d of a 3-request budget was refused", i)
		}
	}

	ok, retryAfter := limiter.Allow("device-a")
	if ok {
		t.Fatal("the 4th request of a 3-request budget was allowed")
	}
	if retryAfter <= 0 {
		t.Errorf("retryAfter = %v on refusal, want a positive wait", retryAfter)
	}
}

// The budget refills continuously, not in a step at the window edge: after a
// third of a 3-per-minute window exactly one request is available again.
func TestRateLimiter_RefillsProportionallyToElapsedTime(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(3, time.Minute, 8), &at)

	for i := 0; i < 3; i++ {
		limiter.Allow("device-a")
	}
	at = at.Add(20 * time.Second)

	if ok, _ := limiter.Allow("device-a"); !ok {
		t.Fatal("no token available after a third of the window elapsed")
	}
	if ok, _ := limiter.Allow("device-a"); ok {
		t.Fatal("a second token was available after only one had refilled")
	}
}

func TestRateLimiter_RefillNeverExceedsTheBudget(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(2, time.Minute, 8), &at)

	limiter.Allow("device-a")
	at = at.Add(24 * time.Hour)

	if ok, _ := limiter.Allow("device-a"); !ok {
		t.Fatal("first request after a long idle was refused")
	}
	if ok, _ := limiter.Allow("device-a"); !ok {
		t.Fatal("second request after a long idle was refused")
	}
	if ok, _ := limiter.Allow("device-a"); ok {
		t.Fatal("a day of idling granted more than the 2-request budget")
	}
}

// One device exhausting its budget must not spend anyone else's.
func TestRateLimiter_BudgetsAreIndependentPerDevice(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(1, time.Minute, 8), &at)

	limiter.Allow("device-a")
	if ok, _ := limiter.Allow("device-a"); ok {
		t.Fatal("device-a spent more than its budget")
	}
	if ok, _ := limiter.Allow("device-b"); !ok {
		t.Fatal("device-b was refused because device-a had spent its own budget")
	}
}

// The bucket map is bounded, so a flood of spoofed fingerprints cannot grow
// memory without limit. A newcomer arriving at a full map of ACTIVE spenders is
// refused rather than admitted untracked.
func TestRateLimiter_FullMapOfActiveDevices_RefusesNewcomers(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(4, time.Minute, 1), &at)

	if ok, _ := limiter.Allow("device-a"); !ok {
		t.Fatal("the first device was refused")
	}
	if ok, retryAfter := limiter.Allow("device-b"); ok || retryAfter <= 0 {
		t.Fatalf("newcomer at a full map: allowed=%v retryAfter=%v, want refused with a wait", ok, retryAfter)
	}
}

// A device that stopped spending frees its slot: once its bucket is back at
// full budget it is evicted to make room, so the bound does not lock the
// limiter to whichever devices arrived first.
func TestRateLimiter_FullMapOfIdleDevices_EvictsToAdmitNewcomers(t *testing.T) {
	at := time.Date(2026, 8, 25, 12, 0, 0, 0, time.UTC)
	limiter := withClock(NewRateLimiter(4, time.Minute, 1), &at)

	limiter.Allow("device-a")
	at = at.Add(time.Hour) // device-a's bucket refills to full: it is idle

	if ok, _ := limiter.Allow("device-b"); !ok {
		t.Fatal("newcomer refused although the only tracked device was idle")
	}
	if len(limiter.buckets) != 1 {
		t.Errorf("tracked devices = %d, want 1 (the idle one evicted)", len(limiter.buckets))
	}
}
