package lab

import (
	"math"
	"sort"
)

// FloorBaseMultiplier controls the FLOOR boundary as a multiple of the
// variant's median base gem price. Tunable — higher = more gems in FLOOR.
const FloorBaseMultiplier = 3.0

// MidLowRatio controls where MID/LOW boundary falls relative to the
// MID-HIGH boundary. 0.4 = LOW tops out at 40% of the MID-HIGH boundary.
const MidLowRatio = 0.4

// MinGemsAboveGap is the minimum number of gems that must be above a gap
// for it to qualify as a tier boundary (prevents single-gem tiers).
const MinGemsAboveGap = 3

// GapSnapWindow is the number of positions to search in each direction
// when snapping a statistical boundary to the nearest natural gap.
const GapSnapWindow = 5

// detectLowConfidence identifies thin-market gems whose prices are unreliable.
// Returns map["name|variant"] → true for low-confidence gems.
// Uses per-variant median listings — gems with depth < 0.4 (listings < 40% of
// variant median) are flagged.
func detectLowConfidence(gems []GemPrice) map[string]bool {
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
		listings := make([]float64, len(vGems))
		for i, g := range vGems {
			listings[i] = float64(g.listings)
		}
		sort.Float64s(listings)
		median := listings[len(listings)/2]
		if median <= 0 {
			median = 1
		}
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
// Returns map["name|variant"] → true for TOP gems.
func detectTops(gems []GemPrice, lowConf map[string]bool) map[string]bool {
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

		if len(withTop.Boundaries) <= len(withoutTop.Boundaries) {
			continue
		}
		if len(withTop.Boundaries) == 0 {
			continue
		}

		topBoundary := withTop.Boundaries[0]
		for _, g := range vGems {
			if g.Chaos >= topBoundary {
				tops[g.Name+"|"+variant] = true
			}
		}
	}
	return tops
}

// computeMedianBasePrice returns the median chaos price of non-transfigured,
// non-corrupted base gems for a given variant.
func computeMedianBasePrice(gems []GemPrice, variant string) float64 {
	var prices []float64
	for _, g := range gems {
		if g.IsTransfigured || g.IsCorrupted || g.Chaos <= 0 || g.Listings < 5 {
			continue
		}
		if g.Variant != variant {
			continue
		}
		prices = append(prices, g.Chaos)
	}
	if len(prices) == 0 {
		return 1 // fallback
	}
	sort.Float64s(prices)
	return prices[len(prices)/2]
}

// largestGapWithMinAbove finds the largest absolute gap in a descending price
// slice where at least minAbove gems are above the gap. Returns the price at
// the top of the gap (the boundary value), or 0 if no qualifying gap exists.
func largestGapWithMinAbove(prices []float64, minAbove int) float64 {
	if len(prices) < minAbove+1 {
		return 0
	}

	bestGap := 0.0
	bestPrice := 0.0
	for i := minAbove - 1; i < len(prices)-1; i++ {
		gap := prices[i] - prices[i+1]
		if gap > bestGap {
			bestGap = gap
			bestPrice = prices[i]
		}
	}

	// Only accept if the gap is significant (at least 2× average gap).
	totalGap := prices[0] - prices[len(prices)-1]
	avgGap := totalGap / float64(len(prices)-1)
	if bestGap < avgGap*2 {
		return 0
	}

	return bestPrice
}

// gapSnap adjusts a statistical boundary to the nearest natural gap within
// ±window positions. Returns the price at the top of the widest gap found.
func gapSnap(prices []float64, target float64, window int) float64 {
	if len(prices) < 2 {
		return target
	}

	// Find the position closest to the target.
	targetIdx := -1
	for i, p := range prices {
		if p <= target {
			targetIdx = i
			break
		}
	}
	if targetIdx < 0 {
		targetIdx = len(prices) - 1
	}

	// Search ±window for the largest gap.
	lo := targetIdx - window
	if lo < 0 {
		lo = 0
	}
	hi := targetIdx + window
	if hi >= len(prices) {
		hi = len(prices) - 1
	}

	bestGap := 0.0
	bestPrice := target
	for i := lo; i < hi; i++ {
		gap := prices[i] - prices[i+1]
		if gap > bestGap {
			bestGap = gap
			bestPrice = prices[i]
		}
	}
	return bestPrice
}

