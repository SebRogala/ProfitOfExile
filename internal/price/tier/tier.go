// Package tier is the one price-tier algorithm shared by every market that
// ranks items into TOP, HIGH, MID-HIGH, MID, LOW and FLOOR: Lab's Font and
// Dedication pools, the double-corrupt gate and the Harvest fixture generator.
//
// Per group (one market — a gem variant, a Harvest family), independently:
//  1. Low confidence: items whose depth is under 40% of the group median.
//  2. TOP: the gap-detected cluster at the top of the confident pool.
//  3. Boundaries: FLOOR anchored to the top-5 average, HIGH and MID-HIGH
//     gap-snapped, MID at the largest relative gap below MID-HIGH.
//  4. Every item is classified; a low-confidence item above the highest
//     boundary is CHAOTIC.
//
// Callers own eligibility and cutoff: they pass only the items that belong in
// a market, and a Policy. The zero Policy applies no price cutoff.
//
// The package imports the standard library only (boundary_test.go).
package tier

import (
	"math"
	"sort"
)

// Tier names.
const (
	Top     = "TOP"
	High    = "HIGH"
	MidHigh = "MID-HIGH"
	Mid     = "MID"
	Low     = "LOW"
	Floor   = "FLOOR"
	Chaotic = "CHAOTIC"
)

// Order lists tier names from highest to lowest. Of indexes into it when a
// Boundaries carries no Names: boundary[0] -> Order[0] (TOP), etc.
var Order = []string{Top, High, MidHigh, Mid, Low, Floor}

// floorTopRatio sets FLOOR as a share of the top-5 average price, so the floor
// scales with market maturity.
const floorTopRatio = 0.08

// floorTop5Count is how many top prices are averaged for the FLOOR anchor.
const floorTop5Count = 5

// highRatio sets the HIGH target as a ratio of the top non-TOP price: HIGH
// holds items within 30% of it.
const highRatio = 0.7

// minGemsAboveGap is the minimum number of items above a gap for it to qualify
// as the MID boundary (prevents single-item tiers).
const minGemsAboveGap = 3

// lowConfidenceDepth is the fraction of the group's median depth below which
// an item is low-confidence.
const lowConfidenceDepth = 0.4

// Item is one priced entry of a market. Group is the market and is required;
// Key identifies the item within it; Depth is its listing count.
type Item struct {
	Group string
	Key   string
	Price float64
	Depth int
}

// Key identifies a classified item.
type Key struct {
	Group string
	Key   string
}

// Policy is the caller's choice of what the algorithm leaves out. The zero
// value means no cutoff and depth-aware low-confidence detection.
type Policy struct {
	// CutoffChaos excludes every item priced at or below it from every step,
	// when HasCutoff is set.
	CutoffChaos float64
	HasCutoff   bool
	// DepthUnknown skips low-confidence detection for markets whose depth is
	// not measured: no item is low-confidence and none is CHAOTIC.
	DepthUnknown bool
}

// excludes reports whether the policy's cutoff leaves price out.
func (p Policy) excludes(price float64) bool {
	return p.HasCutoff && price <= p.CutoffChaos
}

// Boundaries holds tier thresholds sorted descending. Names, when set, names
// the tier at and above each boundary plus the tier below the last one;
// otherwise Order applies.
type Boundaries struct {
	Boundaries []float64 `json:"boundaries"`
	Names      []string  `json:"names,omitempty"` // if nil, use Order
}

// Classification is one item's tier.
type Classification struct {
	Tier          string
	LowConfidence bool
}

// Result is the output of Classify. TopBoundary holds, per group with a TOP
// cluster, the lowest TOP price.
type Result struct {
	Items       map[Key]Classification
	Boundaries  map[string]Boundaries
	TopBoundary map[string]float64
}

// Classify tiers items per group under the policy. Items the policy's cutoff
// excludes are absent from Result.Items.
func Classify(items []Item, p Policy) Result {
	lowConf := LowConfidence(items, p)
	tops, topBoundary := detectTops(items, lowConf, p)
	boundaries := cleanBoundaries(items, lowConf, tops, p)

	result := Result{
		Items:       make(map[Key]Classification),
		Boundaries:  boundaries,
		TopBoundary: topBoundary,
	}

	for _, it := range items {
		if p.excludes(it.Price) {
			continue
		}
		key := Key{it.Group, it.Key}

		isLowConf := lowConf[key]

		var tier string
		if tops[key] {
			tier = Top
		} else if tb, ok := boundaries[it.Group]; ok {
			tier = Of(it.Price, tb)
			// Low-confidence items with prices above the highest tier boundary
			// have unreliable inflated prices (CASCADE, manipulation, thin market
			// spike). Classify as CHAOTIC — excluded from font EV and rankings.
			if isLowConf && len(tb.Boundaries) > 0 && it.Price > tb.Boundaries[0] {
				tier = Chaotic
			}
		} else {
			tier = Floor
		}

		result.Items[key] = Classification{
			Tier:          tier,
			LowConfidence: isLowConf,
		}
	}

	return result
}

