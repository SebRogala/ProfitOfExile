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
		variant := r.URL.Query().Get("variant")

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
				trends = filterTrends(ctr, variant, "", "", 5000)
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

			trends, err = repo.LatestTrendResults(r.Context(), variant, "", "", 5000)
			if err != nil {
				slog.Error("collective analysis: trends query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		results := lab.RankCollective(transfigure, trends, budget, limit)

		type row struct {
			TransfiguredName     string  `json:"transfiguredName"`
			BaseName             string  `json:"baseName"`
			Variant              string  `json:"variant"`
			GemColor             string  `json:"gemColor"`
			ROI                  float64 `json:"roi"`
			WeightedROI          float64 `json:"weightedRoi"`
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
		}

		rows := make([]row, 0, len(results))
		for _, cr := range results {
			rows = append(rows, row{
				TransfiguredName:     cr.TransfiguredName,
				BaseName:             cr.BaseName,
				Variant:              cr.Variant,
				GemColor:             cr.GemColor,
				ROI:                  cr.ROI,
				WeightedROI:          cr.WeightedROI,
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
			})
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

		variant := r.URL.Query().Get("variant")

		// Fast path: serve from cache.
		var transfigure []lab.TransfigureResult
		var trends []lab.TrendResult
		usedCache := false

		if cache != nil {
			ct := cache.Transfigure()
			ctr := cache.Trends()
			if len(ct) > 0 && len(ctr) > 0 {
				transfigure = filterTransfigure(ct, variant, 1000)
				trends = filterTrends(ctr, variant, "", "", 5000)
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

			trends, err = repo.LatestTrendResults(r.Context(), variant, "", "", 5000)
			if err != nil {
				slog.Error("compare analysis: trends query failed", "error", err)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		// Load sparkline data (last 2 hours).
		var warnings []string
		sparklines, err := repo.SparklineData(r.Context(), names, variant, 2)
		if err != nil {
			slog.Error("compare analysis: sparkline query failed", "error", err)
			sparklines = make(map[string][]lab.SparklinePoint)
			warnings = append(warnings, "Sparkline data temporarily unavailable")
		}

		results := lab.BuildCompareResults(names, transfigure, trends, sparklines)

		type row struct {
			TransfiguredName  string              `json:"transfiguredName"`
			BaseName          string              `json:"baseName"`
			Variant           string              `json:"variant"`
			GemColor          string              `json:"gemColor"`
			ROI               float64             `json:"roi"`
			BasePrice         float64             `json:"basePrice"`
			TransfiguredPrice float64             `json:"transfiguredPrice"`
			Confidence        string              `json:"confidence"`
			Signal            string              `json:"signal"`
			CV                float64             `json:"cv"`
			PriceVelocity     float64             `json:"priceVelocity"`
			ListingVelocity   float64             `json:"listingVelocity"`
			HistPosition      float64             `json:"histPosition"`
			Sparkline         []lab.SparklinePoint `json:"sparkline"`
			Recommendation    string              `json:"recommendation"`
		}

		rows := make([]row, 0, len(results))
		for _, cr := range results {
			rows = append(rows, row{
				TransfiguredName:  cr.TransfiguredName,
				BaseName:          cr.BaseName,
				Variant:           cr.Variant,
				GemColor:          cr.GemColor,
				ROI:               cr.ROI,
				BasePrice:         cr.BasePrice,
				TransfiguredPrice: cr.TransfiguredPrice,
				Confidence:        cr.Confidence,
				Signal:            cr.Signal,
				CV:                cr.CV,
				PriceVelocity:     cr.PriceVelocity,
				ListingVelocity:   cr.ListingVelocity,
				HistPosition:      cr.HistPosition,
				Sparkline:         cr.Sparkline,
				Recommendation:    cr.Recommendation,
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

// GemNamesAutocomplete returns distinct transfigured gem names matching a prefix.
// Query params: q (required, search prefix), limit (default 10, max 50).
func GemNamesAutocomplete(repo *lab.Repository) http.HandlerFunc {
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

		names, err := repo.GemNamesAutocomplete(r.Context(), q, limit)
		if err != nil {
			slog.Error("gem names autocomplete: query failed", "error", err, "q", q)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
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
