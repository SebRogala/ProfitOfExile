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
func AnalyzeTrends(snapTime time.Time, current []GemPrice, history []GemPriceHistory) []TrendResult {
	// Index history by (name, variant) for fast lookup.
	type histKey struct{ name, variant string }
	histIndex := make(map[histKey]*GemPriceHistory, len(history))
	for i := range history {
		h := &history[i]
		histIndex[histKey{h.Name, h.Variant}] = h
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

		results = append(results, TrendResult{
			Time:            snapTime,
			Name:            g.Name,
			Variant:         g.Variant,
			GemColor:        g.GemColor,
			CurrentPrice:    g.Chaos,
			CurrentListings: g.Listings,
			PriceVelocity:   sanitizeFloat(priceVel),
			ListingVelocity: sanitizeFloat(listingVel),
			CV:              cv,
			Signal:          signal,
			HistPosition:    sanitizeFloat(histPos),
			PriceHigh7d:     high7d,
			PriceLow7d:      low7d,
		})
	}

	return results
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
