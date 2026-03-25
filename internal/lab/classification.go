package lab

import (
	"sort"
)

// detectLowConfidence identifies thin-market gems whose prices are unreliable.
// Returns map["name|variant"] -> true for low-confidence gems.
// Uses per-variant median listings -- gems with depth < 0.4 (listings < 40% of
// variant median) are flagged. Filtering matches DetectTierBoundaries: chaos > 5,
// isAnalyzableGem.
func detectLowConfidence(gems []GemPrice) map[string]bool {
	// Group analyzable gems by variant.
	type gemInfo struct {
		name     string
		variant  string
		listings int
	}
	byVariant := make(map[string][]gemInfo)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		byVariant[g.Variant] = append(byVariant[g.Variant], gemInfo{g.Name, g.Variant, g.Listings})
	}

	result := make(map[string]bool)
	for variant, vGems := range byVariant {
		// Compute median listings for this variant.
		listings := make([]float64, len(vGems))
		for i, g := range vGems {
			listings[i] = float64(g.listings)
		}
		sort.Float64s(listings)
		median := listings[len(listings)/2]
		if median <= 0 {
			median = 1
		}

		// Flag gems below 40% of median.
		for _, g := range vGems {
			depth := float64(g.listings) / median
			if depth < 0.4 {
				result[g.name+"|"+variant] = true
			}
		}
	}
	return result
}

// detectTops identifies TOP-tier gems per variant using gap detection.
// Low-confidence gems are excluded from the pool before detection.
// Returns map["name|variant"] -> true for TOP gems.
//
// Compares DetectTierBoundaries (which includes TOP gap detection) against
// DetectTierBoundariesNoTop. If the full algorithm produces an extra boundary,
// that boundary is the TOP threshold and gems at or above it are TOP.
func detectTops(gems []GemPrice, lowConf map[string]bool) map[string]bool {
	// Group non-low-confidence gems by variant.
	byVariant := make(map[string][]GemPrice)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		key := g.Name + "|" + g.Variant
		if lowConf[key] {
			continue
		}
		byVariant[g.Variant] = append(byVariant[g.Variant], g)
	}

	tops := make(map[string]bool)
	for variant, vGems := range byVariant {
		withTop := DetectTierBoundaries(vGems)
		withoutTop := DetectTierBoundariesNoTop(vGems)

		// A TOP boundary exists only when the full algorithm produces more
		// boundaries than the no-TOP variant.
		if len(withTop.Boundaries) <= len(withoutTop.Boundaries) {
			continue
		}
		if len(withTop.Boundaries) == 0 {
			continue
		}

		// First boundary is the TOP threshold.
		topBoundary := withTop.Boundaries[0]
		for _, g := range vGems {
			if g.Chaos >= topBoundary {
				tops[g.Name+"|"+variant] = true
			}
		}
	}
	return tops
}

// ComputeGemClassification is the unified tier pipeline entry point.
// Per variant independently:
//  1. Low Confidence detection (thin-market gems, depth < 0.4)
//  2. TOP detection (gap-based, from low-confidence-filtered pool)
//  3. Tier boundaries (gap-based, from pool minus low-confidence and TOP)
//  4. Classify every gem
func ComputeGemClassification(gems []GemPrice) ClassificationResult {
	// Step 1: Low confidence detection.
	lowConf := detectLowConfidence(gems)

	// Step 2: TOP detection on clean pool.
	tops := detectTops(gems, lowConf)

	// Step 3: Tier boundaries from clean pool minus TOPs.
	boundaries := computeCleanTierBoundaries(gems, lowConf, tops)

	// Step 4: Classify every gem.
	result := ClassificationResult{
		Gems:        make(GemClassificationMap),
		Boundaries:  boundaries,
		TopBoundary: make(map[string]float64),
	}

	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		gemKey := g.Name + "|" + g.Variant

		isLowConf := lowConf[gemKey]

		var tier string
		if tops[gemKey] {
			tier = "TOP"
		} else if tb, ok := boundaries[g.Variant]; ok {
			tier = classifyTier(g.Chaos, tb)
		} else {
			tier = "FLOOR"
		}

		result.Gems[key] = GemClassification{
			Tier:          tier,
			LowConfidence: isLowConf,
		}
	}

	return result
}

// computeCleanTierBoundaries produces per-variant tier boundaries from the
// pool after removing low-confidence and TOP gems. Uses gap detection
// WITHOUT TOP step (DetectTierBoundariesNoTop) since TOPs are already removed.
func computeCleanTierBoundaries(gems []GemPrice, lowConf map[string]bool, tops map[string]bool) map[string]TierBoundaries {
	byVariant := make(map[string][]GemPrice)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		key := g.Name + "|" + g.Variant
		if lowConf[key] || tops[key] {
			continue
		}
		byVariant[g.Variant] = append(byVariant[g.Variant], g)
	}

	// Tier names for clean boundaries: skip TOP since TOPs are classified
	// separately. The first boundary maps to HIGH instead.
	noTopNames := TierNames[1:] // ["HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"]

	result := make(map[string]TierBoundaries)
	for variant, vGems := range byVariant {
		tb := DetectTierBoundariesNoTop(vGems)
		tb.Names = noTopNames
		result[variant] = tb
	}
	return result
}
