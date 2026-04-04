package lab

import (
	"math"
	"time"

	"profitofexile/internal/trade"
)

// ComputeGemFeatures produces per-gem feature vectors from raw gem data, history,
// and market context. It is a pure function with no side effects -- called from RunV2.
// Filters to transfigured, non-corrupted, non-Trarthus gems with Chaos > 5.
//
// tradeCache is nil-safe: when nil, trade enrichment is skipped entirely.
// When non-nil, a snapshot is taken once at the start and each gem is enriched
// with trade data subject to freshness-based degradation.
func ComputeGemFeatures(snapTime time.Time, gems []GemPrice, history []GemPriceHistory, mc MarketContext, cls GemClassificationMap, tradeCache *trade.TradeCache) []GemFeature {
	// Take a snapshot of all trade data once (single lock acquisition).
	var tradeSnap map[string]*trade.TradeLookupResult
	if tradeCache != nil {
		tradeSnap = tradeCache.GetSnapshot()
	}

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
			GemColor: g.GemColor,
		}
		if c, ok := cls[GemClassificationKey{g.Name, g.Variant}]; ok {
			f.Tier = c.Tier
			f.LowConfidence = c.LowConfidence
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

			// CV from all history prices (7-day, used for TRAP detection).
			prices := make([]float64, len(h.Points))
			for i, p := range h.Points {
				prices[i] = p.Chaos
			}
			f.CV = coefficientOfVariation(prices)

			// Short-window CV (6h) for stability discount.
			recentPrices := extractRecentPrices(h.Points, snapTime, 6*time.Hour)
			f.CVShort = coefficientOfVariation(recentPrices)

			// Historical position.
			f.High7Days, f.Low7Days, f.HistPosition = historicalPosition(g.Chaos, prices)
		} else {
			// No history: defaults.
			f.High7Days = g.Chaos
			f.Low7Days = g.Chaos
			f.HistPosition = 50
		}

		// Relative metrics.
		if p50 > 0 {
			f.RelativePrice = g.Chaos / p50
		}
		if avgListings > 0 {
			f.RelativeListings = float64(g.Listings) / avgListings
		}

		// Per-variant market depth (league-invariant).
		f.MarketDepth = computeMarketDepthForGem(g.Listings, g.Variant, mc, avgListings)
		if f.MarketDepth < 0.4 {
			f.MarketRegime = "CASCADE"
		} else {
			f.MarketRegime = "TEMPORAL"
		}

		// Behavioral profiles: flood/crash detection and listing elasticity.
		if h != nil && len(h.Points) >= 5 {
			f.FloodCount = countFloods(h.Points)
			f.CrashCount = countCrashes(h.Points)
			f.ListingElasticity = sanitizeFloat(computeListingElasticity(h.Points))
		}
		// else: stay at zero defaults (insufficient history)

		// Risk-adjusted scoring fields.
		f.SellProbabilityFactor = sellProbabilityFactor(g.Listings, f.Low7Days, g.Chaos)
		f.StabilityDiscount = stabilityDiscount(f.CVShort)

		// Trade enrichment: populate trade fields from snapshot.
		if tradeSnap != nil {
			tradeKey := trade.CacheKey(g.Name, g.Variant)
			if tr, ok := tradeSnap[tradeKey]; ok && tr != nil {
				ageSeconds := snapTime.Sub(tr.FetchedAt).Seconds()
				if ageSeconds < 0 {
					ageSeconds = 0 // guard against clock skew (FetchedAt slightly in future)
				}
				weight := tradeDataWeight(ageSeconds)
				if weight > 0 {
					f.TradeDataAvailable = true
					f.TradeDataAge = ageSeconds
					f.TradeSellerConcentration = tr.Signals.SellerConcentration
					f.TradeCheapestStaleness = tr.Signals.CheapestStaleness
					f.TradePriceOutlier = tr.Signals.PriceOutlier
					f.TradePriceFloor = tr.PriceFloor * weight
					f.TradeMedianTop10 = tr.MedianTop10 * weight

					// Apply trade multipliers to sell probability.
					f.SellProbabilityFactor = applyTradeMultipliers(f.SellProbabilityFactor, tr.Signals)
				}
			}
		}

		// Sanitize non-velocity float fields.
		// Velocity fields are already sanitized by velocityWindow; CV, hist, and
		// relative fields are sanitized here because their producers do not guarantee it.
		f.CV = sanitizeFloat(f.CV)
		f.CVShort = sanitizeFloat(f.CVShort)
		f.HistPosition = sanitizeFloat(f.HistPosition)
		f.High7Days = sanitizeFloat(f.High7Days)
		f.Low7Days = sanitizeFloat(f.Low7Days)
		f.RelativePrice = sanitizeFloat(f.RelativePrice)
		f.RelativeListings = sanitizeFloat(f.RelativeListings)
		f.MarketDepth = sanitizeFloat(f.MarketDepth)
		f.SellProbabilityFactor = sanitizeFloat(f.SellProbabilityFactor)
		f.StabilityDiscount = sanitizeFloat(f.StabilityDiscount)
		f.TradePriceFloor = sanitizeFloat(f.TradePriceFloor)
		f.TradeMedianTop10 = sanitizeFloat(f.TradeMedianTop10)
		f.TradeDataAge = sanitizeFloat(f.TradeDataAge)

		features = append(features, f)
	}

	return features
}

