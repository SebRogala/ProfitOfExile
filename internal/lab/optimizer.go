package lab

import (
	"sort"
	"time"
)

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
	var grid []SigmaConfig

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
