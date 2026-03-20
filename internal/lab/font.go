package lab

import (
	"strings"
	"time"
)

// FontResult holds the computed EV for a single (color, variant) Font of Divine Skill analysis.
type FontResult struct {
	Time          time.Time
	Color         string  // RED, GREEN, BLUE
	Variant       string  // "1", "1/20", "20", "20/20"
	Pool          int     // total unique transfigured gem names of that color
	Winners       int
	PWin          float64
	AvgWin        float64
	EV            float64
	InputCost     float64
	Profit        float64
	Mode          string // "safe" or "jackpot"
	ThinPoolGems  int    // count of winners with < 5 listings
	LiquidityRisk string // "LOW", "MEDIUM", "HIGH"
}

// FontAnalysis holds the results of both Safe and Jackpot font analysis modes.
type FontAnalysis struct {
	Safe    []FontResult
	Jackpot []FontResult
}

// InputCostForVariant returns the estimated input cost in chaos for a gem variant.
func InputCostForVariant(variant string) float64 {
	switch variant {
	case "1":
		return 0.5
	case "1/20":
		return 1.5
	case "20":
		return 2.0
	case "20/20":
		return 3.5
	default:
		return 0
	}
}

// pWin3Picks computes the probability of getting at least one winner
// when drawing 3 gems without replacement from a pool.
func pWin3Picks(winners, total int) float64 {
	if winners <= 0 {
		return 0.0
	}
	if total < 3 || winners >= total {
		return 1.0
	}
	losers := total - winners
	return 1.0 - (float64(losers)/float64(total))*
		(float64(losers-1)/float64(total-1))*
		(float64(losers-2)/float64(total-2))
}

// isSafeTierWinner returns true if the tier qualifies as a winner in Safe mode (MID+).
func isSafeTierWinner(tier string) bool {
	return tier == "MID" || tier == "MID-HIGH" || tier == "HIGH" || tier == "TOP"
}

// isJackpotTierWinner returns true if the tier qualifies as a winner in Jackpot mode (HIGH+).
func isJackpotTierWinner(tier string) bool {
	return tier == "HIGH" || tier == "TOP"
}

// computeLiquidityRisk classifies liquidity risk based on the ratio of thin-market winners.
func computeLiquidityRisk(thinCount, winnerCount int) string {
	if winnerCount == 0 {
		return "LOW"
	}
	ratio := float64(thinCount) / float64(winnerCount)
	if ratio > 0.5 {
		return "HIGH"
	}
	if ratio > 0.2 {
		return "MEDIUM"
	}
	return "LOW"
}

// AnalyzeFont computes Font of Divine Skill EV per (color, variant) in two modes:
// Safe (MID+ tier winners) and Jackpot (HIGH+ tier winners).
// Winner contributions are risk-adjusted using SellProbabilityFactor and StabilityDiscount.
// Pool size = count of distinct transfigured gem NAMES per color (across all variants).
func AnalyzeFont(snapTime time.Time, gems []GemPrice, features []GemFeature) FontAnalysis {
	// Build feature lookup: "name|variant" -> *GemFeature
	type featureKey struct{ name, variant string }
	featureLookup := make(map[featureKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featureLookup[featureKey{f.Name, f.Variant}] = f
	}

	// Step 1: Build pool sizes — unique transfigured gem names per color (all variants).
	poolNames := map[string]map[string]struct{}{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	// Also index variant-specific gem entries for winner evaluation.
	type gemEntry struct {
		name     string
		chaos    float64
		listings int
	}
	// byColor[color][variant] = []gemEntry
	type colorVariantGems map[string][]gemEntry
	byColor := map[string]colorVariantGems{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	for _, g := range gems {
		if g.IsCorrupted {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}
		if !g.IsTransfigured {
			continue
		}
		color := g.GemColor
		if color != "RED" && color != "GREEN" && color != "BLUE" {
			continue
		}

		poolNames[color][g.Name] = struct{}{}

		byColor[color][g.Variant] = append(byColor[color][g.Variant], gemEntry{
			name:     g.Name,
			chaos:    g.Chaos,
			listings: g.Listings,
		})
	}

	variants := []string{"1", "1/20", "20", "20/20"}
	colors := []string{"RED", "GREEN", "BLUE"}

	var analysis FontAnalysis

	for _, color := range colors {
		pool := len(poolNames[color])
		if pool == 0 {
			continue
		}

		for _, variant := range variants {
			inputCost := InputCostForVariant(variant)
			entries := byColor[color][variant]

			// Accumulators for Safe mode (MID+)
			var safeWinnerCount int
			var safeWinnerSum float64
			var safeThinCount int

			// Accumulators for Jackpot mode (HIGH+)
			var jackpotWinnerCount int
			var jackpotWinnerSum float64
			var jackpotThinCount int

			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, variant}]

				// Determine tier: use feature tier if available, skip otherwise.
				var tier string
				var sellProb, stabDisc float64
				if feat != nil {
					tier = feat.Tier
					sellProb = feat.SellProbabilityFactor
					stabDisc = feat.StabilityDiscount
				} else {
					// No feature data for this gem — skip for tier-based classification.
					continue
				}

				adjustedPrice := e.chaos * sellProb * stabDisc
				isThin := e.listings < 5

				if isSafeTierWinner(tier) {
					safeWinnerCount++
					safeWinnerSum += adjustedPrice
					if isThin {
						safeThinCount++
					}
				}
				if isJackpotTierWinner(tier) {
					jackpotWinnerCount++
					jackpotWinnerSum += adjustedPrice
					if isThin {
						jackpotThinCount++
					}
				}
			}

			// Safe mode result
			var safeAvgWin float64
			if safeWinnerCount > 0 {
				safeAvgWin = safeWinnerSum / float64(safeWinnerCount)
			}
			safePWin := pWin3Picks(safeWinnerCount, pool)
			safeEV := safePWin * safeAvgWin
			safeProfit := safeEV - inputCost

			analysis.Safe = append(analysis.Safe, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
				Pool:          pool,
				Winners:       safeWinnerCount,
				PWin:          safePWin,
				AvgWin:        safeAvgWin,
				EV:            safeEV,
				InputCost:     inputCost,
				Profit:        safeProfit,
				Mode:          "safe",
				ThinPoolGems:  safeThinCount,
				LiquidityRisk: computeLiquidityRisk(safeThinCount, safeWinnerCount),
			})

			// Jackpot mode result
			var jackpotAvgWin float64
			if jackpotWinnerCount > 0 {
				jackpotAvgWin = jackpotWinnerSum / float64(jackpotWinnerCount)
			}
			jackpotPWin := pWin3Picks(jackpotWinnerCount, pool)
			jackpotEV := jackpotPWin * jackpotAvgWin
			jackpotProfit := jackpotEV - inputCost

			analysis.Jackpot = append(analysis.Jackpot, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
				Pool:          pool,
				Winners:       jackpotWinnerCount,
				PWin:          jackpotPWin,
				AvgWin:        jackpotAvgWin,
				EV:            jackpotEV,
				InputCost:     inputCost,
				Profit:        jackpotProfit,
				Mode:          "jackpot",
				ThinPoolGems:  jackpotThinCount,
				LiquidityRisk: computeLiquidityRisk(jackpotThinCount, jackpotWinnerCount),
			})
		}
	}

	return analysis
}
