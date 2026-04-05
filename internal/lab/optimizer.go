package lab

import (
	"math"
	"sort"
	"time"
)

// SweepResultV2 holds the evaluation metrics for a single σ-multiplier configuration.
type SweepResultV2 struct {
	Sigma         SigmaConfig
	WeightedScore float64 // primary sort key — confidence-weighted directional accuracy
	TopAcc        float64 // TOP tier accuracy %
	HighAcc       float64 // HIGH tier accuracy %
	MidAcc        float64 // MID tier accuracy %
	LowAcc        float64 // LOW tier accuracy %
	OverallAcc    float64 // all tiers
	ConfBands     []ConfidenceBand
	SweetSpot     int                // min confidence for >=80% accuracy, -1 if not found
	TemporalAcc   map[string]float64 // "weekday-peak", "weekday-offpeak", "weekend"
	TotalEvals    int
	HighConfEvals int // evals with confidence >= 70
}

// ConfidenceBand groups accuracy metrics within a confidence score range.
type ConfidenceBand struct {
	MinConf  int
	MaxConf  int
	Accuracy float64
	Count    int
}

// predictedDirection maps a signal to an expected price direction.
// HERD/RECOVERY → UP, DUMPING/TRAP → DOWN, STABLE/UNCERTAIN → FLAT.
func predictedDirection(signal string) string {
	switch signal {
	case "HERD", "RECOVERY":
		return "UP"
	case "DUMPING", "CAUTION":
		return "DOWN"
	case "STABLE", "UNCERTAIN":
		return "FLAT"
	default:
		return "FLAT"
	}
}

// directionFromChange maps a price change % into UP, DOWN, or FLAT.
// >2% → UP, <-2% → DOWN, else → FLAT.
func directionFromChange(pctChange float64) string {
	if pctChange > 2 {
		return "UP"
	}
	if pctChange < -2 {
		return "DOWN"
	}
	return "FLAT"
}

// tierWeight returns the scoring weight for a gem price tier.
// TOP gems are weighted more heavily because mispredictions on expensive gems
// carry higher financial risk.
func tierWeight(tier string) float64 {
	switch tier {
	case "TOP":
		return 2.0
	case "HIGH":
		return 1.5
	case "MID":
		return 1.0
	case "LOW":
		return 0.5
	default:
		return 1.0
	}
}

// classifyTemporalPhase categorizes a timestamp into a temporal phase for
// accuracy analysis: "weekend", "weekday-peak" (14-22 UTC), "weekday-offpeak".
func classifyTemporalPhase(t time.Time) string {
	weekday := t.UTC().Weekday()
	if weekday == time.Saturday || weekday == time.Sunday {
		return "weekend"
	}
	hour := t.UTC().Hour()
	if hour >= 14 && hour < 22 {
		return "weekday-peak"
	}
	return "weekday-offpeak"
}

// sweepAcc is a simple correct/total accumulator used for per-tier, per-band,
// and per-temporal-phase accuracy tracking within SweepV2.
type sweepAcc struct {
	correct int
	total   int
}

