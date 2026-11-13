package trade

import (
	"context"
	"sync"
	"time"

	"profitofexile/internal/league"
)

// FixedTargetRole tells the outcome of a craft from its input (the feeder).
type FixedTargetRole string

const (
	FixedTargetRoleOutcome FixedTargetRole = "outcome"
	FixedTargetRoleFeeder  FixedTargetRole = "feeder"
)

// Fixed target identities are server constants stored in the Gem/Variant
// columns. They are never valid gem names and never contain " of ", so they
// cannot collide with gem lookups, the public validators or the trade tick's
// transfigured filter (POE-329 H2).
const (
	FixedTargetLens60Outcome = "font-lens-60-outcome"
	FixedTargetLens60Feeder  = "font-lens-60-feeder"
	FixedTargetVariantFont   = "font-craft"
)

// FixedTarget is one non-gem item the server prices on the trade queue. Gem
// and Variant are its registry identity; the cache, dedup and persistence key
// is CacheKey(Gem, Variant).
type FixedTarget struct {
	Gem     string
	Variant string
	Role    FixedTargetRole
	query   func() map[string]interface{}
}

// Key returns the cache/dedup/persistence key of the target.
func (t FixedTarget) Key() string { return CacheKey(t.Gem, t.Variant) }

// fixedTargets is the closed registry, in scheduling order. The queries are
// the ticket's decoded trade searches (POE-329); the support query keeps the
// ticket's filters without a gem type (Supervisor ruling).
var fixedTargets = []FixedTarget{
	{
		Gem: FixedTargetLens60Outcome, Variant: FixedTargetVariantFont, Role: FixedTargetRoleOutcome,
		query: func() map[string]interface{} {
			return map[string]interface{}{
				"type": "Facetor's Lens",
				"filters": map[string]interface{}{
					"misc_filters": map[string]interface{}{
						"filters": map[string]interface{}{
							"stored_experience": map[string]interface{}{"min": 999627000},
						},
					},
				},
			}
		},
	},
	{
		Gem: FixedTargetLens60Feeder, Variant: FixedTargetVariantFont, Role: FixedTargetRoleFeeder,
		query: func() map[string]interface{} {
			return map[string]interface{}{
				"filters": map[string]interface{}{
					"type_filters": map[string]interface{}{
						"filters": map[string]interface{}{
							"category": map[string]string{"option": "gem.supportgem"},
						},
					},
					"req_filters": map[string]interface{}{
						"filters": map[string]interface{}{
							"lvl": map[string]interface{}{"min": 76},
						},
					},
					"misc_filters": map[string]interface{}{
						"filters": map[string]interface{}{
							"gem_level": map[string]interface{}{"min": 3},
						},
					},
				},
			}
		},
	},
}

// FixedTargets returns the registry in scheduling order.
func FixedTargets() []FixedTarget {
	return append([]FixedTarget(nil), fixedTargets...)
}

// LookupFixedTarget returns the registry entry with the given identity.
func LookupFixedTarget(gem, variant string) (FixedTarget, bool) {
	for _, t := range fixedTargets {
		if t.Gem == gem && t.Variant == variant {
			return t, true
		}
	}
	return FixedTarget{}, false
}

// IsFixedTargetKey reports whether a cache key belongs to the registry.
func IsFixedTargetKey(key string) bool {
	gem, variant := ParseCacheKey(key)
	_, ok := LookupFixedTarget(gem, variant)
	return ok
}

// FixedTargetMinAge is how old a fixed target's quote, or its last claim, must
// be before the trade tick refreshes it again (POE-329 D2).
const FixedTargetMinAge = 30 * time.Minute

// FixedTargetScheduler decides which fixed target the trade tick refreshes.
// Overlapping tick goroutines share one scheduler, so a claim is recorded
// before the target is enqueued; the claim time, not FetchedAt, is the retry
// backoff after a failed lookup or a rejected submit.
type FixedTargetScheduler struct {
	cache  *TradeCache
	scope  league.Scope
	mu     sync.Mutex
	claims map[string]time.Time
}

// NewFixedTargetScheduler binds a scheduler to the trade cache of scope.
func NewFixedTargetScheduler(cache *TradeCache, scope league.Scope) *FixedTargetScheduler {
	return &FixedTargetScheduler{cache: cache, scope: scope, claims: make(map[string]time.Time)}
}

// Claim selects the due fixed target that was checked longest ago (never
// checked first, ties in registry order), records the claim at now and returns
// it. It returns false when no target is due.
func (s *FixedTargetScheduler) Claim(now time.Time) (FixedTarget, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()

	var (
		picked   FixedTarget
		pickedAt time.Time
		found    bool
		cache    = s.cache.For(s.scope)
	)
	for _, t := range fixedTargets {
		key := t.Key()
		last := s.claims[key]
		if r, ok := cache.Get(key); ok && r.FetchedAt.After(last) {
			last = r.FetchedAt
		}
		if !last.IsZero() && now.Sub(last) < FixedTargetMinAge {
			continue
		}
		if !found || last.Before(pickedAt) {
			picked, pickedAt, found = t, last, true
		}
	}
	if found {
		s.claims[picked.Key()] = now
	}
	return picked, found
}

// SubmitFixedTargetRefresh enqueues a low-priority lookup of target and blocks
// until the gate responds or ctx is cancelled. Logs the outcome.
func SubmitFixedTargetRefresh(ctx context.Context, gate *Gate, target FixedTarget) {
	submitLowRefresh(ctx, gate, target.Gem, target.Variant, true)
}
