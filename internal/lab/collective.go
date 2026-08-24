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
	Signal           string  `json:"signal"`
	PriceVelocity    float64 `json:"priceVelocity"`
	ListingVelocity  float64 `json:"listingVelocity"`
	CV               float64 `json:"cv"`
	HistPosition     float64 `json:"histPosition"`
	WindowSignal     string  `json:"windowSignal"`
	AdvancedSignal   string  `json:"advancedSignal"`
	LiquidityTier    string  `json:"liquidityTier"`
	PriceTier        string  `json:"priceTier"`
	TierAction       string  `json:"tierAction"`
	SellUrgency      string  `json:"sellUrgency"`
	SellReason       string  `json:"sellReason"`
	Sellability      int     `json:"sellability"`
	SellabilityLabel string  `json:"sellabilityLabel"`
	// From features/signals (risk-adjusted display)
	Low7Days            float64 `json:"low7d"`
	High7Days           float64 `json:"high7d"`
	SellConfidence      string  `json:"sellConfidence"`
	TradeConfidenceNote string  `json:"tradeConfidenceNote,omitempty"`
	LowConfidence       bool    `json:"lowConfidence,omitempty"`
}

// CompareResult is a side-by-side gem comparison entry with sparkline data.
type CompareResult struct {
	TransfiguredName  string           `json:"transfiguredName"`
	BaseName          string           `json:"baseName"`
	Variant           string           `json:"variant"`
	GemColor          string           `json:"gemColor"`
	ROI               float64          `json:"roi"`
	ROIPct            float64          `json:"roiPct"`
	BasePrice         float64          `json:"basePrice"`
	TransfiguredPrice float64          `json:"transfiguredPrice"`
	Confidence        string           `json:"confidence"`
	Signal            string           `json:"signal"`
	CV                float64          `json:"cv"`
	PriceVelocity     float64          `json:"priceVelocity"`
	ListingVelocity   float64          `json:"listingVelocity"`
	HistPosition      float64          `json:"histPosition"`
	Sparkline         []SparklinePoint `json:"sparkline"`
	Recommendation    string           `json:"recommendation"`
	SellUrgency       string           `json:"sellUrgency"`
	SellReason        string           `json:"sellReason"`
	Sellability       int              `json:"sellability"`
	SellabilityLabel  string           `json:"sellabilityLabel"`
	PriceTier         string           `json:"priceTier"`
	TierAction        string           `json:"tierAction"`
	WindowSignal      string           `json:"windowSignal"`
	BaseListings      int              `json:"baseListings"`
	LiquidityTier     string           `json:"liquidityTier"`
	TransListings     int              `json:"transListings"`
	// Risk-adjusted display fields (from features/signals)
	WeightedROI          float64 `json:"weightedRoi"`
	WeightedROIPct       float64 `json:"weightedRoiPct"`
	Low7Days             float64 `json:"low7d"`
	High7Days            float64 `json:"high7d"`
	SellConfidence       string  `json:"sellConfidence"`
	SellConfidenceReason string  `json:"sellConfidenceReason"`
	QuickSellPrice       float64 `json:"quickSellPrice"`
	RiskAdjustedPrice    float64 `json:"riskAdjustedPrice"`
	// Double-corruption (POE-125). Present on every candidate the calculator
	// priced, so the UI can flag "weak as this variant, strong double-corrupt
	// candidate" whether or not the tiebreaker fired. DoubleCorruptModel carries
	// the estimated-model marker: these odds come from community documentation,
	// not from GGG, and no surface may present them as confirmed.
	DoubleCorruptEV     float64 `json:"doubleCorruptEv,omitempty"`
	DoubleCorruptProfit float64 `json:"doubleCorruptProfit,omitempty"`
	DoubleCorruptModel  string  `json:"doubleCorruptModel,omitempty"`
	// DoubleCorruptPricedProbability is the share of the outcome distribution
	// the corrupted market actually prices — the rest contributes 0 to the EV.
	// It travels with the EV because roughly a fifth of the mass is
	// structurally unpriceable (poe.ninja publishes no corrupted variant below
	// level 20, and the quality reroll lands there often), so an EV shown alone
	// reads as a full expectation when it is a floor over the priced share.
	DoubleCorruptPricedProbability float64 `json:"doubleCorruptPricedProbability,omitempty"`
	// DoubleCorruptTiebreak marks the one candidate this recommendation was
	// decided by double-corrupt profit rather than by the Font score. It is a
	// separate field rather than a Recommendation value so the badge the UI
	// already renders keeps working, and never reads as an ordinary win.
	DoubleCorruptTiebreak bool `json:"doubleCorruptTiebreak,omitempty"`
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
		noBase := tr.Confidence == ConfidenceNoBase

		// Only include profitable, confident results — except NO_BASE gems, whose
		// ROI is unknown rather than zero. Those are kept on their own price alone
		// so a gem the market prices stays visible (the Font EV analyzer already
		// shows it), with the missing base surfaced via Confidence.
		if noBase {
			if tr.TransfiguredPrice <= 0 {
				continue
			}
		} else if tr.ROI <= 0 {
			continue
		}

		// Budget filter on base price. A NO_BASE gem has no known cost basis, so it
		// cannot be shown to fit a budget — drop it whenever a budget is in play.
		if budget > 0 && (noBase || tr.BasePrice > budget) {
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

		// A thin transfigure market marks the row low-confidence rather than
		// removing it. AnalyzeTransfigure gates Confidence on an absolute
		// listings >= 5 on both sides, which is a mature-market assumption: at
		// 20/20 the transfigured side averages 2 listings, so dropping LOW rows
		// hid 78 of 93 positive-ROI gems from the ranking entirely.
		//
		// Set after the feature join, which assigns LowConfidence rather than
		// OR-ing it — the join would otherwise clobber this.
		if tr.Confidence == "LOW" {
			cr.LowConfidence = true
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

	// Sort by chosen metric descending. Ties break on transfigured price so that
	// NO_BASE gems — all of which score 0 on any ROI metric — still rank by value
	// among themselves instead of in map order.
	if sortBy == SortPct {
		sort.Slice(results, func(i, j int) bool {
			if results[i].WeightedROIPct != results[j].WeightedROIPct {
				return results[i].WeightedROIPct > results[j].WeightedROIPct
			}
			return results[i].TransfiguredPrice > results[j].TransfiguredPrice
		})
	} else {
		sort.Slice(results, func(i, j int) bool {
			if results[i].WeightedROI != results[j].WeightedROI {
				return results[i].WeightedROI > results[j].WeightedROI
			}
			return results[i].TransfiguredPrice > results[j].TransfiguredPrice
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
//
// doubleCorrupt is the double-corruption result for each compared gem, keyed by
// name and already narrowed to the requested variant as an INPUT variant (see
// SelectDoubleCorruptByNames). It may be nil — the calculator only models the
// input variants in DoubleCorruptVariants, and every other request gets the
// pre-POE-125 behaviour unchanged. Its role is the tiebreaker below.
func BuildCompareResults(
	names []string,
	transfigure []TransfigureResult,
	signals []GemSignal,
	features []GemFeature,
	sparklines map[string][]SparklinePoint,
	requestedVariant string,
	doubleCorrupt map[string]DoubleCorruptResult,
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
		// NO_BASE rows carry BasePrice 0 (unknown, not free) — letting one seed the
		// map would make every gem of that color/variant look like a free base.
		if t.BasePrice <= 0 {
			continue
		}
		key := colorVariantKey{t.GemColor, t.Variant}
		if existing, ok := cheapestBase[key]; !ok || t.BasePrice < existing {
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
			if k.name != name {
				continue
			}
			// A NO_BASE row's ROI is 0 meaning "unknown", so it must never win this
			// comparison against a row with a real (possibly negative) ROI — that
			// would pick the variant with no data over the one that has it.
			if bestTr == nil {
				bestTr = tr
				continue
			}
			bestIsNoBase := bestTr.Confidence == ConfidenceNoBase
			trIsNoBase := tr.Confidence == ConfidenceNoBase
			if bestIsNoBase != trIsNoBase {
				if bestIsNoBase {
					bestTr = tr
				}
				continue
			}
			if tr.ROI > bestTr.ROI {
				bestTr = tr
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
				// The lab cost basis is any base gem of this colour, not this gem's own
				// base — so a NO_BASE row that gets one is no longer missing its cost
				// basis. Keeping the label here would tell the UI to hide real numbers.
				if cr.Confidence == ConfidenceNoBase {
					cr.Confidence = "LOW"
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
			// back to a different one. NO_DATA (not LOW) so the UI can render
			// those zeros as "unknown" rather than as a 0c price; OCR can now
			// recognise gems the market has not priced at all.
			cr.Confidence = ConfidenceNoData
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

		// Join the double-corruption EV. Informational on every candidate: a gem
		// that is weak at this variant but strong double-corrupted is the case
		// this feature exists for, and the UI wants to say so whether or not the
		// tiebreaker below fires.
		// EV and Profit are both risk-adjusted at source (AnalyzeDoubleCorrupt):
		// they are two readings of one number, so the card cannot show a profit
		// larger than the estimate it is derived from.
		if dc, ok := doubleCorrupt[name]; ok {
			cr.DoubleCorruptEV = dc.EV
			cr.DoubleCorruptProfit = dc.Profit
			cr.DoubleCorruptModel = dc.Model
			cr.DoubleCorruptPricedProbability = dc.PricedProbability
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

		applyDoubleCorruptTiebreak(results)
	}

	return results
}

// applyDoubleCorruptTiebreak decides the Font pick by double-corruption profit
// when the score-based pass decided nothing (POE-125).
//
// The gap it fills: the pass above only promotes the top-RANKED candidate, and
// promotes it only if that candidate cleared every disqualifier. When the
// top-ranked gem is disqualified into AVOID, no candidate is promoted at all and
// the player is left with a comparison that names no winner — even though one of
// the survivors may be a strong double-corrupt candidate precisely because it is
// weak at this variant.
//
// Three rules keep it narrow:
//
//   - It fires ONLY when the pass produced no BEST. A comparison that already
//     named a winner is not a tie, and this must never overturn one.
//   - Only a non-AVOID candidate can be promoted. The disqualifiers above are
//     about the gem's own market, not about the craft, and they still bind.
//   - Only a positive double-corrupt profit qualifies. Profit is the
//     risk-adjusted EV minus the gem's own price at this variant, so a positive
//     number is exactly the claim "the corrupted market pays more for this gem,
//     at the rate it actually clears, than selling it here does" — the
//     opportunity cost the player is choosing between. A negative one is a
//     reason not to corrupt, never a reason to pick a gem. Measuring it off
//     EVRaw instead would promote gems whose corrupted upside sits in cells
//     nobody buys.
//
// The winner is tagged with DoubleCorruptTiebreak as well as BEST, so the reason
// travels with the recommendation instead of disappearing into a shared enum
// value the UI already renders as an ordinary win.
//
// This lives in the Font compare path only, deliberately. Dedication feeds an
// already-corrupted gem in for another corrupted gem, and a corrupted gem cannot
// be double-corrupted at all — so BuildDedicationCompareResults has no tie of
// this kind to break.
func applyDoubleCorruptTiebreak(results []CompareResult) {
	for _, cr := range results {
		if cr.Recommendation == "BEST" {
			return
		}
	}

	winner := -1
	for i, cr := range results {
		if cr.Recommendation == "AVOID" || cr.DoubleCorruptProfit <= 0 {
			continue
		}
		if winner < 0 {
			winner = i
			continue
		}
		best := results[winner]
		if cr.DoubleCorruptProfit > best.DoubleCorruptProfit ||
			(cr.DoubleCorruptProfit == best.DoubleCorruptProfit &&
				cr.TransfiguredName < best.TransfiguredName) {
			winner = i
		}
	}
	if winner < 0 {
		return
	}

	results[winner].Recommendation = "BEST"
	results[winner].DoubleCorruptTiebreak = true
}

// BuildDedicationCompareResults builds compare results for Dedication lab mode.
// Each gem is scored against the corrupted 21/23c pool. The pool type (skill vs
// transfigured) is auto-detected from the gem name: names containing " of " are
// transfigured, others are non-transfigured skills.
// dedicationResults provides per-color input costs and pool context.
// RankDedicationCollective returns the corrupted gems of one Dedication variant
// ranked by price for the Dedication lab rankings table. Maps gems into
// CollectiveResult shape so the existing ByVariant/BestPlays frontend
// components can render them. dedicationResults must be the same variant's —
// its input costs are what each gem's ROI is measured against.
//
// It is the cold-cache path only. The tick pre-computes the same list unfiltered
// (limit 0, no search) and the request narrows it with FilterDedicationRankings;
// this composes the two so the served and pre-computed orderings cannot drift.
func RankDedicationCollective(
	gems []GemPrice,
	dedicationResults []DedicationResult,
	limit int,
	searchName string,
	variant string,
) []CollectiveResult {
	return FilterDedicationRankings(
		rankDedicationCollectiveAll(gems, dedicationResults, variant),
		searchName, limit,
	)
}

// FilterDedicationRankings narrows an already-ranked Dedication list by gem-name
// search and then by limit — in that order, so a search returns its top matches
// rather than the matches within the top N.
//
// It runs entirely in memory over a list the tick built, and never mutates it:
// the search allocates a new slice and the limit only reslices, so the cached
// ranking is left exactly as stored.
func FilterDedicationRankings(ranked []CollectiveResult, searchName string, limit int) []CollectiveResult {
	if searchName != "" {
		q := strings.ToLower(searchName)
		matched := make([]CollectiveResult, 0, len(ranked))
		for _, cr := range ranked {
			if strings.Contains(strings.ToLower(cr.TransfiguredName), q) {
				matched = append(matched, cr)
			}
		}
		ranked = matched
	}
	if limit > 0 && len(ranked) > limit {
		ranked = ranked[:limit]
	}
	return ranked
}

// SelectGemPricesByNames returns the entries of gems whose name is in names,
// preserving the order of gems. It is the in-memory equivalent of
// Repository.CorruptedGemPricesByNames' `name = ANY($2)` over a corpus the cache
// already holds narrowed to one variant.
func SelectGemPricesByNames(gems []GemPrice, names []string) []GemPrice {
	if len(gems) == 0 || len(names) == 0 {
		return nil
	}
	wanted := make(map[string]struct{}, len(names))
	for _, n := range names {
		wanted[n] = struct{}{}
	}
	out := make([]GemPrice, 0, len(names))
	for _, g := range gems {
		if _, ok := wanted[g.Name]; ok {
			out = append(out, g)
		}
	}
	return out
}

// rankDedicationCollectiveAll ranks every gem of one variant, unfiltered.
func rankDedicationCollectiveAll(
	gems []GemPrice,
	dedicationResults []DedicationResult,
	variant string,
) []CollectiveResult {
	// Index Dedication results by (color, gemType) for input cost + tier lookup.
	type dedKey struct{ color, gemType string }
	inputCosts := make(map[dedKey]float64)
	for _, dr := range dedicationResults {
		k := dedKey{dr.Color, dr.GemType}
		if _, exists := inputCosts[k]; !exists {
			inputCosts[k] = dr.InputCost
		}
	}

	// Filter to this variant's corrupted gems (no supports, no Trarthus).
	var pool []GemPrice
	for _, g := range gems {
		if !isDedicationOutcome(g) {
			continue
		}
		if g.Variant != variant {
			continue
		}
		pool = append(pool, g)
	}

	// Sort by price descending.
	sort.Slice(pool, func(i, j int) bool {
		return pool[i].Chaos > pool[j].Chaos
	})

	results := make([]CollectiveResult, 0, len(pool))
	for _, g := range pool {
		gemType := "skill"
		if g.IsTransfigured {
			gemType = "transfigured"
		}

		inputCost := inputCosts[dedKey{g.GemColor, gemType}]
		roi := g.Chaos - inputCost

		// No input cost for this (color, pool) means the analysis has nothing for
		// the market being ranked — not that the gems are free. Reporting
		// `roi = full listed price` there is a fabricated number that reads
		// exactly like a real one, so the row is marked no-base instead and the
		// ROI columns are left at zero. ConfidenceNoBase is what BestPlays
		// already renders as "—".
		if inputCost <= 0 {
			results = append(results, CollectiveResult{
				TransfiguredName:     g.Name,
				BaseName:             gemType,
				Variant:              strings.TrimSuffix(variant, "c"),
				GemColor:             g.GemColor,
				TransfiguredPrice:    g.Chaos,
				TransfiguredListings: g.Listings,
				Confidence:           ConfidenceNoBase,
				LowConfidence:        true,
			})
			continue
		}

		cr := CollectiveResult{
			TransfiguredName:     g.Name,
			BaseName:             gemType,
			Variant:              strings.TrimSuffix(variant, "c"),
			GemColor:             g.GemColor,
			TransfiguredPrice:    g.Chaos,
			TransfiguredListings: g.Listings,
			BasePrice:            inputCost,
			ROI:                  roi,
		}
		if inputCost > 0 {
			cr.ROIPct = (roi / inputCost) * 100
		}

		// Confidence based on listings.
		switch {
		case g.Listings >= 5:
			cr.Confidence = "OK"
		case g.Listings >= 2:
			cr.SellConfidence = "FAIR"
			cr.Confidence = "LOW"
		default:
			cr.SellConfidence = "RISKY"
			cr.Confidence = "LOW"
			cr.LowConfidence = true
		}

		results = append(results, cr)
	}

	return results
}

func BuildDedicationCompareResults(
	names []string,
	gemPrices []GemPrice,
	dedicationResults []DedicationResult,
	sparklines map[string][]SparklinePoint,
	variant string,
) []CompareResult {
	// Index gem prices by name. Within one corrupted variant there should be at
	// most one entry per name, but if duplicates exist we keep the last (highest chaos).
	// Only this variant's prices are indexed. Every row is labelled with the
	// caller's variant, so admitting a price from another one would put a
	// 21/23c number under a 21/20c label with nothing left to detect it — the
	// query that feeds this already filters, and this keeps the function honest
	// on its own terms rather than by its caller's discipline.
	priceIndex := make(map[string]*GemPrice, len(gemPrices))
	for i := range gemPrices {
		g := &gemPrices[i]
		if g.Variant != variant {
			continue
		}
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
	// Rows the font cannot produce. They are still returned — the caller asked
	// for these names — but they carry no font ROI and cannot be recommended.
	notAnOutcome := make(map[int]bool)

	for _, name := range names {
		cr := CompareResult{
			TransfiguredName: name,
			Variant:          variant,
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

			// ROI here means "what the font returns, less what feeding it costs".
			// For a gem the craft can never hand out — a Vaal gem above all —
			// that subtraction describes a trade nobody can make, and the
			// ranking below would happily call it the BEST play.
			if !isDedicationOutcome(*g) {
				notAnOutcome[len(results)] = true
				cr.ROI = 0
				cr.ROIPct = 0
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
			if notAnOutcome[r.idx] {
				results[r.idx].Recommendation = "AVOID"
			} else if cr.Confidence == "LOW" && cr.TransListings < 2 {
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
