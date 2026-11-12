package handlers

import (
	"encoding/json"
	"errors"
	"log/slog"
	"math"
	"net/http"
	"strconv"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/server/middleware"
)

type labRunRequest struct {
	Difficulty     string       `json:"difficulty"`
	Strategy       string       `json:"strategy"`
	ElapsedSeconds int          `json:"elapsed_seconds"`
	KillSeconds    *int         `json:"kill_seconds,omitempty"`
	RoomCount      int          `json:"room_count"`
	HasGoldenDoor  bool         `json:"has_golden_door"`
	StartedAt      time.Time    `json:"started_at"`
	Rooms          []labRunRoom `json:"rooms"`
}

type labRunRoom struct {
	RoomName   string    `json:"room_name"`
	EnteredAt  time.Time `json:"entered_at"`
	RoomNumber int       `json:"room_number"`
}

var allowedDifficulties = map[string]bool{
	"Normal": true, "Cruel": true, "Merciless": true, "Uber": true,
}

// StoreLabRun handles POST /api/lab/runs. Stores a completed lab run with
// room-by-room timing data from the desktop app timer overlay.
func StoreLabRun(repo *lab.LabRunRepository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		r.Body = http.MaxBytesReader(w, r.Body, 32768)

		var body labRunRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			jsonError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		if !allowedDifficulties[body.Difficulty] {
			jsonError(w, http.StatusBadRequest, "difficulty must be Normal, Cruel, Merciless, or Uber")
			return
		}
		if body.Strategy == "" {
			jsonError(w, http.StatusBadRequest, "strategy is required")
			return
		}
		if body.ElapsedSeconds <= 0 || body.ElapsedSeconds > 86400 {
			jsonError(w, http.StatusBadRequest, "elapsed_seconds must be 1-86400")
			return
		}
		if body.KillSeconds != nil && (*body.KillSeconds <= 0 || *body.KillSeconds > body.ElapsedSeconds) {
			jsonError(w, http.StatusBadRequest, "kill_seconds must be 1..elapsed_seconds")
			return
		}
		if body.RoomCount <= 0 {
			jsonError(w, http.StatusBadRequest, "room_count must be positive")
			return
		}
		if len(body.Rooms) > 50 {
			jsonError(w, http.StatusBadRequest, "too many rooms (max 50)")
			return
		}
		if body.StartedAt.IsZero() {
			body.StartedAt = time.Now()
		}

		dev := middleware.DeviceFromContext(r.Context())
		if dev == nil {
			jsonError(w, http.StatusUnauthorized, "device identification required")
			return
		}
		deviceID := dev.Fingerprint

		run := lab.LabRun{
			Difficulty:     body.Difficulty,
			Strategy:       body.Strategy,
			StartedAt:      body.StartedAt,
			ElapsedSeconds: body.ElapsedSeconds,
			KillSeconds:    body.KillSeconds,
			RoomCount:      body.RoomCount,
			HasGoldenDoor:  body.HasGoldenDoor,
			Rooms:          make([]lab.LabRunRoom, len(body.Rooms)),
		}
		for i, room := range body.Rooms {
			run.Rooms[i] = lab.LabRunRoom{RoomNumber: room.RoomNumber, RoomName: room.RoomName, EnteredAt: room.EnteredAt}
		}

		runID, err := repo.Store(r.Context(), deviceID, run)
		if err != nil {
			var storeErr *lab.LabRunStoreError
			if !errors.As(err, &storeErr) {
				storeErr = &lab.LabRunStoreError{Stage: lab.LabRunStageInsertRun, Err: err}
			}
			logLabRunStoreError(storeErr)
			jsonError(w, http.StatusInternalServerError, labRunStoreErrorMessage(storeErr.Stage))
			return
		}

		slog.Info("lab run: stored",
			"run_id", runID,
			"difficulty", body.Difficulty,
			"elapsed", body.ElapsedSeconds,
			"rooms", len(body.Rooms),
			"device_id", deviceID,
		)

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		if err := json.NewEncoder(w).Encode(map[string]any{
			"run_id": runID,
			"rooms":  len(body.Rooms),
		}); err != nil {
			slog.Warn("lab run: encode response", "error", err)
		}
	}
}

