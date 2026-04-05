package lab

import (
	"time"
)

// ComputeGemSignals produces signal classifications with confidence scores for
// each GemFeature. It combines v1 signal classifiers (from trends.go) with the
// v2 confidence scoring engine (from confidence.go).
//
// Parameters:
//   - snapTime: the snapshot timestamp
//   - features: pre-computed per-gem feature vectors from ComputeGemFeatures
//   - mc: market context for confidence scoring and tier boundaries
//   - gems: raw gem prices used to build baseCurrentListings map
//   - baseHistory: base gem price history keyed by base gem name
//   - marketAvgBaseLst: market-wide average base listings for relative liquidity
func ComputeGemSignals(
	snapTime time.Time,
	features []GemFeature,
	mc MarketContext,
	gems []GemPrice,
	baseHistory map[string][]PricePoint,
	marketAvgBaseLst float64,
) []GemSignal {
	if len(features) == 0 {
		return nil
	}

	// Build baseCurrentListings from gems slice: non-transfigured, non-corrupted,
	// keeping highest listings per base name (follows trends.go base listing pattern).
	baseCurrentListings := make(map[string]int)
	for _, g := range gems {
		if g.IsCorrupted || g.IsTransfigured {
			continue
		}
		if g.Listings > baseCurrentListings[g.Name] {
			baseCurrentListings[g.Name] = g.Listings
		}
	}

	signals := make([]GemSignal, 0, len(features))

	for _, f := range features {
		// 1. Primary signal from v1 classifier.
		// Uses 6h velocity (VelLong) — 2h was too sensitive to temporal normalization noise.
		// CVShort (6h) for TRAP — more meaningful than 7-day CV for detecting current instability.
		signal := classifySignal(f.VelLongPrice, f.VelLongListing, f.CVShort, f.Chaos, f.Listings)

		// 2. Advanced signal from v1 classifier.
		advSignal := classifyAdvancedSignal(f.Chaos, f.Listings, f.VelLongPrice, f.VelLongListing, f.CV, f.HistPosition)

		// CASCADE: buyout aftermath — price spiked and crashed back.
		// Detected by extreme CV (>200%) combined with high spike ratio (high7d/low7d > 20x).
		// The old depth-based rule missed cascades once listings recovered.
		// PRICE_MANIPULATION keeps priority (active manipulation vs aftermath).
		if advSignal != "PRICE_MANIPULATION" {
			if f.CV > 200 && f.Low7Days > 0 && f.High7Days/f.Low7Days > 20 {
				advSignal = "CASCADE"
			}
		}

		// 3. Sellability from v1 classifier.
		sellScore, _ := sellability(f.Listings, f.VelLongListing, f.VelLongPrice, f.CV, signal, f.MarketDepth, f.Chaos)

		// 4. Base-dependent signals.
		baseName := extractBaseName(f.Name)

		baseLst := -1 // sentinel: base not found
		if v, ok := baseCurrentListings[baseName]; ok {
			baseLst = v
		}

		var baseVel float64
		if bp, ok := baseHistory[baseName]; ok && len(bp) >= 2 {
			baseVel = velocity(bp, func(p PricePoint) float64 { return float64(p.Listings) })
		}

		// Sell urgency using base-side data (6h velocity for consistency).
		sUrgency, sReason := sellUrgency(f.VelLongPrice, f.VelLongListing, baseVel, f.HistPosition, baseLst, f.Listings, signal, f.Tier)

		// Window signal using base-side data (6h velocity for consistency).
		baseLstForCalc := float64(baseLst)
		if baseLst < 0 {
			baseLstForCalc = 0
		}
		relLiq := computeRelativeLiquidity(baseLstForCalc, marketAvgBaseLst)
		winScore := computeWindowScore(f.Chaos, baseVel, float64(f.Listings), relLiq)
		winSignal := classifyWindowSignal(winScore, baseVel, f.VelLongListing, baseLst, f.VelLongPrice)

		// 5. Confidence scoring.
		confidence, phaseMod := computeConfidence(signal, f, mc, snapTime)

		// 6. Recommendation (follows collective.go:311 pattern).
		recommendation := computeRecommendation(signal, sUrgency, confidence)

		// 7. Trade-adjusted sellability.
		adjSellScore, adjSellLabel := adjustSellabilityForTrade(sellScore, f)

		// 8. Risk-adjusted scoring (POE-69).
		// Use trade price floor for quick-sell when fresh trade data is available
		// and the listing is not a price outlier, with at least 3 total listings.
		basePrice := f.Chaos
		if f.TradeDataAvailable && f.TradeDataAge < 300 && !f.TradePriceOutlier && f.TradePriceFloor > 0 && f.Listings >= 3 {
			basePrice = f.TradePriceFloor
		}

		// Use trade median for risk-adjusted value when fresh, else ninja price.
		ravPrice := f.Chaos
		if f.TradeDataAvailable && f.TradeDataAge < 1800 && f.TradeMedianTop10 > 0 {
			ravPrice = f.TradeMedianTop10
		}

		riskAdjValue := ravPrice * f.SellProbabilityFactor * f.StabilityDiscount
		undercutFactor := quickSellUndercutFactor(f.Listings, f.Tier, signal)
		quickSell := basePrice * (1.0 - undercutFactor)
		sellConf, tradeNote := classifySellConfidence(f.SellProbabilityFactor, f.StabilityDiscount, f)

		signals = append(signals, GemSignal{
			Time:                snapTime,
			Name:                f.Name,
			Variant:             f.Variant,
			Signal:              signal,
			Confidence:          confidence,
			SellUrgency:         sUrgency,
			SellReason:          sReason,
			Sellability:         adjSellScore,
			SellabilityLabel:    adjSellLabel,
			WindowSignal:        winSignal,
			AdvancedSignal:      advSignal,
			PhaseModifier:       phaseMod,
			Recommendation:      recommendation,
			Tier:                f.Tier,
			RiskAdjustedValue:   riskAdjValue,
			QuickSellPrice:      quickSell,
			SellConfidence:      sellConf,
			TradeConfidenceNote: tradeNote,
		})
	}

	return signals
}