// SweepV2 evaluates each σ-multiplier configuration against the eval points,
// computing confidence-weighted directional accuracy. Results are sorted by
// WeightedScore descending (best first).
//
// For each SigmaConfig, it converts to absolute thresholds via MarketContext,
// classifies signals, computes confidence, and compares predicted direction
// against actual price movement. Scoring is weighted by tier importance and
// confidence level.
func SweepV2(evals []EvalPoint, mc MarketContext, grid []SigmaConfig) []SweepResultV2 {
	results := make([]SweepResultV2, 0, len(grid))

	for _, sigma := range grid {
		cfg := sigma.ToSignalConfig(mc)

		// Per-tier accumulators.
		tiers := map[string]*sweepAcc{
			"TOP":  {},
			"HIGH": {},
			"MID":  {},
			"LOW":  {},
		}

		// Confidence band accumulators (0-9, 10-19, ..., 90-100).
		bands := make([]sweepAcc, 10) // index 0 = [0,9], ..., 9 = [90,100]

		// Temporal phase accumulators.
		phases := map[string]*sweepAcc{
			"weekend":         {},
			"weekday-peak":    {},
			"weekday-offpeak": {},
		}

		var weightedCorrect float64
		var weightedTotal float64
		var overallCorrect int
		var highConfEvals int

		for _, ep := range evals {
			signal := classifySignalWithConfig(
				ep.Feature.VelLongPrice,
				ep.Feature.VelLongListing,
				ep.Feature.CV,
				ep.Feature.Chaos,
				ep.Feature.Listings,
				cfg,
			)

			confidence, _ := computeConfidence(signal, ep.Feature, mc, ep.SnapTime)

			predicted := predictedDirection(signal)
			actual := directionFromChange(ep.FuturePct)
			correct := predicted == actual

			// Tier-weighted scoring.
			tw := tierWeight(ep.Feature.Tier)
			confWeight := float64(confidence) / 100.0
			weight := tw * confWeight
			weightedTotal += weight
			if correct {
				weightedCorrect += weight
				overallCorrect++
			}

			// Per-tier accuracy.
			tier := ep.Feature.Tier
			if tier == "" {
				tier = "LOW"
			}
			ta, ok := tiers[tier]
			if !ok {
				ta = &sweepAcc{}
				tiers[tier] = ta
			}
			ta.total++
			if correct {
				ta.correct++
			}

			// Confidence band accumulation.
			bandIdx := confidence / 10
			if bandIdx > 9 {
				bandIdx = 9
			}
			bands[bandIdx].total++
			if correct {
				bands[bandIdx].correct++
			}

			// Temporal phase.
			phase := classifyTemporalPhase(ep.SnapTime)
			pa := phases[phase]
			pa.total++
			if correct {
				pa.correct++
			}

			// High confidence count.
			if confidence >= 70 {
				highConfEvals++
			}
		}

		// Compute weighted score.
		var weightedScore float64
		if weightedTotal > 0 {
			weightedScore = weightedCorrect / weightedTotal * 100
		}

		// Build confidence bands.
		confBands := make([]ConfidenceBand, 0, 10)
		for i, b := range bands {
			if b.total == 0 {
				continue
			}
			confBands = append(confBands, ConfidenceBand{
				MinConf:  i * 10,
				MaxConf:  i*10 + 9,
				Accuracy: float64(b.correct) / float64(b.total) * 100,
				Count:    b.total,
			})
		}
		// Fix last band max to 100.
		if len(confBands) > 0 && confBands[len(confBands)-1].MinConf == 90 {
			confBands[len(confBands)-1].MaxConf = 100
		}

		// Compute sweet spot: scan from highest confidence band down,
		// accumulating correct/total. Find the minimum confidence threshold
		// where cumulative accuracy >= 80%.
		sweetSpot := -1
		cumCorrect := 0
		cumTotal := 0
		for i := len(bands) - 1; i >= 0; i-- {
			cumCorrect += bands[i].correct
			cumTotal += bands[i].total
			if cumTotal > 0 {
				cumAcc := float64(cumCorrect) / float64(cumTotal) * 100
				if cumAcc >= 80 {
					sweetSpot = i * 10
				} else {
					// Once accuracy drops below 80%, stop scanning down.
					break
				}
			}
		}

		// Build temporal accuracy map.
		temporalAcc := make(map[string]float64, 3)
		for phase, pa := range phases {
			if pa.total > 0 {
				temporalAcc[phase] = float64(pa.correct) / float64(pa.total) * 100
			}
		}

		// Per-tier accuracy.
		accForTier := func(tier string) float64 {
			ta := tiers[tier]
			if ta.total == 0 {
				return 0
			}
			return float64(ta.correct) / float64(ta.total) * 100
		}

		var overallAcc float64
		if len(evals) > 0 {
			overallAcc = float64(overallCorrect) / float64(len(evals)) * 100
		}

		results = append(results, SweepResultV2{
			Sigma:         sigma,
			WeightedScore: weightedScore,
			TopAcc:        accForTier("TOP"),
			HighAcc:       accForTier("HIGH"),
			MidAcc:        accForTier("MID"),
			LowAcc:        accForTier("LOW"),
			OverallAcc:    overallAcc,
			ConfBands:     confBands,
			SweetSpot:     sweetSpot,
			TemporalAcc:   temporalAcc,
			TotalEvals:    len(evals),
			HighConfEvals: highConfEvals,
		})
	}

	// Sort by WeightedScore descending.
	sort.Slice(results, func(i, j int) bool {
		return results[i].WeightedScore > results[j].WeightedScore
	})

	return results
}

