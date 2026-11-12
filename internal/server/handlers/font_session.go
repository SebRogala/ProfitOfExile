package handlers

import (
	"encoding/json"
	"errors"
	"log/slog"
	"net/http"

	"profitofexile/internal/lab"
	"profitofexile/internal/server/middleware"
)

// fontSessionRequest is the expected JSON body for POST /api/desktop/font-session.
// The submitting device is taken from the authenticated request context, not the
// body — a client cannot attribute crowd-sourced font data to an arbitrary device.
type fontSessionRequest struct {
	LabType     string      `json:"lab_type"`
	TotalCrafts int         `json:"total_crafts"`
	Variant     string      `json:"variant"`
	PairCode    string      `json:"pair_code"`
	Rounds      []fontRound `json:"rounds"`
}

type fontRound struct {
	CraftOptions    []lab.FontCraftOption `json:"craft_options"`
	OptionChosen    string                `json:"option_chosen"`
	GemsOffered     []string              `json:"gems_offered"`
	GemPicked       string                `json:"gem_picked"`
	CraftsRemaining *int                  `json:"crafts_remaining"`
}

// FontSession handles POST /api/desktop/font-session. Stores crowd-sourced
// font crafting data from the desktop app's OCR pipeline.
func FontSession(repo *lab.FontSessionRepository) http.HandlerFunc {
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
		if body.PairCode != "" && !pairPattern.MatchString(body.PairCode) {
			jsonError(w, http.StatusBadRequest, "invalid pair_code format")
			return
		}

		dev := middleware.DeviceFromContext(r.Context())
		if dev == nil {
			jsonError(w, http.StatusUnauthorized, "device identification required")
			return
		}
		deviceID := dev.Fingerprint

		session := lab.FontSession{
			LabType:     body.LabType,
			TotalCrafts: body.TotalCrafts,
			Variant:     coalesce(body.Variant, "20/20"),
			PairCode:    body.PairCode,
			Rounds:      make([]lab.FontRound, len(body.Rounds)),
		}
		for i, round := range body.Rounds {
			session.Rounds[i] = lab.FontRound{
				CraftOptions:    round.CraftOptions,
				OptionChosen:    round.OptionChosen,
				GemsOffered:     round.GemsOffered,
				GemPicked:       round.GemPicked,
				CraftsRemaining: round.CraftsRemaining,
			}
		}

		sessionID, err := repo.Store(r.Context(), deviceID, session)
		if err != nil {
			var storeErr *lab.FontSessionStoreError
			if !errors.As(err, &storeErr) {
				storeErr = &lab.FontSessionStoreError{Stage: lab.FontSessionStageInsertSession, Err: err}
			}
			logFontSessionStoreError(storeErr)
			jsonError(w, http.StatusInternalServerError, fontSessionStoreErrorMessage(storeErr.Stage))
			return
		}

		slog.Info("font session: stored",
			"session_id", sessionID,
			"lab_type", body.LabType,
			"rounds", len(body.Rounds),
			"device_id", deviceID,
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

// logFontSessionStoreError logs a failed store stage with the handler's
// original message texts and attributes.
func logFontSessionStoreError(e *lab.FontSessionStoreError) {
	switch e.Stage {
	case lab.FontSessionStageBegin:
		slog.Error("font session: begin tx", "error", e.Err)
	case lab.FontSessionStageMarshalOptions:
		slog.Error("font session: marshal options", "error", e.Err, "round", e.RoundIndex)
	case lab.FontSessionStageInsertRound:
		slog.Error("font session: insert round", "error", e.Err, "round", e.RoundIndex, "session", e.SessionID)
	case lab.FontSessionStageCommit:
		slog.Error("font session: commit", "error", e.Err)
	default:
		slog.Error("font session: insert session", "error", e.Err)
	}
}

// fontSessionStoreErrorMessage is the 500 body text for a failed store stage.
func fontSessionStoreErrorMessage(stage lab.FontSessionStage) string {
	switch stage {
	case lab.FontSessionStageMarshalOptions:
		return "failed to serialize round options"
	case lab.FontSessionStageInsertRound:
		return "failed to store round"
	case lab.FontSessionStageCommit:
		return "failed to commit session"
	default:
		return "failed to store session"
	}
}
