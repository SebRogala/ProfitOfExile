package lab

import (
	"sort"
)

// TierNames lists tier names in order from highest to lowest.
// The algorithm produces N boundaries -> N+1 tiers. Index into this array
// based on boundary position: boundary[0] -> TierNames[0] (TOP), etc.
// Anything below all boundaries gets the last applicable name.
var TierNames = []string{"TOP", "HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"}

// DetectTierBoundariesRecursive produces dynamic tier boundaries using
// recursive average splitting. Each step computes the average of the
// remaining pool and splits: below-average gems go to the current bottom
// tier, above-average gems continue. Stops when pool < 4 gems.
//
// Step 1: Find TOP via largest ABSOLUTE gap (not relative) at the top.
//
//	Remove TOP gems from pool.
//
// Step 2+: Compute average of pool. Below = next tier. Repeat.
//
// TOP gems are NEVER included in any average computation.
func DetectTierBoundariesRecursive(gems []GemPrice) TierBoundaries {
	// Step 1: collect max price per gem name (same dedup as before).
	prices := collectAndSortPrices(gems) // descending

	if len(prices) < 4 {
		return tierFallback(prices)
	}

	// Step 2: Find TOP via largest ABSOLUTE gap in the top portion of the
	// distribution. TOP gems are rare outliers — the gap must be in the top 10%
	// (minimum 3 positions) to prevent a mid-distribution gap from absorbing
	// too many gems into TOP.
	topCap := len(prices) / 10
	if topCap < 3 {
		topCap = 3
	}
	topIdx := findLargestGapInRange(prices, topCap)
	topGap := prices[topIdx] - prices[topIdx+1]

	// Compute average gap for comparison.
	var totalGap float64
	for i := 0; i < len(prices)-1; i++ {
		totalGap += prices[i] - prices[i+1]
	}
	avgGap := totalGap / float64(len(prices)-1)

	var boundaries []float64
	var pool []float64

	if topGap >= avgGap*3 && topIdx < topCap {
		// Significant outlier gap — create TOP tier.
		topBoundary := prices[topIdx]
		boundaries = append(boundaries, topBoundary)
		pool = make([]float64, len(prices)-topIdx-1)
		copy(pool, prices[topIdx+1:])
	} else {
		// No significant TOP gap — all gems start in the recursive pool.
		pool = make([]float64, len(prices))
		copy(pool, prices)
	}

	for len(pool) >= 4 {
		avg := average(pool)
		// Find the split point: first price below average.
		splitIdx := -1
		for i, p := range pool {
			if p < avg {
				splitIdx = i
				break
			}
		}
		if splitIdx <= 0 || splitIdx >= len(pool) {
			break // can't split meaningfully
		}
		boundaries = append(boundaries, pool[splitIdx])
		pool = pool[:splitIdx] // keep only above-average gems for next iteration
	}

	// Sort boundaries descending for classifyTier to work correctly.
	sort.Sort(sort.Reverse(sort.Float64Slice(boundaries)))

	return TierBoundaries{Boundaries: boundaries}
}

// DetectTierBoundaries is an alias for DetectTierBoundariesRecursive.
// Keeps call sites simple while the old name is phased out.
func DetectTierBoundaries(gems []GemPrice) TierBoundaries {
	return DetectTierBoundariesRecursive(gems)
}

// collectAndSortPrices extracts max price per gem name from analysable gems.
// Uses a dynamic listing floor: computes median listings across the pool,
// then excludes gems below 25% of median (minimum 2). This prevents
// low-confidence prices from distorting tier boundaries while adapting to
// league phase (early league has fewer listings overall).
func collectAndSortPrices(gems []GemPrice) []float64 {
	// First pass: collect all listings to compute median.
	var allListings []float64
	for _, g := range gems {
		if isAnalyzableGem(g) && g.Chaos > 5 {
			allListings = append(allListings, float64(g.Listings))
		}
	}
	minListings := 2
	if len(allListings) > 0 {
		sort.Float64s(allListings)
		median := allListings[len(allListings)/2]
		dynamic := int(median * 0.25)
		if dynamic > minListings {
			minListings = dynamic
		}
	}

	// Second pass: collect prices, filtering by dynamic listing floor.
	nameMax := make(map[string]float64)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Listings < minListings {
			continue
		}
		if g.Chaos > nameMax[g.Name] {
			nameMax[g.Name] = g.Chaos
		}
	}

	prices := make([]float64, 0, len(nameMax))
	for _, p := range nameMax {
		prices = append(prices, p)
	}
	sort.Float64s(prices)
	// Reverse to descending.
	for i, j := 0, len(prices)-1; i < j; i, j = i+1, j-1 {
		prices[i], prices[j] = prices[j], prices[i]
	}
	return prices
}

// findLargestGapInRange returns the index of the price just ABOVE the largest
// absolute gap within the first maxIdx positions of a descending price slice.
// Used by DetectTierBoundariesRecursive to constrain TOP detection to the top
// portion of the distribution.
func findLargestGapInRange(prices []float64, maxIdx int) int {
	bestIdx := 0
	bestGap := 0.0
	limit := maxIdx
	if limit > len(prices)-1 {
		limit = len(prices) - 1
	}
	for i := 0; i < limit; i++ {
		gap := prices[i] - prices[i+1]
		if gap > bestGap {
			bestGap = gap
			bestIdx = i
		}
	}
	return bestIdx
}

