package handlers

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"math"
	"net/http"
	"strconv"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

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
func StoreLabRun(pool *pgxpool.Pool) http.HandlerFunc {
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

		ctx := r.Context()
		tx, err := pool.Begin(ctx)
		if err != nil {
			slog.Error("lab run: begin tx", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to store run")
			return
		}
		defer tx.Rollback(ctx)

		var runID int64
		err = tx.QueryRow(ctx,
			`INSERT INTO lab_runs (device_id, difficulty, strategy, started_at, elapsed_seconds, kill_seconds, room_count, has_golden_door)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
			 RETURNING id`,
			deviceID,
			body.Difficulty,
			body.Strategy,
			body.StartedAt,
			body.ElapsedSeconds,
			body.KillSeconds,
			body.RoomCount,
			body.HasGoldenDoor,
		).Scan(&runID)
		if err != nil {
			slog.Error("lab run: insert run", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to store run")
			return
		}

		for i, room := range body.Rooms {
			_, err = tx.Exec(ctx,
				`INSERT INTO lab_run_rooms (run_id, room_number, room_name, entered_at)
				 VALUES ($1, $2, $3, $4)`,
				runID,
				room.RoomNumber,
				room.RoomName,
				room.EnteredAt,
			)
			if err != nil {
				slog.Error("lab run: insert room", "error", err, "room", i, "run", runID)
				jsonError(w, http.StatusInternalServerError, "failed to store room")
				return
			}
		}

		if err := tx.Commit(ctx); err != nil {
			slog.Error("lab run: commit", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to commit run")
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
func ListLabRuns(pool *pgxpool.Pool) http.HandlerFunc {
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

		ctx := r.Context()

		// Query runs
		query := `SELECT id, difficulty, strategy, started_at, elapsed_seconds, kill_seconds, room_count, has_golden_door
			FROM lab_runs WHERE device_id = $1`
		args := []any{dev.Fingerprint}

		nextParam := 2
		if difficulty != "" && allowedDifficulties[difficulty] {
			query += fmt.Sprintf(` AND difficulty = $%d`, nextParam)
			args = append(args, difficulty)
			nextParam++
		}
		query += fmt.Sprintf(` ORDER BY started_at DESC LIMIT $%d`, nextParam)
		args = append(args, limit)

		rows, err := pool.Query(ctx, query, args...)
		if err != nil {
			slog.Error("lab runs: query", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to query runs")
			return
		}
		defer rows.Close()

		var runs []labRunResponse
		for rows.Next() {
			var run labRunResponse
			if err := rows.Scan(&run.ID, &run.Difficulty, &run.Strategy, &run.StartedAt, &run.ElapsedSeconds, &run.KillSeconds, &run.RoomCount, &run.HasGoldenDoor); err != nil {
				slog.Error("lab runs: scan", "error", err)
				jsonError(w, http.StatusInternalServerError, "failed to read runs")
				return
			}
			runs = append(runs, run)
		}
		if err := rows.Err(); err != nil {
			slog.Error("lab runs: rows iteration", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to read runs")
			return
		}
		if runs == nil {
			runs = []labRunResponse{}
		}

		// Compute stats — kill_seconds is the competitive metric (Izaro death),
		// elapsed_seconds is total time (including looting). Use COALESCE for
		// backwards compat with runs that don't have kill_seconds.
		statsQuery := `SELECT
				COALESCE(AVG(elapsed_seconds), 0),
				COALESCE(MIN(elapsed_seconds), 0),
				COALESCE(AVG(kill_seconds), 0),
				COALESCE(MIN(kill_seconds), 0),
				COUNT(*)
			FROM lab_runs WHERE device_id = $1`
		statsArgs := []any{dev.Fingerprint}
		if difficulty != "" && allowedDifficulties[difficulty] {
			statsQuery += ` AND difficulty = $2`
			statsArgs = append(statsArgs, difficulty)
		}

		var stats labRunStats
		var avgRaw, avgKillRaw float64
		if err := pool.QueryRow(ctx, statsQuery, statsArgs...).Scan(&avgRaw, &stats.BestSeconds, &avgKillRaw, &stats.BestKillSecs, &stats.TotalRuns); err != nil {
			slog.Error("lab runs: stats", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to compute stats")
			return
		}
		stats.AvgSeconds = math.Round(avgRaw*10) / 10
		stats.AvgKillSeconds = math.Round(avgKillRaw*10) / 10

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"runs":  runs,
			"stats": stats,
		}); err != nil {
			slog.Warn("lab runs: encode response", "error", err)
		}
	}
}
