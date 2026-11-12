package handlers

import "profitofexile/internal/lab"

func assembleCollectiveRows(query lab.CollectiveQueryResult) []collectiveRow {
	rows := make([]collectiveRow, 0, len(query.Results))
	for _, result := range query.Results {
		row := collectiveRow{
			TransfiguredName:     result.TransfiguredName,
			BaseName:             result.BaseName,
			Variant:              result.Variant,
			GemColor:             result.GemColor,
			ROI:                  result.ROI,
			ROIPct:               result.ROIPct,
			WeightedROI:          result.WeightedROI,
			WeightedROIPct:       result.WeightedROIPct,
			BasePrice:            result.BasePrice,
			TransfiguredPrice:    result.TransfiguredPrice,
			BaseListings:         result.BaseListings,
			TransfiguredListings: result.TransfiguredListings,
			Confidence:           result.Confidence,
			Signal:               result.Signal,
			PriceVelocity:        result.PriceVelocity,
			ListingVelocity:      result.ListingVelocity,
			CV:                   result.CV,
			HistPosition:         result.HistPosition,
			WindowSignal:         result.WindowSignal,
			AdvancedSignal:       result.AdvancedSignal,
			LiquidityTier:        result.LiquidityTier,
			PriceTier:            result.PriceTier,
			TierAction:           result.TierAction,
			SellUrgency:          result.SellUrgency,
			SellReason:           result.SellReason,
			Sellability:          result.Sellability,
			SellabilityLabel:     result.SellabilityLabel,
			Sparkline:            nonNilSparkline(query.Sparklines[lab.CollectiveSparklineKey{Name: result.TransfiguredName, Variant: result.Variant}]),
			Low7Days:             result.Low7Days,
			High7Days:            result.High7Days,
			SellConfidence:       result.SellConfidence,
			TradeConfidenceNote:  result.TradeConfidenceNote,
			LowConfidence:        result.LowConfidence,
		}

		// GCP recipe for 20/20 variants: buy 20/0 base + 20×GCP.
		if result.Variant == "20/20" {
			if base20, ok := query.BasePriceIndex[lab.CollectiveBaseKey{Name: result.BaseName, Variant: "20"}]; ok && base20 > 0 {
				recipeCost := base20 + 20*query.GCPPrice
				row.GCPRecipeCost = recipeCost
				row.GCPRecipeBase = base20
				row.GCPRecipeSaves = result.BasePrice - recipeCost
			}
		}
		rows = append(rows, row)
	}
	return rows
}
