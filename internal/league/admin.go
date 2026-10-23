package league

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Administration refusals leave the caller's transaction usable.
var (
	ErrInvalidLeagueID    = errors.New("league id must be non-empty and have no surrounding whitespace")
	ErrLeagueExists       = errors.New("league id or display name already exists")
	ErrLeagueNotFound     = errors.New("league not found")
	ErrAlreadyActive      = errors.New("league is already active")
	ErrNotPrepared        = errors.New("league is not prepared")
	ErrCollectorRunning   = errors.New("collector holds the runtime lock")
	ErrAdministrationBusy = errors.New("another league administration transaction is running")
)

// PrepareInput registers the exact upstream identifier and optional start time.
type PrepareInput struct {
	ID          string
	DisplayName string
	StartsAt    *time.Time
}

// LeagueRow carries the control row, including nullable lifecycle timestamps.
type LeagueRow struct {
	ID              string
	DisplayName     string
	StartsAt        *time.Time
	EndsAt          *time.Time
	CollectionState string
	PreparedAt      *time.Time
	ActivatedAt     *time.Time
	ArchivedAt      *time.Time
	CreatedAt       time.Time
	UpdatedAt       time.Time
}

// Activation records the runtime selection before and after a prepared league
// becomes collecting and the outgoing league becomes archived.
type Activation struct {
	FromID       string
	FromRevision int64
	ToID         string
	ToRevision   int64
}

// Status reports runtime selection, registered leagues and optional counts
// indexed by table and league. Rows is nil when counts were not requested.
type Status struct {
	Active   string
	Revision int64
	Leagues  []LeagueRow
	Rows     map[string]map[string]int64
}

// RunAdmin owns the commit boundary for an administration operation. Dry runs
// execute the operation and roll it back, leaving its returned values available
// to the caller for rendering.
func RunAdmin(ctx context.Context, pool *pgxpool.Pool, dryRun bool, op func(pgx.Tx) error) error {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("begin league administration: %w", err)
	}
	// Cleanup also runs after a cancelled operation or panic. Rollback after a
	// completed transaction is a no-op; its error must not mask the op's error.
	defer tx.Rollback(context.Background())
	if err := op(tx); err != nil {
		return err
	}
	if dryRun {
		if err := tx.Rollback(ctx); err != nil {
			return fmt.Errorf("roll back league administration dry run: %w", err)
		}
		return nil
	}
	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("commit league administration: %w", err)
	}
	return nil
}

// guard serializes administration for the transaction's lifetime. Mutations
// that change the runtime selection or remove data also exclude the collector.
func guard(ctx context.Context, tx pgx.Tx, collectorMustBeStopped bool) error {
	var acquired bool
	if err := tx.QueryRow(ctx, `SELECT pg_try_advisory_xact_lock($1)`, AdministrationLockKey()).Scan(&acquired); err != nil {
		return fmt.Errorf("try league administration lock: %w", err)
	}
	if !acquired {
		return ErrAdministrationBusy
	}
	if collectorMustBeStopped {
		if err := tx.QueryRow(ctx, `SELECT pg_try_advisory_xact_lock($1)`, RuntimeLockKey()).Scan(&acquired); err != nil {
			return fmt.Errorf("try league runtime lock: %w", err)
		}
		if !acquired {
			return ErrCollectorRunning
		}
	}
	return nil
}

const leagueRowColumns = `id, display_name, starts_at, ends_at, collection_state,
	prepared_at, activated_at, archived_at, created_at, updated_at`

func scanLeagueRow(row pgx.Row) (LeagueRow, error) {
	var league LeagueRow
	err := row.Scan(&league.ID, &league.DisplayName, &league.StartsAt, &league.EndsAt,
		&league.CollectionState, &league.PreparedAt, &league.ActivatedAt,
		&league.ArchivedAt, &league.CreatedAt, &league.UpdatedAt)
	return league, err
}

// Prepare registers an inert league without changing the active selection.
func Prepare(ctx context.Context, tx pgx.Tx, input PrepareInput) (LeagueRow, error) {
	if err := guard(ctx, tx, false); err != nil {
		return LeagueRow{}, err
	}
	if input.ID == "" || input.ID != strings.TrimSpace(input.ID) {
		return LeagueRow{}, ErrInvalidLeagueID
	}
	if input.DisplayName == "" {
		input.DisplayName = input.ID
	}
	// Omitting a conflict target covers both the id and display_name constraints
	// without a uniqueness error that would abort the caller's transaction.
	row, err := scanLeagueRow(tx.QueryRow(ctx, `
		INSERT INTO leagues (id, display_name, collection_state, prepared_at, starts_at)
		VALUES ($1, $2, 'prepared', NOW(), $3)
		ON CONFLICT DO NOTHING
		RETURNING `+leagueRowColumns, input.ID, input.DisplayName, input.StartsAt))
	if errors.Is(err, pgx.ErrNoRows) {
		return LeagueRow{}, ErrLeagueExists
	}
	if err != nil {
		return LeagueRow{}, fmt.Errorf("prepare league %q: %w", input.ID, err)
	}
	return row, nil
}

