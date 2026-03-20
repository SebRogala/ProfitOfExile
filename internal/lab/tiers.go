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

	// Step 2: Find TOP via largest ABSOLUTE gap — only if it's significantly
	// larger than the average gap (at least 3x). Otherwise there's no real
	// outlier cluster and everything starts from HIGH.
	topIdx := findLargestAbsoluteGap(prices)
	topGap := prices[topIdx] - prices[topIdx+1]

	// Compute average gap for comparison.
	var totalGap float64
	for i := 0; i < len(prices)-1; i++ {
		totalGap += prices[i] - prices[i+1]
	}
	avgGap := totalGap / float64(len(prices)-1)

	var boundaries []float64
	var pool []float64

	if topGap >= avgGap*3 && topIdx < len(prices)-1 {
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

// collectAndSortPrices extracts max price per gem name from analysable gems
// (transfigured, non-corrupted, non-Trarthus, Chaos > 5) and returns them
// sorted descending.
func collectAndSortPrices(gems []GemPrice) []float64 {
	nameMax := make(map[string]float64)
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
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
	for i, boundary := range tb.Boundaries {
		if price >= boundary {
			if i < len(TierNames) {
				return TierNames[i]
			}
			return TierNames[len(TierNames)-1]
		}
	}
	// Below all boundaries.
	belowIdx := len(tb.Boundaries)
	if belowIdx < len(TierNames) {
		return TierNames[belowIdx]
	}
	return TierNames[len(TierNames)-1]
}

// ClassifyTier is the exported variant for use by the optimizer.
func ClassifyTier(price float64, tb TierBoundaries) string {
	return classifyTier(price, tb)
}