// SnapshotPrice is a lightweight price observation for ground truth lookup.
type SnapshotPrice struct {
	Time    time.Time
	Name    string
	Variant string
	Chaos   float64
}

// EvalPoint pairs a pre-computed feature with its ground truth future price change.
type EvalPoint struct {
	Feature   GemFeature
	FuturePct float64   // actual price change % over horizon
	SnapTime  time.Time // snapshot time used as price baseline
}

// BuildEvalPoints pairs features with future snapshot prices.
// Groups prices by (name, variant) into sorted time slices.
// For each feature, finds the nearest snapshot price within [horizon-30m, horizon+30m].
// Uses the snapshot chaos at the feature's time as the price baseline for futurePct
// (not GemFeature.Chaos, which may differ due to aggregation).
// Returns eval points and count of dropped features (no valid future price match).
func BuildEvalPoints(features []GemFeature, prices []SnapshotPrice, horizon time.Duration) ([]EvalPoint, int) {
	// Build a lookup index: (name, variant) → time-sorted prices.
	type gemKey struct {
		Name    string
		Variant string
	}
	priceIndex := make(map[gemKey][]SnapshotPrice)
	for _, p := range prices {
		k := gemKey{p.Name, p.Variant}
		priceIndex[k] = append(priceIndex[k], p)
	}
	for k, ps := range priceIndex {
		sort.Slice(ps, func(i, j int) bool { return ps[i].Time.Before(ps[j].Time) })
		priceIndex[k] = ps
	}

	// Build a baseline price index: (name, variant, truncated time) → chaos.
	// This lets us find the snapshot price at the feature's time for the baseline.
	type baselineKey struct {
		Name    string
		Variant string
		Time    time.Time
	}
	baselineIndex := make(map[baselineKey]float64, len(prices))
	for _, p := range prices {
		bk := baselineKey{p.Name, p.Variant, p.Time.Truncate(time.Minute)}
		baselineIndex[bk] = p.Chaos
	}

	const tolerance = 30 * time.Minute

	var result []EvalPoint
	dropped := 0

	for _, f := range features {
		k := gemKey{f.Name, f.Variant}
		ps, ok := priceIndex[k]
		if !ok {
			dropped++
			continue
		}

		targetTime := f.Time.Add(horizon)
		minTime := targetTime.Add(-tolerance)
		maxTime := targetTime.Add(tolerance)

		// Binary search for the first price >= minTime.
		idx := sort.Search(len(ps), func(i int) bool {
			return !ps[i].Time.Before(minTime)
		})

		// Find the closest price within [minTime, maxTime].
		bestIdx := -1
		bestDist := time.Duration(0)
		for i := idx; i < len(ps); i++ {
			if ps[i].Time.After(maxTime) {
				break
			}
			dist := ps[i].Time.Sub(targetTime)
			if dist < 0 {
				dist = -dist
			}
			if bestIdx == -1 || dist < bestDist {
				bestIdx = i
				bestDist = dist
			}
		}

		if bestIdx == -1 {
			dropped++
			continue
		}

		// Use the snapshot price at the feature's time as baseline.
		// Fall back to GemFeature.Chaos if no exact baseline snapshot is found.
		bk := baselineKey{f.Name, f.Variant, f.Time.Truncate(time.Minute)}
		baseline, found := baselineIndex[bk]
		if !found {
			baseline = f.Chaos
		}

		if baseline <= 0 {
			dropped++
			continue
		}

		futureChaos := ps[bestIdx].Chaos
		futurePct := (futureChaos - baseline) / baseline * 100

		result = append(result, EvalPoint{
			Feature:   f,
			FuturePct: futurePct,
			SnapTime:  f.Time,
		})
	}

	return result, dropped
}

