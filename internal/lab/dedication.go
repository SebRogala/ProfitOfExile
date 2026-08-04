package lab

import (
	"math"
	"sort"
	"strings"
	"time"
)

// DedicationVariants are the corrupted gem variants the Dedication font pool is
// analyzed for. Each is a separate market with its own pool, tiers and input
// cost — they are never merged into one pool.
var DedicationVariants = []string{"21/23c", "21/20c"}

// DefaultDedicationVariant is the variant served when a caller names none.
const DefaultDedicationVariant = "21/23c"

// IsDedicationVariant reports whether variant is one of the analyzed corrupted
// Dedication variants.
func IsDedicationVariant(variant string) bool {
	for _, v := range DedicationVariants {
		if v == variant {
			return true
		}
	}
	return false
}

// DedicationResult holds the computed EV for a single (variant, color, gemType) Dedication lab analysis.
// GemType is "skill" (non-transfigured corrupted gem) or "transfigured" (transfigured corrupted gem).
type DedicationResult struct {
	Time              time.Time
	Variant           string                 // corrupted variant the pool was computed over, e.g. "21/23c"
	Color             string                 // RED, GREEN, BLUE
	GemType           string                 // "skill" or "transfigured"
	Pool              int                    // total unique gem names of that color in the pool
	Winners           int                    // count of tier-qualifying gems
	PWin              float64                // probability of seeing at least 1 winner in 3 picks
	AvgWin            float64                // average risk-adjusted value when you hit
	AvgWinRaw         float64                // average RAW listed price when you hit
	EV                float64                // expected income per font (risk-adjusted)
	EVRaw             float64                // expected income per font using raw listed prices
	InputCost         float64                // avg of 10 cheapest per color per pool
	Profit            float64                // EVRaw - InputCost
	FontsToHit        float64                // expected fonts until hitting a winner (1/pWin)
	JackpotGems       []JackpotGemInfo       // TOP gem names+prices (only for jackpot mode)
	Mode              string                 // "safe", "premium", or "jackpot"
	ThinPoolGems      int                    // count of winners with < 5 listings
	LiquidityRisk     string                 // "LOW", "MEDIUM", "HIGH"
	PoolBreakdown     []TierPoolInfo         `json:"poolBreakdown,omitempty"`
	LowConfidenceGems []LowConfidenceGemInfo `json:"lowConfidenceGems,omitempty"`
}

// DedicationAnalysis holds the results of Dedication lab analysis for both pools.
// Each slice carries the results of every variant in DedicationVariants; read
// them through FilterDedicationVariant rather than assuming a single variant.
type DedicationAnalysis struct {
	Skills       []DedicationResult
	Transfigured []DedicationResult
}

// FilterDedicationVariant returns the subset of results computed for variant.
func FilterDedicationVariant(results []DedicationResult, variant string) []DedicationResult {
	out := make([]DedicationResult, 0, len(results))
	for _, dr := range results {
		if dr.Variant == variant {
			out = append(out, dr)
		}
	}
	return out
}

// isDedicationFeed returns true if the gem can be fed INTO the Dedication craft:
// corrupted, one of the three gem colours, not a support gem, not Trarthus.
//
// The colour requirement carries the rule for anything the colour resolver
// cannot place. The craft transforms a corrupted gem into another "of the same
// colour", so a colourless item is neither a legal input nor a possible
// outcome — the 3.29 Pact gems (Beidat, Ghorr, K'Tash, Lycia) are Exceptional
// skill gems with no attribute requirement and belong out of the pool.
//
// It is a blunt instrument: a gem missing from the gem_colors seed resolves the
// same way and is dropped for a reason that is not true of it. Dark Bargain and
// Mana-Infused Staff are exactly that today — real coloured skill gems (3.29
// renamed Dark Pact and added Mana-Infused Staff as an Intelligence/Strength
// skill) that we cannot colour, so they leave the pool silently.
func isDedicationFeed(g GemPrice) bool {
	switch g.GemColor {
	case "RED", "GREEN", "BLUE":
	default:
		return false
	}
	// A Vaal gem at 21/23 cannot exist: it would take three corruption outcomes
	// — the Vaal transform, the extra level and the extra quality — and the
	// Temple's double corrupt grants two. At 21/20 it is two outcomes and does
	// exist (25 such listings on 2026-07-30), so it stays a legal feed there.
	// The price feed carries none today; this keeps a bad listing from pricing
	// into what a run costs.
	if strings.HasPrefix(g.Name, "Vaal ") && g.Variant == "21/23c" {
		return false
	}
	return g.IsCorrupted &&
		!strings.Contains(g.Name, "Support") &&
		!strings.Contains(g.Name, "Trarthus")
}

