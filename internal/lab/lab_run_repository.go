package lab

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// LabRun is one completed lab run as reported by the desktop timer overlay.
type LabRun struct {
	Difficulty     string
	Strategy       string
	StartedAt      time.Time
	ElapsedSeconds int
	KillSeconds    *int
	RoomCount      int
	HasGoldenDoor  bool
	Rooms          []LabRunRoom
}

// LabRunRoom is the entry time of one room within a lab run.
type LabRunRoom struct {
	RoomNumber int
	RoomName   string
	EnteredAt  time.Time
}

// LabRunSummary is a stored run without its room detail.
type LabRunSummary struct {
	ID             int64
	Difficulty     string
	Strategy       string
	StartedAt      time.Time
	ElapsedSeconds int
	KillSeconds    *int
	RoomCount      int
	HasGoldenDoor  bool
}

// LabRunStats aggregates a device's runs. Averages are unrounded; zero when
// there are no runs (or no kill times).
type LabRunStats struct {
	AvgSeconds      float64
	BestSeconds     int
	AvgKillSeconds  float64
	BestKillSeconds int
	TotalRuns       int
}

// LabRunStage names the step of the store transaction that failed.
type LabRunStage string

const (
	LabRunStageBegin      LabRunStage = "begin"
	LabRunStageInsertRun  LabRunStage = "insert run"
	LabRunStageInsertRoom LabRunStage = "insert room"
	LabRunStageCommit     LabRunStage = "commit"
)

// LabRunStoreError reports which stage of Store failed. RoomIndex and RunID
// are set for LabRunStageInsertRoom.
type LabRunStoreError struct {
	Stage     LabRunStage
	RoomIndex int
	RunID     int64
	Err       error
}

func (e *LabRunStoreError) Error() string {
	return fmt.Sprintf("lab run repo: %s: %v", e.Stage, e.Err)
}

func (e *LabRunStoreError) Unwrap() error { return e.Err }

// LabRunListStage names the step of List that failed.
type LabRunListStage string

const (
	LabRunListStageQuery LabRunListStage = "query"
	LabRunListStageScan  LabRunListStage = "scan"
	LabRunListStageRows  LabRunListStage = "rows iteration"
	LabRunListStageStats LabRunListStage = "stats"
)

// LabRunListError reports which stage of List failed.
type LabRunListError struct {
	Stage LabRunListStage
	Err   error
}

func (e *LabRunListError) Error() string {
	return fmt.Sprintf("lab run repo: list %s: %v", e.Stage, e.Err)
}

func (e *LabRunListError) Unwrap() error { return e.Err }

// LabRunRepository persists lab runs. lab_runs is keyed by device, not league.
type LabRunRepository struct {
	pool *pgxpool.Pool
}

// NewLabRunRepository creates a LabRunRepository backed by the given pool.
func NewLabRunRepository(pool *pgxpool.Pool) *LabRunRepository {
	return &LabRunRepository{pool: pool}
}

// Store inserts the run and its rooms in one transaction and returns the run
// id. Errors are *LabRunStoreError.
func (r *LabRunRepository) Store(ctx context.Context, deviceID string, run LabRun) (int64, error) {
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, &LabRunStoreError{Stage: LabRunStageBegin, Err: err}
	}
	defer tx.Rollback(ctx)

	var runID int64
	err = tx.QueryRow(ctx,
		`INSERT INTO lab_runs (device_id, difficulty, strategy, started_at, elapsed_seconds, kill_seconds, room_count, has_golden_door)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
		 RETURNING id`,
		deviceID,
		run.Difficulty,
		run.Strategy,
		run.StartedAt,
		run.ElapsedSeconds,
		run.KillSeconds,
		run.RoomCount,
		run.HasGoldenDoor,
	).Scan(&runID)
	if err != nil {
		return 0, &LabRunStoreError{Stage: LabRunStageInsertRun, Err: err}
	}

	for i, room := range run.Rooms {
		if _, err := tx.Exec(ctx,
			`INSERT INTO lab_run_rooms (run_id, room_number, room_name, entered_at)
			 VALUES ($1, $2, $3, $4)`,
			runID,
			room.RoomNumber,
			room.RoomName,
			room.EnteredAt,
		); err != nil {
			return 0, &LabRunStoreError{Stage: LabRunStageInsertRoom, RoomIndex: i, RunID: runID, Err: err}
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, &LabRunStoreError{Stage: LabRunStageCommit, Err: err}
	}
	return runID, nil
}

// List returns the device's newest runs (at most limit) and stats over all of
// its runs. A non-empty difficulty filters both. Errors are *LabRunListError.
// The returned slice is never nil.
func (r *LabRunRepository) List(ctx context.Context, deviceID, difficulty string, limit int) ([]LabRunSummary, LabRunStats, error) {
	query := `SELECT id, difficulty, strategy, started_at, elapsed_seconds, kill_seconds, room_count, has_golden_door
		FROM lab_runs WHERE device_id = $1`
	args := []any{deviceID}

	nextParam := 2
	if difficulty != "" {
		query += fmt.Sprintf(` AND difficulty = $%d`, nextParam)
		args = append(args, difficulty)
		nextParam++
	}
	query += fmt.Sprintf(` ORDER BY started_at DESC LIMIT $%d`, nextParam)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, LabRunStats{}, &LabRunListError{Stage: LabRunListStageQuery, Err: err}
	}
	defer rows.Close()

	runs := []LabRunSummary{}
	for rows.Next() {
		var run LabRunSummary
		if err := rows.Scan(&run.ID, &run.Difficulty, &run.Strategy, &run.StartedAt, &run.ElapsedSeconds, &run.KillSeconds, &run.RoomCount, &run.HasGoldenDoor); err != nil {
			return nil, LabRunStats{}, &LabRunListError{Stage: LabRunListStageScan, Err: err}
		}
		runs = append(runs, run)
	}
	if err := rows.Err(); err != nil {
		return nil, LabRunStats{}, &LabRunListError{Stage: LabRunListStageRows, Err: err}
	}

	// kill_seconds is the competitive metric (Izaro death), elapsed_seconds is
	// total time (including looting). COALESCE covers runs without kill_seconds.
	statsQuery := `SELECT
			COALESCE(AVG(elapsed_seconds), 0),
			COALESCE(MIN(elapsed_seconds), 0),
			COALESCE(AVG(kill_seconds), 0),
			COALESCE(MIN(kill_seconds), 0),
			COUNT(*)
		FROM lab_runs WHERE device_id = $1`
	statsArgs := []any{deviceID}
	if difficulty != "" {
		statsQuery += ` AND difficulty = $2`
		statsArgs = append(statsArgs, difficulty)
	}

	var stats LabRunStats
	if err := r.pool.QueryRow(ctx, statsQuery, statsArgs...).Scan(&stats.AvgSeconds, &stats.BestSeconds, &stats.AvgKillSeconds, &stats.BestKillSeconds, &stats.TotalRuns); err != nil {
		return nil, LabRunStats{}, &LabRunListError{Stage: LabRunListStageStats, Err: err}
	}
	return runs, stats, nil
}
