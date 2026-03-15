package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strconv"
	"time"

	"profitofexile/internal/lab"
)

// TransfigureAnalysis returns the latest transfigure ROI results.
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
func TransfigureAnalysis(repo *lab.Repository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit := 50
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 500 {
				n = 500
			}
			limit = n
		}

		results, err := repo.LatestTransfigureResults(r.Context(), variant, limit)
		if err != nil {
			slog.Error("transfigure analysis: query failed", "error", err, "variant", variant)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
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

// FontAnalysis returns the latest Font of Divine Skill EV results.
// Query params: variant (optional, e.g. "20/20"), limit (default 50, max 500).
func FontAnalysis(repo *lab.Repository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit := 50
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 500 {
				n = 500
			}
			limit = n
		}

		results, err := repo.LatestFontResults(r.Context(), variant, limit)
		if err != nil {
			slog.Error("font analysis: query failed", "error", err, "variant", variant)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
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

// QualityAnalysis returns the latest quality-roll ROI results.
// Query params: variant (optional, maps to level: "1"/"1/20" → level 1, "20"/"20/20" → level 20), limit (default 50, max 500).
func QualityAnalysis(repo *lab.Repository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		variant := r.URL.Query().Get("variant")

		limit := 50
		if v := r.URL.Query().Get("limit"); v != "" {
			n, err := strconv.Atoi(v)
			if err != nil || n < 1 {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "limit must be a positive integer"})
				return
			}
			if n > 500 {
				n = 500
			}
			limit = n
		}

		results, err := repo.LatestQualityResults(r.Context(), variant, limit)
		if err != nil {
			slog.Error("quality analysis: query failed", "error", err, "variant", variant)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
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
