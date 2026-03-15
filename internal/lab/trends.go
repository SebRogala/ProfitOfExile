package lab

import (
	"math"
	"sort"
	"strings"
	"time"
)

// TrendResult holds computed trend signals for a single transfigured gem.
type TrendResult struct {
	Time            time.Time
	Name            string
	Variant         string
	GemColor        string
	CurrentPrice    float64
	CurrentListings int
	PriceVelocity   float64 // chaos/hour, rolling 2h
	ListingVelocity float64 // listings/hour, rolling 2h
	CV              float64 // coefficient of variation (%)
	Signal          string  // STABLE, RISING, FALLING, DUMPING, HERD, RECOVERY, TRAP
	HistPosition    float64 // 0-100 percentile vs 7-day range
	PriceHigh7d     float64
	PriceLow7d      float64

	// Base-side signals
	BaseListings      int     // current base gem listings
	BaseVelocity      float64 // base listing change per hour
	RelativeLiquidity float64 // gem's base listings / market avg (0.0-N, where 1.0 = average)
	LiquidityTier     string  // LOW, MED, HIGH (derived from RelativeLiquidity)
	WindowScore       float64 // 0-100 composite score for farming opportunity
	WindowSignal      string  // CLOSED, BREWING, OPENING, OPEN, CLOSING, EXHAUSTED

	// Advanced signals (coexist with primary Signal)
	AdvancedSignal string // PRICE_MANIPULATION, BREAKOUT, COMEBACK, POTENTIAL, or "" (none)

	// Price tier signals
	PriceTier  string // TOP, MID, LOW — dynamic based on current market
	TierAction string // tier-specific recommended action, empty if no special guidance
}

// GemPriceHistory contains time-series data for a single gem.
type GemPriceHistory struct {
	Name     string
	Variant  string
	GemColor string
	Points   []PricePoint // sorted by time ASC
}

// PricePoint is a single price observation at a point in time.
type PricePoint struct {
	Time     time.Time
	Chaos    float64
	Listings int
}

// analysisVariants are the gem variants we analyze trends for.
var analysisVariants = map[string]bool{
	"1": true, "1/20": true, "20": true, "20/20": true,
}

// computePriceTiers calculates dynamic TOP/MID/LOW boundaries using the
// winsorized top-10 average (C_win formula). Outlier-proof via p99 cap.
func computePriceTiers(gems []GemPrice) (topThreshold, midThreshold float64) {
	return computePriceTiersWithConfig(gems, DefaultSignalConfig())
}

// computePriceTiersWithConfig uses custom tier multipliers (for optimizer).
func computePriceTiersWithConfig(gems []GemPrice, cfg SignalConfig) (topThreshold, midThreshold float64) {
	// Collect all transfigured gem prices
	var prices []float64
	for _, g := range gems {
		if g.IsTransfigured && !g.IsCorrupted && g.Chaos > 0 {
			prices = append(prices, g.Chaos)
		}
	}
	if len(prices) < 10 {
		return 100, 30 // fallback for very early league
	}

	sort.Float64s(prices)

	// p99 winsorization
	p99idx := int(float64(len(prices)) * 0.99)
	if p99idx >= len(prices) {
		p99idx = len(prices) - 1
	}
	p99 := prices[p99idx]

	// Winsorize at p99 to cap outliers.
	winsorized := make([]float64, len(prices))
	for i, p := range prices {
		winsorized[i] = math.Min(p, p99)
	}
	sort.Float64s(winsorized)

	// Use MEDIAN of positions #6-#15 (the mid-band of the top 15).
	// The top 5 are excluded as outliers; #6-#15 form a stable reference band.
	// This resists single-snapshot spikes much better than mean of top 10.
	top15start := len(winsorized) - 15
	if top15start < 0 {
		top15start = 0
	}
	top5start := len(winsorized) - 5
	if top5start < 0 {
		top5start = 0
	}
	// Fallback: if not enough gems for a mid-band, use the old top-10 average.
	var wt10 float64
	if top5start <= top15start {
		// Too few gems — fall back to top-10 average.
		top10start := len(winsorized) - 10
		if top10start < 0 {
			top10start = 0
		}
		sum := 0.0
		count := 0
		for i := top10start; i < len(winsorized); i++ {
			sum += winsorized[i]
			count++
		}
		wt10 = sum / float64(count)
	} else {
		// Mid-band: gems ranked #6-#15 by price. Take median.
		var midBand []float64
		for i := top15start; i < top5start; i++ {
			midBand = append(midBand, winsorized[i])
		}
		sort.Float64s(midBand)
		wt10 = midBand[len(midBand)/2]
	}

	return wt10 * cfg.TierTopMult, wt10 * cfg.TierMidMult
}

