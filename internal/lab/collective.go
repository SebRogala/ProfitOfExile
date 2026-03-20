package lab

import (
	"fmt"
	"sort"
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
	GlobalTier       string `json:"globalTier"`
	TierAction       string `json:"tierAction"`
	SellUrgency      string `json:"sellUrgency"`
	SellReason       string `json:"sellReason"`
	Sellability      int    `json:"sellability"`
	SellabilityLabel string `json:"sellabilityLabel"`
	// From features/signals (risk-adjusted display)
	Low7d          float64 `json:"low7d"`
	High7d         float64 `json:"high7d"`
	SellConfidence string  `json:"sellConfidence"`
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
	WeightedROI        float64 `json:"weightedRoi"`
	Low7d              float64 `json:"low7d"`
	High7d             float64 `json:"high7d"`
	SellConfidence     string  `json:"sellConfidence"`
	SellConfidenceReason string `json:"sellConfidenceReason"`
	QuickSellPrice     float64 `json:"quickSellPrice"`
}

// SparklinePoint is a single data point for sparkline charts.
type SparklinePoint struct {
	Time     string  `json:"time"`
	Price    float64 `json:"price"`
	Listings int     `json:"listings"`
}

// signalWeight returns the ROI multiplier for a given trend signal.
// UNCERTAIN maps to 1.0 (neutral) — no directional prediction, no weight adjustment.
func signalWeight(signal string) float64 {
	switch signal {
	case "TRAP":
		return 0 // excluded
	case "DUMPING":
		return 0.3
	case "HERD":
		return 0.8
	case "STABLE", "UNCERTAIN":
		return 1.0
	case "RECOVERY":
		return 1.2
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

// RankCollective combines transfigure results with trend signals to produce
// a ranked list of profitable farming targets. Results with TRAP signal are
// excluded. Budget filters on basePrice. The returned slice is sorted by
// the chosen metric descending and capped at limit entries.
// When budget <= 50 and sortBy is empty, defaults to SortPct.
func RankCollective(transfigure []TransfigureResult, trends []TrendResult, budget float64, limit int, sortBy SortMode) []CollectiveResult {
	// Budget-aware default: small budgets benefit from ROI% ranking.
	if sortBy == "" {
		if budget > 0 && budget <= 50 {
			sortBy = SortPct
		} else {
			sortBy = SortChaos
		}
	}
	// Index trends by (name, variant) for fast lookup.
	type trendKey struct{ name, variant string }
	trendIndex := make(map[trendKey]*TrendResult, len(trends))
	for i := range trends {
		t := &trends[i]
		trendIndex[trendKey{t.Name, t.Variant}] = t
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
			Signal:               "STABLE", // default when no trend data
		}

		// Join with trend data.
		if t, ok := trendIndex[trendKey{tr.TransfiguredName, tr.Variant}]; ok {
			cr.Signal = t.Signal
			cr.PriceVelocity = t.PriceVelocity
			cr.ListingVelocity = t.ListingVelocity
			cr.CV = t.CV
			cr.HistPosition = t.HistPosition
			cr.WindowSignal = t.WindowSignal
			cr.AdvancedSignal = t.AdvancedSignal
			cr.LiquidityTier = t.LiquidityTier
			cr.PriceTier = t.PriceTier
			cr.TierAction = t.TierAction
			cr.SellUrgency = t.SellUrgency
			cr.SellReason = t.SellReason
			cr.Sellability = t.Sellability
			cr.SellabilityLabel = t.SellabilityLabel
			cr.Low7d = t.PriceLow7d
			cr.High7d = t.PriceHigh7d
			cr.SellConfidence = deriveSellConfidence(t.CurrentListings, t.CV, t.Signal)
		}

		// Exclude TRAP gems entirely — no actionable signal.
		if cr.Signal == "TRAP" {
			continue
		}

		// Weighted ROI: liquidity-based scoring with saturation penalty.
		// Default sellability to 50 (neutral) when no trend data exists,
		// so gems without signals still appear in rankings.
		sellability := cr.Sellability
		if sellability == 0 && cr.Signal == "" {
			sellability = 50
		}
		liquidityScore := float64(sellability) / 100.0
		var saturationPenalty float64
		if cr.Signal == "DUMPING" {
			saturationPenalty = 0.5
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
func BuildCompareResults(
	names []string,
	transfigure []TransfigureResult,
	trends []TrendResult,
	sparklines map[string][]SparklinePoint,
) []CompareResult {
	// Index transfigure by transfigured name + variant.
	type trKey struct{ name, variant string }
	trIndex := make(map[trKey]*TransfigureResult, len(transfigure))
	for i := range transfigure {
		t := &transfigure[i]
		trIndex[trKey{t.TransfiguredName, t.Variant}] = t
	}

	// Index trends by (name, variant).
	trendIndex := make(map[trKey]*TrendResult, len(trends))
	for i := range trends {
		t := &trends[i]
		trendIndex[trKey{t.Name, t.Variant}] = t
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
			cr.ROI = bestTr.ROI
			cr.ROIPct = bestTr.ROIPct
			cr.BasePrice = bestTr.BasePrice
			cr.TransfiguredPrice = bestTr.TransfiguredPrice
			cr.Confidence = bestTr.Confidence
			found = true
		}

		if !found {
			// Gem not found in transfigure results — include with zero values.
			cr.Confidence = "LOW"
		}

		// Join trend data.
		if t, ok := trendIndex[trKey{name, cr.Variant}]; ok {
			cr.Signal = t.Signal
			cr.CV = t.CV
			cr.PriceVelocity = t.PriceVelocity
			cr.ListingVelocity = t.ListingVelocity
			cr.HistPosition = t.HistPosition
			cr.SellUrgency = t.SellUrgency
			cr.SellReason = t.SellReason
			cr.Sellability = t.Sellability
			cr.SellabilityLabel = t.SellabilityLabel
			cr.PriceTier = t.PriceTier
			cr.TierAction = t.TierAction
			cr.WindowSignal = t.WindowSignal
			cr.BaseListings = t.BaseListings
			cr.LiquidityTier = t.LiquidityTier
			cr.TransListings = t.CurrentListings
			cr.Low7d = t.PriceLow7d
			cr.High7d = t.PriceHigh7d
			cr.SellConfidence = deriveSellConfidence(t.CurrentListings, t.CV, t.Signal)
			cr.SellConfidenceReason = deriveSellConfidenceReason(t.CurrentListings, t.CV, t.Signal)
			cr.QuickSellPrice = deriveQuickSellPrice(t.CurrentPrice, t.CurrentListings, t.CV)
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
				sell = 50 // default if no trend data
			}
			score := cr.ROI * w * (sell / 100)
			ranks[i] = ranked{idx: i, score: score}
			results[i].WeightedROI = score
		}
		sort.Slice(ranks, func(i, j int) bool {
			return ranks[i].score > ranks[j].score
		})

		for pos, r := range ranks {
			cr := results[r.idx]
			if cr.Signal == "TRAP" || cr.Signal == "DUMPING" || cr.SellUrgency == "SELL_NOW" {
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

// deriveSellConfidence returns SAFE/FAIR/RISKY based on market conditions.
// SAFE = liquid + stable, FAIR = moderate risk, RISKY = thin/volatile.
func deriveSellConfidence(listings int, cv float64, signal string) string {
	if signal == "TRAP" || signal == "DUMPING" {
		return "RISKY"
	}
	if listings >= 15 && cv < 30 {
		return "SAFE"
	}
	if listings >= 5 && cv < 60 {
		return "FAIR"
	}
	return "RISKY"
}

// deriveSellConfidenceReason returns a human-readable explanation of sell confidence.
func deriveSellConfidenceReason(listings int, cv float64, signal string) string {
	if signal == "TRAP" {
		return "extreme volatility — price unpredictable"
	}
	if signal == "DUMPING" {
		return "price dropping — sellers undercutting"
	}
	conf := deriveSellConfidence(listings, cv, signal)
	switch conf {
	case "SAFE":
		return fmt.Sprintf("%d listings — liquid, stable", listings)
	case "FAIR":
		return fmt.Sprintf("%d listings — moderate liquidity", listings)
	default:
		if listings < 5 {
			return fmt.Sprintf("%d listings — thin market", listings)
		}
		return fmt.Sprintf("CV %.0f%% — volatile pricing", cv)
	}
}

// deriveQuickSellPrice estimates an aggressive undercut price.
// Thin markets need larger undercuts; stable markets need minimal.
func deriveQuickSellPrice(currentPrice float64, listings int, cv float64) float64 {
	discount := 0.05 // 5% default undercut
	if listings < 5 {
		discount = 0.15
	} else if listings < 15 {
		discount = 0.10
	}
	if cv > 50 {
		discount += 0.05
	}
	return currentPrice * (1.0 - discount)
}
