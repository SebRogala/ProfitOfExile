package lab

import (
	"math"
	"sort"
	"strings"
	"time"
)

// ComputeMarketContext produces market-wide statistics from raw gem data and history.
// It is a pure function with no side effects — called from RunV2 and the optimizer.
// History may be nil when no historical data is available; velocity stats will be zero.
func ComputeMarketContext(snapTime time.Time, gems []GemPrice, history []GemPriceHistory) MarketContext {
	mc := MarketContext{
		Time:               snapTime,
		PricePercentiles:   make(map[string]float64),
		ListingPercentiles: make(map[string]float64),
		HourlyBias:         make([]float64, 24),
		WeekdayBias:        make([]float64, 7),
	}

	// Stub bias values: 1.0 (neutral). POE-60 fills in real temporal patterns.
	for i := range mc.HourlyBias {
		mc.HourlyBias[i] = 1.0
	}
	for i := range mc.WeekdayBias {
		mc.WeekdayBias[i] = 1.0
	}

	// Filter to active transfigured gems (not corrupted, exclude Trarthus).
	var active []GemPrice
	for _, g := range gems {
		if !g.IsTransfigured || g.IsCorrupted {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
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

		for _, key := range []string{"P5", "P10", "P25", "P50", "P75", "P90", "P99"} {
			p := percentileKeyToFraction(key)
			mc.PricePercentiles[key] = percentile(prices, p)
			mc.ListingPercentiles[key] = percentile(listings, p)
		}
	}

	// Compute velocity distribution from history.
	if len(history) > 0 {
		var velPrices, velListings []float64
		for _, h := range history {
			if len(h.Points) < 2 {
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

	// Compute tier boundaries using existing computePriceTiers.
	// Pass all gems (including non-transfigured) since computePriceTiers filters internally.
	top, mid := computePriceTiers(gems)
	mc.TierBoundaries.Top = top
	mc.TierBoundaries.Mid = mid
	// High = midpoint between Top and Mid (placeholder — POE-57 adds gap detection).
	mc.TierBoundaries.High = (top + mid) / 2

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
		return 0.50
	}
}

// meanStddev computes the mean and population standard deviation of a float slice.
// Returns (0, 0) for empty input.
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
		return mean, 0
	}

	var variance float64
	for _, v := range vals {
		d := v - mean
		variance += d * d
	}
	variance /= float64(n)

	return mean, math.Sqrt(variance)
}
