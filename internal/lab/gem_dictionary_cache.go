package lab

import (
	"context"
	"fmt"

	"profitofexile/internal/league"
)

// gemDictionarySource is the seed read the OCR dictionary cache needs. The
// repository satisfies it; tests substitute a fake so the seed-then-union
// behaviour can be exercised without a database.
type gemDictionarySource interface {
	GemNameDictionary(ctx context.Context, scope league.Scope, transfigured bool) ([]string, error)
}

// populateGemDictionary keeps the OCR gem-name dictionary warm: seeded once from
// the repository, then grown by unioning each tick's snapshot names into it.
//
// The endpoint's own semantics are what make this a cache field rather than
// static data. GemNameDictionary unions gem_colors — which the collector's
// colour resolver writes to as it meets names (ADR-006) — with every name this
// league's snapshots have carried, so its answer is league-scoped, market-derived
// and monotonically growing. "Names this league's market has shown" is exactly
// what the tick observes, so it can extend the answer without re-asking.
//
// The two halves are split by the snapshot's own is_transfigured, the same
// authority the query's `WHERE is_transfigured = $2` uses — never by the base-name
// rule FilterGemDictionary applies, which exists for gem_colors' unflagged names
// and would re-classify against the wrong universe here. The eligibility rules
// this applies match the query's exactly (gemDictionaryNames) — the Heist rule
// on both halves, the support strip on the skill half only, for the reasons
// FilterGemDictionary records.
// TestGemNameDictionary_includesLeagueNamesMissingFromGemColors guards the
// snapshot half of that union in the repository.
//
// A failed seed leaves the cache COLD rather than storing a snapshot-only
// dictionary: without gem_colors the answer would be missing every gem the
// market has not priced this league, which is the blindness at league start that
// the union exists to prevent. Handlers take the repository fallback until a
// later tick seeds successfully.
func populateGemDictionary(ctx context.Context, src gemDictionarySource, cache *Cache, scope league.Scope, gems []GemPrice) error {
	c := cache.For(scope)

	skills, transfigured := gemDictionaryNames(gems)

	existingSkills, existingTransfigured, warm := c.gemDictionarySnapshot()
	if !warm {
		seedSkills, err := src.GemNameDictionary(ctx, scope, false)
		if err != nil {
			return fmt.Errorf("seed gem dictionary (skills): %w", err)
		}
		seedTransfigured, err := src.GemNameDictionary(ctx, scope, true)
		if err != nil {
			return fmt.Errorf("seed gem dictionary (transfigured): %w", err)
		}
		existingSkills, existingTransfigured = seedSkills, seedTransfigured
	}

	// MergeGemDictionary allocates a fresh sorted, deduplicated slice, so the
	// previously stored lists are never appended to in place.
	c.SetGemDictionary(
		MergeGemDictionary(existingSkills, skills),
		MergeGemDictionary(existingTransfigured, transfigured),
	)
	return nil
}

// gemDictionaryNames splits a snapshot's gem names into the two dictionary
// pools, applying the same eligibility rules the seed query does: the Heist rule
// on both halves, the support strip on the skill half only. FilterGemDictionary
// documents why the asymmetry is deliberate.
func gemDictionaryNames(gems []GemPrice) (skills, transfigured []string) {
	for _, g := range gems {
		if isHeistOnlyGemName(g.Name) {
			continue
		}
		if g.IsTransfigured {
			transfigured = append(transfigured, g.Name)
			continue
		}
		if isSupportGemName(g.Name) {
			continue
		}
		skills = append(skills, g.Name)
	}
	return skills, transfigured
}

// gemDictionarySnapshot returns both stored pools and their warmth in one lock
// acquisition. The slices are the stored ones: callers read them and build
// replacements, never mutate them in place.
func (c *Cache) gemDictionarySnapshot() (skills, transfigured []string, ok bool) {
	c.mu.RLock()
	defer c.mu.RUnlock()
	return c.gemDictSkills, c.gemDictTransfigured, c.gemDictSet
}
