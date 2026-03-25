package lab

import "sort"

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
