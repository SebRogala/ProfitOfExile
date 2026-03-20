package lab

import (
	"sort"
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

// expectedBestOf3 computes the expected value of the best gem when drawing
// 3 without replacement from a pool. This models what the farmer actually does:
// they see 3 gems and pick the most valuable one.
//
// values must be sorted descending. Each value is the risk-adjusted price of a gem
// in the pool (0 for non-winners).
//
// Uses exact combinatorial calculation: for each possible "best gem" at rank i,
// compute the probability that all 3 draws are at rank i or worse, minus the
// probability that all 3 are at rank i+1 or worse.
func expectedBestOf3(values []float64) float64 {
	n := len(values)
	if n == 0 {
		return 0
	}
	if n < 3 {
		return values[0]
	}

	// The farmer draws 3 gems without replacement and picks the best.
	// E[max of 3] = sum over each gem i of: value[i] * P(gem i is the best of 3 drawn)
	//
	// P(gem at rank i is the best of 3) = P(gem i is drawn AND all other drawn gems are ranked worse)
	// = C(n-i-1, 2) / C(n, 3)  (choose 2 companions from the n-i-1 gems ranked worse)
	//
	// values are sorted descending: values[0] is the most valuable.
	total := float64(n*(n-1)*(n-2)) / 6.0 // C(n, 3)

	var ev float64
	for i := 0; i < n; i++ {
		if values[i] == 0 {
			break // all remaining are 0, no point continuing
		}
		worse := n - i - 1 // gems ranked worse than i
		if worse < 2 {
			// Gem i is among the bottom 2 — it can still be "best" if the other 2 are also bottom
			if worse == 1 {
				// Only 1 gem worse — need that 1 gem + gem i + 1 gem from above (impossible for "best")
				continue
			}
			continue
		}
		companions := float64(worse*(worse-1)) / 2.0 // C(worse, 2)
		prob := companions / total
		ev += values[i] * prob
	}

	return ev
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

			// Build per-gem value arrays for best-of-3 calculation.
			// Each gem contributes its risk-adjusted price to the pool.
			// Non-winners contribute 0 (you'd never pick them).
			safeValues := make([]float64, 0, pool)   // one entry per pool gem
			jackpotValues := make([]float64, 0, pool)
			var safeThinCount, jackpotThinCount int
			var safeWinnerCount, jackpotWinnerCount int

			// Track which gems we've seen (entries are variant-specific).
			// Fill values for gems that have entries in this variant.
			gemAdjustedPrice := make(map[string]float64) // name -> adjusted price
			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, variant}]
				if feat == nil {
					continue
				}

				adjustedPrice := e.chaos * feat.SellProbabilityFactor * feat.StabilityDiscount
				gemAdjustedPrice[e.name] = adjustedPrice
				isThin := e.listings < 5

				if isSafeTierWinner(feat.Tier) {
					safeWinnerCount++
					if isThin {
						safeThinCount++
					}
				}
				if isJackpotTierWinner(feat.Tier) {
					jackpotWinnerCount++
					if isThin {
						jackpotThinCount++
					}
				}
			}

			// Build the full pool value arrays — one value per pool gem name.
			// Gems not in this variant's entries get 0 (non-winner).
			for name := range poolNames[color] {
				adjPrice := gemAdjustedPrice[name] // 0 if not found
				feat := featureLookup[featureKey{name, variant}]

				var safeVal, jackpotVal float64
				if feat != nil && isSafeTierWinner(feat.Tier) {
					safeVal = adjPrice
				}
				if feat != nil && isJackpotTierWinner(feat.Tier) {
					jackpotVal = adjPrice
				}
				safeValues = append(safeValues, safeVal)
				jackpotValues = append(jackpotValues, jackpotVal)
			}

			// Sort descending for expectedBestOf3.
			sort.Float64s(safeValues)
			for i, j := 0, len(safeValues)-1; i < j; i, j = i+1, j-1 {
				safeValues[i], safeValues[j] = safeValues[j], safeValues[i]
			}
			sort.Float64s(jackpotValues)
			for i, j := 0, len(jackpotValues)-1; i < j; i, j = i+1, j-1 {
				jackpotValues[i], jackpotValues[j] = jackpotValues[j], jackpotValues[i]
			}

			// Safe mode result — expected value of the best gem from 3 random draws.
			safePWin := pWin3Picks(safeWinnerCount, pool)
			safeEV := expectedBestOf3(safeValues)
			safeProfit := safeEV - inputCost
			var safeAvgWin float64
			if safeWinnerCount > 0 {
				// Average winner value — still useful as context.
				var sum float64
				for _, v := range safeValues {
					if v > 0 {
						sum += v
					}
				}
				safeAvgWin = sum / float64(safeWinnerCount)
			}

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

			// Jackpot mode result — expected value of the best gem from 3 random draws.
			jackpotPWin := pWin3Picks(jackpotWinnerCount, pool)
			jackpotEV := expectedBestOf3(jackpotValues)
			jackpotProfit := jackpotEV - inputCost
			var jackpotAvgWin float64
			if jackpotWinnerCount > 0 {
				var sum float64
				for _, v := range jackpotValues {
					if v > 0 {
						sum += v
					}
				}
				jackpotAvgWin = sum / float64(jackpotWinnerCount)
			}

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