// classifyPriceTier assigns a price tier based on dynamic thresholds.
func classifyPriceTier(price, topThreshold, midThreshold float64) string {
	if price > topThreshold {
		return "TOP"
	}
	if price > midThreshold {
		return "MID"
	}
	return "LOW"
}

// tierAction returns the recommended action based on signal + tier combination.
func tierAction(signal, windowSignal, priceTier string) string {
	switch priceTier {
	case "TOP":
		switch signal {
		case "HERD":
			return "WATCH — early stage, monitor closely"
		case "DUMPING":
			return "SELL IMMEDIATELY"
		}
		switch windowSignal {
		case "BREWING":
			return "URGENT — window opens in ~45min"
		case "OPEN":
			return "HIGH RISK — act fast or skip"
		}
	case "MID":
		switch signal {
		case "HERD":
			return "SELL — move is over, exit position"
		case "RISING":
			return "CAUTIOUS — may reverse"
		}
		switch windowSignal {
		case "BREWING":
			return "WATCH — may reverse before opening"
		}
	case "LOW":
		switch signal {
		case "HERD":
			return "MOMENTUM — rising with crowd, watch for reversal"
		}
		switch windowSignal {
		case "OPEN":
			return "UNRELIABLE — low-value windows are traps"
		case "BREWING":
			return "SKIP — not actionable at this price"
		}
	}
	return ""
}

// AnalyzeTrends computes trend signals for all transfigured gems.
// current provides the latest snapshot; history provides time-series data per gem.
// baseHistory maps baseName → []PricePoint for non-transfigured gems.
// marketAvgBaseLst is the market-wide average base listings (denominator for relative liquidity).
func AnalyzeTrends(snapTime time.Time, current []GemPrice, history []GemPriceHistory,
	baseHistory map[string][]PricePoint, marketAvgBaseLst float64) []TrendResult {

	// Compute dynamic price tier thresholds from current snapshot.
	topThresh, midThresh := computePriceTiers(current)

	// Index history by (name, variant) for fast lookup.
	type histKey struct{ name, variant string }
	histIndex := make(map[histKey]*GemPriceHistory, len(history))
	for i := range history {
		h := &history[i]
		histIndex[histKey{h.Name, h.Variant}] = h
	}

	// Index current base gem listings by name for quick lookup.
	baseCurrentListings := make(map[string]int)
	for _, g := range current {
		if g.IsCorrupted || g.IsTransfigured {
			continue
		}
		// Keep the highest listing count per base name across variants.
		if g.Listings > baseCurrentListings[g.Name] {
			baseCurrentListings[g.Name] = g.Listings
		}
	}

	var results []TrendResult

	for _, g := range current {
		if g.IsCorrupted {
			continue
		}
		if !g.IsTransfigured {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}
		if g.Chaos <= 5 {
			continue
		}
		if !analysisVariants[g.Variant] {
			continue
		}

		h := histIndex[histKey{g.Name, g.Variant}]

		var priceVel, listingVel, cv, histPos, high7d, low7d float64
		if h != nil && len(h.Points) >= 2 {
			priceVel = velocity(h.Points, func(p PricePoint) float64 { return p.Chaos })
			listingVel = velocity(h.Points, func(p PricePoint) float64 { return float64(p.Listings) })

			prices := make([]float64, len(h.Points))
			for i, p := range h.Points {
				prices[i] = p.Chaos
			}
			cv = coefficientOfVariation(prices)
			high7d, low7d, histPos = historicalPosition(g.Chaos, prices)
		} else {
			// Not enough data — defaults: velocity 0, CV 0, position 50 (midpoint).
			high7d = g.Chaos
			low7d = g.Chaos
			histPos = 50
		}

		signal := classifySignal(priceVel, listingVel, cv, g.Listings)

		// Base-side signals.
		baseName := extractBaseName(g.Name)
		baseLst := -1 // sentinel: base not found
		if v, ok := baseCurrentListings[baseName]; ok {
			baseLst = v
		}
		var baseVel float64
		if bp, ok := baseHistory[baseName]; ok && len(bp) >= 2 {
			baseVel = velocity(bp, func(p PricePoint) float64 { return float64(p.Listings) })
		}

		baseLstForCalc := float64(baseLst)
		if baseLst < 0 {
			baseLstForCalc = 0
		}
		relLiq := computeRelativeLiquidity(baseLstForCalc, marketAvgBaseLst)
		liqTier := liquidityTier(relLiq)
		winScore := computeWindowScore(g.Chaos, baseVel, float64(g.Listings), relLiq)
		winSignal := classifyWindowSignal(winScore, baseVel, listingVel, baseLst, priceVel)
		advSignal := classifyAdvancedSignal(g.Chaos, g.Listings, priceVel, listingVel, cv, histPos)
		priceTier := classifyPriceTier(g.Chaos, topThresh, midThresh)
		tAction := tierAction(signal, winSignal, priceTier)

		results = append(results, TrendResult{
			Time:              snapTime,
			Name:              g.Name,
			Variant:           g.Variant,
			GemColor:          g.GemColor,
			CurrentPrice:      g.Chaos,
			CurrentListings:   g.Listings,
			PriceVelocity:     sanitizeFloat(priceVel),
			ListingVelocity:   sanitizeFloat(listingVel),
			CV:                cv,
			Signal:            signal,
			HistPosition:      sanitizeFloat(histPos),
			PriceHigh7d:       high7d,
			PriceLow7d:        low7d,
			BaseListings:      baseLst,
			BaseVelocity:      sanitizeFloat(baseVel),
			RelativeLiquidity: sanitizeFloat(relLiq),
			LiquidityTier:     liqTier,
			WindowScore:       sanitizeFloat(winScore),
			WindowSignal:      winSignal,
			AdvancedSignal:    advSignal,
			PriceTier:         priceTier,
			TierAction:        tAction,
		})
	}

	return results
}

