package lab

import (
	"sort"
	"strings"
)

// CollectiveResult is a cross-analyzer "what to farm now" entry combining
// transfigure ROI with trend signals.
type CollectiveResult struct {
	TransfiguredName     string  `json:"transfiguredName"`
	BaseName             string  `json:"baseName"`
	Variant              string  `json:"variant"`
	GemColor             string  `json:"gemColor"`
	ROI                  float64 `json:"roi"`
	ROIPct               float64 `json:"roiPct"`
	WeightedROI          float64 `json:"weightedRoi"`
	WeightedROIPct       float64 `json:"weightedRoiPct"`
	BasePrice            float64 `json:"basePrice"`
	TransfiguredPrice    float64 `json:"transfiguredPrice"`
	BaseListings         int     `json:"baseListings"`
	TransfiguredListings int     `json:"transfiguredListings"`
	Confidence           string  `json:"confidence"`
	// From trends
	Signal          string  `json:"signal"`
	PriceVelocity   float64 `json:"priceVelocity"`
	ListingVelocity float64 `json:"listingVelocity"`
	CV              float64 `json:"cv"`
	HistPosition    float64 `json:"histPosition"`
	WindowSignal     string `json:"windowSignal"`
	AdvancedSignal   string `json:"advancedSignal"`
	LiquidityTier    string `json:"liquidityTier"`
	PriceTier        string `json:"priceTier"`
	TierAction       string `json:"tierAction"`
	SellUrgency      string `json:"sellUrgency"`
	SellReason       string `json:"sellReason"`
	Sellability      int    `json:"sellability"`
	SellabilityLabel string `json:"sellabilityLabel"`
	// From features/signals (risk-adjusted display)
	Low7Days            float64 `json:"low7d"`
	High7Days           float64 `json:"high7d"`
	SellConfidence      string  `json:"sellConfidence"`
	TradeConfidenceNote string `json:"tradeConfidenceNote,omitempty"`
	LowConfidence       bool   `json:"lowConfidence,omitempty"`
}

// CompareResult is a side-by-side gem comparison entry with sparkline data.
type CompareResult struct {
	TransfiguredName     string           `json:"transfiguredName"`
	BaseName             string           `json:"baseName"`
	Variant              string           `json:"variant"`
	GemColor             string           `json:"gemColor"`
	ROI                  float64          `json:"roi"`
	ROIPct               float64          `json:"roiPct"`
	BasePrice            float64          `json:"basePrice"`
	TransfiguredPrice    float64          `json:"transfiguredPrice"`
	Confidence           string           `json:"confidence"`
	Signal               string           `json:"signal"`
	CV                   float64          `json:"cv"`
	PriceVelocity        float64          `json:"priceVelocity"`
	ListingVelocity      float64          `json:"listingVelocity"`
	HistPosition         float64          `json:"histPosition"`
	Sparkline            []SparklinePoint `json:"sparkline"`
	Recommendation       string           `json:"recommendation"`
	SellUrgency          string           `json:"sellUrgency"`
	SellReason           string           `json:"sellReason"`
	Sellability          int              `json:"sellability"`
	SellabilityLabel     string           `json:"sellabilityLabel"`
	PriceTier            string           `json:"priceTier"`
	TierAction           string           `json:"tierAction"`
	WindowSignal         string           `json:"windowSignal"`
	BaseListings         int              `json:"baseListings"`
	LiquidityTier        string           `json:"liquidityTier"`
	TransListings        int              `json:"transListings"`
	// Risk-adjusted display fields (from features/signals)
	WeightedROI          float64 `json:"weightedRoi"`
	WeightedROIPct       float64 `json:"weightedRoiPct"`
	Low7Days                float64 `json:"low7d"`
	High7Days               float64 `json:"high7d"`
	SellConfidence       string  `json:"sellConfidence"`
	SellConfidenceReason string  `json:"sellConfidenceReason"`
	QuickSellPrice       float64 `json:"quickSellPrice"`
	RiskAdjustedPrice    float64 `json:"riskAdjustedPrice"`
}

// SparklinePoint is a single data point for sparkline charts.
type SparklinePoint struct {
	Time     string  `json:"time"`
	Price    float64 `json:"price"`
	Listings int     `json:"listings"`
}