// extractChaos extracts the chaos price from a PricePoint.
func extractChaos(p PricePoint) float64 { return p.Chaos }

// extractListings extracts the listing count from a PricePoint as float64.
func extractListings(p PricePoint) float64 { return float64(p.Listings) }

// computeMarketDepthForGem returns listings / per-variant median listings.
// Falls back to market-wide average if VariantStats unavailable.
func computeMarketDepthForGem(listings int, variant string, mc MarketContext, fallbackAvg float64) float64 {
	if vs, ok := mc.VariantStats[variant]; ok && vs.MedianListings > 0 {
		return float64(listings) / vs.MedianListings
	}
	if fallbackAvg > 0 {
		return float64(listings) / fallbackAvg
	}
	return 0
}

// sellProbabilityFactor returns a 0.3-1.0 factor representing how likely this gem
// is to sell within an hour. Research showed the sigmoid on listing count adds only
// 0.79pp of discrimination vs 7.24pp from CV — so we use a simple listings floor
// with context-aware thin-market adjustments, and let stabilityDiscount (CV-based)
// do the heavy lifting.
func sellProbabilityFactor(listings int, low7d, currentPrice float64) float64 {
	// Simple listings-based factor: floor at 5 listings, linear to 1.0 at 50+.
	// This replaces the sigmoid which was nearly irrelevant (0.79pp contribution).
	var base float64
	switch {
	case listings >= 50:
		base = 1.0
	case listings >= 5:
		// Linear interpolation: 5 listings → 0.5, 50 listings → 1.0
		base = 0.5 + 0.5*float64(listings-5)/45.0
	default:
		base = 0.3 // thin market floor
	}

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

	// Enforce floor — thin-market penalty can push below 0.3 but
	// even manipulated gems have some non-zero sell chance.
	return math.Max(base, 0.3)
}

// stabilityDiscount returns a 0.7-1.0 discount factor based on the short-window
// coefficient of variation. Calibrated from 54K observations: P90 of 2h price
// change is 20%, P95 is 33%. Scale: 0% CV -> 1.0, 60% CV -> 0.7 (max 30% penalty).
func stabilityDiscount(cvShort float64) float64 {
	d := 1.0 - (cvShort / 200.0)
	if d < 0.7 {
		return 0.7
	}
	if d > 1.0 {
		return 1.0
	}
	return d
}

// tradeDataWeight returns a freshness-based weight for trade data.
// <5min: full weight (1.0), 5-30min: 0.75, 30-90min: 0.50, >90min: 0 (ignore).
func tradeDataWeight(ageSeconds float64) float64 {
	switch {
	case ageSeconds < 300: // <5min
		return 1.0
	case ageSeconds < 1800: // 5-30min
		return 0.75
	case ageSeconds < 5400: // 30-90min
		return 0.50
	default:
		return 0 // stale — ignore
	}
}

// applyTradeMultipliers adjusts the sell probability factor based on trade signals.
// MONOPOLY: 0.8x, CONCENTRATED: 0.9x, STALE: 0.9x, FRESH: 1.05x.
// Result is clamped to [0.3, 1.0].
func applyTradeMultipliers(base float64, signals trade.TradeSignals) float64 {
	switch signals.SellerConcentration {
	case "MONOPOLY":
		base *= 0.8
	case "CONCENTRATED":
		base *= 0.9
	}

	switch signals.CheapestStaleness {
	case "STALE":
		base *= 0.9
	case "FRESH":
		base *= 1.05
	}

	return math.Max(math.Min(base, 1.0), 0.3)
}
