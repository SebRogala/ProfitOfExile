package trade

import (
	"strings"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestFixedTargets_RegistryIdentitiesInOrder(t *testing.T) {
	got := FixedTargets()
	want := []struct {
		gem  string
		role FixedTargetRole
	}{
		{"font-lens-60-outcome", FixedTargetRoleOutcome},
		{"font-lens-60-feeder", FixedTargetRoleFeeder},
	}
	if len(got) != len(want) {
		t.Fatalf("registry has %d targets, want %d", len(got), len(want))
	}
	for i, w := range want {
		if got[i].Gem != w.gem || got[i].Variant != "font-craft" || got[i].Role != w.role {
			t.Errorf("target %d = %s|%s %s, want %s|font-craft %s", i, got[i].Gem, got[i].Variant, got[i].Role, w.gem, w.role)
		}
		if strings.Contains(got[i].Gem, " of ") {
			t.Errorf("identity %q contains \" of \" and would collide with the transfigured filter", got[i].Gem)
		}
		if got[i].Key() != CacheKey(w.gem, "font-craft") {
			t.Errorf("key = %q, want CacheKey of the identity", got[i].Key())
		}
	}
}

func TestLookupFixedTarget_RejectsIdentitiesOutsideRegistry(t *testing.T) {
	for _, tc := range []struct{ gem, variant string }{
		{"font-lens-60-outcome", "20/20"},
		{"Empower Support", "font-craft"},
		{"font-lens-30-outcome", "font-craft"},
	} {
		if _, ok := LookupFixedTarget(tc.gem, tc.variant); ok {
			t.Errorf("LookupFixedTarget(%q, %q) found a target", tc.gem, tc.variant)
		}
		if IsFixedTargetKey(CacheKey(tc.gem, tc.variant)) {
			t.Errorf("IsFixedTargetKey(%q|%q) = true", tc.gem, tc.variant)
		}
	}
	if !IsFixedTargetKey(CacheKey(FixedTargetLens60Feeder, FixedTargetVariantFont)) {
		t.Error("feeder key not recognised as a fixed target")
	}
}

func newSchedulerForTest() (*FixedTargetScheduler, *TradeCache) {
	scope := league.Historical("Mirage")
	cache := NewTradeCache(10, scope)
	return NewFixedTargetScheduler(cache, scope), cache
}

func claimGem(t *testing.T, s *FixedTargetScheduler, now time.Time) string {
	t.Helper()
	target, ok := s.Claim(now)
	if !ok {
		return ""
	}
	return target.Gem
}

func TestFixedTargetScheduler_EmptyCacheClaimsLensThenSupportThenNothing(t *testing.T) {
	s, _ := newSchedulerForTest()
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	got := []string{claimGem(t, s, now), claimGem(t, s, now), claimGem(t, s, now)}
	want := []string{FixedTargetLens60Outcome, FixedTargetLens60Feeder, ""}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("claims = %q, want %q", got, want)
		}
	}
}

func TestFixedTargetScheduler_QuoteYoungerThanMinAgeIsNotDue(t *testing.T) {
	s, cache := newSchedulerForTest()
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	for _, target := range FixedTargets() {
		cache.Set(target.Key(), &TradeLookupResult{Gem: target.Gem, Variant: target.Variant, Total: 5,
			FetchedAt: now.Add(-FixedTargetMinAge + time.Second)})
	}
	if gem := claimGem(t, s, now); gem != "" {
		t.Fatalf("claimed %q with both quotes younger than %v", gem, FixedTargetMinAge)
	}
}

func TestFixedTargetScheduler_QuoteExactlyMinAgeOldIsDue(t *testing.T) {
	s, cache := newSchedulerForTest()
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	cache.Set(CacheKey(FixedTargetLens60Outcome, FixedTargetVariantFont), &TradeLookupResult{Total: 5, FetchedAt: now.Add(-time.Minute)})
	cache.Set(CacheKey(FixedTargetLens60Feeder, FixedTargetVariantFont), &TradeLookupResult{Total: 5, FetchedAt: now.Add(-FixedTargetMinAge)})
	if gem := claimGem(t, s, now); gem != FixedTargetLens60Feeder {
		t.Fatalf("claimed %q, want the support whose quote is exactly %v old", gem, FixedTargetMinAge)
	}
}

func TestFixedTargetScheduler_EmptyResultObeysMinAge(t *testing.T) {
	s, cache := newSchedulerForTest()
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	for _, target := range FixedTargets() {
		cache.Set(target.Key(), &TradeLookupResult{Total: 0, FetchedAt: now.Add(-10 * time.Minute)})
	}
	if gem := claimGem(t, s, now); gem != "" {
		t.Fatalf("claimed %q although both empty results are 10 minutes old", gem)
	}
	if gem := claimGem(t, s, now.Add(20*time.Minute)); gem != FixedTargetLens60Outcome {
		t.Fatalf("claimed %q at 30 minutes, want the lens", gem)
	}
}

func TestFixedTargetScheduler_NeverCheckedTargetGoesBeforeStaleOne(t *testing.T) {
	s, cache := newSchedulerForTest()
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	cache.Set(CacheKey(FixedTargetLens60Outcome, FixedTargetVariantFont), &TradeLookupResult{Total: 5, FetchedAt: now.Add(-2 * time.Hour)})
	if gem := claimGem(t, s, now); gem != FixedTargetLens60Feeder {
		t.Fatalf("claimed %q, want the never-checked support", gem)
	}
}

func TestFixedTargetScheduler_FailedClaimBacksOffWithoutStarvingTheOther(t *testing.T) {
	s, _ := newSchedulerForTest()
	t0 := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	// The lens lookup fails: nothing is cached, only the claim remains.
	if gem := claimGem(t, s, t0); gem != FixedTargetLens60Outcome {
		t.Fatalf("first claim = %q, want the lens", gem)
	}
	if gem := claimGem(t, s, t0.Add(time.Minute)); gem != FixedTargetLens60Feeder {
		t.Fatalf("second claim = %q, want the support", gem)
	}
	if gem := claimGem(t, s, t0.Add(FixedTargetMinAge-time.Second)); gem != "" {
		t.Fatalf("claimed %q within %v of the failed claim", gem, FixedTargetMinAge)
	}
	if gem := claimGem(t, s, t0.Add(FixedTargetMinAge)); gem != FixedTargetLens60Outcome {
		t.Fatalf("retry claim = %q, want the lens after %v", gem, FixedTargetMinAge)
	}
}
