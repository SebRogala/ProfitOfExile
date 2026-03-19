package lab

import (
	"strings"
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

	var p50 float64
	if mc.PricePercentiles != nil {
		p50 = mc.PricePercentiles["P50"]
	}

	var features []GemFeature

	for _, g := range gems {
		if g.IsCorrupted || !g.IsTransfigured {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}
		if g.Chaos <= 5 {
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

		// Stubs for POE-59.
		f.FloodCount = 0
		f.CrashCount = 0
		f.ListingElasticity = 0

		// Sanitize all float fields.
		f.VelShortPrice = sanitizeFloat(f.VelShortPrice)
		f.VelShortListing = sanitizeFloat(f.VelShortListing)
		f.VelMedPrice = sanitizeFloat(f.VelMedPrice)
		f.VelMedListing = sanitizeFloat(f.VelMedListing)
		f.VelLongPrice = sanitizeFloat(f.VelLongPrice)
		f.VelLongListing = sanitizeFloat(f.VelLongListing)
		f.CV = sanitizeFloat(f.CV)
		f.HistPosition = sanitizeFloat(f.HistPosition)
		f.High7d = sanitizeFloat(f.High7d)
		f.Low7d = sanitizeFloat(f.Low7d)
		f.RelativePrice = sanitizeFloat(f.RelativePrice)
		f.RelativeListings = sanitizeFloat(f.RelativeListings)

		features = append(features, f)
	}

	return features
}

// extractChaos extracts the chaos price from a PricePoint.
func extractChaos(p PricePoint) float64 { return p.Chaos }

// extractListings extracts the listing count from a PricePoint as float64.
func extractListings(p PricePoint) float64 { return float64(p.Listings) }
