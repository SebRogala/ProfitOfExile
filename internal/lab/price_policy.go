package lab

import "profitofexile/internal/price/resolve"

// GemPricePolicy is the price engine policy for gems: k 3×, 2× when the
// poe.ninja sample is thin by the lab's own rule (POE-310 decision 2026-10-25).
// MaxRevisitAge stays 0 until POE-309 sets it.
func GemPricePolicy() resolve.Policy {
	return resolve.Policy{
		ListingCount:     3,
		Disagreement:     3,
		ThinDisagreement: 2,
		ThinSample:       labThinSampleCount,
	}
}
