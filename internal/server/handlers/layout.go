package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"github.com/go-chi/chi/v5"
)

// GetLayout handles GET /api/lab/layout/{difficulty}.
// Returns today's lab layout for the given difficulty, or 404 if not yet uploaded.
func GetLayout(repo *lab.LayoutRepository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		raw := chi.URLParam(r, "difficulty")
		if !isValidDifficulty(raw) {
			jsonError(w, http.StatusBadRequest, "invalid difficulty: must be Normal, Cruel, Merciless, or Uber")
			return
		}
		difficulty := normalizeDifficulty(raw)

		today := time.Now().UTC().Format("2006-01-02")
		layout, err := repo.GetLayout(r.Context(), difficulty, today)
		if err != nil {
			slog.Error("get layout: query failed", "error", err, "difficulty", difficulty)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		if layout == nil {
			jsonError(w, http.StatusNotFound, "no layout available for today")
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.Write(layout)
	}
}

// UploadLayout handles POST /api/lab/layout/{difficulty}.
// Validates and stores a poelab.com layout JSON. First upload per difficulty+date wins.
func UploadLayout(repo *lab.LayoutRepository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		raw := chi.URLParam(r, "difficulty")
		if !isValidDifficulty(raw) {
			jsonError(w, http.StatusBadRequest, "invalid difficulty: must be Normal, Cruel, Merciless, or Uber")
			return
		}
		difficulty := normalizeDifficulty(raw)

		r.Body = http.MaxBytesReader(w, r.Body, 102400) // 100KB max

		var layout lab.LabLayout
		if err := json.NewDecoder(r.Body).Decode(&layout); err != nil {
			jsonError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		lab.SanitizeLayout(&layout)

		if err := lab.ValidateLayout(&layout); err != nil {
			jsonError(w, http.StatusBadRequest, err.Error())
			return
		}

		if !strings.EqualFold(layout.Difficulty, difficulty) {
			jsonError(w, http.StatusBadRequest, "difficulty in body does not match URL")
			return
		}

		rawJSON, err := json.Marshal(layout)
		if err != nil {
			slog.Error("upload layout: marshal failed", "error", err)
			http.Error(w, `{"error":"internal error"}`, http.StatusInternalServerError)
			return
		}

		inserted, err := repo.SaveLayout(r.Context(), layout.Difficulty, layout.Date, rawJSON)
		if err != nil {
			slog.Error("upload layout: save failed", "error", err, "difficulty", difficulty)
			http.Error(w, `{"error":"save failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if inserted {
			w.WriteHeader(http.StatusCreated)
			json.NewEncoder(w).Encode(map[string]any{
				"status":     "created",
				"difficulty": layout.Difficulty,
				"date":       layout.Date,
				"rooms":      len(layout.Rooms),
			})
		} else {
			w.WriteHeader(http.StatusOK)
			json.NewEncoder(w).Encode(map[string]string{
				"status": "already exists",
			})
		}
	}
}

func isValidDifficulty(d string) bool {
	switch strings.ToLower(d) {
	case "normal", "cruel", "merciless", "uber":
		return true
	}
	return false
}

// normalizeDifficulty returns the canonical title-case form.
func normalizeDifficulty(d string) string {
	switch strings.ToLower(d) {
	case "normal":
		return "Normal"
	case "cruel":
		return "Cruel"
	case "merciless":
		return "Merciless"
	case "uber":
		return "Uber"
	}
	return d
}
