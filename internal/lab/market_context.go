package lab

import (
	"fmt"
	"math"
	"sort"
	"time"
)

// PercentileKeys is the canonical list of percentile keys used for market context
// price and listing distributions. All call sites should iterate this slice
// rather than hardcoding key strings.
var PercentileKeys = []string{"P5", "P10", "P25", "P50", "P75", "P90", "P99"}

// ComputeMarketContext produces market-wide statistics from raw gem data and history.
// It is a pure function with no side effects — called from RunV2 and the optimizer.
// History may be nil when no historical data is available; velocity stats will be zero.
func ComputeMarketContext(snapTime time.Time, gems []GemPrice, history []GemPriceHistory) MarketContext {
	mc := MarketContext{
		Time:               snapTime,
		PricePercentiles:   make(map[string]float64),
		ListingPercentiles: make(map[string]float64),
	}

	// Compute temporal biases from historical data.
	tb := computeTemporalBiases(history)
	mc.HourlyBias = tb.HourlyBias[:]
	mc.HourlyVolatility = tb.HourlyVolatility[:]
	mc.HourlyActivity = tb.HourlyActivity[:]
	mc.WeekdayBias = tb.WeekdayBias[:]
	mc.WeekdayVolatility = tb.WeekdayVolatility[:]
	mc.WeekdayActivity = tb.WeekdayActivity[:]

	// Filter to active transfigured gems (not corrupted, exclude Trarthus).
	var active []GemPrice
	for _, g := range gems {
		if !isAnalyzableGem(g) {
			continue
		}
		active = append(active, g)
	}

	mc.TotalGems = len(active)
	for _, g := range active {
		mc.TotalListings += g.Listings
	}

	// Compute percentiles from active gems.
	if len(active) > 0 {
		prices := make([]float64, len(active))
		listings := make([]float64, len(active))
		for i, g := range active {
			prices[i] = g.Chaos
			listings[i] = float64(g.Listings)
		}
		sort.Float64s(prices)
		sort.Float64s(listings)

		for _, key := range PercentileKeys {
			p := percentileKeyToFraction(key)
			mc.PricePercentiles[key] = percentile(prices, p)
			mc.ListingPercentiles[key] = percentile(listings, p)
		}
	}

	// Compute velocity distribution from history.
	// Only include gems where velocity is computable (positive time span).
	// Entries with hours <= 0 (identical/very close timestamps) produce
	// false-zero velocities that would bias market-wide statistics.
	if len(history) > 0 {
		var velPrices, velListings []float64
		for _, h := range history {
			if len(h.Points) < 2 {
				continue
			}
			if !velocityComputable(h.Points) {
				continue
			}
			pv := velocity(h.Points, func(p PricePoint) float64 { return p.Chaos })
			lv := velocity(h.Points, func(p PricePoint) float64 { return float64(p.Listings) })
			pv = sanitizeFloat(pv)
			lv = sanitizeFloat(lv)
			velPrices = append(velPrices, pv)
			velListings = append(velListings, lv)
		}
		mc.VelocityMean, mc.VelocitySigma = meanStddev(velPrices)
		mc.ListingVelMean, mc.ListingVelSigma = meanStddev(velListings)
	}

	// Compute tier boundaries using natural gap detection.
	mc.TierBoundaries = DetectTierBoundaries(gems)

	return mc
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

// percentileKeyToFraction converts "P5", "P10", etc. to 0.05, 0.10, etc.
// Panics on unknown keys — the key list is hardcoded in PercentileKeys, so any
// mismatch is a programmer error that must be caught immediately rather than
// silently returning a wrong value.
func percentileKeyToFraction(key string) float64 {
	switch key {
	case "P5":
		return 0.05
	case "P10":
		return 0.10
	case "P25":
		return 0.25
	case "P50":
		return 0.50
	case "P75":
		return 0.75
	case "P90":
		return 0.90
	case "P99":
		return 0.99
	default:
		panic(fmt.Sprintf("percentileKeyToFraction: unknown key %q — update PercentileKeys and this switch together", key))
	}
}

// velocityComputable checks whether a price point series has a positive time span
// between first and last relevant points (matching the velocity() function's windowing).
// Returns false when timestamps are identical or reversed, which would cause velocity()
// to return a degenerate zero that does not represent genuine zero price change.
func velocityComputable(points []PricePoint) bool {
	n := len(points)
	if n < 2 {
		return false
	}
	// Match velocity()'s 2h window: find first point within 2h of the last.
	cutoff := points[n-1].Time.Add(-2 * time.Hour)
	for i := 0; i < n; i++ {
		if !points[i].Time.Before(cutoff) {
			// Need at least 2 points in window with a positive time span.
			if n-i < 2 {
				return false
			}
			return points[n-1].Time.Sub(points[i].Time).Hours() > 0
		}
	}
	return false
}

// meanStddev computes the mean and population standard deviation of a float slice.
// Returns (0, 0) for empty input. Output is sanitized to prevent NaN/Inf
// propagation from floating point overflow on extreme input magnitudes.
func meanStddev(vals []float64) (float64, float64) {
	n := len(vals)
	if n == 0 {
		return 0, 0
	}

	var sum float64
	for _, v := range vals {
		sum += v
	}
	mean := sum / float64(n)

	if n == 1 {
		return sanitizeFloat(mean), 0
	}

	var variance float64
	for _, v := range vals {
		d := v - mean
		variance += d * d
	}
	variance /= float64(n)

	return sanitizeFloat(mean), sanitizeFloat(math.Sqrt(variance))
}