// computeRelativeLiquidity returns the gem's base listings relative to the market average.
// Returns 1.0 (average) when market data is unavailable.
func computeRelativeLiquidity(gemBaseListings, marketAvgBaseListings float64) float64 {
	if marketAvgBaseListings <= 0 {
		return 1.0
	}
	return gemBaseListings / marketAvgBaseListings
}

// liquidityTier classifies relative liquidity into LOW, MED, or HIGH.
// Thresholds are relative to the market average — no hardcoded listing counts.
func liquidityTier(relativeLiquidity float64) string {
	if relativeLiquidity < 0.3 {
		return "LOW"
	}
	if relativeLiquidity < 0.8 {
		return "MED"
	}
	return "HIGH"
}

// computeWindowScore produces a 0-100 composite score for farming opportunity.
// currentPrice is the transfigured gem's chaos price (used as a proxy for opportunity value).
// All inputs are relative — the score auto-adjusts for league phase, time of day, etc.
func computeWindowScore(currentPrice, baseVelocity, transListings, relativeLiquidity float64) float64 {
	score := 0.0

	// High price contributes (capped contribution — expensive gems are more interesting targets).
	if currentPrice > 0 {
		score += math.Min(currentPrice/10, 30) // max 30 points from price
	}

	// Base draining (negative velocity = draining = good for window).
	if baseVelocity < 0 {
		score += math.Min(math.Abs(baseVelocity)*5, 25) // max 25 points
	}

	// Low trans listings (less competition).
	if transListings < 30 {
		score += 30 - transListings // max 30 points
	}

	// Low relative liquidity = window closes faster (urgency bonus).
	if relativeLiquidity < 0.5 {
		score += 15
	}

	return math.Min(score, 100)
}

// classifyWindowSignal determines the window state from score, velocities, and base listings.
// transListingVel is the transfigured gem's listing velocity (used for both CLOSING and BREWING checks).
func classifyWindowSignal(windowScore, baseVelocity, transListingVel float64, baseListings int, priceVelocity float64) string {
	return classifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel, baseListings, priceVelocity, DefaultSignalConfig())
}