// isDedicationGem returns true if the gem can come OUT of the Dedication craft.
// That is every feed gem except a Vaal gem: a Vaal gem is a legal input but is
// never an outcome (measured in game, 2026-07-30), so it prices into what the
// run costs but never into what it returns.
//
// The distinction has to live in the predicate rather than in the pool loop:
// tier classification and the rankings read this directly, and a colourless 36k
// listing was setting the TOP boundary for every colour before it did.
func isDedicationGem(g GemPrice) bool {
	return isDedicationFeed(g) && !strings.HasPrefix(g.Name, "Vaal ")
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

// AnalyzeDedication computes Dedication lab EV for every variant in
// DedicationVariants, per (color, gemType) in three modes. The gems slice must
// be the full latest snapshot. Features are used for risk-adjustment.
func AnalyzeDedication(snapTime time.Time, gems []GemPrice, features []GemFeature) DedicationAnalysis {
	var combined DedicationAnalysis
	for _, variant := range DedicationVariants {
		a := analyzeDedicationVariant(snapTime, gems, features, variant)
		combined.Skills = append(combined.Skills, a.Skills...)
		combined.Transfigured = append(combined.Transfigured, a.Transfigured...)
	}
	return combined
}

// BuildDedicationCorpus derives everything the Dedication read paths need from
// the snapshot the analysis was computed over, so a request never has to load
// the snapshot itself.
//
// The two request-shaped halves are the whole point. The collective rankings
// used to be built per request from a full LatestGemPrices materialisation
// (7,325 rows, two queries) whose only other input — the analysis — was already
// cached; the compare path re-queried the same snapshot for a handful of names.
// Both are pure functions of (gems, analysis), so the tick computes them once.
//
// A key is emitted for every DedicationVariants entry even when the snapshot
// holds nothing at that variant, because an absent key and an empty one are the
// same answer and the caller must not be able to read either as cold.
func BuildDedicationCorpus(gems []GemPrice, analysis DedicationAnalysis) DedicationCorpus {
	corpus := DedicationCorpus{
		Analysis:  analysis,
		Rankings:  make(map[string][]CollectiveResult, len(DedicationVariants)),
		GemPrices: make(map[string][]GemPrice, len(DedicationVariants)),
	}
	for _, variant := range DedicationVariants {
		// This variant's results from both pools — the same narrowing the
		// handlers used to do per request, and what the ranking's ROI is
		// measured against.
		results := FilterDedicationVariant(analysis.Skills, variant)
		results = append(results, FilterDedicationVariant(analysis.Transfigured, variant)...)

		// Unfiltered: search and limit are the request's, applied by
		// FilterDedicationRankings over this list.
		corpus.Rankings[variant] = RankDedicationCollective(gems, results, 0, "", variant)

		// Every corrupted gem at the variant, not just the rankable ones. The
		// compare path answers for names the font can never hand out — a Vaal
		// gem above all — and marks them as not an outcome, so narrowing this to
		// isDedicationGem would silently turn those rows into "no price found".
		var prices []GemPrice
		for _, g := range gems {
			if g.IsCorrupted && g.Variant == variant {
				prices = append(prices, g)
			}
		}
		corpus.GemPrices[variant] = prices
	}
	return corpus
}

// analyzeDedicationVariant computes the analysis for a single corrupted variant.
func analyzeDedicationVariant(snapTime time.Time, gems []GemPrice, features []GemFeature, variant string) DedicationAnalysis {
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
	// Feed prices are a superset of the outcome pool: a Vaal gem is something you
	// can buy and put into the font, it is just never what comes back out. Input
	// cost is what the run costs, so it is priced over what you may feed.
	feedPrices := make(map[poolKey][]float64)

	colors := []string{"RED", "GREEN", "BLUE"}
	gemTypes := []string{"skill", "transfigured"}

	for _, c := range colors {
		for _, gt := range gemTypes {
			k := poolKey{c, gt}
			poolNames[k] = make(map[string]struct{})
		}
	}

	for _, g := range gems {
		if !isDedicationFeed(g) || g.Variant != variant {
			continue
		}

		gt := "skill"
		if g.IsTransfigured {
			gt = "transfigured"
		}
		k := poolKey{g.GemColor, gt}

		feedPrices[k] = append(feedPrices[k], g.Chaos)

		if !isDedicationGem(g) {
			continue
		}
		poolNames[k][g.Name] = struct{}{}
		poolGems[k] = append(poolGems[k], gemEntry{
			name:     g.Name,
			chaos:    g.Chaos,
			listings: g.Listings,
		})
	}

	// Precompute classification once per pool type (not once per color).
	classificationByType := map[string]ClassificationResult{
		"skill":        ComputeDedicationClassification(gems, false, variant),
		"transfigured": ComputeDedicationClassification(gems, true, variant),
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

			// Priced over what may be fed, not over what may come out — the two
			// differ by the Vaal gems, which are buyable inputs and never outputs.
			inputCost := dedicationInputCostFromPrices(feedPrices[k])

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
				feat := featureLookup[featureKey{e.name, variant}]

				// Get classification from the dedication-specific classification.
				classKey := GemClassificationKey{e.name, variant}
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

			// Build pool value arrays — ALL gems including low-confidence (RISKY).
			//
			// WHY RISKY GEMS STAY IN THE POOL AT PRICE 0:
			// The font draws from ALL corrupted gems of the color — it doesn't
			// skip low-confidence ones. If 23 of 51 RED gems are RISKY, drawing
			// 3 random gems means ~45% chance each pick is a RISKY gem worth 0.
			// Excluding them from the pool would overestimate EV by pretending
			// you only ever draw reliable gems. Including them at 0 correctly
			// models the real probability: sometimes you draw junk, and that
			// drags down your expected best-of-3 pick value.
			//
			// Pool size = total gems (including RISKY) for accurate pWin3Picks.
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
				classKey := GemClassificationKey{e.name, variant}
				gc := classification.Gems[classKey]
				if gc.LowConfidence {
					continue
				}

				feat := featureLookup[featureKey{e.name, variant}]
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
				Variant:           variant,
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
				Variant:       variant,
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
				Variant:       variant,
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