// SigmaConfig holds σ-multiplier parameters for optimizer grid sweeping.
// Each field is a multiplier of the corresponding market-wide sigma value.
// ToSignalConfig converts these relative multipliers into absolute thresholds
// using observed market statistics from MarketContext.
type SigmaConfig struct {
	HERDPriceMult    float64 // priceVel threshold = VelocityMean + N * VelocitySigma
	HERDListingMult  float64 // listingVel threshold = ListingVelMean + N * ListingVelSigma
	StablePriceMult  float64 // max |priceVel| for STABLE = M * VelocitySigma
	BrewingPriceMult float64 // min priceVel for BREWING = M * VelocitySigma
	DumpPriceMult    float64 // dump priceVel = -(M * VelocitySigma)
}

// ToSignalConfig converts σ-multipliers to absolute thresholds using market context.
// Starts from DefaultSignalConfig and overrides the swept fields. Fields not swept
// (HERDPriceVel, HERDListingVel, StableListingVel, RecoveryMaxList, TrapCV) keep defaults.
func (sc SigmaConfig) ToSignalConfig(mc MarketContext) SignalConfig {
	cfg := DefaultSignalConfig()

	// Sigma-multiplied overrides for percentage-based thresholds.
	// Market-wide sigma is in absolute terms; convert to approximate percentage
	// using the median price (P50) as reference. This preserves the optimizer's
	// ability to sweep relative to market conditions.
	p50 := mc.PricePercentiles["P50"]
	if p50 <= 0 {
		p50 = 100 // fallback
	}
	cfg.PreHERDPriceVelPct = (mc.VelocityMean + sc.HERDPriceMult*mc.VelocitySigma) / p50 * 100
	// Approximate: MarketContext lacks listing percentiles, so we use a rough estimate.
	const approxMedianListings = 50.0
	cfg.PreHERDListingVelPct = (mc.ListingVelMean + sc.HERDListingMult*mc.ListingVelSigma) * 100 / approxMedianListings
	cfg.StablePriceVelPct = sc.StablePriceMult * mc.VelocitySigma / p50 * 100
	cfg.BrewingMinPVel = sc.BrewingPriceMult * mc.VelocitySigma
	cfg.DumpPriceVelPct = -(sc.DumpPriceMult * mc.VelocitySigma) / p50 * 100

	return cfg
}

// GenerateSigmaGrid produces a focused σ-multiplier grid for sweeping.
// Grid: 5×4×4×4 = 320 combos (same size as v1 absolute grid).
// DumpPriceMult is fixed at 2.0 (not swept).
func GenerateSigmaGrid() []SigmaConfig {
	grid := make([]SigmaConfig, 0, 320)

	for _, herdP := range []float64{1.5, 2.0, 2.5, 3.0, 3.5} {
		for _, herdL := range []float64{1.0, 1.5, 2.0, 2.5} {
			for _, stableP := range []float64{0.3, 0.5, 0.7, 1.0} {
				for _, brewP := range []float64{0.5, 1.0, 1.5, 2.0} {
					grid = append(grid, SigmaConfig{
						HERDPriceMult:    herdP,
						HERDListingMult:  herdL,
						StablePriceMult:  stableP,
						BrewingPriceMult: brewP,
						DumpPriceMult:    2.0,
					})
				}
			}
		}
	}

	return grid
}

// SignalAccuracy holds per-signal accuracy metrics for validation reporting.
type SignalAccuracy struct {
	Signal        string  // signal name (e.g. HERD, DUMPING, STABLE)
	Predicted     string  // predicted direction (UP, DOWN, FLAT)
	Count         int     // total times this signal fired
	Correct       int     // how many times the prediction was correct
	Accuracy      float64 // Correct/Count * 100
	AvgConfidence float64 // average confidence score when this signal fires
}

// ValidationReport holds the full output of ValidateDefaults.
type ValidationReport struct {
	PerSignal       map[string]SignalAccuracy   // keyed by signal name
	ConfusionMatrix map[string]map[string]int   // [predicted_direction][actual_direction] = count
	ConfBands       []ConfidenceBand
	PerTier         map[string]float64
	PerPhase        map[string]float64
	SweetSpot       int
	TotalEvals      int
	OverallAcc      float64
}