// Activate promotes a prepared league and archives the outgoing league in the
// caller's transaction. Running processes pick up the selection on restart.
func Activate(ctx context.Context, tx pgx.Tx, id string) (Activation, error) {
	if err := guard(ctx, tx, true); err != nil {
		return Activation{}, err
	}
	var result Activation
	if err := tx.QueryRow(ctx, `
		SELECT active_league, revision FROM runtime_config
		WHERE singleton = TRUE FOR UPDATE`).Scan(&result.FromID, &result.FromRevision); err != nil {
		return Activation{}, fmt.Errorf("read league runtime for activation: %w", err)
	}
	var state string
	err := tx.QueryRow(ctx, `SELECT collection_state FROM leagues WHERE id = $1 FOR UPDATE`, id).Scan(&state)
	if errors.Is(err, pgx.ErrNoRows) {
		return Activation{}, ErrLeagueNotFound
	}
	if err != nil {
		return Activation{}, fmt.Errorf("read activation target %q: %w", id, err)
	}
	if id == result.FromID {
		return Activation{}, ErrAlreadyActive
	}
	if state != "prepared" {
		return Activation{}, ErrNotPrepared
	}
	if _, err := tx.Exec(ctx, `
		UPDATE leagues SET collection_state = 'collecting', activated_at = NOW(), updated_at = NOW()
		WHERE id = $1`, id); err != nil {
		return Activation{}, fmt.Errorf("activate league %q: %w", id, err)
	}
	if err := tx.QueryRow(ctx, `
		UPDATE runtime_config SET active_league = $1, revision = revision + 1, updated_at = NOW()
		WHERE singleton = TRUE RETURNING revision`, id).Scan(&result.ToRevision); err != nil {
		return Activation{}, fmt.Errorf("update active league selection: %w", err)
	}
	if _, err := tx.Exec(ctx, `
		UPDATE leagues SET collection_state = 'archived', archived_at = NOW(),
			ends_at = COALESCE(ends_at, NOW()), updated_at = NOW()
		WHERE id = $1`, result.FromID); err != nil {
		return Activation{}, fmt.Errorf("archive outgoing league %q: %w", result.FromID, err)
	}
	result.ToID = id
	return result, nil
}

// ReadStatus lists registered leagues and optionally scans every scoped table
// for counts, including rows belonging to leagues other than the active one.
func ReadStatus(ctx context.Context, tx pgx.Tx, withCounts bool) (Status, error) {
	if err := guard(ctx, tx, false); err != nil {
		return Status{}, err
	}
	var status Status
	if err := tx.QueryRow(ctx, `SELECT active_league, revision FROM runtime_config WHERE singleton = TRUE`).
		Scan(&status.Active, &status.Revision); err != nil {
		return Status{}, fmt.Errorf("read league runtime status: %w", err)
	}
	rows, err := tx.Query(ctx, `SELECT `+leagueRowColumns+` FROM leagues ORDER BY id`)
	if err != nil {
		return Status{}, fmt.Errorf("list leagues: %w", err)
	}
	status.Leagues = make([]LeagueRow, 0)
	for rows.Next() {
		row, err := scanLeagueRow(rows)
		if err != nil {
			rows.Close()
			return Status{}, fmt.Errorf("scan league status: %w", err)
		}
		status.Leagues = append(status.Leagues, row)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return Status{}, fmt.Errorf("list leagues: %w", err)
	}
	if withCounts {
		status.Rows = make(map[string]map[string]int64)
		for _, table := range ScopedTables() {
			counts, err := readLeagueCounts(ctx, tx, table)
			if err != nil {
				return Status{}, err
			}
			status.Rows[table] = counts
		}
	}
	return status, nil
}

func readLeagueCounts(ctx context.Context, tx pgx.Tx, table string) (map[string]int64, error) {
	rows, err := tx.Query(ctx, `SELECT league, count(*) FROM `+pgx.Identifier{table}.Sanitize()+` GROUP BY league`)
	if err != nil {
		return nil, fmt.Errorf("count league rows in %s: %w", table, err)
	}
	defer rows.Close()
	counts := make(map[string]int64)
	for rows.Next() {
		var id string
		var count int64
		if err := rows.Scan(&id, &count); err != nil {
			return nil, fmt.Errorf("scan league count in %s: %w", table, err)
		}
		counts[id] = count
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("count league rows in %s: %w", table, err)
	}
	return counts, nil
}
