package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/mercure"
	"github.com/go-chi/chi/v5"
)

func publishLayoutEvent(pub mercure.Publisher, action, difficulty string) {
	if pub == nil {
		return
	}
	payload, _ := json.Marshal(map[string]string{
		"action":     action,
		"difficulty": difficulty,
		"topic":      "poe/lab/layout",
	})
	go func() {
		if err := pub.Publish(context.Background(), "poe/lab/layout", string(payload)); err != nil {
			slog.Warn("layout: mercure publish failed", "error", err)
		}
	}()
}

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
func UploadLayout(repo *lab.LayoutRepository, pub mercure.Publisher) http.HandlerFunc {
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
			publishLayoutEvent(pub, "created", layout.Difficulty)
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

// PatchRoom handles PATCH /api/lab/layout/{difficulty}/room/{roomId}.
// Updates a single room's areacode, contents, or secret_passage in today's layout.
func PatchRoom(repo *lab.LayoutRepository, pub mercure.Publisher) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		raw := chi.URLParam(r, "difficulty")
		if !isValidDifficulty(raw) {
			jsonError(w, http.StatusBadRequest, "invalid difficulty")
			return
		}
		difficulty := normalizeDifficulty(raw)
		roomId := chi.URLParam(r, "roomId")

		today := time.Now().UTC().Format("2006-01-02")
		layoutJSON, err := repo.GetLayout(r.Context(), difficulty, today)
		if err != nil {
			slog.Error("patch room: get layout failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		if layoutJSON == nil {
			jsonError(w, http.StatusNotFound, "no layout for today")
			return
		}

		r.Body = http.MaxBytesReader(w, r.Body, 4096)
		type patchRequest struct {
			AreaCode      *string  `json:"areacode,omitempty"`
			Contents      []string `json:"contents,omitempty"`
			SecretPassage *string  `json:"secret_passage,omitempty"`
		}
		var patch patchRequest
		if err := json.NewDecoder(r.Body).Decode(&patch); err != nil {
			jsonError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		// Parse layout, find room, apply patch
		var layout lab.LabLayout
		if err := json.Unmarshal(layoutJSON, &layout); err != nil {
			slog.Error("patch room: unmarshal layout failed", "error", err)
			http.Error(w, `{"error":"corrupt layout"}`, http.StatusInternalServerError)
			return
		}

		found := false
		for i := range layout.Rooms {
			if layout.Rooms[i].ID == roomId {
				if patch.AreaCode != nil {
					layout.Rooms[i].AreaCode = *patch.AreaCode
				}
				if patch.Contents != nil {
					layout.Rooms[i].Contents = patch.Contents
				}
				if patch.SecretPassage != nil {
					layout.Rooms[i].SecretPassage = *patch.SecretPassage
				}
				found = true
				break
			}
		}
		if !found {
			jsonError(w, http.StatusNotFound, "room not found")
			return
		}

		lab.SanitizeLayout(&layout)
		if err := lab.ValidateLayout(&layout); err != nil {
			jsonError(w, http.StatusBadRequest, err.Error())
			return
		}

		updatedJSON, err := json.Marshal(layout)
		if err != nil {
			http.Error(w, `{"error":"marshal failed"}`, http.StatusInternalServerError)
			return
		}

		if err := repo.UpdateLayout(r.Context(), difficulty, today, updatedJSON); err != nil {
			slog.Error("patch room: update failed", "error", err)
			http.Error(w, `{"error":"update failed"}`, http.StatusInternalServerError)
			return
		}

		publishLayoutEvent(pub, "updated", difficulty)
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "updated"})
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
