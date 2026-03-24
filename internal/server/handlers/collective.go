package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strconv"
	"strings"

	"profitofexile/internal/lab"
)

// CollectiveAnalysis returns a ranked "what to farm now" list combining
// transfigure ROI with trend signals.
// Query params: variant (optional), budget (optional, max base price), limit (default 20, max 100).
func CollectiveAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := normalizeVariant(r.URL.Query().Get("variant"))

		var budget float64
		if v := r.URL.Query().Get("budget"); v != "" {
			b, err := strconv.ParseFloat(v, 64)
			if err != nil || b < 0 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "budget must be a non-negative number"})
				return
			}
			budget = b
		}

		limit := 20
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 100 {
				n = 100
			}
			limit = n
		}

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var trends []lab.TrendResult
		usedCache := false

		if cache != nil {
			ct := cache.Transfigure()
			ctr := cache.Trends()
			if len(ct) > 0 && len(ctr) > 0 {
				transfigure = filterTransfigure(ct, variant, 1000)
				trends = filterTrends(ctr, variant, "", "", "", "", 5000)
				usedCache = true
			}
		}

		// Slow path: fall back to DB query.
		if !usedCache {
			var err error
			transfigure, err = repo.LatestTransfigureResults(r.Context(), variant, 1000)
			if err != nil {
				slog.Error("collective analysis: transfigure query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			trends, err = repo.LatestTrendResults(r.Context(), variant, "", "", "", "", 5000)
			if err != nil {
				slog.Error("collective analysis: trends query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Parse sort mode: "pct" for ROI%, "chaos" (default) for absolute ROI.
		var sortBy lab.SortMode
		if s := r.URL.Query().Get("sort"); s == "pct" {
			sortBy = lab.SortPct
		}
		// Empty sortBy lets RankCollective apply budget-aware default.

		results := lab.RankCollective(transfigure, trends, budget, limit, sortBy)

		// Build base price index: baseName → variant → basePrice (for GCP recipe).
		type bKey struct{ name, variant string }
		basePriceIndex := make(map[bKey]float64, len(transfigure))
		for _, tr := range transfigure {
			basePriceIndex[bKey{tr.BaseName, tr.Variant}] = tr.BasePrice
		}

		// GCP recipe: for 20/20 gems, compare buying 20/20 base vs 20/0 base + 20×GCP.
		var gcpPrice float64
		if cache != nil {
			gcpPrice = cache.GCPPrice()
		}
		if gcpPrice <= 0 {
			gcpPrice = 4.0
			slog.Warn("collective: GCP price not cached, using fallback", "fallback", gcpPrice)
		}

		// Enrich results with GlobalTier from cached gem features.
		if cache != nil {
			if features := cache.GemFeatures(); len(features) > 0 {
				type fKey struct{ name, variant string }
				featureIndex := make(map[fKey]string, len(features))
				for _, f := range features {
					featureIndex[fKey{f.Name, f.Variant}] = f.GlobalTier
				}
				for i := range results {
					if gt, ok := featureIndex[fKey{results[i].TransfiguredName, results[i].Variant}]; ok {
						results[i].GlobalTier = gt
					}
				}
			}
		}

		// Fetch sparkline data for the result gems.
		// When a specific variant is selected, one query suffices. When "ALL
		// variants", group gems by their own variant and query per group so
		// sparklines don't mix different variant prices.
		sparkVariant := r.URL.Query().Get("variant")
		sparklines := make(map[string][]lab.SparklinePoint)

		// Load MarketContext for sparkline normalization.
		var mc *lab.MarketContext
		if cache != nil {
			mc = cache.MarketContext()
		}
		if mc == nil {
			mc, _ = repo.LatestMarketContext(r.Context())
		}

		if sparkVariant != "" {
			// Single variant — one query for all gems.
			sparkNames := make([]string, 0, len(results))
			for _, cr := range results {
				sparkNames = append(sparkNames, cr.TransfiguredName)
			}
			sp, err := repo.SparklineData(r.Context(), sparkNames, sparkVariant, 12)
			if err != nil {
				slog.Error("collective analysis: sparkline query failed", "error", err)
			} else {
				sparklines = normalizeSparklines(sp, mc, sparkVariant)
			}
		} else {
			// ALL variants — group by each gem's own variant.
			byVariant := make(map[string][]string) // variant -> []name
			for _, cr := range results {
				byVariant[cr.Variant] = append(byVariant[cr.Variant], cr.TransfiguredName)
			}
			for v, names := range byVariant {
				sp, err := repo.SparklineData(r.Context(), names, v, 12)
				if err != nil {
					slog.Error("collective analysis: sparkline query failed", "variant", v, "error", err)
					continue
				}
				normalized := normalizeSparklines(sp, mc, v)
				for k, pts := range normalized {
					sparklines[k] = pts
				}
			}
		}

		type row struct {
			TransfiguredName     string  `json:"transfiguredName"`
			BaseName             string  `json:"baseName"`
			Variant              string  `json:"variant"`
			GemColor             string  `json:"gemColor"`
			ROI                  float64 `json:"roi"`
			ROIPct               float64 `json:"roiPct"`
			WeightedROI          float64 `json:"weightedRoi"`
			WeightedROIPct       float64 `json:"weightedRoiPct"`
			BasePrice            float64 `json:"basePrice"`
			TransfiguredPrice    float64 `json:"transfiguredPrice"`
			BaseListings         int     `json:"baseListings"`
			TransfiguredListings int     `json:"transfiguredListings"`
			Confidence           string  `json:"confidence"`
			Signal               string  `json:"signal"`
			PriceVelocity        float64 `json:"priceVelocity"`
			ListingVelocity      float64 `json:"listingVelocity"`
			CV                   float64 `json:"cv"`
			HistPosition         float64 `json:"histPosition"`
			WindowSignal         string  `json:"windowSignal"`
			AdvancedSignal       string  `json:"advancedSignal"`
			LiquidityTier        string  `json:"liquidityTier"`
			PriceTier            string  `json:"priceTier"`
			GlobalTier           string  `json:"globalTier"`
			TierAction           string  `json:"tierAction"`
			SellUrgency          string  `json:"sellUrgency"`
			SellReason           string  `json:"sellReason"`
			Sellability          int     `json:"sellability"`
			SellabilityLabel     string              `json:"sellabilityLabel"`
			Sparkline            []lab.SparklinePoint `json:"sparkline"`
			Low7Days                float64 `json:"low7d"`
			High7Days               float64 `json:"high7d"`
			SellConfidence       string  `json:"sellConfidence"`
			// GCP recipe: buy 20/0 base + 20 GCPs instead of 20/20 base.
			GCPRecipeCost  float64 `json:"gcpRecipeCost,omitempty"`  // 20/0 base + 20×GCP
			GCPRecipeBase  float64 `json:"gcpRecipeBase,omitempty"`  // 20/0 base price alone
			GCPRecipeSaves float64 `json:"gcpRecipeSaves,omitempty"` // 20/20 base - recipe cost
		}

		rows := make([]row, 0, len(results))
		for _, cr := range results {
			r := row{
				TransfiguredName:     cr.TransfiguredName,
				BaseName:             cr.BaseName,
				Variant:              cr.Variant,
				GemColor:             cr.GemColor,
				ROI:                  cr.ROI,
				ROIPct:               cr.ROIPct,
				WeightedROI:          cr.WeightedROI,
				WeightedROIPct:       cr.WeightedROIPct,
				BasePrice:            cr.BasePrice,
				TransfiguredPrice:    cr.TransfiguredPrice,
				BaseListings:         cr.BaseListings,
				TransfiguredListings: cr.TransfiguredListings,
				Confidence:           cr.Confidence,
				Signal:               cr.Signal,
				PriceVelocity:        cr.PriceVelocity,
				ListingVelocity:      cr.ListingVelocity,
				CV:                   cr.CV,
				HistPosition:         cr.HistPosition,
				WindowSignal:         cr.WindowSignal,
				AdvancedSignal:       cr.AdvancedSignal,
				LiquidityTier:        cr.LiquidityTier,
				PriceTier:            cr.PriceTier,
				GlobalTier:           cr.GlobalTier,
				TierAction:           cr.TierAction,
				SellUrgency:          cr.SellUrgency,
				SellReason:           cr.SellReason,
				Sellability:          cr.Sellability,
				SellabilityLabel:     cr.SellabilityLabel,
				Sparkline:           sparklines[cr.TransfiguredName],
				Low7Days:               cr.Low7Days,
				High7Days:              cr.High7Days,
				SellConfidence:      cr.SellConfidence,
			}

			// GCP recipe for 20/20 variants: buy 20/0 base + 20×GCP.
			// Always show — even when more expensive, it's useful context.
			if cr.Variant == "20/20" {
				if base20, ok := basePriceIndex[bKey{cr.BaseName, "20"}]; ok && base20 > 0 {
					recipeCost := base20 + 20*gcpPrice
					r.GCPRecipeCost = recipeCost
					r.GCPRecipeBase = base20
					r.GCPRecipeSaves = cr.BasePrice - recipeCost // negative = recipe is more expensive
				}
			}

			rows = append(rows, r)
			if rows[len(rows)-1].Sparkline == nil {
				rows[len(rows)-1].Sparkline = []lab.SparklinePoint{}
			}
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("collective analysis: encode response", "error", err)
		}
	}
}

// CompareAnalysis returns side-by-side comparison of 1-5 specific gems.
// Query params: gems (comma-separated, required, max 5), variant (optional).
func CompareAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		gemsParam := r.URL.Query().Get("gems")
		if gemsParam == "" {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "gems parameter is required"})
			return
		}

		names := strings.Split(gemsParam, ",")
		if len(names) > 5 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "maximum 5 gems allowed"})
			return
		}

		// Trim whitespace and filter empty names.
		filtered := names[:0]
		for _, n := range names {
			n = strings.TrimSpace(n)
			if n != "" {
				filtered = append(filtered, n)
			}
		}
		names = filtered

		if len(names) == 0 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "at least one gem name is required"})
			return
		}

		variant := normalizeVariant(r.URL.Query().Get("variant"))

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var trends []lab.TrendResult
		usedCache := false

		if cache != nil {
			ct := cache.Transfigure()
			ctr := cache.Trends()
			if len(ct) > 0 && len(ctr) > 0 {
				transfigure = filterTransfigure(ct, variant, 1000)
				trends = filterTrends(ctr, variant, "", "", "", "", 5000)
				usedCache = true
			}
		}

		// Slow path: fall back to DB query.
		if !usedCache {
			var err error
			transfigure, err = repo.LatestTransfigureResults(r.Context(), variant, 1000)
			if err != nil {
				slog.Error("compare analysis: transfigure query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}

			trends, err = repo.LatestTrendResults(r.Context(), variant, "", "", "", "", 5000)
			if err != nil {
				slog.Error("compare analysis: trends query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Load sparkline data (last 12 hours) and normalize with temporal coefficients.
		var warnings []string
		sparklines, err := repo.SparklineData(r.Context(), names, variant, 12)
		if err != nil {
			slog.Error("compare analysis: sparkline query failed", "error", err)
			sparklines = make(map[string][]lab.SparklinePoint)
			warnings = append(warnings, "Sparkline data temporarily unavailable")
		}

		// Load MarketContext for sparkline normalization.
		var compareMC *lab.MarketContext
		if cache != nil {
			compareMC = cache.MarketContext()
		}
		if compareMC == nil {
			compareMC, _ = repo.LatestMarketContext(r.Context())
		}
		sparklines = normalizeSparklines(sparklines, compareMC, variant)

		results := lab.BuildCompareResults(names, transfigure, trends, sparklines)

		// Enrich with risk-adjusted price from cached v2 GemSignals.
		if cache != nil {
			if signals := cache.GemSignals(); len(signals) > 0 {
				type sKey struct{ name, variant string }
				sigIndex := make(map[sKey]float64, len(signals))
				for _, s := range signals {
					sigIndex[sKey{s.Name, s.Variant}] = s.RiskAdjustedValue
				}
				for i := range results {
					if raVal, ok := sigIndex[sKey{results[i].TransfiguredName, results[i].Variant}]; ok {
						results[i].RiskAdjustedPrice = raVal
					}
				}
			}
		}

		type row struct {
			TransfiguredName     string              `json:"transfiguredName"`
			BaseName             string              `json:"baseName"`
			Variant              string              `json:"variant"`
			GemColor             string              `json:"gemColor"`
			ROI                  float64             `json:"roi"`
			ROIPct               float64             `json:"roiPct"`
			BasePrice            float64             `json:"basePrice"`
			TransfiguredPrice    float64             `json:"transfiguredPrice"`
			Confidence           string              `json:"confidence"`
			Signal               string              `json:"signal"`
			CV                   float64             `json:"cv"`
			PriceVelocity        float64             `json:"priceVelocity"`
			ListingVelocity      float64             `json:"listingVelocity"`
			HistPosition         float64             `json:"histPosition"`
			Sparkline            []lab.SparklinePoint `json:"sparkline"`
			Recommendation       string              `json:"recommendation"`
			SellUrgency          string              `json:"sellUrgency"`
			SellReason           string              `json:"sellReason"`
			Sellability          int                 `json:"sellability"`
			SellabilityLabel     string              `json:"sellabilityLabel"`
			PriceTier            string              `json:"priceTier"`
			TierAction           string              `json:"tierAction"`
			WindowSignal         string              `json:"windowSignal"`
			BaseListings         int                 `json:"baseListings"`
			LiquidityTier        string              `json:"liquidityTier"`
			TransListings        int                 `json:"transListings"`
			TransfiguredListings int                 `json:"transfiguredListings"`
			WeightedROI          float64             `json:"weightedRoi"`
			Low7Days                float64             `json:"low7d"`
			High7Days               float64             `json:"high7d"`
			SellConfidence       string              `json:"sellConfidence"`
			SellConfidenceReason string              `json:"sellConfidenceReason"`
			QuickSellPrice       float64             `json:"quickSellPrice"`
			RiskAdjustedPrice    float64             `json:"riskAdjustedPrice"`
		}

		rows := make([]row, 0, len(results))
		for _, cr := range results {
			rows = append(rows, row{
				TransfiguredName:     cr.TransfiguredName,
				BaseName:             cr.BaseName,
				Variant:              cr.Variant,
				GemColor:             cr.GemColor,
				ROI:                  cr.ROI,
				ROIPct:               cr.ROIPct,
				BasePrice:            cr.BasePrice,
				TransfiguredPrice:    cr.TransfiguredPrice,
				Confidence:           cr.Confidence,
				Signal:               cr.Signal,
				CV:                   cr.CV,
				PriceVelocity:        cr.PriceVelocity,
				ListingVelocity:      cr.ListingVelocity,
				HistPosition:         cr.HistPosition,
				Sparkline:            cr.Sparkline,
				Recommendation:       cr.Recommendation,
				SellUrgency:          cr.SellUrgency,
				SellReason:           cr.SellReason,
				Sellability:          cr.Sellability,
				SellabilityLabel:     cr.SellabilityLabel,
				PriceTier:            cr.PriceTier,
				TierAction:           cr.TierAction,
				WindowSignal:         cr.WindowSignal,
				BaseListings:         cr.BaseListings,
				LiquidityTier:        cr.LiquidityTier,
				TransListings:        cr.TransListings,
				TransfiguredListings: cr.TransListings,
				WeightedROI:          cr.WeightedROI,
				Low7Days:                cr.Low7Days,
				High7Days:               cr.High7Days,
				SellConfidence:       cr.SellConfidence,
				SellConfidenceReason: cr.SellConfidenceReason,
				QuickSellPrice:       cr.QuickSellPrice,
				RiskAdjustedPrice:    cr.RiskAdjustedPrice,
			})
		}

		resp := map[string]any{
			"count": len(rows),
			"data":  rows,
		}
		if len(warnings) > 0 {
			resp["warnings"] = warnings
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("compare analysis: encode response", "error", err)
		}
	}
}

// GemNamesAutocomplete returns distinct transfigured gem names matching a query.
// Query params: q (required, matches all words in any order), limit (default 10, max 50).
func GemNamesAutocomplete(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query().Get("q")
		if q == "" {
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(map[string]any{"names": []string{}})
			return
		}

		limit := 10
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 50 {
				n = 50
			}
			limit = n
		}

		// Fast path: in-memory search over cached gem names (~200 entries).
		// Falls back to DB query only if cache is empty (cold start).
		var names []string
		if cache != nil {
			names = cache.GemNamesSearch(q, limit)
		}
		if names == nil {
			var err error
			names, err = repo.GemNamesAutocomplete(r.Context(), q, limit)
			if err != nil {
				slog.Error("gem names autocomplete: query failed", "error", err, "q", q)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		if names == nil {
			names = []string{}
		}

		w.Header().Set("Content-Type", "application/json")
		// Cache autocomplete responses briefly.
		w.Header().Set("Cache-Control", "public, max-age=60")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"names": names,
		}); err != nil {
			slog.Error("gem names autocomplete: encode response", "error", err)
		}

	}
}