// labRunResponse is the summary view — intentionally omits room-level detail
// (stored in lab_run_rooms) to keep list queries fast. Room splits can be added
// via a separate endpoint or ?include=rooms param when needed.
type labRunResponse struct {
	ID             int64     `json:"id"`
	Difficulty     string    `json:"difficulty"`
	Strategy       string    `json:"strategy"`
	StartedAt      time.Time `json:"started_at"`
	ElapsedSeconds int       `json:"elapsed_seconds"`
	KillSeconds    *int      `json:"kill_seconds"`
	RoomCount      int       `json:"room_count"`
	HasGoldenDoor  bool      `json:"has_golden_door"`
}

type labRunStats struct {
	AvgSeconds     float64 `json:"avg_seconds"`
	BestSeconds    int     `json:"best_seconds"`
	AvgKillSeconds float64 `json:"avg_kill_seconds"`
	BestKillSecs   int     `json:"best_kill_seconds"`
	TotalRuns      int     `json:"total_runs"`
}

// ListLabRuns handles GET /api/lab/runs. Returns run history and aggregate
// stats for the current device, optionally filtered by difficulty.
func ListLabRuns(repo *lab.LabRunRepository) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		dev := middleware.DeviceFromContext(r.Context())
		if dev == nil {
			jsonError(w, http.StatusUnauthorized, "device identification required")
			return
		}

		difficulty := r.URL.Query().Get("difficulty")
		limit := 50
		if l := r.URL.Query().Get("limit"); l != "" {
			if n, err := strconv.Atoi(l); err == nil && n > 0 && n <= 200 {
				limit = n
			}
		}

		if !allowedDifficulties[difficulty] {
			difficulty = ""
		}

		summaries, rawStats, err := repo.List(r.Context(), dev.Fingerprint, difficulty, limit)
		if err != nil {
			var listErr *lab.LabRunListError
			if !errors.As(err, &listErr) {
				listErr = &lab.LabRunListError{Stage: lab.LabRunListStageQuery, Err: err}
			}
			slog.Error("lab runs: "+string(listErr.Stage), "error", listErr.Err)
			jsonError(w, http.StatusInternalServerError, labRunListErrorMessage(listErr.Stage))
			return
		}

		runs := make([]labRunResponse, len(summaries))
		for i, s := range summaries {
			runs[i] = labRunResponse{
				ID:             s.ID,
				Difficulty:     s.Difficulty,
				Strategy:       s.Strategy,
				StartedAt:      s.StartedAt,
				ElapsedSeconds: s.ElapsedSeconds,
				KillSeconds:    s.KillSeconds,
				RoomCount:      s.RoomCount,
				HasGoldenDoor:  s.HasGoldenDoor,
			}
		}
		stats := labRunStats{
			AvgSeconds:     math.Round(rawStats.AvgSeconds*10) / 10,
			BestSeconds:    rawStats.BestSeconds,
			AvgKillSeconds: math.Round(rawStats.AvgKillSeconds*10) / 10,
			BestKillSecs:   rawStats.BestKillSeconds,
			TotalRuns:      rawStats.TotalRuns,
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"runs":  runs,
			"stats": stats,
		}); err != nil {
			slog.Warn("lab runs: encode response", "error", err)
		}
	}
}

// logLabRunStoreError keeps the per-stage log texts the handler used before the
// transaction moved into lab.LabRunRepository.
func logLabRunStoreError(e *lab.LabRunStoreError) {
	switch e.Stage {
	case lab.LabRunStageBegin:
		slog.Error("lab run: begin tx", "error", e.Err)
	case lab.LabRunStageInsertRoom:
		slog.Error("lab run: insert room", "error", e.Err, "room", e.RoomIndex, "run", e.RunID)
	case lab.LabRunStageCommit:
		slog.Error("lab run: commit", "error", e.Err)
	default:
		slog.Error("lab run: insert run", "error", e.Err)
	}
}

// labRunStoreErrorMessage is the 500 body text for a failed store stage.
func labRunStoreErrorMessage(stage lab.LabRunStage) string {
	switch stage {
	case lab.LabRunStageInsertRoom:
		return "failed to store room"
	case lab.LabRunStageCommit:
		return "failed to commit run"
	default:
		return "failed to store run"
	}
}

// labRunListErrorMessage is the 500 body text for a failed list stage.
func labRunListErrorMessage(stage lab.LabRunListStage) string {
	switch stage {
	case lab.LabRunListStageScan, lab.LabRunListStageRows:
		return "failed to read runs"
	case lab.LabRunListStageStats:
		return "failed to compute stats"
	default:
		return "failed to query runs"
	}
}