// ValidateDefaults evaluates the current default signal configuration against
// the provided eval points, producing a detailed accuracy breakdown by signal,
// confusion matrix, confidence bands, tier, and temporal phase.
func ValidateDefaults(evals []EvalPoint, mc MarketContext) ValidationReport {
	report := ValidationReport{
		PerSignal:       make(map[string]SignalAccuracy),
		ConfusionMatrix: make(map[string]map[string]int),
		PerTier:         make(map[string]float64),
		PerPhase:        make(map[string]float64),
		SweetSpot:       -1,
	}

	if len(evals) == 0 {
		return report
	}

	cfg := DefaultSignalConfig()

	// Per-tier accumulators.
	tiers := map[string]*sweepAcc{
		"TOP":  {},
		"HIGH": {},
		"MID":  {},
		"LOW":  {},
	}

	// Confidence band accumulators (0-9, 10-19, ..., 90-100).
	bands := make([]sweepAcc, 10)

	// Temporal phase accumulators.
	phases := map[string]*sweepAcc{
		"weekend":         {},
		"weekday-peak":    {},
		"weekday-offpeak": {},
	}

	// Per-signal accumulators.
	type signalAcc struct {
		count      int
		correct    int
		confSum    float64
		predicted  string
	}
	signalAccs := make(map[string]*signalAcc)

	// Initialize confusion matrix directions.
	for _, dir := range []string{"UP", "DOWN", "FLAT"} {
		report.ConfusionMatrix[dir] = make(map[string]int)
	}

	var overallCorrect int

	for _, ep := range evals {
		signal := classifySignalWithConfig(
			ep.Feature.VelLongPrice,
			ep.Feature.VelLongListing,
			ep.Feature.CV,
			ep.Feature.Chaos,
			ep.Feature.Listings,
			cfg,
		)

		confidence, _ := computeConfidence(signal, ep.Feature, mc, ep.SnapTime)

		predicted := predictedDirection(signal)
		actual := directionFromChange(ep.FuturePct)
		correct := predicted == actual

		if correct {
			overallCorrect++
		}

		// Per-signal accumulation.
		sa, ok := signalAccs[signal]
		if !ok {
			sa = &signalAcc{predicted: predicted}
			signalAccs[signal] = sa
		}
		sa.count++
		if correct {
			sa.correct++
		}
		sa.confSum += float64(confidence)

		// Confusion matrix.
		if _, ok := report.ConfusionMatrix[predicted]; !ok {
			report.ConfusionMatrix[predicted] = make(map[string]int)
		}
		report.ConfusionMatrix[predicted][actual]++

		// Per-tier accuracy.
		tier := ep.Feature.Tier
		if tier == "" {
			tier = "LOW"
		}
		ta, ok := tiers[tier]
		if !ok {
			ta = &sweepAcc{}
			tiers[tier] = ta
		}
		ta.total++
		if correct {
			ta.correct++
		}

		// Confidence band accumulation.
		bandIdx := confidence / 10
		if bandIdx > 9 {
			bandIdx = 9
		}
		bands[bandIdx].total++
		if correct {
			bands[bandIdx].correct++
		}

		// Temporal phase.
		phase := classifyTemporalPhase(ep.SnapTime)
		pa := phases[phase]
		pa.total++
		if correct {
			pa.correct++
		}
	}

	// Build per-signal map.
	for sig, sa := range signalAccs {
		var acc float64
		if sa.count > 0 {
			acc = float64(sa.correct) / float64(sa.count) * 100
		}
		var avgConf float64
		if sa.count > 0 {
			avgConf = sa.confSum / float64(sa.count)
		}
		report.PerSignal[sig] = SignalAccuracy{
			Signal:        sig,
			Predicted:     sa.predicted,
			Count:         sa.count,
			Correct:       sa.correct,
			Accuracy:      acc,
			AvgConfidence: avgConf,
		}
	}

	// Build confidence bands.
	confBands := make([]ConfidenceBand, 0, 10)
	for i, b := range bands {
		if b.total == 0 {
			continue
		}
		confBands = append(confBands, ConfidenceBand{
			MinConf:  i * 10,
			MaxConf:  i*10 + 9,
			Accuracy: float64(b.correct) / float64(b.total) * 100,
			Count:    b.total,
		})
	}
	if len(confBands) > 0 && confBands[len(confBands)-1].MinConf == 90 {
		confBands[len(confBands)-1].MaxConf = 100
	}
	report.ConfBands = confBands

	// Compute sweet spot.
	cumCorrect := 0
	cumTotal := 0
	for i := len(bands) - 1; i >= 0; i-- {
		cumCorrect += bands[i].correct
		cumTotal += bands[i].total
		if cumTotal > 0 {
			cumAcc := float64(cumCorrect) / float64(cumTotal) * 100
			if cumAcc >= 80 {
				report.SweetSpot = i * 10
			} else {
				break
			}
		}
	}

	// Build per-tier map.
	for tier, ta := range tiers {
		if ta.total > 0 {
			report.PerTier[tier] = float64(ta.correct) / float64(ta.total) * 100
		}
	}

	// Build per-phase map.
	for phase, pa := range phases {
		if pa.total > 0 {
			report.PerPhase[phase] = float64(pa.correct) / float64(pa.total) * 100
		}
	}

	report.TotalEvals = len(evals)
	report.OverallAcc = float64(overallCorrect) / float64(len(evals)) * 100

	return report
}

