package lab

import (
	"math"
	"sort"
	"strings"
	"time"
)

// DedicationResult holds the computed EV for a single (color, gemType) Dedication lab analysis.
// GemType is "skill" (non-transfigured corrupted 21/23) or "transfigured" (transfigured corrupted 21/23).
type DedicationResult struct {
	Time              time.Time
	Color             string  // RED, GREEN, BLUE
	GemType           string  // "skill" or "transfigured"
	Pool              int     // total unique gem names of that color in the pool
	Winners           int     // count of tier-qualifying gems
	PWin              float64 // probability of seeing at least 1 winner in 3 picks
	AvgWin            float64 // average risk-adjusted value when you hit
	AvgWinRaw         float64 // average RAW listed price when you hit
	EV                float64 // expected income per font (risk-adjusted)
	EVRaw             float64 // expected income per font using raw listed prices
	InputCost         float64 // avg of 10 cheapest per color per pool
	Profit            float64 // EVRaw - InputCost
	FontsToHit        float64          // expected fonts until hitting a winner (1/pWin)
	JackpotGems       []JackpotGemInfo // TOP gem names+prices (only for jackpot mode)
	Mode              string           // "safe", "premium", or "jackpot"
	ThinPoolGems      int              // count of winners with < 5 listings
	LiquidityRisk     string           // "LOW", "MEDIUM", "HIGH"
	PoolBreakdown     []TierPoolInfo         `json:"poolBreakdown,omitempty"`
	LowConfidenceGems []LowConfidenceGemInfo `json:"lowConfidenceGems,omitempty"`
}

// DedicationAnalysis holds the results of Dedication lab analysis for both pools.
type DedicationAnalysis struct {
	Skills       []DedicationResult
	Transfigured []DedicationResult
}

// isDedicationGem returns true if the gem belongs to the Dedication corrupted 21/23 pool:
// corrupted, not a support gem, not Trarthus.
func isDedicationGem(g GemPrice) bool {
	return g.IsCorrupted && !strings.Contains(g.Name, "Support") && !strings.Contains(g.Name, "Trarthus")
}

// dedicationInputCostFromPrices computes the average of the 10 cheapest prices in the pool.
// If the pool has fewer than 10 entries, averages all of them.
func dedicationInputCostFromPrices(prices []float64) float64 {
	if len(prices) == 0 {
		return 0
	}
	sorted := make([]float64, len(prices))
	copy(sorted, prices)
	sort.Float64s(sorted)

	n := 10
	if n > len(sorted) {
		n = len(sorted)
	}
	sum := 0.0
	for i := 0; i < n; i++ {
		sum += sorted[i]
	}
	return sum / float64(n)
}

