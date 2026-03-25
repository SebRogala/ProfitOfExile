package lab

import (
	"math"
	"sort"
)

// FloorTopRatio controls the FLOOR boundary as a percentage of the top-5
// average gem price. 0.07 = FLOOR is below 7% of the top-5 average.
// Scales dynamically with market maturity — early league has low floor,
// mature league has high floor as prices diverge.
const FloorTopRatio = 0.08

// FloorTop5Count is how many top gems to average for the FLOOR anchor.
const FloorTop5Count = 5

// HighRatio controls the HIGH boundary as a ratio of the top non-TOP gem.
// 0.7 means HIGH includes gems within 30% of the highest price.
const HighRatio = 0.7

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
		// Collect prices descending.
		var prices []float64
		for _, g := range vGems {
			prices = append(prices, g.Chaos)
		}
		sort.Sort(sort.Reverse(sort.Float64Slice(prices)))

		if len(prices) < 4 {
			continue
		}

		// Find the natural TOP cluster by scanning gaps from the top.
		// The TOP boundary is where a gap is significantly larger than
		// the gap ABOVE it (the cluster gets "tight" above, then "breaks" below).
		//
		// Example: 1318→1116=202, 1116→930=186, 930→900=30
		//   Gap 186→30 is a 6x drop — the cluster of {1318,1116} ends here.
		//
		// Also works for: 1300→1200=100 (small), 1200→800=400 (big)
		//   Gap 100→400 is a 4x increase — boundary is at 1200 (below the jump).
		//
		// Algorithm: find the gap where gap[i] > gap[i-1] * 2 (gap doubles).
		// The TOP cluster is everything above that gap.
		// Constrain to top 10% of the pool.

		topCap := len(prices) / 10
		if topCap < 3 {
			topCap = 3
		}
		if topCap >= len(prices) {
			continue
		}

		// Compute average gap from the top portion only (top 10%).
		// Using the full pool's avgGap is too low (dragged down by
		// masses of 20-60c gems with 0-3c gaps), making every gap
		// above 14c look "significant."
		topPoolSize := topCap
		if topPoolSize > len(prices)-1 {
			topPoolSize = len(prices) - 1
		}
		topTotalGap := prices[0] - prices[topPoolSize]
		avgGap := topTotalGap / float64(topPoolSize)

		// Scan from top: find the TOP cluster boundary.
		// Start including gems while their gaps stay large (>= avgGap*2).
		// Stop when the gap drops below that threshold.
		// The TOP cluster must have at least 1 significant gap to exist.

		foundTop := false
		topEnd := 0 // number of gems in TOP

		for i := 0; i < topCap && i < len(prices)-1; i++ {
			gap := prices[i] - prices[i+1]
			if gap >= avgGap*2 {
				// This gap is significant — the gem above it is in the TOP cluster.
				topEnd = i + 1
				foundTop = true
			} else if foundTop {
				// Gap shrunk below threshold — cluster ends.
				break
			}
		}

		if !foundTop || topEnd < 1 {
			continue
		}

		topBoundary := prices[topEnd-1]
		for _, g := range vGems {
			if g.Chaos >= topBoundary {
				tops[g.Name+"|"+variant] = true
			}
		}
	}
	return tops
}

// computeTop5Avg returns the average price of the top N most valuable
// transfigured gems for a given variant (excluding low-confidence).
// Used to anchor the FLOOR boundary relative to the market's top end.
func computeTop5Avg(prices []float64) float64 {
	if len(prices) == 0 {
		return 1 // fallback
	}
	n := FloorTop5Count
	if n > len(prices) {
		n = len(prices)
	}
	sum := 0.0
	for i := 0; i < n; i++ {
		sum += prices[i] // prices are descending
	}
	return sum / float64(n)
}

// largestGapWithMinAbove finds the largest absolute gap in a descending price
// slice where at least minAbove gems are above the gap. Returns the price at
// the top of the gap (the boundary value), or 0 if no qualifying gap exists.
// significanceMultiplier controls how large the gap must be relative to the
// average gap (e.g., 1.5 = gap must be 50% larger than average).
func largestGapWithMinAbove(prices []float64, minAbove int, significanceMultiplier float64) float64 {
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

	// Only accept if the gap is significant relative to average.
	totalGap := prices[0] - prices[len(prices)-1]
	avgGap := totalGap / float64(len(prices)-1)
	if bestGap < avgGap*significanceMultiplier {
		return 0
	}

	return bestPrice
}

// largestRelativeGapWithMinAbove finds the largest percentage gap in a
// descending price slice where at least minAbove gems are above the gap.
// Returns the price at the top of the gap, or 0 if none qualifies.
func largestRelativeGapWithMinAbove(prices []float64, minAbove int) float64 {
	if len(prices) < minAbove+1 {
		return 0
	}

	bestRelGap := 0.0
	bestPrice := 0.0
	for i := minAbove - 1; i < len(prices)-1; i++ {
		if prices[i] <= 0 {
			continue
		}
		relGap := (prices[i] - prices[i+1]) / prices[i]
		if relGap > bestRelGap {
			bestRelGap = relGap
			bestPrice = prices[i]
		}
	}

	// Must be at least 8% relative gap to qualify.
	if bestRelGap < 0.08 {
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
//  2. HIGH = HighRatio × top gem price (gems within 30% of the highest)
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
		return TierBoundaries{
			Boundaries: []float64{floorBoundary},
			Names:      []string{"LOW", "FLOOR"},
		}
	}

	// HIGH boundary: ratio of the top gem price, gap-snapped ±2 positions.
	// Small window avoids jumping too far but catches the nearest natural gap.
	highBound := gapSnap(aboveFloor, aboveFloor[0]*HighRatio, 2)

	// MID-HIGH boundary: half of HIGH, gap-snapped ±2 positions.
	midHighBound := gapSnap(aboveFloor, highBound*0.5, 2)

	// MID/LOW boundary: largest relative gap below MID-HIGH, gap-snapped.
	var belowMidHigh []float64
	for _, p := range aboveFloor {
		if p >= midHighBound {
			continue
		}
		belowMidHigh = append(belowMidHigh, p)
	}
	midLowTarget := 0.0
	if len(belowMidHigh) >= MinGemsAboveGap+1 {
		midLowTarget = largestRelativeGapWithMinAbove(belowMidHigh, MinGemsAboveGap)
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

		floorBound := math.Max(computeTop5Avg(prices)*FloorTopRatio, 1)
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