// ValueCapture holds percentile statistics for actual-vs-predicted value ratios.
type ValueCapture struct {
	Count         int     `json:"count"`
	AvgCapture    float64 `json:"avg_capture"`    // mean(actualPrice / predictedRiskAdjValue)
	MedianCapture float64 `json:"median_capture"`
	P25Capture    float64 `json:"p25_capture"`    // 25th percentile
	P75Capture    float64 `json:"p75_capture"`    // 75th percentile
}

// ConfidenceCalResult holds calibration metrics for a sell confidence level.
type ConfidenceCalResult struct {
	Count     int     `json:"count"`
	PriceHeld int     `json:"price_held"` // count where future_price >= 0.9 * current_price
	HeldRate  float64 `json:"held_rate"`  // PriceHeld / Count
	AvgChange float64 `json:"avg_change"` // mean price change %
}

// SellabilityReport holds the full output of ValidateSellability.
type SellabilityReport struct {
	TotalEvals            int                            `json:"total_evals"`
	PerSignalCapture      map[string]ValueCapture        `json:"per_signal_capture"`
	FloorHoldRate         map[string]FloorHoldResult     `json:"floor_hold_rate"`
	ConfidenceCalibration map[string]ConfidenceCalResult `json:"confidence_calibration"`
	PerTierCapture        map[string]ValueCapture        `json:"per_tier_capture"`
	PerVariant            map[string]VariantReport       `json:"per_variant"`
}

// VariantReport holds per-variant breakdown of sellability validation metrics.
type VariantReport struct {
	TotalEvals            int                            `json:"total_evals"`
	PerSignalCapture      map[string]ValueCapture        `json:"per_signal_capture"`
	FloorHoldRate         map[string]FloorHoldResult     `json:"floor_hold_rate"`
	ConfidenceCalibration map[string]ConfidenceCalResult `json:"confidence_calibration"`
}

// FloorHoldResult holds floor hold statistics for a tier.
type FloorHoldResult struct {
	Count    int     `json:"count"`
	Held     int     `json:"held"`
	HeldRate float64 `json:"held_rate"`
}

