package lab

import (
	"math"
	"time"
)

// ComputeGemFeatures produces per-gem feature vectors from raw gem data, history,
// and market context. It is a pure function with no side effects -- called from RunV2.
// Filters to transfigured, non-corrupted, non-Trarthus gems with Chaos > 5.
func ComputeGemFeatures(snapTime time.Time, gems []GemPrice, history []GemPriceHistory, mc MarketContext) []GemFeature {
	// Index history by (name, variant) for fast lookup.
	type histKey struct{ name, variant string }
	histIndex := make(map[histKey]*GemPriceHistory, len(history))
	for i := range history {
		h := &history[i]
		histIndex[histKey{h.Name, h.Variant}] = h
	}

	// Precompute market averages for relative metrics.
	var avgListings float64
	if mc.TotalGems > 0 {
		avgListings = float64(mc.TotalListings) / float64(mc.TotalGems)
	}

	p50 := mc.PriceP50()

	var features []GemFeature

	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}

		f := GemFeature{
			Time:     snapTime,
			Name:     g.Name,
			Variant:  g.Variant,
			Chaos:    g.Chaos,
			Listings: g.Listings,
			Tier:     classifyTier(g.Chaos, mc.TierBoundaries),
		}

		h := histIndex[histKey{g.Name, g.Variant}]
		if h != nil && len(h.Points) >= 2 {
			// Multi-window velocities.
			f.VelShortPrice = velocityWindow(h.Points, 1*time.Hour, extractChaos)
			f.VelShortListing = velocityWindow(h.Points, 1*time.Hour, extractListings)
			f.VelMedPrice = velocityWindow(h.Points, 2*time.Hour, extractChaos)
			f.VelMedListing = velocityWindow(h.Points, 2*time.Hour, extractListings)
			f.VelLongPrice = velocityWindow(h.Points, 6*time.Hour, extractChaos)
			f.VelLongListing = velocityWindow(h.Points, 6*time.Hour, extractListings)

			// CV from all history prices.
			prices := make([]float64, len(h.Points))
			for i, p := range h.Points {
				prices[i] = p.Chaos
			}
			f.CV = coefficientOfVariation(prices)

			// Historical position.
			f.High7d, f.Low7d, f.HistPosition = historicalPosition(g.Chaos, prices)
		} else {
			// No history: defaults.
			f.High7d = g.Chaos
			f.Low7d = g.Chaos
			f.HistPosition = 50
		}

		// Relative metrics.
		if p50 > 0 {
			f.RelativePrice = g.Chaos / p50
		}
		if avgListings > 0 {
			f.RelativeListings = float64(g.Listings) / avgListings
		}

		// Behavioral profiles: flood/crash detection and listing elasticity.
		if h != nil && len(h.Points) >= 5 {
			f.FloodCount = countFloods(h.Points)
			f.CrashCount = countCrashes(h.Points)
			f.ListingElasticity = sanitizeFloat(computeListingElasticity(h.Points))
		}
		// else: stay at zero defaults (insufficient history)

		// Risk-adjusted scoring fields.
		f.SellProbabilityFactor = sellProbabilityFactor(g.Listings, f.HistPosition, f.Low7d, g.Chaos)
		f.StabilityDiscount = stabilityDiscount(f.CV)

		// Sanitize non-velocity float fields.
		// Velocity fields are already sanitized by velocityWindow; CV, hist, and
		// relative fields are sanitized here because their producers do not guarantee it.
		f.CV = sanitizeFloat(f.CV)
		f.HistPosition = sanitizeFloat(f.HistPosition)
		f.High7d = sanitizeFloat(f.High7d)
		f.Low7d = sanitizeFloat(f.Low7d)
		f.RelativePrice = sanitizeFloat(f.RelativePrice)
		f.RelativeListings = sanitizeFloat(f.RelativeListings)
		f.SellProbabilityFactor = sanitizeFloat(f.SellProbabilityFactor)
		f.StabilityDiscount = sanitizeFloat(f.StabilityDiscount)

		features = append(features, f)
	}

	return features
}

// extractChaos extracts the chaos price from a PricePoint.
func extractChaos(p PricePoint) float64 { return p.Chaos }

// extractListings extracts the listing count from a PricePoint as float64.
func extractListings(p PricePoint) float64 { return float64(p.Listings) }

// sellProbabilityFactor returns a 0.3-1.0 factor representing how likely this gem
// is to sell within an hour, calibrated from listing count with context-aware
// thin-market adjustments.
func sellProbabilityFactor(listings int, histPosition, low7d, currentPrice float64) float64 {
	// Base: sigmoid on listing count.
	base := 0.3 + 0.7*(1.0/(1.0+math.Exp(-0.1*float64(listings-20))))

	// Context-aware thin-market adjustment.
	if listings < 10 && currentPrice > 0 {
		if low7d > 0.7*currentPrice {
			// Historically stable price with thin listings = genuine rarity.
			base = math.Min(base*1.5, 1.0)
		} else if low7d < 0.5*currentPrice {
			// Recent price spike with thin listings = manipulation risk.
			base *= 0.5
		}
	}

	return base
}

// stabilityDiscount returns a 0.5-1.0 discount factor based on the coefficient
// of variation. Higher CV means less stable prices, lower discount.
func stabilityDiscount(cv float64) float64 {
	d := 1.0 - (cv / 200.0)
	if d < 0.5 {
		return 0.5
	}
	if d > 1.0 {
		return 1.0
	}
	return d
}
