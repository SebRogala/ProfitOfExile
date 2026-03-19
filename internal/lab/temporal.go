package lab

import "math"

// TemporalBiases holds all computed temporal patterns for hourly and weekday buckets.
// Fixed-size arrays are used internally; the caller copies to slices for MarketContext.
type TemporalBiases struct {
	HourlyBias        [24]float64 // direction: 1.0+mean(pctChange), neutral=1.0
	HourlyVolatility  [24]float64 // σ of pctChanges per hour
	HourlyActivity    [24]float64 // fraction of observations with |pctChange|>2%
	WeekdayBias       [7]float64  // direction per weekday (Sun=0..Sat=6)
	WeekdayVolatility [7]float64
	WeekdayActivity   [7]float64
}

// movingThreshold defines the minimum absolute percent change for a gem to be
// considered "moving" in activity ratio calculations. 2% is relative to each
// gem's own price, not a hardcoded chaos amount.
const movingThreshold = 0.02

// computeTemporalBiases computes direction bias, volatility, and activity ratio
// for each hour of day (0-23 UTC) and day of week (Sun=0..Sat=6).
//
// For each gem's price history, consecutive point pairs produce a percent change.
// These are bucketed by the earlier point's timestamp (the "cause" time).
//
// Per bucket:
//   - DirectionBias = 1.0 + mean(pctChanges) — neutral=1.0, bearish<1.0, bullish>1.0
//   - Volatility = σ(pctChanges) — raw standard deviation of price movements
//   - Activity = count(|pctChange|>2%) / count(total) — fraction of moving observations
//
// Empty buckets get direction=1.0, volatility=0, activity=0.
func computeTemporalBiases(history []GemPriceHistory) TemporalBiases {
	var tb TemporalBiases

	// Initialize direction biases to neutral 1.0.
	for i := range tb.HourlyBias {
		tb.HourlyBias[i] = 1.0
	}
	for i := range tb.WeekdayBias {
		tb.WeekdayBias[i] = 1.0
	}

	// Collect pctChanges per hourly and weekday bucket.
	type bucketData struct {
		pctChanges []float64
		movingCnt  int
	}

	hourlyBuckets := make([]bucketData, 24)
	weekdayBuckets := make([]bucketData, 7)

	for _, h := range history {
		for i := 0; i < len(h.Points)-1; i++ {
			prev := h.Points[i]
			curr := h.Points[i+1]

			if prev.Chaos <= 0 {
				continue
			}

			pctChange := (curr.Chaos - prev.Chaos) / prev.Chaos
			pctChange = sanitizeFloat(pctChange)
			if math.IsNaN(pctChange) || math.IsInf(pctChange, 0) {
				continue
			}

			isMoving := math.Abs(pctChange) > movingThreshold

			hour := prev.Time.UTC().Hour()
			hourlyBuckets[hour].pctChanges = append(hourlyBuckets[hour].pctChanges, pctChange)
			if isMoving {
				hourlyBuckets[hour].movingCnt++
			}

			weekday := int(prev.Time.UTC().Weekday())
			weekdayBuckets[weekday].pctChanges = append(weekdayBuckets[weekday].pctChanges, pctChange)
			if isMoving {
				weekdayBuckets[weekday].movingCnt++
			}
		}
	}

	// Compute stats per bucket.
	for i := 0; i < 24; i++ {
		b := hourlyBuckets[i]
		if len(b.pctChanges) == 0 {
			continue
		}
		mean, sigma := meanStddev(b.pctChanges)
		tb.HourlyBias[i] = sanitizeFloat(1.0 + mean)
		tb.HourlyVolatility[i] = sanitizeFloat(sigma)
		tb.HourlyActivity[i] = sanitizeFloat(float64(b.movingCnt) / float64(len(b.pctChanges)))
	}

	for i := 0; i < 7; i++ {
		b := weekdayBuckets[i]
		if len(b.pctChanges) == 0 {
			continue
		}
		mean, sigma := meanStddev(b.pctChanges)
		tb.WeekdayBias[i] = sanitizeFloat(1.0 + mean)
		tb.WeekdayVolatility[i] = sanitizeFloat(sigma)
		tb.WeekdayActivity[i] = sanitizeFloat(float64(b.movingCnt) / float64(len(b.pctChanges)))
	}

	return tb
}
