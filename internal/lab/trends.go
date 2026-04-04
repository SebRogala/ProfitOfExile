package lab

import (
	"math"
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
	PriceVelocity   float64 // chaos/hour, computed from last 4 data points
	ListingVelocity float64 // listings/hour, computed from last 4 data points
	CV              float64 // coefficient of variation (%)
	Signal          string  // TRAP, DUMPING, HERD, RECOVERY, STABLE, UNCERTAIN (priority order)
	HistPosition    float64 // 0-100 percentile vs 7-day range
	PriceHigh7Days     float64
	PriceLow7Days      float64

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
	PriceTier  string // TOP, HIGH, MID, LOW — dynamic based on current market
	TierAction string // tier-specific recommended action, empty if no special guidance

	// Sell urgency (for gems you already have or are farming)
	SellUrgency string // SELL_NOW, UNDERCUT, HOLD, WAIT — actionable sell timing
	SellReason  string // human-readable explanation

	// Sellability (how easily will this gem sell right now, 0-100)
	Sellability      int    // 0-100 score
	SellabilityLabel string // FAST SELL, GOOD, MODERATE, SLOW, UNLIKELY
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

// sellUrgency computes actionable sell timing for gems you already have.
// Uses price velocity, listing velocity, base drain, and historical position.
func sellUrgency(priceVel, listingVel, baseVel, histPosition float64, baseListings, transListings int, signal, priceTier string) (urgency, reason string) {
	// LOW tier: suppress most sell signals — low gems bounce constantly
	if priceTier == "LOW" {
		if signal == "HERD" {
			return "SELL_NOW", "Herd arrived on low-value gem — sell into momentum"
		}
		if math.Abs(priceVel) < 2 {
			return "WAIT", "Low-tier gem — stable, list at market price"
		}
		return "HOLD", "Low-tier gem — volatile, wait for direction"
	}

	// TRAP = sell immediately regardless
	if signal == "TRAP" {
		return "SELL_NOW", "Extreme volatility — sell at any price before crash"
	}

	// HERD at peak = override everything to UNDERCUT (catches Lacerate peak scenario)
	if signal == "HERD" && histPosition > 90 {
		return "UNDERCUT", "HERD at historical peak — undercut 10-15% NOW before crash"
	}

	// DUMPING: thin market = sell immediately, liquid market = undercut (likely noise).
	if signal == "DUMPING" {
		if transListings >= 20 {
			return "UNDERCUT", "Price softening — undercut 5-10% for fast sale"
		}
		return "SELL_NOW", "Price dropping with rising supply — undercut hard"
	}

	// HIGH tier — competitive cluster, sell timing critical but less extreme than TOP
	if priceTier == "HIGH" {
		if signal == "HERD" && histPosition > 80 {
			return "UNDERCUT", "Herd at elevated price — undercut 5-10% for fast sale"
		}
		if signal == "HERD" {
			return "UNDERCUT", "Herd on competitive gem — undercut to sell into pressure"
		}
	}

	// Bases evaporating on a TOP gem = herd output coming in ~30min
	if priceTier == "TOP" && baseListings >= 0 && baseListings < 10 && baseVel < -2 {
		return "UNDERCUT", "Bases nearly gone — herd output arrives in ~30min, undercut 10-15%"
	}

	// HERD on MID = the move is over, sell into the herd
	if signal == "HERD" && priceTier == "MID" {
		return "SELL_NOW", "Herd arrived — sell into buying pressure before price drops"
	}

	// Near peak + listings rising = confirmed peak zone (backtested: -2.87% avg, works)
	if histPosition > 80 && listingVel > 3 {
		return "UNDERCUT", "Near peak — listings rising, undercut 5-10% for fast sale"
	}

	// Price rising fast + trans listings very thin = hold for peak
	if priceVel > 10 && transListings < 10 {
		return "HOLD", "Price surging with very thin supply — hold, but watch for listing spike"
	}

	// Price rising moderately + stable listings = healthy, no rush
	if priceVel > 2 && math.Abs(listingVel) < 5 {
		return "HOLD", "Price rising steadily — no rush to sell"
	}

	// Price stable
	if math.Abs(priceVel) < 2 {
		return "WAIT", "Price stable — list at market price, will sell eventually"
	}

	// Price falling from elevated position = undercut (gated by histPos > 60)
	if priceVel < -2 && histPosition > 60 {
		return "UNDERCUT", "Price softening from elevated level — list below current"
	}

	return "HOLD", "No strong sell signal — hold and monitor"
}

// sellability computes how easily a gem will sell (0-100).
// Higher = faster sale expected. Combines listing dynamics + signal health.
// All thresholds are relative (percentage-based) so they work across price tiers.
func sellability(transListings int, listingVel, priceVel, cv float64, signal string, marketDepth, chaos float64) (score int, label string) {
	s := 50 // baseline

	// Percentage-based velocities for tier-agnostic comparison.
	// A 46c/hr move on a 1099c gem (4.2%) is very different from 46c/hr on a 50c gem (92%).
	var pctPriceVel, pctListingVel float64
	if chaos > 0 {
		pctPriceVel = priceVel / chaos * 100
	}
	if transListings > 0 {
		pctListingVel = listingVel / float64(transListings) * 100
	}

	// Market depth relative to per-variant median (league-invariant).
	// Deep markets (>2x) are proven markets with active buyers — bonus, not penalty.
	// Thin markets have less competition but uncertain demand.
	if marketDepth < 0.5 {
		s += 15 // thin: less competition but uncertain demand
	} else if marketDepth > 2.0 {
		s += 15 // deep: proven market, active buyers, your gem WILL sell
	}

	// Price rising = buyers active (percentage-based)
	if pctPriceVel > 3 {
		s += 15
	} else if pctPriceVel > 0 {
		s += 5
	} else if pctPriceVel < -3 {
		s -= 15
	}

	// Listings changing relative to current count
	if pctListingVel < -5 {
		s += 10 // supply drying = your listing gets seen
	} else if pctListingVel > 10 {
		s -= 10 // supply flooding
	}

	// Recent stability bonus: currently calm (percentage-based)
	if math.Abs(pctPriceVel) < 2 {
		s += 20
	}

	// Turnover proxy: listings dropping while price holds = items ARE selling
	if pctListingVel < -2 && math.Abs(pctPriceVel) < 3 {
		s += 25
	}

	// Low CV = predictable market = buyers trust the price
	if cv < 25 {
		s += 10
	} else if cv > 80 {
		s -= 10
	}

	// HERD/DUMPING = price is moving, creates urgency for buyers
	if signal == "HERD" {
		s += 5 // momentum creates FOMO buyers
	}
	if signal == "DUMPING" || signal == "TRAP" {
		s -= 20 // buyers avoid these
	}

	// Clamp
	if s > 100 {
		s = 100
	}
	if s < 0 {
		s = 0
	}

	// Label
	switch {
	case s >= 80:
		label = "FAST SELL"
	case s >= 60:
		label = "GOOD"
	case s >= 40:
		label = "MODERATE"
	case s >= 20:
		label = "SLOW"
	default:
		label = "UNLIKELY"
	}

	return s, label
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
	case "HIGH":
		switch signal {
		case "HERD":
			return "UNDERCUT — herd arrived, sell into pressure"
		case "DUMPING":
			return "SELL IMMEDIATELY"
		case "UNCERTAIN":
			return "MONITOR — direction unclear, watch velocity"
		}
		switch windowSignal {
		case "BREWING":
			return "WATCH — window forming on competitive gem"
		case "OPEN":
			return "ACT — window open, time-sensitive"
		}
	case "MID":
		switch signal {
		case "HERD":
			return "SELL — move is over, exit position"
		case "UNCERTAIN":
			return "MONITOR — direction unclear, check listings"
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

// extractRecentPrices returns the chaos prices from points within the given
// duration before refTime. Used to compute short-window CV for stability discount.
func extractRecentPrices(points []PricePoint, refTime time.Time, window time.Duration) []float64 {
	cutoff := refTime.Add(-window)
	var prices []float64
	for _, p := range points {
		if !p.Time.Before(cutoff) {
			prices = append(prices, p.Chaos)
		}
	}
	return prices
}

// sanitizeFloat returns 0 for NaN or Inf values, preventing bad data
// from poisoning batch INSERTs into PostgreSQL NUMERIC columns.
func sanitizeFloat(v float64) float64 {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return 0
	}
	return v
}

// velocity and velocityWindow are defined in velocity.go.

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
// Velocities are converted to percentages relative to price/listings before threshold checks,
// making signals tier-agnostic (a 3% move means the same for 1500c and 30c gems).
func classifySignal(priceVel, listingVel, cv float64, currentPrice float64, currentListings int) string {
	return classifySignalWithConfig(priceVel, listingVel, cv, currentPrice, currentListings, DefaultSignalConfig())
}

// classifySignalWithConfig uses custom thresholds (for optimizer).
// All velocity thresholds in cfg are percentages; priceVel/listingVel are absolute and converted here.
func classifySignalWithConfig(priceVel, listingVel, cv float64, currentPrice float64, currentListings int, cfg SignalConfig) string {
	// Convert absolute velocities to percentages.
	// Gems with zero price or zero listings can't be meaningfully classified.
	if currentPrice <= 0 || currentListings <= 0 {
		return "UNCERTAIN"
	}
	pVelPct := priceVel / currentPrice * 100
	lVelPct := listingVel / float64(currentListings) * 100

	// TRAP requires BOTH high historical volatility AND current instability.
	// A gem with high 7-day CV but stable recent prices is not a trap —
	// it had a volatile episode earlier but has since settled.
	if cv > cfg.TrapCV && (math.Abs(pVelPct) > cfg.TrapVelPct || math.Abs(lVelPct) > cfg.TrapVelPct) {
		return "TRAP"
	}
	if pVelPct < cfg.DumpPriceVelPct && lVelPct > cfg.DumpListingVelPct {
		return "DUMPING"
	}
	// High-velocity pre-HERD: extreme price movement with moderate listing growth
	if pVelPct > cfg.PreHERDPriceVelPct && lVelPct > cfg.PreHERDListingVelPct {
		return "HERD"
	}
	if pVelPct > cfg.HERDPriceVelPct && lVelPct > cfg.HERDListingVelPct {
		return "HERD"
	}
	// RECOVERY: price drifting down slowly, thin listings dropping = supply exhaustion (bottom forming).
	// Requires thin market (absolute listing cap) + listing drain (relative %).
	if pVelPct < 0 && pVelPct > cfg.DumpPriceVelPct && lVelPct < cfg.RecoveryListingVelPct && currentListings < cfg.RecoveryMaxListings {
		return "RECOVERY"
	}
	if math.Abs(pVelPct) < cfg.StablePriceVelPct && math.Abs(lVelPct) < cfg.StableListingVelPct {
		return "STABLE"
	}
	return "UNCERTAIN"
}

// ClassifySignalWithConfig is the exported variant for use by the optimizer.
func ClassifySignalWithConfig(priceVel, listingVel, cv float64, currentPrice float64, currentListings int, cfg SignalConfig) string {
	return classifySignalWithConfig(priceVel, listingVel, cv, currentPrice, currentListings, cfg)
}

// ClassifyWindowSignalWithConfig is the exported variant for use by the optimizer.
func ClassifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel float64, baseListings int, priceVelocity float64, cfg SignalConfig) string {
	return classifyWindowSignalWithConfig(windowScore, baseVelocity, transListingVel, baseListings, priceVelocity, cfg)
}

// TierActionFor is the exported wrapper around tierAction for handler use.
func TierActionFor(signal, windowSignal, priceTier string) string {
	return tierAction(signal, windowSignal, priceTier)
}

// LiquidityTierFor is the exported wrapper around liquidityTier for handler use.
func LiquidityTierFor(marketDepth float64) string {
	return liquidityTier(marketDepth)
}