// classifyWindowSignalWithConfig uses custom thresholds (for optimizer).
func classifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel float64, baseListings int, priceVelocity float64, cfg SignalConfig) string {
	// Base gems exhausted — unfarmable, window is dead.
	// baseListings < 0 means base gem not found (no data), skip exhaustion check.
	if baseListings >= 0 && baseListings <= 2 {
		return "EXHAUSTED"
	}
	// Herd output arriving — window closing
	if windowScore >= 50 && transListingVel > 3 {
		return "CLOSING"
	}
	// Dynamic drain threshold: configured % of base listings per hour.
	// Floor at ThinPoolFloor for thin pools (base<20), NormalFloor otherwise.
	floor := cfg.NormalFloor
	if baseListings > 0 && baseListings < 20 {
		floor = cfg.ThinPoolFloor
	}
	drainThreshold := math.Max(float64(baseListings)*-cfg.DrainPct, floor)

	// OPEN: high score + base draining relative to size + price momentum
	if windowScore >= 70 && baseVelocity < drainThreshold && priceVelocity > cfg.OpenMinPVel {
		return "OPEN"
	}
	// OPENING: moderate drain + some price momentum
	if windowScore >= 50 && baseVelocity < drainThreshold*0.5 && priceVelocity > 0 {
		return "OPENING"
	}
	// Pre-window: price rising + trans listings falling + bases still available
	if priceVelocity > cfg.BrewingMinPVel && transListingVel < 0 && baseListings > 10 {
		return "BREWING"
	}
	return "CLOSED"
}

// coefficientOfVariation computes stdev/|mean| * 100 for a slice of prices.
// Returns 0 for fewer than 2 values or zero mean.
func coefficientOfVariation(prices []float64) float64 {
	if len(prices) < 2 {
		return 0
	}

	var sum float64
	for _, p := range prices {
		sum += p
	}
	mean := sum / float64(len(prices))
	if mean == 0 {
		return 0
	}

	var variance float64
	for _, p := range prices {
		d := p - mean
		variance += d * d
	}
	variance /= float64(len(prices))

	return sanitizeFloat((math.Sqrt(variance) / math.Abs(mean)) * 100)
}

// sanitizeFloat returns 0 for NaN or Inf values, preventing bad data
// from poisoning batch INSERTs into PostgreSQL NUMERIC columns.
func sanitizeFloat(v float64) float64 {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return 0
	}
	return v
}

// velocity computes the rate of change per hour using last 4 data points (or fewer).
// Uses simple (last - first) / hours between first and last point.
func velocity(points []PricePoint, extract func(PricePoint) float64) float64 {
	n := len(points)
	if n < 2 {
		return 0
	}

	// Use at most the last 4 points for rolling 2h window.
	start := 0
	if n > 4 {
		start = n - 4
	}

	first := points[start]
	last := points[n-1]

	hours := last.Time.Sub(first.Time).Hours()
	if hours <= 0 {
		return 0
	}

	return (extract(last) - extract(first)) / hours
}

// historicalPosition returns the 7-day high, low, and the current price as a
// percentile (0-100) within that range.
func historicalPosition(current float64, prices []float64) (high, low, position float64) {
	if len(prices) == 0 {
		return current, current, 50
	}

	high = prices[0]
	low = prices[0]
	for _, p := range prices[1:] {
		if p > high {
			high = p
		}
		if p < low {
			low = p
		}
	}

	// Include current price in range.
	if current > high {
		high = current
	}
	if current < low {
		low = current
	}

	rang := high - low
	if rang <= 0 {
		return high, low, 50
	}

	position = ((current - low) / rang) * 100
	return high, low, position
}

// detectPriceManipulation identifies fake price floor attempts: very few listings
// at extreme price with no velocity and high historical volatility.
func detectPriceManipulation(currentListings int, currentPrice, priceVelocity, cv float64) bool {
	return currentListings <= 3 && currentPrice > 200 &&
		math.Abs(priceVelocity) < 1 && cv > 80
}

// detectRotationCandidate identifies gems that were previously high-ROI but dropped,
// now showing recovery signals: price in the bottom 30% of its 7-day range,
// price rising, and listings dropping (supply drying up).
func detectRotationCandidate(histPosition, priceVelocity, listingVelocity float64) bool {
	return histPosition < 30 && priceVelocity > 0 && listingVelocity < 0
}

// detectUndervalued identifies hidden opportunities: mid-range price, low listings,
// price rising, below historical midpoint — hasn't been discovered yet.
func detectUndervalued(currentPrice float64, currentListings int, priceVelocity, histPosition float64) bool {
	return currentPrice < 200 && currentPrice > 30 &&
		currentListings < 40 && priceVelocity > 2 &&
		histPosition < 50
}

