package lab

import (
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
// HERD/RISING/RECOVERY → UP, DUMPING/FALLING/TRAP → DOWN, STABLE → FLAT.
func predictedDirection(signal string) string {
	switch signal {
	case "HERD", "RISING", "RECOVERY":
		return "UP"
	case "DUMPING", "FALLING", "TRAP":
		return "DOWN"
	case "STABLE":
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
		type tierAcc struct {
			correct int
			total   int
		}
		tiers := map[string]*tierAcc{
			"TOP":  {},
			"HIGH": {},
			"MID":  {},
			"LOW":  {},
		}

		// Confidence band accumulators (0-9, 10-19, ..., 90-100).
		type bandAcc struct {
			correct int
			total   int
		}
		bands := make([]bandAcc, 10) // index 0 = [0,9], ..., 9 = [90,100]

		// Temporal phase accumulators.
		phases := map[string]*tierAcc{
			"weekend":         {},
			"weekday-peak":    {},
			"weekday-offpeak": {},
		}

		var weightedCorrect float64
		var weightedTotal float64
		var overallCorrect int
		highConfEvals := 0

		for _, ep := range evals {
			signal := classifySignalWithConfig(
				ep.Feature.VelMedPrice,
				ep.Feature.VelMedListing,
				ep.Feature.CV,
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
				ta = &tierAcc{}
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
		{
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

	cfg.PreHERDPriceVel = mc.VelocityMean + sc.HERDPriceMult*mc.VelocitySigma
	cfg.PreHERDListingVel = mc.ListingVelMean + sc.HERDListingMult*mc.ListingVelSigma
	cfg.StablePriceVel = sc.StablePriceMult * mc.VelocitySigma
	cfg.BrewingMinPVel = sc.BrewingPriceMult * mc.VelocitySigma
	cfg.DumpPriceVel = -(sc.DumpPriceMult * mc.VelocitySigma)

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
