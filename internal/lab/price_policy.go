package lab

import (
	"strings"
	"time"

	"profitofexile/internal/price/resolve"
)

const GemTradeMaxRevisitAge = 6 * time.Hour
const FontTradeVariantSet = "1,1/20,20,20/20"

// FontTradeVariants returns an independently owned interim market set.
func FontTradeVariants() []string {
	return strings.Split(FontTradeVariantSet, ",")
}

// GemPricePolicy is the price engine policy for gems: k 3×, 2× when the
// poe.ninja sample is thin by the lab's own rule (POE-310 decision 2026-10-25).
func GemPricePolicy() resolve.Policy {
	return resolve.Policy{
		ListingCount:     3,
		Disagreement:     3,
		ThinDisagreement: 2,
		ThinSample:       labThinSampleCount,
		MaxRevisitAge:    GemTradeMaxRevisitAge,
	}
}
