package lab

import "sort"

// FilterTransfigure filters and limits cached transfigure results.
// Results are sorted by ROI descending (matching the DB query order).
func FilterTransfigure(all []TransfigureResult, variant string, limit int) []TransfigureResult {
	var filtered []TransfigureResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].ROI > filtered[j].ROI
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// FilterGemSignals filters and limits cached gem signals.
// Results are sorted by Confidence descending, then Sellability descending (matching the DB query order).
func FilterGemSignals(all []GemSignal, variant, tier string, limit int) []GemSignal {
	var filtered []GemSignal
	for _, s := range all {
		if variant != "" && s.Variant != variant {
			continue
		}
		if tier != "" && s.Tier != tier {
			continue
		}
		filtered = append(filtered, s)
	}

	sort.Slice(filtered, func(i, j int) bool {
		if filtered[i].Confidence != filtered[j].Confidence {
			return filtered[i].Confidence > filtered[j].Confidence
		}
		return filtered[i].Sellability > filtered[j].Sellability
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}
