package lab

import (
	"encoding/json"
	"math"
	"sort"
	"time"
)

// TemporalBucket holds the computed statistics for a single hourly bucket
// in the temporal normalization system.
type TemporalBucket struct {
	Hour  int     `json:"hour"`
	Coeff float64 `json:"coeff"` // coefficient = bucket_median / baseline (1.0 = neutral)
	N     int     `json:"n"`     // number of data points in this bucket
}

// minBucketSamples is the minimum number of samples required in every bucket
// before that granularity level is considered valid.
const minBucketSamples = 3

// observation is a single price data point used for temporal detrending.
type observation struct {
	t     time.Time
	chaos float64
}

// bucketEntry collects detrended price values for a single temporal bucket.
type bucketEntry struct {
	values []float64
}

// computeTemporalCoefficients computes per-variant temporal coefficients from
// raw gem price history. It returns the coefficient for the given snapTime,
// the active mode, and the raw bucket data as JSONB.
//
// Algorithm:
//  1. Group transfigured gem prices (chaos > 1, not corrupted) by variant.
//  2. For each variant, group historical prices by (weekday, hour) bucket
//     using a rolling 7-day window up to snapTime.
//  3. Detrend: compute linear trend (price vs time) over the 7-day window.
//     Subtract the trend from each price point before computing bucket medians.
//  4. For each bucket: compute median price of all detrended data points.
//  5. Global baseline = mean of per-hour bucket medians (robust to unbalanced sample counts).
//  6. Coefficient for bucket = bucket_median / global_baseline.
//  7. Current coefficient = lookup the bucket matching snapTime's (weekday, hour).
//
// Progressive granularity:
//   - If fewer than minBucketSamples in any weekday x hour bucket: fall back to hour-only.
//   - If fewer than minBucketSamples in any hour-only bucket: mode = "none", coefficient = 1.0.
func computeTemporalCoefficients(snapTime time.Time, history []GemPriceHistory) (float64, string, []byte) {
	if len(history) == 0 {
		return 1.0, "none", []byte("{}")
	}

	// Collect per-variant price observations within the 7-day window.
	cutoff := snapTime.Add(-7 * 24 * time.Hour)

	// variantObs maps variant -> observations
	variantObs := make(map[string][]observation)

	for _, h := range history {
		if h.GemColor == "" {
			continue // skip non-transfigured
		}
		for _, p := range h.Points {
			if p.Chaos <= 1 || p.Time.Before(cutoff) || p.Time.After(snapTime) {
				continue
			}
			variantObs[h.Variant] = append(variantObs[h.Variant], observation{
				t:     p.Time,
				chaos: p.Chaos,
			})
		}
	}

	if len(variantObs) == 0 {
		return 1.0, "none", []byte("{}")
	}

	// For each variant, detrend and bucket.
	// Try weekday_hour first, then hourly, then none.
	// allBuckets maps variant -> buckets
	allWeekdayHourBuckets := make(map[string]map[int]*bucketEntry)  // key: weekday*24+hour
	allHourlyBuckets := make(map[string]map[int]*bucketEntry)       // key: hour
	allVariantBaselines := make(map[string]float64)

	// Result bucket data for JSONB
	resultBuckets := make(map[string][]TemporalBucket)

	for variant, obs := range variantObs {
		if len(obs) < 2 {
			continue
		}

		// Detrend: compute linear regression (price vs time).
		detrended := detrendPrices(obs)

		// Bucket by weekday*24+hour.
		whBuckets := make(map[int]*bucketEntry)
		hBuckets := make(map[int]*bucketEntry)
		for _, d := range detrended {
			wh := int(d.t.UTC().Weekday())*24 + d.t.UTC().Hour()
			h := d.t.UTC().Hour()

			if whBuckets[wh] == nil {
				whBuckets[wh] = &bucketEntry{}
			}
			whBuckets[wh].values = append(whBuckets[wh].values, d.chaos)

			if hBuckets[h] == nil {
				hBuckets[h] = &bucketEntry{}
			}
			hBuckets[h].values = append(hBuckets[h].values, d.chaos)
		}

		allWeekdayHourBuckets[variant] = whBuckets
		allHourlyBuckets[variant] = hBuckets

		// Compute baseline = mean of hourly bucket medians.
		// Using mean of bucket medians (not global median) is robust to unbalanced
		// sample counts across hours — each hour contributes equally to the baseline.
		var bucketMedianSum float64
		var bucketCount int
		for _, be := range hBuckets {
			if len(be.values) > 0 {
				bucketMedianSum += medianFloat64(be.values)
				bucketCount++
			}
		}
		if bucketCount > 0 {
			allVariantBaselines[variant] = bucketMedianSum / float64(bucketCount)
		}
	}

	if len(allVariantBaselines) == 0 {
		return 1.0, "none", []byte("{}")
	}

	// Determine mode by checking the variant matching the snapTime.
	// We check ALL variants — if any variant has insufficient data, we downgrade.
	mode := determineMode(allWeekdayHourBuckets, allHourlyBuckets)

	// Build the result bucket data and compute the current coefficient.
	var currentCoeff float64 = 1.0
	snapHour := snapTime.UTC().Hour()
	snapWeekdayHour := int(snapTime.UTC().Weekday())*24 + snapHour

	for variant, baseline := range allVariantBaselines {
		var buckets []TemporalBucket

		switch mode {
		case "weekday_hour":
			whBuckets := allWeekdayHourBuckets[variant]
			for key, be := range whBuckets {
				med := medianFloat64(be.values)
				buckets = append(buckets, TemporalBucket{
					Hour: key,
					Coeff:  sanitizeCoeff(med / baseline),
					N:    len(be.values),
				})
			}
		case "hourly":
			hBuckets := allHourlyBuckets[variant]
			for hour, be := range hBuckets {
				med := medianFloat64(be.values)
				buckets = append(buckets, TemporalBucket{
					Hour: hour,
					Coeff:  sanitizeCoeff(med / baseline),
					N:    len(be.values),
				})
			}
		default:
			// mode = "none" — no bucketing
		}

		// Sort buckets by hour for deterministic output.
		sort.Slice(buckets, func(i, j int) bool {
			return buckets[i].Hour < buckets[j].Hour
		})

		resultBuckets[variant] = buckets
	}

	// Compute the current coefficient for the snapTime.
	// Use an average across all variants at the current time bucket.
	// Fallback chain: weekday×hour → hour-only → 1.0
	if mode != "none" {
		var coeffSum float64
		var coeffCount int
		for variant, baseline := range allVariantBaselines {
			var bucketMedian float64
			if mode == "weekday_hour" {
				if be, ok := allWeekdayHourBuckets[variant][snapWeekdayHour]; ok && len(be.values) > 0 {
					bucketMedian = medianFloat64(be.values)
				}
			}
			// Fallback to hour-only if weekday×hour bucket has no data for current time.
			if bucketMedian == 0 {
				if be, ok := allHourlyBuckets[variant][snapHour]; ok && len(be.values) > 0 {
					bucketMedian = medianFloat64(be.values)
				}
			}
			if bucketMedian > 0 && baseline > 0 {
				coeffSum += bucketMedian / baseline
				coeffCount++
			}
		}
		if coeffCount > 0 {
			currentCoeff = sanitizeCoeff(coeffSum / float64(coeffCount))
		}
	}

	bucketsJSON, err := json.Marshal(resultBuckets)
	if err != nil {
		return 1.0, "none", []byte("{}")
	}

	return currentCoeff, mode, bucketsJSON
}

