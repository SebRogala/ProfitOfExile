package lab

import (
	"math"
	"sort"
	"time"
)

// FontResult holds the computed EV for a single (color, variant) Font of Divine Skill analysis.
type FontResult struct {
	Time              time.Time
	Color             string  // RED, GREEN, BLUE
	Variant           string  // "1", "1/20", "20", "20/20"
	Pool              int     // total unique transfigured gem names of that color
	Winners           int     // count of tier-qualifying gems (LOW+ for safe, MID-HIGH+ for premium, TOP for jackpot)
	PWin              float64 // probability of seeing at least 1 winner in 3 picks
	AvgWin            float64 // average risk-adjusted value when you hit (secondary info)
	AvgWinRaw         float64 // average RAW listed price when you hit (primary display)
	EV                float64 // expected income per font (risk-adjusted, internal use)
	EVRaw             float64 // expected income per font using raw listed prices (primary display)
	InputCost         float64
	Profit            float64 // EVRaw - InputCost
	FontsToHit        float64          // expected fonts until hitting a winner (1/pWin), 0 if pWin=0
	JackpotGems       []JackpotGemInfo // TOP gem names+prices (only for jackpot mode, 1-3 gems)
	Mode              string  // "safe", "premium", or "jackpot"
	ThinPoolGems      int     // count of winners with < 5 listings
	LiquidityRisk     string  // "LOW", "MEDIUM", "HIGH"
	PoolBreakdown     []TierPoolInfo         `json:"poolBreakdown,omitempty"`     // per-tier gem counts and price ranges
	LowConfidenceGems []LowConfidenceGemInfo `json:"lowConfidenceGems,omitempty"` // gems excluded from EV due to thin market
}

// LowConfidenceGemInfo holds basic info for a gem excluded from EV calculations
// due to low market confidence (thin listings, unreliable price).
type LowConfidenceGemInfo struct {
	Name     string  `json:"name"`
	Chaos    float64 `json:"chaos"`
	Listings int     `json:"listings"`
}

// TierPoolInfo holds the count and price range for gems in a specific tier within a color pool.
type TierPoolInfo struct {
	Tier     string  `json:"tier"`
	Count    int     `json:"count"`
	MinPrice float64 `json:"minPrice"`
	MaxPrice float64 `json:"maxPrice"`
}

// JackpotGemInfo holds name, price and trade URL for a TOP-tier gem shown in Jackpot tooltip.
type JackpotGemInfo struct {
	Name           string  `json:"name"`
	Chaos          float64 `json:"chaos"`
	TradeURL       string  `json:"tradeUrl"`
	GCPRecipeCost  float64 `json:"gcpRecipeCost,omitempty"`  // 20/0 base + 20×GCP
	GCPRecipeBase  float64 `json:"gcpRecipeBase,omitempty"`  // 20/0 base price alone
	GCPRecipeSaves float64 `json:"gcpRecipeSaves,omitempty"` // savings vs 20/20 base
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
func AnalyzeFont(snapTime time.Time, features []GemFeature) FontAnalysis {
	// Build feature lookup: "name|variant" -> *GemFeature
	// Build pool and entries from features — single source of truth.
	// Features already filter: transfigured, not corrupted, not Trarthus, chaos > 5.
	type featureKey struct{ name, variant string }
	featureLookup := make(map[featureKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featureLookup[featureKey{f.Name, f.Variant}] = f
	}

	// Pool sizes: unique gem names per color (across all variants).
	poolNames := map[string]map[string]struct{}{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	// Variant-specific entries for winner evaluation.
	type gemEntry struct {
		name     string
		chaos    float64
		listings int
	}
	type colorVariantGems map[string][]gemEntry
	byColor := map[string]colorVariantGems{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	for i := range features {
		f := &features[i]
		color := f.GemColor
		if color != "RED" && color != "GREEN" && color != "BLUE" {
			continue
		}

		poolNames[color][f.Name] = struct{}{}

		byColor[color][f.Variant] = append(byColor[color][f.Variant], gemEntry{
			name:     f.Name,
			chaos:    f.Chaos,
			listings: f.Listings,
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
			var jackpotGems []JackpotGemInfo
			var lowConfGems []LowConfidenceGemInfo

			// Pool breakdown: track count and price range per tier.
			tierStats := make(map[string]*TierPoolInfo)

			gemAdjustedPrice := make(map[string]float64)
			gemRawPrice := make(map[string]float64)
			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, variant}]
				if feat == nil {
					continue
				}

				// Low-confidence gems: track separately, exclude from EV.
				if feat.LowConfidence {
					lowConfGems = append(lowConfGems, LowConfidenceGemInfo{
						Name: e.name, Chaos: e.chaos, Listings: e.listings,
					})
					continue
				}

				// Use unified tier from classification.
				tier := feat.Tier

				// Pool values for EV calculation (raw prices, no capping).
				sellProb := sellProbabilityFactor(e.listings, feat.Low7Days, e.chaos)
				stabDisc := stabilityDiscount(feat.CVShort)
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

				// Winner counting — all use feat.Tier now.
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

			// Build the full pool value array — one value per pool gem name.
			// EVERY gem gets its risk-adjusted price (the farmer always picks the best
			// of 3 regardless of tier). Winner tracking is for hit-rate display only.
			// The same values array is used for all three modes since EV is identical.
			poolValues := make([]float64, 0, pool)
			poolValuesRaw := make([]float64, 0, pool)
			for name := range poolNames[color] {
				poolValues = append(poolValues, gemAdjustedPrice[name])
				poolValuesRaw = append(poolValuesRaw, gemRawPrice[name])
			}

			// Sort descending for expectedBestOf3.
			sort.Float64s(poolValues)
			for i, j := 0, len(poolValues)-1; i < j; i, j = i+1, j-1 {
				poolValues[i], poolValues[j] = poolValues[j], poolValues[i]
			}

			// Sort raw pool descending too.
			sort.Float64s(poolValuesRaw)
			for i, j := 0, len(poolValuesRaw)-1; i < j; i, j = i+1, j-1 {
				poolValuesRaw[i], poolValuesRaw[j] = poolValuesRaw[j], poolValuesRaw[i]
			}

			// EV is identical for all three modes — same pool, same best-of-3 formula.
			ev := expectedBestOf3(poolValues)
			evRaw := expectedBestOf3(poolValuesRaw)
			profit := evRaw - inputCost

			// Compute per-mode average winner value from the tier-specific gems.
			// Both raw (listed price) and risk-adjusted sums.
			var safeWinnerSum, premiumWinnerSum, jackpotWinnerSum float64
			var safeWinnerRawSum, premiumWinnerRawSum, jackpotWinnerRawSum float64
			for _, e := range entries {
				feat := featureLookup[featureKey{e.name, variant}]
				if feat == nil || feat.LowConfidence {
					continue
				}
				sellProb := sellProbabilityFactor(e.listings, feat.Low7Days, e.chaos)
				stabDisc := stabilityDiscount(feat.CVShort)
				adjPrice := e.chaos * sellProb * stabDisc
				tier := feat.Tier
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

			// Build sorted pool breakdown (TOP → FLOOR).
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
			analysis.Safe = append(analysis.Safe, FontResult{
				Time:              snapTime,
				Color:             color,
				Variant:           variant,
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
			})

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
			analysis.Premium = append(analysis.Premium, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
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
			})

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
			analysis.Jackpot = append(analysis.Jackpot, FontResult{
				Time:          snapTime,
				Color:         color,
				Variant:       variant,
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
			})
		}
	}

	return analysis
}
