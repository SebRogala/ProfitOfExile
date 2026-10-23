package league

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

var (
	ErrWipeActive  = errors.New("cannot wipe the active league")
	ErrNotArchived = errors.New("league is not archived")
	ErrForeignRows = errors.New("scoped tables contain other leagues; deletion requires explicit opt-in")
	// ErrTablesBusy follows a SQL lock timeout; the caller must roll back.
	ErrTablesBusy = errors.New("scoped tables are busy")
)

// wipeLockTimeout bounds the wait for the exclusive table locks.
var wipeLockTimeout = 10 * time.Second

// WipeResult describes the selected mode and its parameterized statements.
// Statements use $1 for the target league. Preview counts target rows in every
// table; Wipe counts affected rows only in delete mode. Other Wipe modes have
// nil Counts. A preview is advisory: Wipe checks again under exclusive locks.
type WipeResult struct {
	Mode       string
	Counts     map[string]int64
	Statements []string
}

// PreviewWipe counts the target's rows and reports the statements Wipe would
// select. It neither locks the data tables explicitly nor executes mutations,
// and the collector may remain running.
func PreviewWipe(ctx context.Context, tx pgx.Tx, id string, allowDelete bool) (WipeResult, error) {
	if err := guard(ctx, tx, false); err != nil {
		return WipeResult{}, err
	}
	if err := checkWipeTarget(ctx, tx, id); err != nil {
		return WipeResult{}, err
	}
	tables := ScopedTables()
	counts := make(map[string]int64, len(tables))
	var hasTarget, hasForeign bool
	for _, table := range tables {
		var count int64
		var foreign bool
		name := pgx.Identifier{table}.Sanitize()
		if err := tx.QueryRow(ctx, `SELECT count(*) FROM `+name+` WHERE league = $1`, id).Scan(&count); err != nil {
			return WipeResult{}, fmt.Errorf("preview wipe count in %s: %w", table, err)
		}
		if err := tx.QueryRow(ctx, `SELECT EXISTS (SELECT 1 FROM `+name+` WHERE league <> $1)`, id).Scan(&foreign); err != nil {
			return WipeResult{}, fmt.Errorf("preview wipe foreign rows in %s: %w", table, err)
		}
		counts[table] = count
		hasTarget = hasTarget || count > 0
		hasForeign = hasForeign || foreign
	}
	result, err := planWipe(tables, hasTarget, hasForeign, allowDelete)
	if err != nil {
		return WipeResult{}, err
	}
	result.Counts = counts
	return result, nil
}

// Wipe removes an archived league's raw scoped data in the caller's
// transaction. All scoped tables are locked before the EXISTS probes so a
// concurrent insert cannot be lost between classification and TRUNCATE.
// Counts stay outside that lock window; PreviewWipe supplies them beforehand.
func Wipe(ctx context.Context, tx pgx.Tx, id string, allowDelete bool) (WipeResult, error) {
	if err := guard(ctx, tx, true); err != nil {
		return WipeResult{}, err
	}
	if err := checkWipeTarget(ctx, tx, id); err != nil {
		return WipeResult{}, err
	}
	// SET does not accept a bind parameter. Render only the duration's integer
	// milliseconds, never caller input, into the SET LOCAL statement.
	if _, err := tx.Exec(ctx, fmt.Sprintf("SET LOCAL lock_timeout = '%dms'", wipeLockTimeout.Milliseconds())); err != nil {
		return WipeResult{}, fmt.Errorf("set wipe lock timeout: %w", err)
	}
	tables := ScopedTables()
	if _, err := tx.Exec(ctx, `LOCK TABLE `+wipeTableList(tables)+` IN ACCESS EXCLUSIVE MODE`); err != nil {
		var pgErr *pgconn.PgError
		if errors.As(err, &pgErr) && pgErr.Code == "55P03" {
			return WipeResult{}, fmt.Errorf("lock wipe tables: %w: %w", ErrTablesBusy, err)
		}
		return WipeResult{}, fmt.Errorf("lock wipe tables: %w", err)
	}
	var hasTarget, hasForeign bool
	for _, table := range tables {
		var target, foreign bool
		name := pgx.Identifier{table}.Sanitize()
		if err := tx.QueryRow(ctx, `SELECT
			EXISTS (SELECT 1 FROM `+name+` WHERE league = $1),
			EXISTS (SELECT 1 FROM `+name+` WHERE league <> $1)`, id).Scan(&target, &foreign); err != nil {
			return WipeResult{}, fmt.Errorf("probe wipe rows in %s: %w", table, err)
		}
		hasTarget = hasTarget || target
		hasForeign = hasForeign || foreign
	}
	result, err := planWipe(tables, hasTarget, hasForeign, allowDelete)
	if err != nil {
		return WipeResult{}, err
	}
	switch result.Mode {
	case "truncate":
		if _, err := tx.Exec(ctx, result.Statements[0]); err != nil {
			return WipeResult{}, fmt.Errorf("truncate archived league %q: %w", id, err)
		}
	case "delete":
		result.Counts = make(map[string]int64, len(tables))
		for i, table := range tables {
			tag, err := tx.Exec(ctx, result.Statements[i], id)
			if err != nil {
				return WipeResult{}, fmt.Errorf("delete archived league %q from %s: %w", id, table, err)
			}
			result.Counts[table] = tag.RowsAffected()
		}
	}
	return result, nil
}

// checkWipeTarget is read-only. The administration guard serializes it with
// Activate, which owns changes to runtime selection and league lifecycle state.
func checkWipeTarget(ctx context.Context, tx pgx.Tx, id string) error {
	var state string
	err := tx.QueryRow(ctx, `SELECT collection_state FROM leagues WHERE id = $1`, id).Scan(&state)
	if errors.Is(err, pgx.ErrNoRows) {
		return ErrLeagueNotFound
	}
	if err != nil {
		return fmt.Errorf("read wipe target %q: %w", id, err)
	}
	var active string
	if err := tx.QueryRow(ctx, `SELECT active_league FROM runtime_config WHERE singleton = TRUE`).Scan(&active); err != nil {
		return fmt.Errorf("read active league for wipe: %w", err)
	}
	if id == active {
		return ErrWipeActive
	}
	if state != "archived" {
		return ErrNotArchived
	}
	return nil
}

func wipeTableList(tables []string) string {
	names := make([]string, len(tables))
	for i, table := range tables {
		names[i] = pgx.Identifier{table}.Sanitize()
	}
	return strings.Join(names, ", ")
}

// planWipe is shared by preview and execution so they report the same modes
// and statement text. Only the supplied scoped registry determines table names.
func planWipe(tables []string, hasTarget, hasForeign, allowDelete bool) (WipeResult, error) {
	if !hasTarget {
		return WipeResult{Mode: "none", Statements: []string{}}, nil
	}
	if !hasForeign {
		return WipeResult{Mode: "truncate", Statements: []string{`TRUNCATE ` + wipeTableList(tables)}}, nil
	}
	if !allowDelete {
		return WipeResult{}, ErrForeignRows
	}
	statements := make([]string, len(tables))
	for i, table := range tables {
		statements[i] = `DELETE FROM ` + pgx.Identifier{table}.Sanitize() + ` WHERE league = $1`
	}
	return WipeResult{Mode: "delete", Statements: statements}, nil
}