// signalWeight returns the ROI multiplier for a given trend signal.
// Weights are gentle adjustments, NOT hard penalties — a DUMPING 1000c gem
// is still far better than a STABLE 80c gem. The price difference dominates;
// the signal is a tiebreaker within similar price ranges.
func signalWeight(signal string) float64 {
	switch signal {
	case "DUMPING":
		return 0.85
	case "HERD":
		return 0.95
	case "STABLE", "UNCERTAIN", "CAUTION":
		return 1.0
	case "RECOVERY":
		return 1.05
	case "DEMAND":
		return 1.1
	default:
		return 1.0
	}
}

// SortMode controls ranking order in the collective view.
type SortMode string

const (
	SortChaos SortMode = "chaos" // sort by weighted absolute ROI (default)
	SortPct   SortMode = "pct"   // sort by weighted ROI percentage
)

// RankCollective combines transfigure results with v2 gem signals to produce
// a ranked list of profitable farming targets. Results with TRAP signal are
// excluded. Budget filters on basePrice. The returned slice is sorted by
// the chosen metric descending and capped at limit entries.
// When budget <= 50 and sortBy is empty, defaults to SortPct.
func RankCollective(transfigure []TransfigureResult, signals []GemSignal, features []GemFeature, budget float64, limit int, sortBy SortMode) []CollectiveResult {
	// Budget-aware default: small budgets benefit from ROI% ranking.
	if sortBy == "" {
		if budget > 0 && budget <= 50 {
			sortBy = SortPct
		} else {
			sortBy = SortChaos
		}
	}
	// Index signals by (name, variant) for fast lookup.
	type sigKey struct{ name, variant string }
	sigIndex := make(map[sigKey]*GemSignal, len(signals))
	for i := range signals {
		s := &signals[i]
		sigIndex[sigKey{s.Name, s.Variant}] = s
	}

	// Index features by (name, variant) for CV, velocity, histPosition, etc.
	featIndex := make(map[sigKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featIndex[sigKey{f.Name, f.Variant}] = f
	}

	var results []CollectiveResult

	for _, tr := range transfigure {
		// Only include profitable, confident results.
		if tr.ROI <= 0 || tr.Confidence != "OK" {
			continue
		}

		// Budget filter on base price.
		if budget > 0 && tr.BasePrice > budget {
			continue
		}

		cr := CollectiveResult{
			TransfiguredName:     tr.TransfiguredName,
			BaseName:             tr.BaseName,
			Variant:              tr.Variant,
			GemColor:             tr.GemColor,
			ROI:                  tr.ROI,
			ROIPct:               tr.ROIPct,
			BasePrice:            tr.BasePrice,
			TransfiguredPrice:    tr.TransfiguredPrice,
			BaseListings:         tr.BaseListings,
			TransfiguredListings: tr.TransfiguredListings,
			Confidence:           tr.Confidence,
			Signal:               "STABLE", // default when no signal data
		}

		// Join with v2 gem signal data.
		if s, ok := sigIndex[sigKey{tr.TransfiguredName, tr.Variant}]; ok {
			cr.Signal = s.Signal
			cr.WindowSignal = s.WindowSignal
			cr.AdvancedSignal = s.AdvancedSignal
			cr.PriceTier = s.Tier
			cr.TierAction = tierAction(s.Signal, s.WindowSignal, s.Tier)
			cr.SellUrgency = s.SellUrgency
			cr.SellReason = s.SellReason
			cr.Sellability = s.Sellability
			cr.SellabilityLabel = s.SellabilityLabel
			cr.SellConfidence = s.SellConfidence
			cr.TradeConfidenceNote = s.TradeConfidenceNote
		}

		// Join with v2 gem feature data for velocity, CV, histPosition, etc.
		if f, ok := featIndex[sigKey{tr.TransfiguredName, tr.Variant}]; ok {
			cr.PriceVelocity = f.VelLongPrice
			cr.ListingVelocity = f.VelLongListing
			cr.CV = f.CV
			cr.HistPosition = f.HistPosition
			cr.Low7Days = f.Low7Days
			cr.High7Days = f.High7Days
			cr.LowConfidence = f.LowConfidence
			cr.LiquidityTier = liquidityTier(f.MarketDepth)
		}

		// Exclude TRAP gems entirely — no actionable signal.
		if cr.Signal == "CAUTION" {
			continue
		}

		// Weighted ROI: liquidity-based scoring with saturation penalty.
		// Default sellability to 50 (neutral) when no signal data exists,
		// so gems without signals still appear in rankings.
		sellability := cr.Sellability
		if sellability == 0 && cr.Signal == "" {
			sellability = 50
		}
		liquidityScore := float64(sellability) / 100.0
		var saturationPenalty float64
		if cr.Signal == "DUMPING" {
			if cr.TransfiguredListings < 15 {
				saturationPenalty = 0.5 // thin market DUMPING = real danger
			} else {
				saturationPenalty = 0.15 // liquid market DUMPING = likely noise
			}
		}
		cr.WeightedROI = cr.ROI * liquidityScore * (1.0 - saturationPenalty)
		cr.WeightedROIPct = cr.ROIPct * liquidityScore * (1.0 - saturationPenalty)
		results = append(results, cr)
	}

	// Sort by chosen metric descending.
	if sortBy == SortPct {
		sort.Slice(results, func(i, j int) bool {
			return results[i].WeightedROIPct > results[j].WeightedROIPct
		})
	} else {
		sort.Slice(results, func(i, j int) bool {
			return results[i].WeightedROI > results[j].WeightedROI
		})
	}

	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}

	return results
}

