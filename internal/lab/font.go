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
	Winners       int     // count of tier-qualifying gems (LOW+ for safe, MID-HIGH+ for premium, TOP for jackpot)
	PWin          float64 // probability of seeing at least 1 winner in 3 picks
	AvgWin        float64 // average value when you DO hit a winner
	EV            float64 // expected income per font (best-of-3 from full pool, all gems valued)
	InputCost     float64
	Profit        float64 // EV - InputCost
	FontsToHit    float64 // expected fonts until hitting a winner (1/pWin), 0 if pWin=0
	Mode          string  // "safe", "premium", or "jackpot"
	ThinPoolGems  int     // count of winners with < 5 listings
	LiquidityRisk string  // "LOW", "MEDIUM", "HIGH"
}

// FontAnalysis holds the results of Safe, Premium and Jackpot font analysis modes.
type FontAnalysis struct {
	Safe    []FontResult
	Premium []FontResult
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

// isSafeTierWinner returns true if the tier qualifies as a winner in Safe mode (LOW+).
// LOW+ = upper half of the pool (everything above FLOOR). This matches the farmer's
// experience of "almost guaranteed to hit a decent gem" in small pools like RED.
func isSafeTierWinner(tier string) bool {
	return tier == "LOW" || tier == "MID" || tier == "MID-HIGH" || tier == "HIGH" || tier == "TOP"
}

// isPremiumTierWinner returns true if the tier qualifies as a winner in Premium mode (MID-HIGH+).
func isPremiumTierWinner(tier string) bool {
	return tier == "MID-HIGH" || tier == "HIGH" || tier == "TOP"
}

// isJackpotTierWinner returns true if the tier qualifies as a winner in Jackpot mode (TOP only).
func isJackpotTierWinner(tier string) bool {
	return tier == "TOP"
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

// AnalyzeFont computes Font of Divine Skill EV per (color, variant) in three modes:
// Safe (LOW+ tier winners), Premium (MID-HIGH+ tier winners), and Jackpot (TOP only).
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

			// Count winners and thin-market gems for each mode.
			var safeWinnerCount, premiumWinnerCount, jackpotWinnerCount int
			var safeThinCount, premiumThinCount, jackpotThinCount int

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
				if isPremiumTierWinner(feat.Tier) {
					premiumWinnerCount++
					if isThin {
						premiumThinCount++
					}
				}
				if isJackpotTierWinner(feat.Tier) {
					jackpotWinnerCount++
					if isThin {
						jackpotThinCount++
					}
				}
			}

			// Build the full pool value array — one value per pool gem name.
			// EVERY gem gets its risk-adjusted price (the farmer always picks the best
			// of 3 regardless of tier). Winner tracking is for hit-rate display only.
			// The same values array is used for all three modes since EV is identical.
			poolValues := make([]float64, 0, pool)
			for name := range poolNames[color] {
				adjPrice := gemAdjustedPrice[name] // 0 if not in this variant
				poolValues = append(poolValues, adjPrice)
			}

			// Sort descending for expectedBestOf3.
			sort.Float64s(poolValues)
			for i, j := 0, len(poolValues)-1; i < j; i, j = i+1, j-1 {
				poolValues[i], poolValues[j] = poolValues[j], poolValues[i]
			}

			// EV is identical for all three modes — same pool, same best-of-3 formula.
			ev := expectedBestOf3(poolValues)
			profit := ev - inputCost

			// Compute per-mode average winner value from the tier-specific gems.
			var safeWinnerSum, premiumWinnerSum, jackpotWinnerSum float64
			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, variant}]
				if feat == nil {
					continue
				}
				adjPrice := e.chaos * feat.SellProbabilityFactor * feat.StabilityDiscount
				if isSafeTierWinner(feat.Tier) {
					safeWinnerSum += adjPrice
				}
				if isPremiumTierWinner(feat.Tier) {
					premiumWinnerSum += adjPrice
				}
				if isJackpotTierWinner(feat.Tier) {
					jackpotWinnerSum += adjPrice
				}
			}

			// Safe mode result.
			safePWin := pWin3Picks(safeWinnerCount, pool)
			var safeAvgWin float64
			if safeWinnerCount > 0 {
				safeAvgWin = safeWinnerSum / float64(safeWinnerCount)
			}
			var safeFontsToHit float64
			if safePWin > 0 {
				safeFontsToHit = 1.0 / safePWin
			}
			analysis.Safe = append(analysis.Safe, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
				Pool:          pool,
				Winners:       safeWinnerCount,
				PWin:          safePWin,
				AvgWin:        safeAvgWin,
				EV:            ev,
				InputCost:     inputCost,
				Profit:        profit,
				FontsToHit:    safeFontsToHit,
				Mode:          "safe",
				ThinPoolGems:  safeThinCount,
				LiquidityRisk: computeLiquidityRisk(safeThinCount, safeWinnerCount),
			})

			// Premium mode result.
			premiumPWin := pWin3Picks(premiumWinnerCount, pool)
			var premiumAvgWin float64
			if premiumWinnerCount > 0 {
				premiumAvgWin = premiumWinnerSum / float64(premiumWinnerCount)
			}
			var premiumFontsToHit float64
			if premiumPWin > 0 {
				premiumFontsToHit = 1.0 / premiumPWin
			}
			analysis.Premium = append(analysis.Premium, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
				Pool:          pool,
				Winners:       premiumWinnerCount,
				PWin:          premiumPWin,
				AvgWin:        premiumAvgWin,
				EV:            ev,
				InputCost:     inputCost,
				Profit:        profit,
				FontsToHit:    premiumFontsToHit,
				Mode:          "premium",
				ThinPoolGems:  premiumThinCount,
				LiquidityRisk: computeLiquidityRisk(premiumThinCount, premiumWinnerCount),
			})

			// Jackpot mode result.
			jackpotPWin := pWin3Picks(jackpotWinnerCount, pool)
			var jackpotAvgWin float64
			if jackpotWinnerCount > 0 {
				jackpotAvgWin = jackpotWinnerSum / float64(jackpotWinnerCount)
			}
			var jackpotFontsToHit float64
			if jackpotPWin > 0 {
				jackpotFontsToHit = 1.0 / jackpotPWin
			}
			analysis.Jackpot = append(analysis.Jackpot, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
				Pool:          pool,
				Winners:       jackpotWinnerCount,
				PWin:          jackpotPWin,
				AvgWin:        jackpotAvgWin,
				EV:            ev,
				InputCost:     inputCost,
				Profit:        profit,
				FontsToHit:    jackpotFontsToHit,
				Mode:          "jackpot",
				ThinPoolGems:  jackpotThinCount,
				LiquidityRisk: computeLiquidityRisk(jackpotThinCount, jackpotWinnerCount),
			})
		}
	}

	return analysis
}
