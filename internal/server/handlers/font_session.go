package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"

	"github.com/jackc/pgx/v5/pgxpool"
)

// fontSessionRequest is the expected JSON body for POST /api/desktop/font-session.
type fontSessionRequest struct {
	LabType     string       `json:"lab_type"`
	TotalCrafts int          `json:"total_crafts"`
	Variant     string       `json:"variant"`
	DeviceID    string       `json:"device_id"`
	PairCode    string       `json:"pair_code"`
	Rounds      []fontRound  `json:"rounds"`
}

type fontRound struct {
	CraftOptions    []craftOption `json:"craft_options"`
	OptionChosen    string        `json:"option_chosen"`
	GemsOffered     []string      `json:"gems_offered"`
	GemPicked       string        `json:"gem_picked"`
	CraftsRemaining *int          `json:"crafts_remaining"`
}

type craftOption struct {
	Type  string `json:"type"`
	Text  string `json:"text"`
	Value *int   `json:"value,omitempty"`
}

// FontSession handles POST /api/desktop/font-session. Stores crowd-sourced
// font crafting data from the desktop app's OCR pipeline.
func FontSession(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		r.Body = http.MaxBytesReader(w, r.Body, 32768) // 32KB max

		var body fontSessionRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON body"})
			return
		}

		// Validate
		if body.LabType == "" {
			jsonError(w, http.StatusBadRequest, "lab_type is required")
			return
		}
		if body.TotalCrafts <= 0 || body.TotalCrafts > 20 {
			jsonError(w, http.StatusBadRequest, "total_crafts must be 1-20")
			return
		}
		if len(body.Rounds) == 0 {
			jsonError(w, http.StatusBadRequest, "rounds must not be empty")
			return
		}
		if len(body.Rounds) > 20 {
			jsonError(w, http.StatusBadRequest, "too many rounds (max 20)")
			return
		}

		ctx := r.Context()

		// Insert session
		var sessionID int64
		err := pool.QueryRow(ctx,
			`INSERT INTO font_sessions (lab_type, total_crafts, variant, device_id, pair_code)
			 VALUES ($1, $2, $3, $4, $5)
			 RETURNING id`,
			body.LabType,
			body.TotalCrafts,
			coalesce(body.Variant, "20/20"),
			coalesce(body.DeviceID, "unknown"),
			body.PairCode,
		).Scan(&sessionID)
		if err != nil {
			slog.Error("font session: insert session", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to store session")
			return
		}

		// Insert rounds
		for i, round := range body.Rounds {
			optionsJSON, err := json.Marshal(round.CraftOptions)
			if err != nil {
				slog.Error("font session: marshal options", "error", err, "round", i)
				continue
			}

			_, err = pool.Exec(ctx,
				`INSERT INTO font_rounds (session_id, round_number, craft_options, option_chosen, gems_offered, gem_picked, crafts_remaining)
				 VALUES ($1, $2, $3, $4, $5, $6, $7)`,
				sessionID,
				i+1,
				optionsJSON,
				nilIfEmpty(round.OptionChosen),
				pgStringArray(round.GemsOffered),
				nilIfEmpty(round.GemPicked),
				round.CraftsRemaining,
			)
			if err != nil {
				slog.Error("font session: insert round", "error", err, "round", i, "session", sessionID)
			}
		}

		slog.Info("font session: stored",
			"session_id", sessionID,
			"lab_type", body.LabType,
			"rounds", len(body.Rounds),
			"device_id", body.DeviceID,
		)

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"session_id": sessionID,
			"rounds":     len(body.Rounds),
		})
	}
}

func jsonError(w http.ResponseWriter, status int, msg string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(map[string]string{"error": msg})
}

func coalesce(s, fallback string) string {
	if s == "" {
		return fallback
	}
	return s
}

func nilIfEmpty(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

func pgStringArray(ss []string) any {
	if len(ss) == 0 {
		return nil
	}
	return ss
}