// BuildCompareResults builds side-by-side comparison data for specific gems.
// It assigns BEST/OK/AVOID recommendations based on weighted ROI ranking.
// ROI is computed using the cheapest base gem of the same color/variant (lab scenario:
// you transform a random gem of that color, not a specific base).
//
// requestedVariant is the user's chosen variant (e.g. "20/20"). When a gem has
// no transfigure data for this variant, the result preserves the requested variant
// instead of falling back to a different one or leaving it empty.
func BuildCompareResults(
	names []string,
	transfigure []TransfigureResult,
	signals []GemSignal,
	features []GemFeature,
	sparklines map[string][]SparklinePoint,
	requestedVariant string,
) []CompareResult {
	// Index transfigure by transfigured name + variant.
	type trKey struct{ name, variant string }
	trIndex := make(map[trKey]*TransfigureResult, len(transfigure))
	for i := range transfigure {
		t := &transfigure[i]
		trIndex[trKey{t.TransfiguredName, t.Variant}] = t
	}

	// Compute cheapest base price per (color, variant) for lab ROI.
	// In the lab, any gem of that color can be transformed — the cost basis
	// is the cheapest available base, not the specific matched base.
	type colorVariantKey struct{ color, variant string }
	cheapestBase := make(map[colorVariantKey]float64)
	for _, t := range transfigure {
		key := colorVariantKey{t.GemColor, t.Variant}
		if existing, ok := cheapestBase[key]; !ok || (t.BasePrice > 0 && t.BasePrice < existing) {
			cheapestBase[key] = t.BasePrice
		}
	}

	// Index signals by (name, variant).
	sigIndex := make(map[trKey]*GemSignal, len(signals))
	for i := range signals {
		s := &signals[i]
		sigIndex[trKey{s.Name, s.Variant}] = s
	}

	// Index features by (name, variant).
	featIndex := make(map[trKey]*GemFeature, len(features))
	for i := range features {
		f := &features[i]
		featIndex[trKey{f.Name, f.Variant}] = f
	}

	var results []CompareResult

	for _, name := range names {
		cr := CompareResult{
			TransfiguredName: name,
			Signal:           "STABLE",
		}

		// Find transfigure data — select the variant with highest ROI.
		found := false
		var bestTr *TransfigureResult
		for k, tr := range trIndex {
			if k.name == name {
				if bestTr == nil || tr.ROI > bestTr.ROI {
					bestTr = tr
				}
			}
		}
		if bestTr != nil {
			cr.BaseName = bestTr.BaseName
			cr.Variant = bestTr.Variant
			cr.GemColor = bestTr.GemColor
			cr.TransfiguredPrice = bestTr.TransfiguredPrice
			cr.Confidence = bestTr.Confidence

			// Use cheapest base of this color/variant as cost basis (lab scenario).
			colorBase, hasColorBase := cheapestBase[colorVariantKey{bestTr.GemColor, bestTr.Variant}]
			if hasColorBase && colorBase > 0 {
				cr.BasePrice = colorBase
				cr.ROI = bestTr.TransfiguredPrice - colorBase
				if colorBase > 0 {
					cr.ROIPct = (cr.ROI / colorBase) * 100
				}
			} else {
				cr.BasePrice = bestTr.BasePrice
				cr.ROI = bestTr.ROI
				cr.ROIPct = bestTr.ROIPct
			}
			found = true
		}

		if !found {
			// Gem not found in transfigure results — include with zero values
			// but preserve the requested variant so the frontend doesn't fall
			// back to a different one.
			cr.Confidence = "LOW"
			if requestedVariant != "" {
				cr.Variant = requestedVariant
			}
		}

		// Join v2 gem signal data.
		if s, ok := sigIndex[trKey{name, cr.Variant}]; ok {
			cr.Signal = s.Signal
			cr.SellUrgency = s.SellUrgency
			cr.SellReason = s.SellReason
			cr.Sellability = s.Sellability
			cr.SellabilityLabel = s.SellabilityLabel
			cr.PriceTier = s.Tier
			cr.TierAction = tierAction(s.Signal, s.WindowSignal, s.Tier)
			cr.WindowSignal = s.WindowSignal
			cr.SellConfidence = s.SellConfidence
			cr.SellConfidenceReason = s.TradeConfidenceNote
			cr.QuickSellPrice = s.QuickSellPrice
			cr.RiskAdjustedPrice = s.RiskAdjustedValue
		}

		// Join v2 gem feature data for velocity, CV, histPosition, etc.
		if f, ok := featIndex[trKey{name, cr.Variant}]; ok {
			cr.CV = f.CV
			cr.PriceVelocity = f.VelLongPrice
			cr.ListingVelocity = f.VelLongListing
			cr.HistPosition = f.HistPosition
			cr.BaseListings = 0 // TODO: base gem listings not available in v2 pipeline — requires separate query
			cr.LiquidityTier = liquidityTier(f.MarketDepth)
			cr.TransListings = f.Listings
			cr.Low7Days = f.Low7Days
			cr.High7Days = f.High7Days
		}

		// Attach sparkline.
		if pts, ok := sparklines[name]; ok {
			cr.Sparkline = pts
		}
		if cr.Sparkline == nil {
			cr.Sparkline = []SparklinePoint{}
		}

		results = append(results, cr)
	}

	// Assign recommendations: rank by ROI × sellability (backtested: 73% vs 67% for pure ROI).
	if len(results) > 0 {
		type ranked struct {
			idx   int
			score float64
		}
		ranks := make([]ranked, len(results))
		for i, cr := range results {
			w := signalWeight(cr.Signal)
			sell := float64(cr.Sellability)
			if sell == 0 {
				sell = 50 // default if no signal data
			}
			score := cr.ROI * w * (sell / 100)
			ranks[i] = ranked{idx: i, score: score}
			results[i].WeightedROI = score
			results[i].WeightedROIPct = cr.ROIPct * w * (sell / 100)
		}
		sort.Slice(ranks, func(i, j int) bool {
			return ranks[i].score > ranks[j].score
		})

		for pos, r := range ranks {
			cr := results[r.idx]
			if cr.Signal == "CAUTION" || cr.SellUrgency == "SELL_NOW" {
				results[r.idx].Recommendation = "AVOID"
			} else if cr.Signal == "DUMPING" && cr.TransListings < 15 {
				// DUMPING on thin market = real danger, avoid.
				results[r.idx].Recommendation = "AVOID"
			} else if cr.Sellability > 0 && cr.Sellability < 20 {
				results[r.idx].Recommendation = "AVOID"
			} else if pos == 0 {
				results[r.idx].Recommendation = "BEST"
			} else {
				results[r.idx].Recommendation = "OK"
			}
		}
	}

	return results
}

