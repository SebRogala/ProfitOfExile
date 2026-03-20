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
		signal := classifySignal(f.VelMedPrice, f.VelMedListing, f.CV, f.Listings)

		// 2. Advanced signal from v1 classifier.
		advSignal := classifyAdvancedSignal(f.Chaos, f.Listings, f.VelMedPrice, f.VelMedListing, f.CV, f.HistPosition)

		// 3. Sellability from v1 classifier.
		sellScore, sellLabel := sellability(f.Listings, f.VelMedListing, f.VelMedPrice, f.CV, signal)

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

		// Sell urgency using base-side data.
		sUrgency, sReason := sellUrgency(f.VelMedPrice, f.VelMedListing, baseVel, f.HistPosition, baseLst, f.Listings, signal, f.Tier)

		// Window signal using base-side data.
		baseLstForCalc := float64(baseLst)
		if baseLst < 0 {
			baseLstForCalc = 0
		}
		relLiq := computeRelativeLiquidity(baseLstForCalc, marketAvgBaseLst)
		winScore := computeWindowScore(f.Chaos, baseVel, float64(f.Listings), relLiq)
		winSignal := classifyWindowSignal(winScore, baseVel, f.VelMedListing, baseLst, f.VelMedPrice)

		// 5. Confidence scoring.
		confidence, phaseMod := computeConfidence(signal, f, mc, snapTime)

		// 6. Recommendation (follows collective.go:311 pattern).
		recommendation := computeRecommendation(signal, sUrgency, confidence)

		signals = append(signals, GemSignal{
			Time:             snapTime,
			Name:             f.Name,
			Variant:          f.Variant,
			Signal:           signal,
			Confidence:       confidence,
			SellUrgency:      sUrgency,
			SellReason:       sReason,
			Sellability:      sellScore,
			SellabilityLabel: sellLabel,
			WindowSignal:     winSignal,
			AdvancedSignal:   advSignal,
			PhaseModifier:    phaseMod,
			Recommendation:   recommendation,
			Tier:             f.Tier,
		})
	}

	return signals
}

// computeRecommendation determines the actionable recommendation for a gem signal.
// Priority: AVOID for dangerous signals, OK for high-confidence positive signals.
func computeRecommendation(signal, sellUrgency string, confidence int) string {
	// Dangerous signals always produce AVOID.
	if signal == "TRAP" || signal == "DUMPING" || sellUrgency == "SELL_NOW" {
		return "AVOID"
	}

	// High-confidence positive signals produce OK.
	if confidence >= 65 && (signal == "HERD" || signal == "RECOVERY") {
		return "OK"
	}

	return ""
}