// determineMode decides the temporal granularity based on available data.
// It checks all variants — if any has a bucket below minBucketSamples,
// it downgrades to the next level.
func determineMode(
	whBuckets map[string]map[int]*bucketEntry,
	hBuckets map[string]map[int]*bucketEntry,
) string {
	// Try weekday_hour first.
	allSufficient := true
	for _, buckets := range whBuckets {
		for _, be := range buckets {
			if len(be.values) < minBucketSamples {
				allSufficient = false
				break
			}
		}
		if !allSufficient {
			break
		}
	}
	if allSufficient && len(whBuckets) > 0 {
		return "weekday_hour"
	}

	// Try hourly.
	allSufficient = true
	for _, buckets := range hBuckets {
		for _, be := range buckets {
			if len(be.values) < minBucketSamples {
				allSufficient = false
				break
			}
		}
		if !allSufficient {
			break
		}
	}
	if allSufficient && len(hBuckets) > 0 {
		return "hourly"
	}

	return "none"
}

// detrendPrices applies linear detrending to a set of observations.
// It computes a linear regression of price vs. time (as hours since earliest point),
// then subtracts the trend component, preserving the mean level.
func detrendPrices(obs []observation) []observation {
	if len(obs) < 2 {
		result := make([]observation, len(obs))
		copy(result, obs)
		return result
	}

	// Use the earliest time as the reference point.
	t0 := obs[0].t
	for _, o := range obs {
		if o.t.Before(t0) {
			t0 = o.t
		}
	}

	// Compute linear regression: price = a + b*hours.
	var sumX, sumY, sumXX, sumXY float64
	n := float64(len(obs))
	for _, o := range obs {
		x := o.t.Sub(t0).Hours()
		y := o.chaos
		sumX += x
		sumY += y
		sumXX += x * x
		sumXY += x * y
	}

	meanX := sumX / n
	meanY := sumY / n
	denom := sumXX - sumX*meanX
	var slope float64
	if math.Abs(denom) > 1e-12 {
		slope = (sumXY - sumX*meanY) / denom
	}

	// Subtract trend, keeping the mean price level.
	result := make([]observation, len(obs))
	for i, o := range obs {
		x := o.t.Sub(t0).Hours()
		trendComponent := slope * (x - meanX)
		detrended := o.chaos - trendComponent
		if detrended < 0 {
			detrended = 0
		}
		result[i] = observation{t: o.t, chaos: detrended}
	}

	return result
}

