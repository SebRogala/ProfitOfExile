package trade

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