// LowConfidence identifies thin-market items whose prices are unreliable: per
// group, items whose depth is under 40% of the group's median depth. Under
// DepthUnknown it flags none.
func LowConfidence(items []Item, p Policy) map[Key]bool {
	result := make(map[Key]bool)
	if p.DepthUnknown {
		return result
	}

	byGroup := make(map[string][]Item)
	for _, it := range items {
		if p.excludes(it.Price) {
			continue
		}
		byGroup[it.Group] = append(byGroup[it.Group], it)
	}

	for group, gItems := range byGroup {
		depths := make([]float64, len(gItems))
		for i, it := range gItems {
			depths[i] = float64(it.Depth)
		}
		sort.Float64s(depths)
		median := depths[len(depths)/2]
		if median <= 0 {
			median = 1
		}
		for _, it := range gItems {
			depth := float64(it.Depth) / median
			if depth < lowConfidenceDepth {
				result[Key{group, it.Key}] = true
			}
		}
	}
	return result
}

// detectTops identifies TOP items per group using gap detection. Low-confidence
// items are excluded from the pool before detection. It returns the TOP set and,
// per group with a TOP cluster, the lowest TOP price.
func detectTops(items []Item, lowConf map[Key]bool, p Policy) (map[Key]bool, map[string]float64) {
	byGroup := make(map[string][]Item)
	for _, it := range items {
		if p.excludes(it.Price) {
			continue
		}
		if lowConf[Key{it.Group, it.Key}] {
			continue
		}
		byGroup[it.Group] = append(byGroup[it.Group], it)
	}

	tops := make(map[Key]bool)
	topBoundaries := make(map[string]float64)
	for group, gItems := range byGroup {
		// Collect prices descending.
		var prices []float64
		for _, it := range gItems {
			prices = append(prices, it.Price)
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
		// masses of 20-60c items with 0-3c gaps), making every gap
		// above 14c look "significant."
		topPoolSize := topCap
		if topPoolSize > len(prices)-1 {
			topPoolSize = len(prices) - 1
		}
		topTotalGap := prices[0] - prices[topPoolSize]
		avgGap := topTotalGap / float64(topPoolSize)

		// Scan from top: find the TOP cluster boundary.
		// Start including items while their gaps stay large (>= avgGap*2).
		// Stop when the gap drops below that threshold.
		// The TOP cluster must have at least 1 significant gap to exist.

		foundTop := false
		topEnd := 0 // number of items in TOP

		for i := 0; i < topCap && i < len(prices)-1; i++ {
			gap := prices[i] - prices[i+1]
			if gap >= avgGap*2 {
				// This gap is significant — the item above it is in the TOP cluster.
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
		topBoundaries[group] = topBoundary
		for _, it := range gItems {
			if it.Price >= topBoundary {
				tops[Key{group, it.Key}] = true
			}
		}
	}
	return tops, topBoundaries
}

// computeTop5Avg returns the average of the top N prices of a descending
// slice. Used to anchor the FLOOR boundary relative to the market's top end.
func computeTop5Avg(prices []float64) float64 {
	if len(prices) == 0 {
		return 1 // fallback
	}
	n := floorTop5Count
	if n > len(prices) {
		n = len(prices)
	}
	sum := 0.0
	for i := 0; i < n; i++ {
		sum += prices[i] // prices are descending
	}
	return sum / float64(n)
}

// largestRelativeGapWithMinAbove finds the largest percentage gap in a
// descending price slice where at least minAbove items are above the gap.
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

// computeVariantTiers produces tier boundaries for a single group using the
// hybrid algorithm:
//  1. FLOOR = floorBoundary (floorTopRatio × top-5 average, at least 1)
//  2. HIGH = highRatio × top price, gap-snapped ±2
//  3. MID-HIGH = half of HIGH, gap-snapped ±2
//  4. MID = largest relative gap below MID-HIGH with min 3 items above
//
// Input: descending sorted prices of clean items (no low-confidence, no TOPs).
func computeVariantTiers(prices []float64, floorBoundary float64) Boundaries {
	if len(prices) < 4 {
		return tierFallback(prices)
	}

	// Separate above-floor items.
	var aboveFloor []float64
	for _, p := range prices {
		if p >= floorBoundary {
			aboveFloor = append(aboveFloor, p)
		}
	}
	if len(aboveFloor) < 4 {
		return Boundaries{
			Boundaries: []float64{floorBoundary},
			Names:      []string{Low, Floor},
		}
	}

	// HIGH boundary: ratio of the top price, gap-snapped ±2 positions.
	// Small window avoids jumping too far but catches the nearest natural gap.
	highBound := gapSnap(aboveFloor, aboveFloor[0]*highRatio, 2)

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
	if len(belowMidHigh) >= minGemsAboveGap+1 {
		midLowTarget = largestRelativeGapWithMinAbove(belowMidHigh, minGemsAboveGap)
	}

	// Build boundaries (descending order).
	var bounds []float64
	names := []string{}

	if highBound > 0 {
		bounds = append(bounds, highBound)
		names = append(names, High)
	}
	if midHighBound > 0 && midHighBound < highBound {
		bounds = append(bounds, midHighBound)
		names = append(names, MidHigh)
	}
	if midLowTarget > 0 && midLowTarget > floorBoundary {
		bounds = append(bounds, midLowTarget)
		names = append(names, Mid)
	}
	bounds = append(bounds, floorBoundary)
	names = append(names, Low)
	names = append(names, Floor)

	sort.Sort(sort.Reverse(sort.Float64Slice(bounds)))

	return Boundaries{
		Boundaries: bounds,
		Names:      names,
	}
}

// cleanBoundaries produces per-group tier boundaries from the clean pool (no
// low-confidence, no TOP items) using the hybrid algorithm.
func cleanBoundaries(items []Item, lowConf map[Key]bool, tops map[Key]bool, p Policy) map[string]Boundaries {
	// Build per-group clean price lists (descending).
	byGroup := make(map[string][]float64)
	for _, it := range items {
		if p.excludes(it.Price) {
			continue
		}
		key := Key{it.Group, it.Key}
		if lowConf[key] || tops[key] {
			continue
		}
		byGroup[it.Group] = append(byGroup[it.Group], it.Price)
	}

	result := make(map[string]Boundaries)
	for group, prices := range byGroup {
		sort.Float64s(prices)
		// Reverse to descending.
		for i, j := 0, len(prices)-1; i < j; i, j = i+1, j-1 {
			prices[i], prices[j] = prices[j], prices[i]
		}

		floorBound := math.Max(computeTop5Avg(prices)*floorTopRatio, 1)
		result[group] = computeVariantTiers(prices, floorBound)
	}
	return result
}

// tierFallback produces boundaries using percentile splitting when there are
// too few prices for gap detection: P75/P50/P25, so that Of still produces
// reasonable results.
func tierFallback(descPrices []float64) Boundaries {
	if len(descPrices) == 0 {
		return Boundaries{}
	}
	// Sort ascending for percentile computation.
	asc := make([]float64, len(descPrices))
	copy(asc, descPrices)
	sort.Float64s(asc)

	return Boundaries{
		Boundaries: []float64{
			percentile(asc, 0.75),
			percentile(asc, 0.50),
			percentile(asc, 0.25),
		},
	}
}

// percentile computes the p-th percentile (0..1) from a sorted slice using
// fractional index p*(n-1) with linear interpolation (numpy "linear" method).
func percentile(sorted []float64, p float64) float64 {
	n := len(sorted)
	if n == 0 {
		return 0
	}
	if n == 1 {
		return sorted[0]
	}

	idx := p * float64(n-1)
	lower := int(math.Floor(idx))
	upper := lower + 1
	if upper >= n {
		return sorted[n-1]
	}

	frac := idx - float64(lower)
	return sorted[lower] + frac*(sorted[upper]-sorted[lower])
}

// Of assigns a price its tier under b. Uses >= semantics: a price exactly at a
// boundary belongs to the higher tier.
func Of(price float64, b Boundaries) string {
	names := Order
	if len(b.Names) > 0 {
		names = b.Names
	}
	for i, boundary := range b.Boundaries {
		if price >= boundary {
			if i < len(names) {
				return names[i]
			}
			return names[len(names)-1]
		}
	}
	// Below all boundaries.
	belowIdx := len(b.Boundaries)
	if belowIdx < len(names) {
		return names[belowIdx]
	}
	return names[len(names)-1]
}