// ValidateSellability evaluates how well the risk-adjusted scoring predicts
// actual value capture. For each EvalPoint, it computes the risk-adjusted value
// and compares it against the realized future price.
func ValidateSellability(evals []EvalPoint, mc MarketContext) SellabilityReport {
	report := SellabilityReport{
		PerSignalCapture:      make(map[string]ValueCapture),
		FloorHoldRate:         make(map[string]FloorHoldResult),
		ConfidenceCalibration: make(map[string]ConfidenceCalResult),
		PerTierCapture:        make(map[string]ValueCapture),
		PerVariant:            make(map[string]VariantReport),
	}

	if len(evals) == 0 {
		return report
	}

	cfg := DefaultSignalConfig()

	// Accumulators: capture ratios grouped by signal, tier, and sell confidence.
	type captureAcc struct {
		ratios []float64
	}
	signalCaptures := make(map[string]*captureAcc)
	tierCaptures := make(map[string]*captureAcc)

	// Floor hold accumulators by tier.
	type floorAcc struct {
		total int
		held  int
	}
	floorAccs := make(map[string]*floorAcc)

	// Confidence calibration accumulators.
	type confCalAcc struct {
		total     int
		priceHeld int
		changeSum float64
	}
	confCalAccs := make(map[string]*confCalAcc)

	// Per-variant accumulators.
	type variantAcc struct {
		totalEvals     int
		signalCaptures map[string]*captureAcc
		floorAccs      map[string]*floorAcc
		confCalAccs    map[string]*confCalAcc
	}
	variantAccs := make(map[string]*variantAcc)

	skipped := 0

	for _, ep := range evals {
		// Always compute sell probability and stability on-the-fly from feature data.
		sellProb := sellProbabilityFactor(ep.Feature.Listings, ep.Feature.Low7Days, ep.Feature.Chaos)
		stabDisc := stabilityDiscount(ep.Feature.CV)

		// Compute risk-adjusted value.
		riskAdjValue := ep.Feature.Chaos * sellProb * stabDisc

		// Skip if risk-adjusted value is too small (avoids division by zero / noise).
		if riskAdjValue < 0.01 {
			skipped++
			continue
		}

		// Derive future price from the percentage change.
		futurePrice := ep.Feature.Chaos * (1.0 + ep.FuturePct/100.0)

		// Actual value capture ratio.
		actualCapture := futurePrice / riskAdjValue

		// Classify the signal for grouping (6h velocity for consistency).
		signal := classifySignalWithConfig(
			ep.Feature.VelLongPrice,
			ep.Feature.VelLongListing,
			ep.Feature.CV,
			ep.Feature.Chaos,
			ep.Feature.Listings,
			cfg,
		)

		// Classify sell confidence.
		sellConf, _ := classifySellConfidence(sellProb, stabDisc, ep.Feature)

		// Determine tier.
		tier := ep.Feature.Tier
		if tier == "" {
			tier = "LOW"
		}

		// Accumulate per-signal capture.
		if _, ok := signalCaptures[signal]; !ok {
			signalCaptures[signal] = &captureAcc{}
		}
		signalCaptures[signal].ratios = append(signalCaptures[signal].ratios, actualCapture)

		// Accumulate per-tier capture.
		if _, ok := tierCaptures[tier]; !ok {
			tierCaptures[tier] = &captureAcc{}
		}
		tierCaptures[tier].ratios = append(tierCaptures[tier].ratios, actualCapture)

		// Floor hold: did price stay above Low7Days?
		if _, ok := floorAccs[tier]; !ok {
			floorAccs[tier] = &floorAcc{}
		}
		fa := floorAccs[tier]
		fa.total++
		if futurePrice >= ep.Feature.Low7Days {
			fa.held++
		}

		// Confidence calibration: did price hold within 10% of current?
		if _, ok := confCalAccs[sellConf]; !ok {
			confCalAccs[sellConf] = &confCalAcc{}
		}
		ca := confCalAccs[sellConf]
		ca.total++
		if futurePrice >= 0.9*ep.Feature.Chaos {
			ca.priceHeld++
		}
		ca.changeSum += ep.FuturePct

		// Per-variant accumulation.
		variant := ep.Feature.Variant
		va, ok := variantAccs[variant]
		if !ok {
			va = &variantAcc{
				signalCaptures: make(map[string]*captureAcc),
				floorAccs:      make(map[string]*floorAcc),
				confCalAccs:    make(map[string]*confCalAcc),
			}
			variantAccs[variant] = va
		}
		va.totalEvals++

		if _, ok := va.signalCaptures[signal]; !ok {
			va.signalCaptures[signal] = &captureAcc{}
		}
		va.signalCaptures[signal].ratios = append(va.signalCaptures[signal].ratios, actualCapture)

		if _, ok := va.floorAccs[tier]; !ok {
			va.floorAccs[tier] = &floorAcc{}
		}
		vfa := va.floorAccs[tier]
		vfa.total++
		if futurePrice >= ep.Feature.Low7Days {
			vfa.held++
		}

		if _, ok := va.confCalAccs[sellConf]; !ok {
			va.confCalAccs[sellConf] = &confCalAcc{}
		}
		vca := va.confCalAccs[sellConf]
		vca.total++
		if futurePrice >= 0.9*ep.Feature.Chaos {
			vca.priceHeld++
		}
		vca.changeSum += ep.FuturePct
	}

	// Build per-signal capture map.
	for sig, acc := range signalCaptures {
		report.PerSignalCapture[sig] = computeValueCapture(acc.ratios)
	}

	// Build per-tier capture map.
	for tier, acc := range tierCaptures {
		report.PerTierCapture[tier] = computeValueCapture(acc.ratios)
	}

	// Build floor hold rate map.
	for tier, fa := range floorAccs {
		var rate float64
		if fa.total > 0 {
			rate = float64(fa.held) / float64(fa.total) * 100
		}
		report.FloorHoldRate[tier] = FloorHoldResult{
			Count:    fa.total,
			Held:     fa.held,
			HeldRate: rate,
		}
	}

	// Build confidence calibration map.
	for conf, ca := range confCalAccs {
		var heldRate float64
		if ca.total > 0 {
			heldRate = float64(ca.priceHeld) / float64(ca.total) * 100
		}
		var avgChange float64
		if ca.total > 0 {
			avgChange = ca.changeSum / float64(ca.total)
		}
		report.ConfidenceCalibration[conf] = ConfidenceCalResult{
			Count:     ca.total,
			PriceHeld: ca.priceHeld,
			HeldRate:  heldRate,
			AvgChange: avgChange,
		}
	}

	// Build per-variant reports.
	for variant, va := range variantAccs {
		vr := VariantReport{
			TotalEvals:            va.totalEvals,
			PerSignalCapture:      make(map[string]ValueCapture, len(va.signalCaptures)),
			FloorHoldRate:         make(map[string]FloorHoldResult, len(va.floorAccs)),
			ConfidenceCalibration: make(map[string]ConfidenceCalResult, len(va.confCalAccs)),
		}

		for sig, acc := range va.signalCaptures {
			vr.PerSignalCapture[sig] = computeValueCapture(acc.ratios)
		}
		for tier, fa := range va.floorAccs {
			var rate float64
			if fa.total > 0 {
				rate = float64(fa.held) / float64(fa.total) * 100
			}
			vr.FloorHoldRate[tier] = FloorHoldResult{
				Count:    fa.total,
				Held:     fa.held,
				HeldRate: rate,
			}
		}
		for conf, ca := range va.confCalAccs {
			var heldRate float64
			if ca.total > 0 {
				heldRate = float64(ca.priceHeld) / float64(ca.total) * 100
			}
			var avgChange float64
			if ca.total > 0 {
				avgChange = ca.changeSum / float64(ca.total)
			}
			vr.ConfidenceCalibration[conf] = ConfidenceCalResult{
				Count:     ca.total,
				PriceHeld: ca.priceHeld,
				HeldRate:  heldRate,
				AvgChange: avgChange,
			}
		}

		report.PerVariant[variant] = vr
	}

	report.TotalEvals = len(evals) - skipped

	return report
}

// computeValueCapture computes ValueCapture statistics from a slice of capture ratios.
func computeValueCapture(ratios []float64) ValueCapture {
	if len(ratios) == 0 {
		return ValueCapture{}
	}

	// Compute mean.
	var sum float64
	for _, r := range ratios {
		sum += r
	}
	avg := sum / float64(len(ratios))

	// Sort for percentiles.
	sorted := make([]float64, len(ratios))
	copy(sorted, ratios)
	sort.Float64s(sorted)

	return ValueCapture{
		Count:         len(ratios),
		AvgCapture:    math.Round(avg*100) / 100,
		MedianCapture: math.Round(percentile(sorted, 0.50)*100) / 100,
		P25Capture:    math.Round(percentile(sorted, 0.25)*100) / 100,
		P75Capture:    math.Round(percentile(sorted, 0.75)*100) / 100,
	}
}