// computeRecommendation determines the actionable recommendation for a gem signal.
// Priority: AVOID for dangerous signals, OK for high-confidence positive signals.
// DUMPING is gated on confidence — low-confidence DUMPING on liquid markets is noise.
func computeRecommendation(signal, sellUrgency string, confidence int) string {
	// SELL_NOW gated on market thinness. CAUTION is informational — no auto-AVOID.
	if sellUrgency == "SELL_NOW" {
		return "AVOID"
	}

	// DUMPING only AVOID when confidence backs it up.
	// Low confidence = 2h noise on liquid market, don't override.
	if signal == "DUMPING" && confidence >= 50 {
		return "AVOID"
	}

	// High-confidence positive signals produce OK.
	if confidence >= 65 && (signal == "HERD" || signal == "RECOVERY") {
		return "OK"
	}

	return ""
}

// quickSellUndercutFactor returns the undercut percentage for quick-sell pricing.
// Data-driven: 30+ listings need MORE undercut (competitive pressure), <5 need LESS
// (prices rarely move). Signal modifier adjusts for active market conditions.
func quickSellUndercutFactor(listings int, tier, signal string) float64 {
	// Listing-based undercut (flipped from original — backed by 63K observation backtest).
	var base float64
	switch {
	case listings >= 30:
		base = 0.09 // competitive market — 5% was too little (71.6% achievable → ~86% at 9%)
	case listings >= 10:
		base = 0.10 // well-calibrated (88.5% achievable)
	case listings >= 5:
		base = 0.11 // was 15%, too aggressive for thin-but-stable markets
	default:
		base = 0.15 // was 25%, overkill — prices rarely move with <5 listings
	}
	// Tier modifier: premium gems face stiffer competition.
	switch tier {
	case "TOP":
		base += 0.05
	case "HIGH":
		base += 0.02
	}
	// Signal modifier: DUMPING needs deeper undercut, STABLE can be gentler.
	switch signal {
	case "DUMPING":
		base += 0.05 // 65% achievable without this — needs aggressive pricing
	case "CAUTION":
		base += 0.03
	case "STABLE":
		base -= 0.03 // 93% achievable — can afford to be less aggressive
	}
	if base < 0.02 {
		base = 0.02 // minimum 2% undercut
	}
	return base
}