// AnalyzeDedication computes Dedication lab EV per (color, gemType) in three modes.
// The gems slice must be the full latest snapshot. Features are used for risk-adjustment.
func AnalyzeDedication(snapTime time.Time, gems []GemPrice, features []GemFeature) DedicationAnalysis {
	// Build feature lookup: "name|variant" -> *GemFeature
	type featureKey struct{ name, variant string }
	featureLookup := make(map[featureKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featureLookup[featureKey{f.Name, f.Variant}] = f
	}

	// Separate dedication gems into two pools by color.
	type gemEntry struct {
		name     string
		chaos    float64
		listings int
	}

	// Pool: unique gem names per color per pool type (for pool size counting).
	type poolKey struct {
		color   string
		gemType string
	}
	poolNames := make(map[poolKey]map[string]struct{})
	poolGems := make(map[poolKey][]gemEntry)

	colors := []string{"RED", "GREEN", "BLUE"}
	gemTypes := []string{"skill", "transfigured"}

	for _, c := range colors {
		for _, gt := range gemTypes {
			k := poolKey{c, gt}
			poolNames[k] = make(map[string]struct{})
		}
	}

	for _, g := range gems {
		if !isDedicationGem(g) {
			continue
		}
		if g.Variant != "21/23c" {
			continue
		}
		color := g.GemColor
		if color != "RED" && color != "GREEN" && color != "BLUE" {
			continue
		}

		gt := "skill"
		if g.IsTransfigured {
			gt = "transfigured"
		}
		k := poolKey{color, gt}

		poolNames[k][g.Name] = struct{}{}
		poolGems[k] = append(poolGems[k], gemEntry{
			name:     g.Name,
			chaos:    g.Chaos,
			listings: g.Listings,
		})
	}

	// Precompute classification once per pool type (not once per color).
	classificationByType := map[string]ClassificationResult{
		"skill":        ComputeDedicationClassification(gems, false),
		"transfigured": ComputeDedicationClassification(gems, true),
	}

	var analysis DedicationAnalysis

	for _, color := range colors {
		for _, gemType := range gemTypes {
			k := poolKey{color, gemType}
			names := poolNames[k]
			entries := poolGems[k]
			pool := len(names)
			if pool == 0 {
				continue
			}

			poolPrices := make([]float64, len(entries))
			for i, e := range entries {
				poolPrices[i] = e.chaos
			}
			inputCost := dedicationInputCostFromPrices(poolPrices)

			// Use precomputed classification for this pool type.
			classification := classificationByType[gemType]

			// Count winners and thin-market gems for each mode.
			var safeWinnerCount, premiumWinnerCount, jackpotWinnerCount int
			var safeThinCount, premiumThinCount, jackpotThinCount int
			var jackpotGems []JackpotGemInfo
			var lowConfGems []LowConfidenceGemInfo

			tierStats := make(map[string]*TierPoolInfo)
			gemAdjustedPrice := make(map[string]float64)
			gemRawPrice := make(map[string]float64)

			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, "21/23c"}]

				// Get classification from the dedication-specific classification.
				classKey := GemClassificationKey{e.name, "21/23c"}
				gc, hasClass := classification.Gems[classKey]

				if gc.LowConfidence {
					lowConfGems = append(lowConfGems, LowConfidenceGemInfo{
						Name: e.name, Chaos: e.chaos, Listings: e.listings,
					})
					continue
				}

				tier := "FLOOR"
				if hasClass {
					tier = gc.Tier
				}

				// Risk-adjusted price.
				var sellProb, stabDisc float64
				if feat != nil {
					sellProb = sellProbabilityFactor(e.listings, feat.Low7Days, e.chaos)
					stabDisc = stabilityDiscount(feat.CVShort)
				} else {
					// No feature data — use reasonable defaults for corrupted gems.
					sellProb = sellProbabilityFactor(e.listings, 0, e.chaos)
					stabDisc = 1.0
				}
				adjustedPrice := e.chaos * sellProb * stabDisc
				gemAdjustedPrice[e.name] = adjustedPrice
				gemRawPrice[e.name] = e.chaos

				isThin := e.listings < 5

				// Track pool breakdown per tier.
				ts, ok := tierStats[tier]
				if !ok {
					ts = &TierPoolInfo{Tier: tier, MinPrice: e.chaos, MaxPrice: e.chaos}
					tierStats[tier] = ts
				}
				ts.Count++
				if e.chaos < ts.MinPrice {
					ts.MinPrice = e.chaos
				}
				if e.chaos > ts.MaxPrice {
					ts.MaxPrice = e.chaos
				}

				if isSafeTierWinner(tier) {
					safeWinnerCount++
					if isThin {
						safeThinCount++
					}
				}
				if isPremiumTierWinner(tier) {
					premiumWinnerCount++
					if isThin {
						premiumThinCount++
					}
				}
				if isJackpotTierWinner(tier) {
					jackpotWinnerCount++
					jackpotGems = append(jackpotGems, JackpotGemInfo{Name: e.name, Chaos: e.chaos})
					if isThin {
						jackpotThinCount++
					}
				}
			}

			// Build pool value arrays — one value per pool gem name.
			poolValues := make([]float64, 0, pool)
			poolValuesRaw := make([]float64, 0, pool)
			for name := range names {
				poolValues = append(poolValues, gemAdjustedPrice[name])
				poolValuesRaw = append(poolValuesRaw, gemRawPrice[name])
			}

			// Sort descending for expectedBestOf3.
			sort.Float64s(poolValues)
			for i, j := 0, len(poolValues)-1; i < j; i, j = i+1, j-1 {
				poolValues[i], poolValues[j] = poolValues[j], poolValues[i]
			}
			sort.Float64s(poolValuesRaw)
			for i, j := 0, len(poolValuesRaw)-1; i < j; i, j = i+1, j-1 {
				poolValuesRaw[i], poolValuesRaw[j] = poolValuesRaw[j], poolValuesRaw[i]
			}

			ev := expectedBestOf3(poolValues)
			evRaw := expectedBestOf3(poolValuesRaw)
			profit := evRaw - inputCost

			// Per-mode average winner value.
			var safeWinnerSum, premiumWinnerSum, jackpotWinnerSum float64
			var safeWinnerRawSum, premiumWinnerRawSum, jackpotWinnerRawSum float64
			for _, e := range entries {
				classKey := GemClassificationKey{e.name, "21/23c"}
				gc := classification.Gems[classKey]
				if gc.LowConfidence {
					continue
				}

				feat := featureLookup[featureKey{e.name, "21/23c"}]
				var sellProb, stabDisc float64
				if feat != nil {
					sellProb = sellProbabilityFactor(e.listings, feat.Low7Days, e.chaos)
					stabDisc = stabilityDiscount(feat.CVShort)
				} else {
					sellProb = sellProbabilityFactor(e.listings, 0, e.chaos)
					stabDisc = 1.0
				}
				adjPrice := e.chaos * sellProb * stabDisc
				tier := gc.Tier

				if isSafeTierWinner(tier) {
					safeWinnerSum += adjPrice
					safeWinnerRawSum += e.chaos
				}
				if isPremiumTierWinner(tier) {
					premiumWinnerSum += adjPrice
					premiumWinnerRawSum += e.chaos
				}
				if isJackpotTierWinner(tier) {
					jackpotWinnerSum += adjPrice
					jackpotWinnerRawSum += e.chaos
				}
			}

			// Build sorted pool breakdown (TOP -> FLOOR).
			tierOrder := []string{"TOP", "HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"}
			var poolBreakdown []TierPoolInfo
			for _, tier := range tierOrder {
				if ts, ok := tierStats[tier]; ok {
					ts.MinPrice = math.Round(ts.MinPrice)
					ts.MaxPrice = math.Round(ts.MaxPrice)
					poolBreakdown = append(poolBreakdown, *ts)
				}
			}

			// Safe mode result.
			safePWin := pWin3Picks(safeWinnerCount, pool)
			var safeAvgWin, safeAvgWinRaw float64
			if safeWinnerCount > 0 {
				safeAvgWin = safeWinnerSum / float64(safeWinnerCount)
				safeAvgWinRaw = safeWinnerRawSum / float64(safeWinnerCount)
			}
			var safeFontsToHit float64
			if safePWin > 0 {
				safeFontsToHit = 1.0 / safePWin
			}
			safeResult := DedicationResult{
				Time:              snapTime,
				Color:             color,
				GemType:           gemType,
				Pool:              pool,
				Winners:           safeWinnerCount,
				PWin:              safePWin,
				AvgWin:            safeAvgWin,
				AvgWinRaw:         safeAvgWinRaw,
				EV:                ev,
				EVRaw:             evRaw,
				InputCost:         inputCost,
				Profit:            profit,
				FontsToHit:        safeFontsToHit,
				Mode:              "safe",
				ThinPoolGems:      safeThinCount,
				LiquidityRisk:     computeLiquidityRisk(safeThinCount, safeWinnerCount),
				PoolBreakdown:     poolBreakdown,
				LowConfidenceGems: lowConfGems,
			}

			// Premium mode result.
			premiumPWin := pWin3Picks(premiumWinnerCount, pool)
			var premiumAvgWin, premiumAvgWinRaw float64
			if premiumWinnerCount > 0 {
				premiumAvgWin = premiumWinnerSum / float64(premiumWinnerCount)
				premiumAvgWinRaw = premiumWinnerRawSum / float64(premiumWinnerCount)
			}
			var premiumFontsToHit float64
			if premiumPWin > 0 {
				premiumFontsToHit = 1.0 / premiumPWin
			}
			premiumResult := DedicationResult{
				Time:          snapTime,
				Color:         color,
				GemType:       gemType,
				Pool:          pool,
				Winners:       premiumWinnerCount,
				PWin:          premiumPWin,
				AvgWin:        premiumAvgWin,
				AvgWinRaw:     premiumAvgWinRaw,
				EV:            ev,
				EVRaw:         evRaw,
				InputCost:     inputCost,
				Profit:        profit,
				FontsToHit:    premiumFontsToHit,
				Mode:          "premium",
				ThinPoolGems:  premiumThinCount,
				LiquidityRisk: computeLiquidityRisk(premiumThinCount, premiumWinnerCount),
			}

			// Jackpot mode result.
			jackpotPWin := pWin3Picks(jackpotWinnerCount, pool)
			var jackpotAvgWin, jackpotAvgWinRaw float64
			if jackpotWinnerCount > 0 {
				jackpotAvgWin = jackpotWinnerSum / float64(jackpotWinnerCount)
				jackpotAvgWinRaw = jackpotWinnerRawSum / float64(jackpotWinnerCount)
			}
			var jackpotFontsToHit float64
			if jackpotPWin > 0 {
				jackpotFontsToHit = 1.0 / jackpotPWin
			}
			jackpotResult := DedicationResult{
				Time:          snapTime,
				Color:         color,
				GemType:       gemType,
				Pool:          pool,
				Winners:       jackpotWinnerCount,
				PWin:          jackpotPWin,
				AvgWin:        jackpotAvgWin,
				AvgWinRaw:     jackpotAvgWinRaw,
				EV:            ev,
				EVRaw:         evRaw,
				InputCost:     inputCost,
				Profit:        profit,
				FontsToHit:    jackpotFontsToHit,
				JackpotGems:   jackpotGems,
				Mode:          "jackpot",
				ThinPoolGems:  jackpotThinCount,
				LiquidityRisk: computeLiquidityRisk(jackpotThinCount, jackpotWinnerCount),
			}

			if gemType == "skill" {
				analysis.Skills = append(analysis.Skills, safeResult, premiumResult, jackpotResult)
			} else {
				analysis.Transfigured = append(analysis.Transfigured, safeResult, premiumResult, jackpotResult)
			}
		}
	}

	return analysis
}
