package lab

import (
	"math"
	"sort"
)

// countFloods counts listing spike events in the price history.
// A flood is a consecutive listing increase that exceeds median + 4*MAD of the
// full delta distribution AND has an absolute delta >= 5. Uses median and MAD
// (median absolute deviation) as robust estimators that resist outlier contamination.
// The 4-sigma-equivalent threshold catches genuine supply shocks. The absolute
// floor of 5 prevents noise on dead gems (e.g. 1 -> 2 listings).
//
// Returns 0 if fewer than 5 points or if listing variance is zero.
func countFloods(points []PricePoint) int {
	if len(points) < 5 {
		return 0
	}

	// Compute all listing deltas between consecutive points.
	deltas := make([]float64, 0, len(points)-1)
	for i := 1; i < len(points); i++ {
		deltas = append(deltas, float64(points[i].Listings-points[i-1].Listings))
	}

	if len(deltas) < 2 {
		return 0
	}

	med, mad := medianAndMAD(deltas)

	// When MAD is 0 (most deltas are identical), use a minimum of 1.0 as the
	// baseline variation unit. This ensures that any spike well above the normal
	// flat line is still detectable. If even the fallback finds no variation
	// (all deltas are truly identical), the absoluteFloor check still protects.
	effectiveMAD := mad
	if effectiveMAD <= 0 {
		effectiveMAD = 1.0
	}

	threshold := med + 4*effectiveMAD
	const absoluteFloor = 5.0

	count := 0
	for _, d := range deltas {
		if d >= threshold && d >= absoluteFloor {
			count++
		}
	}

	return count
}

// countCrashes counts crash events: sharp price drops coinciding with listing increases.
// A crash requires both conditions between consecutive points:
//   - Price dropped more than median - 4*MAD of price-delta distribution (a statistical outlier)
//   - Listings increased in the same interval (supply flood)
//
// Uses median and MAD (median absolute deviation) as robust estimators so that
// the crash outlier itself does not inflate the baseline statistics.
// The dual condition ensures only flood-induced crashes are counted, not organic
// price corrections or market-wide downturns.
//
// Returns 0 if fewer than 5 points or if price-delta variance is zero.
func countCrashes(points []PricePoint) int {
	if len(points) < 5 {
		return 0
	}

	// Compute fractional price deltas.
	var pDeltas []float64
	for i := 1; i < len(points); i++ {
		if points[i-1].Chaos <= 0 {
			continue
		}
		pDeltas = append(pDeltas, (points[i].Chaos-points[i-1].Chaos)/points[i-1].Chaos)
	}

	if len(pDeltas) < 2 {
		return 0
	}

	med, mad := medianAndMAD(pDeltas)

	// When MAD is 0 (most price deltas are identical), use a minimum of 0.01
	// (1% price change) as the baseline variation unit. This ensures crash
	// detection works even when price has been flat for most of the history.
	effectiveMAD := mad
	if effectiveMAD <= 0 {
		effectiveMAD = 0.01
	}

	// Crash threshold: a drop exceeding 4*MAD below the median.
	crashThreshold := med - 4*effectiveMAD

	count := 0
	for i := 1; i < len(points); i++ {
		if points[i-1].Chaos <= 0 {
			continue
		}
		pctChange := (points[i].Chaos - points[i-1].Chaos) / points[i-1].Chaos
		listingDelta := points[i].Listings - points[i-1].Listings

		// Dual condition: price crashed AND listings rose.
		if pctChange < crashThreshold && listingDelta > 0 {
			count++
		}
	}

	return count
}

// computeListingElasticity computes the price sensitivity to listing changes over
// the full history span. Elasticity = %DeltaPrice / %DeltaListings.
//
// Negative elasticity means healthy price discovery (price falls when supply rises).
// Near-zero means price insensitive to supply (thin or manipulated market).
// Positive means unusual (price rises with supply -- possible HERD behavior).
//
// Returns 0 if fewer than 5 points, if start values are zero (can't compute %),
// or if listing change is negligible (< 1%).
func computeListingElasticity(points []PricePoint) float64 {
	if len(points) < 5 {
		return 0
	}

	first := points[0]
	last := points[len(points)-1]

	if first.Chaos <= 0 || first.Listings <= 0 {
		return 0
	}

	pctPrice := (last.Chaos - first.Chaos) / first.Chaos
	pctListings := (float64(last.Listings) - float64(first.Listings)) / float64(first.Listings)

	// Can't compute elasticity with near-zero listing change.
	if math.Abs(pctListings) < 0.01 {
		return 0
	}

	return sanitizeFloat(pctPrice / pctListings)
}

// medianAndMAD computes the median and MAD (median absolute deviation) of a float64 slice.
// MAD is a robust measure of variability that resists outlier contamination, unlike
// standard deviation which is heavily influenced by extreme values. This makes it
// ideal for detecting genuine outliers in price/listing data where the outliers
// themselves would otherwise inflate the sigma and mask the very events we're detecting.
func medianAndMAD(vals []float64) (float64, float64) {
	if len(vals) == 0 {
		return 0, 0
	}

	sorted := make([]float64, len(vals))
	copy(sorted, vals)
	sort.Float64s(sorted)

	med := median(sorted)

	// Compute absolute deviations from median.
	absDevs := make([]float64, len(sorted))
	for i, v := range sorted {
		absDevs[i] = math.Abs(v - med)
	}
	sort.Float64s(absDevs)

	mad := median(absDevs)

	return med, mad
}

// median returns the median of a pre-sorted float64 slice.
func median(sorted []float64) float64 {
	n := len(sorted)
	if n == 0 {
		return 0
	}
	if n%2 == 1 {
		return sorted[n/2]
	}
	return (sorted[n/2-1] + sorted[n/2]) / 2
}