// findLargestAbsoluteGap returns the index of the price just ABOVE the largest
// absolute gap in a descending price slice. The gap is between prices[i] and
// prices[i+1]. Returns 0 if only one gap or no gaps.
func findLargestAbsoluteGap(prices []float64) int {
	bestIdx := 0
	bestGap := 0.0
	for i := 0; i < len(prices)-1; i++ {
		gap := prices[i] - prices[i+1]
		if gap > bestGap {
			bestGap = gap
			bestIdx = i
		}
	}
	return bestIdx
}

// average computes the arithmetic mean of a float64 slice.
func average(vals []float64) float64 {
	if len(vals) == 0 {
		return 0
	}
	var sum float64
	for _, v := range vals {
		sum += v
	}
	return sum / float64(len(vals))
}

// DetectTierBoundariesSimplified produces 3-4 tiers for small pools (Font EV
// color pools of 7-87 gems). Uses median-based FLOOR (not mean) for stable
// ~50% split regardless of outlier prices.
//
// Algorithm:
//  1. TOP: largest absolute gap if > 3x avg gap AND in top third of pool.
//  2. FLOOR: below median of remaining gems.
//  3. HIGH/MID: mean of above-median splits the rest.
//
// Produces TierNames: TOP (if detected), HIGH, MID, FLOOR.
func DetectTierBoundariesSimplified(gems []GemPrice) TierBoundaries {
	// Collect ALL gem prices without filtering (no listing floor).
	// Font EV's small color pools need every gem for accurate median split.
	// Dedup by name (max price per name).
	nameMax := make(map[string]float64)
	for _, g := range gems {
		if g.Chaos > 0 {
			if g.Chaos > nameMax[g.Name] {
				nameMax[g.Name] = g.Chaos
			}
		}
	}
	prices := make([]float64, 0, len(nameMax))
	for _, p := range nameMax {
		prices = append(prices, p)
	}
	sort.Float64s(prices)
	for i, j := 0, len(prices)-1; i < j; i, j = i+1, j-1 {
		prices[i], prices[j] = prices[j], prices[i]
	}

	if len(prices) < 3 {
		return tierFallback(prices)
	}

	var boundaries []float64
	pool := make([]float64, len(prices))
	copy(pool, prices)

	// Step 1: TOP detection — same as recursive, but only in top third.
	topIdx := findLargestAbsoluteGap(pool)
	topGap := pool[topIdx] - pool[topIdx+1]
	var totalGap float64
	for i := 0; i < len(pool)-1; i++ {
		totalGap += pool[i] - pool[i+1]
	}
	avgGap := totalGap / float64(len(pool)-1)

	topThird := len(pool) / 3
	if topThird < 1 {
		topThird = 1
	}
	if topGap >= avgGap*3 && topIdx < topThird {
		boundaries = append(boundaries, pool[topIdx])
		pool = pool[topIdx+1:]
	}

	if len(pool) < 2 {
		return TierBoundaries{Boundaries: boundaries}
	}

	// Step 2: FLOOR — below average. Average is pulled up by expensive gems
	// in right-skewed distributions, giving ~30-40% winners (not 50% like median).
	// This produces meaningful Safe hit rates of 55-75%.
	avg := average(pool)
	// Find the split point: first price below average.
	floorIdx := -1
	for i, p := range pool {
		if p < avg {
			floorIdx = i
			break
		}
	}
	if floorIdx <= 0 {
		return TierBoundaries{Boundaries: boundaries, Names: []string{"TOP", "HIGH", "MID", "FLOOR"}}
	}
	boundaries = append(boundaries, pool[floorIdx])

	// Step 3: HIGH/MID — mean of above-average gems splits the upper portion.
	upperPool := pool[:floorIdx]
	if len(upperPool) >= 2 {
		upperMean := average(upperPool)
		// Find split point.
		for i, p := range upperPool {
			if p < upperMean {
				if i > 0 {
					boundaries = append(boundaries, upperPool[i])
				}
				break
			}
		}
	}

	// Sort descending.
	sort.Sort(sort.Reverse(sort.Float64Slice(boundaries)))

	return TierBoundaries{
		Boundaries: boundaries,
		Names:      []string{"TOP", "HIGH", "MID", "FLOOR"},
	}
}

// tierFallback produces boundaries using percentile splitting when there are
// too few distinct prices for recursive detection. Returns boundaries matching
// P75/P50/P25 positions so that classifyTier still produces reasonable results.
func tierFallback(descPrices []float64) TierBoundaries {
	if len(descPrices) == 0 {
		return TierBoundaries{}
	}
	// Sort ascending for percentile computation.
	asc := make([]float64, len(descPrices))
	copy(asc, descPrices)
	sort.Float64s(asc)

	return TierBoundaries{
		Boundaries: []float64{
			percentile(asc, 0.75),
			percentile(asc, 0.50),
			percentile(asc, 0.25),
		},
	}
}

// classifyTier assigns a price tier based on TierBoundaries.
// Uses >= semantics: a gem priced exactly at a boundary belongs to the higher tier.
func classifyTier(price float64, tb TierBoundaries) string {
	names := TierNames
	if len(tb.Names) > 0 {
		names = tb.Names
	}
	for i, boundary := range tb.Boundaries {
		if price >= boundary {
			if i < len(names) {
				return names[i]
			}
			return names[len(names)-1]
		}
	}
	// Below all boundaries.
	belowIdx := len(tb.Boundaries)
	if belowIdx < len(names) {
		return names[belowIdx]
	}
	return names[len(names)-1]
}

// ClassifyTier is the exported variant for use by the optimizer.
func ClassifyTier(price float64, tb TierBoundaries) string {
	return classifyTier(price, tb)
}