// detectBreakout identifies LOW-tier gems with collapsing supply and rising price.
// These are the "surprise winners" that 3-10x — thin supply + any upward momentum.
func detectBreakout(currentPrice float64, currentListings int, priceVelocity, listingVelocity float64) bool {
	return detectBreakoutWithConfig(currentPrice, currentListings, priceVelocity, listingVelocity, DefaultSignalConfig())
}

// detectBreakoutWithConfig uses custom thresholds (for optimizer).
func detectBreakoutWithConfig(currentPrice float64, currentListings int, priceVelocity, listingVelocity float64, cfg SignalConfig) bool {
	return currentPrice < cfg.BreakoutMaxPrice && currentListings < cfg.BreakoutMaxList && listingVelocity < cfg.BreakoutMinLVel && priceVelocity > 0
}

// classifyAdvancedSignal determines the advanced signal for a gem.
// Priority: PRICE_MANIPULATION > BREAKOUT > COMEBACK > POTENTIAL.
func classifyAdvancedSignal(currentPrice float64, currentListings int, priceVelocity, listingVelocity, cv, histPosition float64) string {
	if detectPriceManipulation(currentListings, currentPrice, priceVelocity, cv) {
		return "PRICE_MANIPULATION"
	}
	if detectBreakout(currentPrice, currentListings, priceVelocity, listingVelocity) {
		return "BREAKOUT"
	}
	if detectRotationCandidate(histPosition, priceVelocity, listingVelocity) {
		return "COMEBACK"
	}
	if detectUndervalued(currentPrice, currentListings, priceVelocity, histPosition) {
		return "POTENTIAL"
	}
	return ""
}

// classifySignal determines the market signal based on velocity, CV, and current listings.
// currentListings is needed for RECOVERY detection (supply exhaustion at bottom).
func classifySignal(priceVel, listingVel, cv float64, currentListings int) string {
	return classifySignalWithConfig(priceVel, listingVel, cv, currentListings, DefaultSignalConfig())
}

// classifySignalWithConfig uses custom thresholds (for optimizer).
func classifySignalWithConfig(priceVel, listingVel, cv float64, currentListings int, cfg SignalConfig) string {
	if cv > cfg.TrapCV {
		return "TRAP"
	}
	if priceVel < cfg.DumpPriceVel && listingVel > cfg.DumpListingVel {
		return "DUMPING"
	}
	// High-velocity pre-HERD: extreme price movement with moderate listing growth
	if priceVel > cfg.PreHERDPriceVel && listingVel > cfg.PreHERDListingVel {
		return "HERD"
	}
	if priceVel > cfg.HERDPriceVel && listingVel > cfg.HERDListingVel {
		return "HERD"
	}
	// RECOVERY: price drifting down slowly, thin listings dropping = supply exhaustion (bottom forming).
	// Old rule (priceVel < -5 && listingVel < -5) fired mid-crash — 22.6% accurate.
	// New rule requires stabilization: price still negative but shallow, listings thin and draining.
	if priceVel < 0 && priceVel > cfg.DumpPriceVel && listingVel < -3 && currentListings < cfg.RecoveryMaxList {
		return "RECOVERY"
	}
	if math.Abs(priceVel) < cfg.StablePriceVel && math.Abs(listingVel) < cfg.StableListingVel {
		return "STABLE"
	}
	if priceVel > 0 {
		return "RISING"
	}
	return "FALLING"
}

// ClassifySignalWithConfig is the exported variant for use by the optimizer.
func ClassifySignalWithConfig(priceVel, listingVel, cv float64, currentListings int, cfg SignalConfig) string {
	return classifySignalWithConfig(priceVel, listingVel, cv, currentListings, cfg)
}

// ClassifyWindowSignalWithConfig is the exported variant for use by the optimizer.
func ClassifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel float64, baseListings int, priceVelocity float64, cfg SignalConfig) string {
	return classifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel, baseListings, priceVelocity, cfg)
}

// ComputePriceTiersWithConfig is the exported variant for use by the optimizer.
func ComputePriceTiersWithConfig(gems []GemPrice, cfg SignalConfig) (topThreshold, midThreshold float64) {
	return computePriceTiersWithConfig(gems, cfg)
}

// ClassifyPriceTier is the exported variant of classifyPriceTier.
func ClassifyPriceTier(price, topThreshold, midThreshold float64) string {
	return classifyPriceTier(price, topThreshold, midThreshold)
}
