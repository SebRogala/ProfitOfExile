package lab

import "profitofexile/internal/price/tier"

// labTierCutoffChaos is Lab's price cutoff: gems priced at or below it take no
// part in tiering or low-confidence detection; double-corrupt applies it to
// confidence detection only and still prices positive-priced cheap rows. It is
// Lab's legacy POE-134 parameter, not a property of the tier algorithm — other
// markets classify with their own tier.Policy.
const labTierCutoffChaos = 5

// labPolicy is the tier policy every Lab pool classifies under.
var labPolicy = tier.Policy{CutoffChaos: labTierCutoffChaos, HasCutoff: true}

// tierItems maps the gems that pass filter to tier items: the market is the
// variant, the key the gem name, the depth its listing count.
func tierItems(gems []GemPrice, filter func(GemPrice) bool) []tier.Item {
	items := make([]tier.Item, 0, len(gems))
	for _, g := range gems {
		if !filter(g) {
			continue
		}
		items = append(items, tier.Item{Group: g.Variant, Key: g.Name, Price: g.Chaos, Depth: g.Listings})
	}
	return items
}

// labLowConfidence flags the thin-market gems among those passing filter, per
// variant, under Lab's policy. Keys are tier.Key{Group: variant, Key: name}.
func labLowConfidence(gems []GemPrice, filter func(GemPrice) bool) map[tier.Key]bool {
	return tier.LowConfidence(tierItems(gems, filter), labPolicy)
}

// classify runs the tier algorithm over the gems passing filter and maps the
// result back to Lab's ClassificationResult. Lab leaves TopBoundary empty.
func classify(gems []GemPrice, filter func(GemPrice) bool) ClassificationResult {
	res := tier.Classify(tierItems(gems, filter), labPolicy)

	result := ClassificationResult{
		Gems:        make(GemClassificationMap, len(res.Items)),
		Boundaries:  res.Boundaries,
		TopBoundary: make(map[string]float64),
	}
	for key, c := range res.Items {
		result.Gems[GemClassificationKey{Name: key.Key, Variant: key.Group}] = GemClassification{
			Tier:          c.Tier,
			LowConfidence: c.LowConfidence,
		}
	}
	return result
}

// ComputeDedicationClassification runs the tier pipeline for one Dedication
// corrupted pool: gems passing isDedicationOutcome in the given variant and
// pool type. isTransfigured = false → skills pool, true → transfigured pool.
//
// Tiers are relative to the pool they are computed over, so each variant is
// classified on its own: 21/20c is a deeper and cheaper market than 21/23c, and
// a gem that is TOP in one is not necessarily TOP in the other.
func ComputeDedicationClassification(gems []GemPrice, isTransfigured bool, variant string) ClassificationResult {
	return classify(gems, func(g GemPrice) bool {
		return isDedicationOutcome(g) && g.Variant == variant && g.IsTransfigured == isTransfigured
	})
}

// ComputeGemClassification is the Font tier pipeline entry point: the gems
// passing isFontOutcome, classified per variant by internal/price/tier under
// Lab's policy (low confidence, TOP, boundaries, then every gem).
func ComputeGemClassification(gems []GemPrice) ClassificationResult {
	return classify(gems, isFontOutcome)
}
