package lab

import (
	"math"
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
	WindowSignal      string  // CLOSED, OPENING, OPEN, CLOSING
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

// AnalyzeTrends computes trend signals for all transfigured gems.
// current provides the latest snapshot; history provides time-series data per gem.
// baseHistory maps baseName → []PricePoint for non-transfigured gems.
// marketAvgBaseLst is the market-wide average base listings (denominator for relative liquidity).
func AnalyzeTrends(snapTime time.Time, current []GemPrice, history []GemPriceHistory,
	baseHistory map[string][]PricePoint, marketAvgBaseLst float64) []TrendResult {

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

		signal := classifySignal(priceVel, listingVel, cv)

		// Base-side signals.
		baseName := extractBaseName(g.Name)
		baseLst := baseCurrentListings[baseName]
		var baseVel float64
		if bp, ok := baseHistory[baseName]; ok && len(bp) >= 2 {
			baseVel = velocity(bp, func(p PricePoint) float64 { return float64(p.Listings) })
		}

		relLiq := computeRelativeLiquidity(float64(baseLst), marketAvgBaseLst)
		liqTier := liquidityTier(relLiq)
		winScore := computeWindowScore(g.Chaos, baseVel, float64(g.Listings), relLiq)
		winSignal := classifyWindowSignal(winScore, baseVel, listingVel)

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
// All inputs are relative — the score auto-adjusts for league phase, time of day, etc.
func computeWindowScore(roi, baseVelocity, transListings, relativeLiquidity float64) float64 {
	score := 0.0

	// High ROI contributes (capped contribution).
	if roi > 0 {
		score += math.Min(roi/10, 30) // max 30 points from ROI
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

// classifyWindowSignal determines the window state from score and velocities.
func classifyWindowSignal(windowScore, baseVelocity, transListingVelocity float64) string {
	if windowScore >= 70 && baseVelocity < -2 {
		return "OPEN"
	}
	if windowScore >= 50 && baseVelocity < 0 {
		return "OPENING"
	}
	if windowScore >= 50 && transListingVelocity > 3 {
		return "CLOSING"
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

// classifySignal determines the market signal based on velocity and CV.
func classifySignal(priceVel, listingVel, cv float64) string {
	if cv > 100 {
		return "TRAP"
	}
	if priceVel < -5 && listingVel > 5 {
		return "DUMPING"
	}
	if priceVel > 5 && listingVel > 10 {
		return "HERD"
	}
	if priceVel < -5 && listingVel < -5 {
		return "RECOVERY"
	}
	if math.Abs(priceVel) < 2 && math.Abs(listingVel) < 3 {
		return "STABLE"
	}
	if priceVel > 0 {
		return "RISING"
	}
	return "FALLING"
}
