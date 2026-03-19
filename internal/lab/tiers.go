package lab

import (
	"sort"
	"strings"
)

// DetectTierBoundaries finds natural price gaps in the transfigured gem market
// to produce adaptive 4-tier boundaries (TOP/HIGH/MID/LOW).
//
// Algorithm:
//  1. Filter to transfigured, non-corrupted, non-Trarthus gems with Chaos > 0
//  2. Deduplicate by gem name (keep max price per name to avoid multi-variant inflation)
//  3. Sort unique prices descending
//  4. Compute relative gaps: gap_pct = (prices[i] - prices[i+1]) / prices[i]
//  5. Find the 3 largest relative gaps → tier boundaries
//  6. Fallback for <4 distinct prices: use P75/P50/P25
func DetectTierBoundaries(gems []GemPrice) TierBoundaries {
	// Step 1: collect max price per gem name (deduplicate variants).
	nameMax := make(map[string]float64)
	for _, g := range gems {
		if !g.IsTransfigured || g.IsCorrupted || g.Chaos <= 0 {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}
		if g.Chaos > nameMax[g.Name] {
			nameMax[g.Name] = g.Chaos
		}
	}

	// Step 2: collect unique prices, sort descending.
	prices := make([]float64, 0, len(nameMax))
	for _, p := range nameMax {
		prices = append(prices, p)
	}
	sort.Float64s(prices)
	// Reverse to descending.
	for i, j := 0, len(prices)-1; i < j; i, j = i+1, j-1 {
		prices[i], prices[j] = prices[j], prices[i]
	}

	// Fallback: fewer than 4 distinct prices cannot produce 3 gaps.
	if len(prices) < 4 {
		return tierFallback(prices)
	}

	// Step 3: compute relative gaps between consecutive prices.
	type gap struct {
		index  int
		relGap float64
	}
	gaps := make([]gap, 0, len(prices)-1)
	for i := 0; i < len(prices)-1; i++ {
		if prices[i] <= 0 {
			continue
		}
		rel := (prices[i] - prices[i+1]) / prices[i]
		gaps = append(gaps, gap{index: i, relGap: rel})
	}

	if len(gaps) < 3 {
		return tierFallback(prices)
	}

	// Step 4: find the 3 largest relative gaps.
	sort.Slice(gaps, func(a, b int) bool { return gaps[a].relGap > gaps[b].relGap })
	topGaps := gaps[:3]

	// Sort the 3 gap indices ascending so gap1 < gap2 < gap3.
	sort.Slice(topGaps, func(a, b int) bool { return topGaps[a].index < topGaps[b].index })

	// Step 5: boundaries are the first price BELOW each gap.
	// gap at index i means the boundary is between prices[i] and prices[i+1].
	// The tier includes prices[i+1] and above up to the previous gap.
	return TierBoundaries{
		Top:  prices[topGaps[0].index+1],
		High: prices[topGaps[1].index+1],
		Mid:  prices[topGaps[2].index+1],
	}
}

// tierFallback produces boundaries using P75/P50/P25 when there are too few
// distinct prices for gap detection.
func tierFallback(descPrices []float64) TierBoundaries {
	if len(descPrices) == 0 {
		return TierBoundaries{}
	}
	// Sort ascending for percentile computation.
	asc := make([]float64, len(descPrices))
	copy(asc, descPrices)
	sort.Float64s(asc)

	return TierBoundaries{
		Top:  percentile(asc, 0.75),
		High: percentile(asc, 0.50),
		Mid:  percentile(asc, 0.25),
	}
}

// classifyTier assigns a price tier based on TierBoundaries.
// Uses >= semantics: a gem priced exactly at a boundary belongs to the higher tier.
func classifyTier(price float64, tb TierBoundaries) string {
	if price >= tb.Top {
		return "TOP"
	}
	if price >= tb.High {
		return "HIGH"
	}
	if price >= tb.Mid {
		return "MID"
	}
	return "LOW"
}

// ClassifyTier is the exported variant for use by the optimizer.
func ClassifyTier(price float64, tb TierBoundaries) string {
	return classifyTier(price, tb)
}
