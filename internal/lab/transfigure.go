// Package lab implements the analysis engine for lab farming profitability.
// Each analyzer reads raw snapshot data, computes metrics, and stores results.
package lab

import (
	"strings"
	"time"
)

// TransfigureResult holds the computed ROI for a single base→transfigured gem pair.
type TransfigureResult struct {
	Time                 time.Time
	BaseName             string
	TransfiguredName     string
	Variant              string
	BasePrice            float64
	TransfiguredPrice    float64
	ROI                  float64
	ROIPct               float64
	BaseListings         int
	TransfiguredListings int
	GemColor             string
	Confidence           string // "OK", "LOW", or ConfidenceNoBase
}

// ConfidenceNoBase marks a transfigured gem the market prices but whose base gem
// is absent from the snapshot, so ROI cannot be computed. Such rows carry
// BasePrice/ROI/ROIPct of 0 — that is "unknown", not "breaks even". They exist so
// a gem the Font EV analyzer can already price does not vanish from the rankings
// (common right after a league start, when poe.ninja lists transfigured gems
// before their bases).
const ConfidenceNoBase = "NO_BASE"

// ConfidenceNoData marks a comparison entry for a gem with no transfigure data at
// all — neither side of the pair is priced. Its zeros are "unknown", not "worthless".
const ConfidenceNoData = "NO_DATA"

// GemPrice is the minimal price data needed for analysis. Shared across analyzers.
type GemPrice struct {
	Name           string
	Variant        string
	Chaos          float64
	Listings       int
	IsTransfigured bool
	IsCorrupted    bool
	GemColor       string
}

// AnalyzeTransfigure computes transfigure ROI for all base→transfigured pairs.
// Input: the latest snapshot of gem prices (non-corrupted only).
// Variants analyzed: "1", "1/20", "20", "20/20".
func AnalyzeTransfigure(snapTime time.Time, gems []GemPrice) []TransfigureResult {
	// Build maps: baseGems[name][variant] and transfiguredGems[name][variant]
	type priceEntry struct {
		chaos    float64
		listings int
		color    string
	}
	baseGems := make(map[string]map[string]priceEntry)
	transGems := make(map[string]map[string]priceEntry)

	for _, g := range gems {
		if !isTransfigurePairCandidate(g) {
			continue
		}

		target := baseGems
		if g.IsTransfigured {
			target = transGems
		}

		if target[g.Name] == nil {
			target[g.Name] = make(map[string]priceEntry)
		}
		target[g.Name][g.Variant] = priceEntry{
			chaos:    g.Chaos,
			listings: g.Listings,
			color:    g.GemColor,
		}
	}

	variants := []string{"1", "1/20", "20", "20/20"}
	var results []TransfigureResult

	for transName, transVariants := range transGems {
		baseName := extractBaseName(transName)
		baseVariants := baseGems[baseName] // nil when the base gem is unpriced

		for _, variant := range variants {
			trans, hasTransVar := transVariants[variant]
			if !hasTransVar {
				continue
			}

			base, hasBaseVar := baseVariants[variant]
			if !hasBaseVar {
				// No base price: emit the gem with ROI unknown rather than
				// dropping it. See ConfidenceNoBase.
				results = append(results, TransfigureResult{
					Time:                 snapTime,
					BaseName:             baseName,
					TransfiguredName:     transName,
					Variant:              variant,
					TransfiguredPrice:    trans.chaos,
					TransfiguredListings: trans.listings,
					GemColor:             trans.color,
					Confidence:           ConfidenceNoBase,
				})
				continue
			}

			roi := trans.chaos - base.chaos
			var roiPct float64
			if base.chaos > 0 {
				roiPct = (roi / base.chaos) * 100
			}

			confidence := "LOW"
			if trans.listings >= 5 && base.listings >= 5 {
				confidence = "OK"
			}

			results = append(results, TransfigureResult{
				Time:                 snapTime,
				BaseName:             baseName,
				TransfiguredName:     transName,
				Variant:              variant,
				BasePrice:            base.chaos,
				TransfiguredPrice:    trans.chaos,
				ROI:                  roi,
				ROIPct:               roiPct,
				BaseListings:         base.listings,
				TransfiguredListings: trans.listings,
				GemColor:             trans.color,
				Confidence:           confidence,
			})
		}
	}

	return results
}

// extractBaseName derives the base gem name from a transfigured name.
// "Rain of Arrows of Saturation" → "Rain of Arrows"
// "Spark of Nova" → "Spark"
// "Vaal Spark of Nova" → "Vaal Spark"
// ExtractBaseName is the exported variant of extractBaseName.
func ExtractBaseName(transfiguredName string) string {
	return extractBaseName(transfiguredName)
}

func extractBaseName(transfiguredName string) string {
	idx := strings.LastIndex(transfiguredName, " of ")
	if idx < 0 {
		return transfiguredName
	}
	return transfiguredName[:idx]
}