// computeVariantTiers produces tier boundaries for a single variant using
// the hybrid algorithm:
//  1. FLOOR = FloorBaseMultiplier × median base price
//  2. HIGH = largest gap above FLOOR with min 3 gems above
//  3. MID-HIGH = largest gap between FLOOR and HIGH with min 3 gems above
//  4. MID/LOW = MidLowRatio × MID-HIGH boundary, gap-snapped ±5
//
// Input: descending sorted prices of clean gems (no low-confidence, no TOPs).
func computeVariantTiers(prices []float64, floorBoundary float64) TierBoundaries {
	if len(prices) < 4 {
		return tierFallback(prices)
	}

	// Separate above-floor gems.
	var aboveFloor []float64
	for _, p := range prices {
		if p >= floorBoundary {
			aboveFloor = append(aboveFloor, p)
		}
	}
	if len(aboveFloor) < 4 {
		// Not enough gems above floor — just use floor as the only boundary.
		return TierBoundaries{
			Boundaries: []float64{floorBoundary},
			Names:      []string{"LOW", "FLOOR"},
		}
	}

	// HIGH boundary: largest gap in above-floor with min 3 above.
	highBound := largestGapWithMinAbove(aboveFloor, MinGemsAboveGap)

	// MID-HIGH boundary: largest gap between FLOOR and HIGH with min 3 above.
	var midHighPool []float64
	for _, p := range aboveFloor {
		if highBound > 0 && p >= highBound {
			continue // exclude HIGH gems
		}
		midHighPool = append(midHighPool, p)
	}
	midHighBound := 0.0
	if len(midHighPool) >= MinGemsAboveGap+1 {
		midHighBound = largestGapWithMinAbove(midHighPool, MinGemsAboveGap)
	}

	// MID/LOW boundary: MidLowRatio × MID-HIGH (or HIGH if no MID-HIGH), gap-snapped.
	referenceForMidLow := midHighBound
	if referenceForMidLow <= 0 {
		referenceForMidLow = highBound
	}
	midLowTarget := 0.0
	if referenceForMidLow > 0 {
		midLowTarget = referenceForMidLow * MidLowRatio
		// Gap-snap within the above-floor pool.
		midLowTarget = gapSnap(aboveFloor, midLowTarget, GapSnapWindow)
	}

	// Build boundaries (descending order).
	var bounds []float64
	names := []string{}

	if highBound > 0 {
		bounds = append(bounds, highBound)
		names = append(names, "HIGH")
	}
	if midHighBound > 0 && midHighBound < highBound {
		bounds = append(bounds, midHighBound)
		names = append(names, "MID-HIGH")
	}
	if midLowTarget > 0 && midLowTarget > floorBoundary {
		bounds = append(bounds, midLowTarget)
		names = append(names, "MID")
	}
	bounds = append(bounds, floorBoundary)
	names = append(names, "LOW")
	names = append(names, "FLOOR")

	// Sort descending (should already be, but ensure).
	sort.Sort(sort.Reverse(sort.Float64Slice(bounds)))

	return TierBoundaries{
		Boundaries: bounds,
		Names:      names,
	}
}

// computeCleanTierBoundaries produces per-variant tier boundaries using the
// hybrid algorithm: base-cost FLOOR + gap detection for HIGH/MID-HIGH +
// ratio-based MID/LOW with gap snapping.
func computeCleanTierBoundaries(gems []GemPrice, lowConf map[string]bool, tops map[string]bool) map[string]TierBoundaries {
	// Build per-variant clean price lists (descending).
	byVariant := make(map[string][]float64)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		key := g.Name + "|" + g.Variant
		if lowConf[key] || tops[key] {
			continue
		}
		byVariant[g.Variant] = append(byVariant[g.Variant], g.Chaos)
	}

	result := make(map[string]TierBoundaries)
	for variant, prices := range byVariant {
		sort.Float64s(prices)
		// Reverse to descending.
		for i, j := 0, len(prices)-1; i < j; i, j = i+1, j-1 {
			prices[i], prices[j] = prices[j], prices[i]
		}

		floorBound := math.Max(computeMedianBasePrice(gems, variant)*FloorBaseMultiplier, 1)
		result[variant] = computeVariantTiers(prices, floorBound)
	}
	return result
}

// ComputeGemClassification is the unified tier pipeline entry point.
// Per variant independently:
//  1. Low Confidence detection (thin-market gems, depth < 0.4)
//  2. TOP detection (gap-based, from low-confidence-filtered pool)
//  3. Tier boundaries (hybrid: FLOOR + gap detection + ratio-based split)
//  4. Classify every gem
func ComputeGemClassification(gems []GemPrice) ClassificationResult {
	lowConf := detectLowConfidence(gems)
	tops := detectTops(gems, lowConf)
	boundaries := computeCleanTierBoundaries(gems, lowConf, tops)

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