// BuildDedicationCompareResults builds compare results for Dedication lab mode.
// Each gem is scored against the corrupted 21/23c pool. The pool type (skill vs
// transfigured) is auto-detected from the gem name: names containing " of " are
// transfigured, others are non-transfigured skills.
// dedicationResults provides per-color input costs and pool context.
func BuildDedicationCompareResults(
	names []string,
	gemPrices []GemPrice,
	dedicationResults []DedicationResult,
	sparklines map[string][]SparklinePoint,
) []CompareResult {
	// Index gem prices by name. For corrupted 21/23c there should be at most one
	// entry per name, but if duplicates exist we keep the last (highest chaos).
	priceIndex := make(map[string]*GemPrice, len(gemPrices))
	for i := range gemPrices {
		g := &gemPrices[i]
		priceIndex[g.Name] = g
	}

	// Index Dedication results by (color, gemType) for input cost lookup.
	// Use the first mode encountered as the baseline — input cost is the same
	// across all modes (safe/premium/jackpot).
	type dedKey struct{ color, gemType string }
	inputCosts := make(map[dedKey]float64)
	for _, dr := range dedicationResults {
		k := dedKey{dr.Color, dr.GemType}
		if _, exists := inputCosts[k]; !exists {
			inputCosts[k] = dr.InputCost
		}
	}

	var results []CompareResult

	for _, name := range names {
		cr := CompareResult{
			TransfiguredName: name,
			Variant:          "21/23c",
			Signal:           "STABLE",
			Confidence:       "LOW",
		}

		// Auto-detect pool type from gem name.
		isTransfigured := strings.Contains(name, " of ")
		gemType := "skill"
		if isTransfigured {
			gemType = "transfigured"
		}

		g, found := priceIndex[name]
		if found {
			cr.GemColor = g.GemColor
			cr.TransfiguredPrice = g.Chaos
			cr.TransListings = g.Listings

			// Confidence based on listings.
			switch {
			case g.Listings >= 5:
				cr.Confidence = "HIGH"
			case g.Listings >= 2:
				cr.Confidence = "MEDIUM"
			default:
				cr.Confidence = "LOW"
			}

			// Look up the input cost for this color/pool.
			inputCost := inputCosts[dedKey{g.GemColor, gemType}]
			cr.BasePrice = inputCost
			cr.BaseName = gemType // "skill" or "transfigured" (pool label, not a specific base gem)

			if inputCost > 0 {
				cr.ROI = g.Chaos - inputCost
				cr.ROIPct = (cr.ROI / inputCost) * 100
			} else {
				cr.ROI = g.Chaos
			}
		}

		// Attach sparkline.
		if pts, ok := sparklines[name]; ok {
			cr.Sparkline = pts
		}
		if cr.Sparkline == nil {
			cr.Sparkline = []SparklinePoint{}
		}

		results = append(results, cr)
	}

	// Assign recommendations based on ROI ranking.
	if len(results) > 0 {
		type ranked struct {
			idx   int
			score float64
		}
		ranks := make([]ranked, len(results))
		for i, cr := range results {
			// For Dedication, weight by ROI and confidence.
			confMultiplier := 0.5
			switch cr.Confidence {
			case "HIGH":
				confMultiplier = 1.0
			case "MEDIUM":
				confMultiplier = 0.75
			}
			score := cr.ROI * confMultiplier
			ranks[i] = ranked{idx: i, score: score}
			results[i].WeightedROI = score
			results[i].WeightedROIPct = cr.ROIPct * confMultiplier
		}
		sort.Slice(ranks, func(i, j int) bool {
			return ranks[i].score > ranks[j].score
		})

		for pos, r := range ranks {
			cr := results[r.idx]
			if cr.Confidence == "LOW" && cr.TransListings < 2 {
				results[r.idx].Recommendation = "AVOID"
			} else if cr.ROI < 0 {
				results[r.idx].Recommendation = "AVOID"
			} else if pos == 0 {
				results[r.idx].Recommendation = "BEST"
			} else {
				results[r.idx].Recommendation = "OK"
			}
		}
	}

	return results
}
