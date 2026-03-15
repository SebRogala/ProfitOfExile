package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"sort"
	"strconv"
	"time"

	"profitofexile/internal/lab"
)

// parseLimit extracts and validates the limit query parameter.
// Returns the parsed limit and true, or writes an error response and returns 0, false.
func parseLimit(w http.ResponseWriter, r *http.Request, defaultLimit, maxLimit int) (int, bool) {
	limit := defaultLimit
	if v := r.URL.Query().Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
			return 0, false
		}
		if n > maxLimit {
			n = maxLimit
		}
		limit = n
	}
	return limit, true
}

// TransfigureAnalysis returns the latest transfigure ROI results.
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func TransfigureAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.TransfigureResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Transfigure(); len(cached) > 0 {
				results = filterTransfigure(cached, variant, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestTransfigureResults(r.Context(), variant, limit)
			if err != nil {
				slog.Error("transfigure analysis: query failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time                 string  `json:"time"`
			BaseName             string  `json:"baseName"`
			TransfiguredName     string  `json:"transfiguredName"`
			Variant              string  `json:"variant"`
			BasePrice            float64 `json:"basePrice"`
			TransfiguredPrice    float64 `json:"transfiguredPrice"`
			ROI                  float64 `json:"roi"`
			ROIPct               float64 `json:"roiPct"`
			BaseListings         int     `json:"baseListings"`
			TransfiguredListings int     `json:"transfiguredListings"`
			GemColor             string  `json:"gemColor"`
			Confidence           string  `json:"confidence"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:                 r.Time.UTC().Format(time.RFC3339),
				BaseName:             r.BaseName,
				TransfiguredName:     r.TransfiguredName,
				Variant:              r.Variant,
				BasePrice:            r.BasePrice,
				TransfiguredPrice:    r.TransfiguredPrice,
				ROI:                  r.ROI,
				ROIPct:               r.ROIPct,
				BaseListings:         r.BaseListings,
				TransfiguredListings: r.TransfiguredListings,
				GemColor:             r.GemColor,
				Confidence:           r.Confidence,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("transfigure analysis: encode response", "error", err)
		}
	}
}

// filterTransfigure filters and limits cached transfigure results.
// Results are sorted by ROI descending (matching the DB query order).
func filterTransfigure(all []lab.TransfigureResult, variant string, limit int) []lab.TransfigureResult {
	var filtered []lab.TransfigureResult
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

// FontAnalysis returns the latest Font of Divine Skill EV results.
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func FontAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.FontResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Font(); len(cached) > 0 {
				results = filterFont(cached, variant, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestFontResults(r.Context(), variant, limit)
			if err != nil {
				slog.Error("font analysis: query failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time      string  `json:"time"`
			Color     string  `json:"color"`
			Variant   string  `json:"variant"`
			Pool      int     `json:"pool"`
			Winners   int     `json:"winners"`
			PWin      float64 `json:"pWin"`
			AvgWin    float64 `json:"avgWin"`
			EV        float64 `json:"ev"`
			InputCost float64 `json:"inputCost"`
			Profit    float64 `json:"profit"`
			Threshold float64 `json:"threshold"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:      r.Time.UTC().Format(time.RFC3339),
				Color:     r.Color,
				Variant:   r.Variant,
				Pool:      r.Pool,
				Winners:   r.Winners,
				PWin:      r.PWin,
				AvgWin:    r.AvgWin,
				EV:        r.EV,
				InputCost: r.InputCost,
				Profit:    r.Profit,
				Threshold: r.Threshold,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("font analysis: encode response", "error", err)
		}
	}
}

// filterFont filters and limits cached font results.
// Results are sorted by Profit descending (matching the DB query order).
func filterFont(all []lab.FontResult, variant string, limit int) []lab.FontResult {
	var filtered []lab.FontResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Profit > filtered[j].Profit
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// TrendAnalysis returns the latest trend analysis results.
// Query params: variant (optional, e.g. "20/20"), signal (optional, e.g. "TRAP"), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func TrendAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")
		signal := r.URL.Query().Get("signal")
		window := r.URL.Query().Get("window")

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.TrendResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Trends(); len(cached) > 0 {
				results = filterTrends(cached, variant, signal, window, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestTrendResults(r.Context(), variant, signal, window, limit)
			if err != nil {
				slog.Error("trend analysis: query failed", "error", err, "variant", variant, "signal", signal)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time              string  `json:"time"`
			Name              string  `json:"name"`
			Variant           string  `json:"variant"`
			GemColor          string  `json:"gemColor"`
			CurrentPrice      float64 `json:"currentPrice"`
			CurrentListings   int     `json:"currentListings"`
			PriceVelocity     float64 `json:"priceVelocity"`
			ListingVelocity   float64 `json:"listingVelocity"`
			CV                float64 `json:"cv"`
			Signal            string  `json:"signal"`
			HistPosition      float64 `json:"histPosition"`
			PriceHigh7d       float64 `json:"priceHigh7d"`
			PriceLow7d        float64 `json:"priceLow7d"`
			BaseListings      int     `json:"baseListings"`
			BaseVelocity      float64 `json:"baseVelocity"`
			RelativeLiquidity float64 `json:"relativeLiquidity"`
			LiquidityTier     string  `json:"liquidityTier"`
			WindowScore       float64 `json:"windowScore"`
			WindowSignal      string  `json:"windowSignal"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:              r.Time.UTC().Format(time.RFC3339),
				Name:              r.Name,
				Variant:           r.Variant,
				GemColor:          r.GemColor,
				CurrentPrice:      r.CurrentPrice,
				CurrentListings:   r.CurrentListings,
				PriceVelocity:     r.PriceVelocity,
				ListingVelocity:   r.ListingVelocity,
				CV:                r.CV,
				Signal:            r.Signal,
				HistPosition:      r.HistPosition,
				PriceHigh7d:       r.PriceHigh7d,
				PriceLow7d:        r.PriceLow7d,
				BaseListings:      r.BaseListings,
				BaseVelocity:      r.BaseVelocity,
				RelativeLiquidity: r.RelativeLiquidity,
				LiquidityTier:     r.LiquidityTier,
				WindowScore:       r.WindowScore,
				WindowSignal:      r.WindowSignal,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("trend analysis: encode response", "error", err)
		}
	}
}

// filterTrends filters and limits cached trend results.
// Results are sorted by CV descending, then current price descending (matching the DB query order).
func filterTrends(all []lab.TrendResult, variant, signal, window string, limit int) []lab.TrendResult {
	var filtered []lab.TrendResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		if signal != "" && r.Signal != signal {
			continue
		}
		if window != "" && r.WindowSignal != window {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		if filtered[i].CV != filtered[j].CV {
			return filtered[i].CV > filtered[j].CV
		}
		return filtered[i].CurrentPrice > filtered[j].CurrentPrice
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// QualityAnalysis returns the latest quality-roll ROI results.
// Query params: variant (optional, maps to level: "1"/"1/20" -> level 1, "20"/"20/20" -> level 20), limit (default 50, max 500).
// Uses in-memory cache when available, falls back to DB query.
func QualityAnalysis(repo *lab.Repository, cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit, ok := parseLimit(w, r, 50, 500)
		if !ok {
			return
		}

		var results []lab.QualityResult

		// Fast path: serve from cache.
		if cache != nil {
			if cached := cache.Quality(); len(cached) > 0 {
				results = filterQuality(cached, variant, limit)
			}
		}

		// Slow path: fall back to DB query.
		if results == nil {
			var err error
			results, err = repo.LatestQualityResults(r.Context(), variant, limit)
			if err != nil {
				slog.Error("quality analysis: query failed", "error", err, "variant", variant)
				http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
				return
			}
		}

		type row struct {
			Time       string  `json:"time"`
			Name       string  `json:"name"`
			Level      int     `json:"level"`
			BuyPrice   float64 `json:"buyPrice"`
			PriceQ20   float64 `json:"priceQ20"`
			ROI4       float64 `json:"roi4"`
			ROI6       float64 `json:"roi6"`
			ROI10      float64 `json:"roi10"`
			ROI15      float64 `json:"roi15"`
			AvgROI     float64 `json:"avgRoi"`
			GCPPrice   float64 `json:"gcpPrice"`
			Listings0  int     `json:"listings0"`
			Listings20 int     `json:"listings20"`
			GemColor   string  `json:"gemColor"`
			Confidence string  `json:"confidence"`
		}

		rows := make([]row, 0, len(results))
		for _, r := range results {
			rows = append(rows, row{
				Time:       r.Time.UTC().Format(time.RFC3339),
				Name:       r.Name,
				Level:      r.Level,
				BuyPrice:   r.BuyPrice,
				PriceQ20:   r.PriceQ20,
				ROI4:       r.ROI4,
				ROI6:       r.ROI6,
				ROI10:      r.ROI10,
				ROI15:      r.ROI15,
				AvgROI:     r.AvgROI,
				GCPPrice:   r.GCPPrice,
				Listings0:  r.Listings0,
				Listings20: r.Listings20,
				GemColor:   r.GemColor,
				Confidence: r.Confidence,
			})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(rows),
			"data":  rows,
		}); err != nil {
			slog.Error("quality analysis: encode response", "error", err)
		}
	}
}

// filterQuality filters and limits cached quality results.
// Results are sorted by AvgROI descending (matching the DB query order).
// Variant maps to level: "1"/"1/20" -> level 1, "20"/"20/20" -> level 20.
func filterQuality(all []lab.QualityResult, variant string, limit int) []lab.QualityResult {
	var level int
	filterByLevel := false
	if variant != "" {
		filterByLevel = true
		level = 20
		if variant == "1" || variant == "1/20" {
			level = 1
		}
	}

	var filtered []lab.QualityResult
	for _, r := range all {
		if filterByLevel && r.Level != level {
			continue
		}
		filtered = append(filtered, r)
	}

	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].AvgROI > filtered[j].AvgROI
	})

	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// AnalysisStatus returns cache health information.
func AnalysisStatus(cache *lab.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		if cache == nil {
			json.NewEncoder(w).Encode(map[string]any{
				"cached":      false,
				"transfigure": 0,
				"font":        0,
				"quality":     0,
				"trends":      0,
			})
			return
		}

		lastUpdated := cache.LastUpdated()
		cached := !lastUpdated.IsZero()

		resp := map[string]any{
			"cached":      cached,
			"transfigure": len(cache.Transfigure()),
			"font":        len(cache.Font()),
			"quality":     len(cache.Quality()),
			"trends":      len(cache.Trends()),
		}
		if cached {
			resp["lastUpdated"] = lastUpdated.UTC().Format(time.RFC3339)
		}

		json.NewEncoder(w).Encode(resp)
	}
}
