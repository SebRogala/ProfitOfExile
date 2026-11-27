package lab

import (
	"profitofexile/internal/price/evidence"
	"profitofexile/internal/price/tier"
)

// labTierCutoffChaos is Lab's price cutoff: gems priced at or below it take no
// part in tiering or low-confidence detection; double-corrupt applies it to
// confidence detection only and still prices positive-priced cheap rows. It is
// Lab's legacy POE-134 parameter, not a property of the tier algorithm — other
// markets classify with their own tier.Policy.
const labTierCutoffChaos = 5

// labThinSampleCount is the fixed poe.ninja sample threshold from POE-296 L6.
const labThinSampleCount = 5

// labPolicy is the tier policy every Lab pool classifies under.
var labPolicy = tier.Policy{CutoffChaos: labTierCutoffChaos, HasCutoff: true, MinConfident: tier.MinTopItems, RatioLadder: true}

func gemDepth(g GemPrice) evidence.Depth {
	source := evidence.SourceNone
	if g.Chaos > 0 {
		source = evidence.SourceNinja
	}
	return evidence.Depth{PricedBy: source, NinjaListings: g.Listings, NinjaSample: g.SampleCount, Hold: g.hold}
}

// classificationInput is the price, depth and previous tier a gem is
// classified on: its trailing inputs when derived, else the observed values.
func classificationInput(g GemPrice) (price float64, depth int, prev string) {
	if g.tierPrice > 0 {
		return g.tierPrice, g.tierDepth, g.prevTier
	}
	return g.Chaos, g.Listings, g.prevTier
}

// tierItems maps the gems that pass filter to tier items: the market is the
// variant, the key the gem name. The gate grades the observed depth.
func tierItems(gems []GemPrice, filter func(GemPrice) bool) []tier.Item {
	items := make([]tier.Item, 0, len(gems))
	for _, g := range gems {
		if !filter(g) {
			continue
		}
		price, depth, prev := classificationInput(g)
		items = append(items, tier.Item{
			Group: g.Variant, Key: g.Name, Price: price, Depth: depth,
			Gated: evidence.Grade(gemDepth(g), GemEvidencePolicy()) != evidence.Solid,
			Held:  g.hold != nil,
			Prev:  prev,
		})
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
	policy := labPolicy
	for _, g := range gems {
		if len(g.prevBoundaries.Names) == 0 || !filter(g) {
			continue
		}
		if policy.PrevBoundaries == nil {
			policy.PrevBoundaries = make(map[string]tier.Boundaries)
		}
		policy.PrevBoundaries[g.Variant] = g.prevBoundaries
	}
	res := tier.Classify(tierItems(gems, filter), policy)

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
	for _, g := range gems {
		key := GemClassificationKey{Name: g.Name, Variant: g.Variant}
		if _, ok := result.Gems[key]; g.holdRejected && filter(g) && (ok || g.Chaos > labTierCutoffChaos) {
			result.Gems[key] = GemClassification{Tier: tier.Chaotic, LowConfidence: true}
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