// classifySellConfidence returns SAFE, FAIR, or RISKY based on the
// sell probability factor, stability discount, and trade signals.
// SAFE: both factors well above typical (top quartile).
// RISKY: at least one factor is poor.
// FAIR: everything in between.
//
// Non-trade override (always applied):
//   - CASCADE regime: always RISKY (listing-depth-based, not trade-data-based)
//
// Trade overrides (applied when TradeDataAvailable && TradeDataAge < 5400):
//   - MONOPOLY: SAFE→FAIR, FAIR→RISKY (hard override)
//   - PriceOutlier + MONOPOLY: always RISKY
//   - STALE cheapest: SAFE→FAIR
//   - FRESH cheapest on borderline RISKY: promote to FAIR if sellProb >= 0.5 && stabilityDisc >= 0.7
func classifySellConfidence(sellProb, stabilityDisc float64, f GemFeature) (string, string) {
	// Base classification.
	var base string
	if sellProb >= 0.8 && stabilityDisc >= 0.85 {
		base = "SAFE"
	} else if sellProb < 0.5 && stabilityDisc < 0.8 {
		base = "RISKY"
	} else {
		base = "FAIR"
	}

	// CASCADE regime: always RISKY regardless of trade data availability.
	// MarketDepth is listing-depth-based, not trade-data-based.
	if f.MarketRegime == "CASCADE" {
		return "RISKY", "CASCADE regime: always RISKY"
	}

	// No trade data or stale (>90min) — return base.
	if !f.TradeDataAvailable || f.TradeDataAge >= 5400 {
		return base, ""
	}

	result := base
	var note string

	// MONOPOLY hard overrides.
	if f.TradeSellerConcentration == "MONOPOLY" {
		if f.TradePriceOutlier {
			return "RISKY", "MONOPOLY + price outlier: always RISKY"
		}
		switch result {
		case "SAFE":
			result = "FAIR"
			note = "MONOPOLY: downgraded from SAFE to FAIR"
		case "FAIR":
			result = "RISKY"
			note = "MONOPOLY: downgraded from FAIR to RISKY"
		}
		return result, note
	}

	// STALE cheapest: SAFE→FAIR.
	if f.TradeCheapestStaleness == "STALE" && result == "SAFE" {
		result = "FAIR"
		note = "STALE cheapest listing: downgraded from SAFE to FAIR"
	}

	// FRESH cheapest on borderline RISKY: promote if sell probability AND stability are decent.
	// Requires sellProb >= 0.5 to confirm price pull, AND stabilityDisc >= 0.7 to exclude
	// deeply volatile gems (the minimum stabilityDiscount floor from stabilityDiscount()).
	if f.TradeCheapestStaleness == "FRESH" && result == "RISKY" && sellProb >= 0.5 && stabilityDisc >= 0.7 {
		result = "FAIR"
		note = "FRESH cheapest listing + decent sell probability: promoted from RISKY to FAIR"
	}

	return result, note
}

// adjustSellabilityForTrade applies trade signal adjustments to the base sellability score.
// Only applied when TradeDataAvailable && TradeDataAge < 5400 (90min).
// Adjustments: MONOPOLY -20, CONCENTRATED -10, STALE -10, FRESH +5.
// Result is clamped to [0, 100]. Always returns a valid label.
func adjustSellabilityForTrade(score int, f GemFeature) (int, string) {
	if !f.TradeDataAvailable || f.TradeDataAge >= 5400 {
		return score, sellabilityLabelFor(score)
	}

	adjusted := score

	switch f.TradeSellerConcentration {
	case "MONOPOLY":
		adjusted -= 20
	case "CONCENTRATED":
		adjusted -= 10
	}

	switch f.TradeCheapestStaleness {
	case "STALE":
		adjusted -= 10
	case "FRESH":
		adjusted += 5
	}

	// Clamp [0, 100].
	if adjusted < 0 {
		adjusted = 0
	}
	if adjusted > 100 {
		adjusted = 100
	}

	return adjusted, sellabilityLabelFor(adjusted)
}

// sellabilityLabelFor maps a sellability score to its human-readable label.
// Same thresholds as sellability() in trends.go.
func sellabilityLabelFor(score int) string {
	switch {
	case score >= 80:
		return "FAST SELL"
	case score >= 60:
		return "GOOD"
	case score >= 40:
		return "MODERATE"
	case score >= 20:
		return "SLOW"
	default:
		return "UNLIKELY"
	}
}