// sanitizeCoeff returns v if finite and positive, otherwise 1.0 (neutral coefficient).
// Unlike sanitizeFloat (which returns 0 for NaN/Inf), coefficients must never be 0
// as they are used as divisors in NormalizeHistory.
func sanitizeCoeff(v float64) float64 {
	if math.IsNaN(v) || math.IsInf(v, 0) || v <= 0 {
		return 1.0
	}
	return v
}

// medianFloat64 computes the median of a float64 slice. Returns 0 for empty input.
// Does NOT modify the input slice.
func medianFloat64(vals []float64) float64 {
	if len(vals) == 0 {
		return 0
	}
	sorted := make([]float64, len(vals))
	copy(sorted, vals)
	sort.Float64s(sorted)
	n := len(sorted)
	if n%2 == 0 {
		return (sorted[n/2-1] + sorted[n/2]) / 2.0
	}
	return sorted[n/2]
}

// CoefficientAt returns the temporal coefficient for the given timestamp and variant.
// It looks up the per-variant coefficient from TemporalBuckets with a fallback chain:
// weekday×hour → hour-only → 1.0. Falls back to 1.0 when data is unavailable or mode is "none".
func (mc MarketContext) CoefficientAt(t time.Time, variant string) float64 {
	if mc.TemporalMode == "none" || mc.TemporalMode == "" || len(mc.TemporalBuckets) == 0 {
		return 1.0
	}

	// Parse bucket data. In hot paths (NormalizeHistory), callers should use
	// ParsedCoefficientAt instead to avoid repeated deserialization.
	var bucketData map[string][]TemporalBucket
	if err := json.Unmarshal(mc.TemporalBuckets, &bucketData); err != nil {
		return 1.0
	}

	return LookupCoefficient(bucketData, mc.TemporalMode, t, variant)
}

// LookupCoefficient performs the actual coefficient lookup with fallback chain.
// Exported for use by handlers that pre-parse bucket data for performance.
func LookupCoefficient(bucketData map[string][]TemporalBucket, mode string, t time.Time, variant string) float64 {
	buckets, ok := bucketData[variant]
	if !ok || len(buckets) == 0 {
		return 1.0
	}

	hour := t.UTC().Hour()

	if mode == "weekday_hour" {
		// Try weekday×hour first.
		whKey := int(t.UTC().Weekday())*24 + hour
		for _, b := range buckets {
			if b.Hour == whKey && b.Coeff > 0 {
				return b.Coeff
			}
		}
		// Fallback: try any bucket matching this hour (collapse weekdays).
		for _, b := range buckets {
			if b.Hour%24 == hour && b.Coeff > 0 {
				return b.Coeff
			}
		}
		return 1.0
	}

	if mode == "hourly" {
		for _, b := range buckets {
			if b.Hour == hour && b.Coeff > 0 {
				return b.Coeff
			}
		}
	}

	return 1.0
}

// NormalizeHistory creates a copy of the history with prices adjusted by temporal
// coefficients. Listings are preserved raw. The coefficientAt function looks up
// the per-variant coefficient for each point's timestamp.
//
// For the hot path, use NormalizeHistoryFromMC which parses bucket data once.
func NormalizeHistory(history []GemPriceHistory, coefficientAt func(time.Time, string) float64) []GemPriceHistory {
	if len(history) == 0 {
		return nil
	}

	normalized := make([]GemPriceHistory, len(history))
	for i, h := range history {
		nh := GemPriceHistory{
			Name:     h.Name,
			Variant:  h.Variant,
			GemColor: h.GemColor,
			Points:   make([]PricePoint, len(h.Points)),
		}
		for j, p := range h.Points {
			coeff := coefficientAt(p.Time, h.Variant)
			if coeff <= 0 {
				coeff = 1.0
			}
			nh.Points[j] = PricePoint{
				Time:     p.Time,
				Chaos:    p.Chaos / coeff,
				Listings: p.Listings,
			}
		}
		normalized[i] = nh
	}

	return normalized
}

// NormalizeHistoryFromMC is the optimized version that parses bucket data once.
// Use this in the hot path (RunV2, backfill) instead of NormalizeHistory + mc.CoefficientAt.
func NormalizeHistoryFromMC(history []GemPriceHistory, mc MarketContext) []GemPriceHistory {
	if mc.TemporalMode == "none" || mc.TemporalMode == "" || len(mc.TemporalBuckets) == 0 {
		return history // no normalization needed, return original (no copy)
	}

	var bucketData map[string][]TemporalBucket
	if err := json.Unmarshal(mc.TemporalBuckets, &bucketData); err != nil {
		return history
	}

	return NormalizeHistory(history, func(t time.Time, variant string) float64 {
		return LookupCoefficient(bucketData, mc.TemporalMode, t, variant)
	})
}
