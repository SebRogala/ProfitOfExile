package bossloot

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"slices"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/league"
)

type Repository struct{ pool *pgxpool.Pool }

func NewRepository(pool *pgxpool.Pool) *Repository { return &Repository{pool: pool} }

const runColumns = `id::text, league, boss, profile_key, profile_version, run_number, date, seed, version, items`

func scanRun(row pgx.Row) (Run, error) {
	var run Run
	var raw []byte
	err := row.Scan(&run.ID, &run.League, &run.Boss, &run.ProfileKey, &run.ProfileVersion, &run.Run, &run.Date, &run.Seed, &run.Version, &raw)
	if errors.Is(err, pgx.ErrNoRows) {
		return run, ErrNotFound
	}
	if err != nil {
		return run, err
	}
	err = json.Unmarshal(raw, &run.Items)
	return run, err
}

// lockWrites orders all same-device/profile writes, including deletion and
// historical edits. The transaction lock stays on its owning connection.
func lockWrites(ctx context.Context, tx pgx.Tx, device string, p Profile) error {
	key, _ := json.Marshal([]any{"bossloot", device, p.Boss, p.ProfileKey, p.ProfileVersion})
	_, err := tx.Exec(ctx, `SELECT pg_advisory_xact_lock(hashtextextended($1,0))`, string(key))
	return err
}
func (r *Repository) Submit(ctx context.Context, scope league.Scope, device string, s Submit) (SubmitOutcome, error) {
	out := SubmitOutcome{State: "saved", DroppedPositiveKeys: []string{}}
	if err := scope.Validate(); err != nil {
		return out, err
	}
	if err := s.Validate(); err != nil {
		return out, err
	}
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return out, err
	}
	defer tx.Rollback(ctx)
	if err = lockWrites(ctx, tx, device, s.Profile); err != nil {
		return out, err
	}
	latest, err := scanRun(tx.QueryRow(ctx, `SELECT `+runColumns+` FROM boss_loot_runs WHERE device_id=$1 AND boss=$2 AND profile_key=$3 AND profile_version=$4 ORDER BY run_number DESC LIMIT 1 FOR UPDATE`, device, s.Boss, s.ProfileKey, s.ProfileVersion))
	if err != nil && !errors.Is(err, ErrNotFound) {
		return out, err
	}
	// A competing first capture must re-read and compose a versioned effective
	// merge in Rust. Only forceNew may append the immediately repeated seed.
	if s.Mode == "new" && err == nil && latest.League == scope.ID() && latest.Seed != nil && s.Seed != nil && *latest.Seed == *s.Seed {
		return out, ErrConflict
	}
	if s.Mode == "merge" {
		if errors.Is(err, ErrNotFound) || latest.ID != s.Target.ID || latest.Version != s.Target.ExpectedVersion || latest.League != scope.ID() || latest.Seed == nil || *latest.Seed != *s.Seed {
			return out, ErrConflict
		}
		if !sameCounted(latest.Items.Counted, s.Items.Counted) {
			return out, fmt.Errorf("%w: historical counted set cannot change", ErrInvalid)
		}
		for _, key := range s.IncomingPositiveKeys {
			if !slices.Contains(latest.Items.Counted, key) {
				out.DroppedPositiveKeys = append(out.DroppedPositiveKeys, key)
			}
		}
		slices.Sort(out.DroppedPositiveKeys)
		out.Run, err = updateRun(ctx, tx, scope, device, latest.ID, latest.Version, Items{Quantities: s.Items.Quantities, Counted: latest.Items.Counted})
	} else {
		number := latest.Run + 1
		raw, _ := json.Marshal(s.Items)
		out.Run, err = scanRun(tx.QueryRow(ctx, `INSERT INTO boss_loot_runs (league,device_id,boss,profile_key,profile_version,run_number,date,seed,items) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING `+runColumns, scope.ID(), device, s.Boss, s.ProfileKey, s.ProfileVersion, number, time.Now().UTC(), s.Seed, raw))
	}
	if err != nil {
		return out, err
	}
	err = tx.Commit(ctx)
	return out, err
}
func sameCounted(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for _, key := range a {
		if !slices.Contains(b, key) {
			return false
		}
	}
	return true
}
func updateRun(ctx context.Context, tx pgx.Tx, scope league.Scope, device, id string, version int64, items Items) (Run, error) {
	if err := scope.Validate(); err != nil {
		return Run{}, err
	}
	raw, _ := json.Marshal(items)
	run, err := scanRun(tx.QueryRow(ctx, `UPDATE boss_loot_runs SET items=$5,version=version+1 WHERE device_id=$1 AND id=$2 AND league=$3 AND version=$4 RETURNING `+runColumns, device, id, scope.ID(), version, raw))
	if errors.Is(err, ErrNotFound) {
		err = ErrConflict
	}
	return run, err
}

// ownedMutation selects historical scope from an owned row, never client league.
// Re-reading after the profile lock makes concurrent edits/deletes observable.
func (r *Repository) ownedMutation(ctx context.Context, device, id string, version int64, items *Items) (Run, error) {
	if err := ValidateTarget(id, version); err != nil {
		return Run{}, err
	}
	if items != nil {
		if err := items.Validate(); err != nil {
			return Run{}, err
		}
	}
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return Run{}, err
	}
	defer tx.Rollback(ctx)
	owned, err := scanRun(tx.QueryRow(ctx, `SELECT `+runColumns+` FROM boss_loot_runs WHERE device_id=$1 AND id=$2`, device, id))
	if err != nil {
		return Run{}, err
	}
	if err = lockWrites(ctx, tx, device, owned.Profile); err != nil {
		return Run{}, err
	}
	owned, err = scanRun(tx.QueryRow(ctx, `SELECT `+runColumns+` FROM boss_loot_runs WHERE device_id=$1 AND id=$2 FOR UPDATE`, device, id))
	if err != nil {
		return Run{}, err
	}
	scope := league.Historical(owned.League)
	if err = scope.Validate(); err != nil {
		return Run{}, err
	}
	if owned.Version != version {
		return Run{}, ErrConflict
	}
	if items != nil {
		if !sameCounted(owned.Items.Counted, items.Counted) {
			return Run{}, fmt.Errorf("%w: historical counted set cannot change", ErrInvalid)
		}
		owned, err = updateRun(ctx, tx, scope, device, id, version, Items{Quantities: items.Quantities, Counted: owned.Items.Counted})
	} else {
		var tagRows int64
		tag, e := tx.Exec(ctx, `DELETE FROM boss_loot_runs WHERE device_id=$1 AND id=$2 AND league=$3 AND version=$4`, device, id, scope.ID(), version)
		err = e
		tagRows = tag.RowsAffected()
		if err == nil && tagRows != 1 {
			err = ErrConflict
		}
	}
	if err != nil {
		return Run{}, err
	}
	err = tx.Commit(ctx)
	return owned, err
}
func (r *Repository) EditOwned(ctx context.Context, device, id string, version int64, items Items) (Run, error) {
	return r.ownedMutation(ctx, device, id, version, &items)
}
func (r *Repository) DeleteOwned(ctx context.Context, device, id string, version int64) error {
	_, err := r.ownedMutation(ctx, device, id, version, nil)
	return err
}

// ListCumulative deliberately spans retained leagues within one profile version.
func (r *Repository) ListCumulative(ctx context.Context, device string, p Profile, page, size int) (Page, error) {
	out := Page{Page: page, PageSize: size, Items: []Run{}}
	if err := p.Validate(); err != nil {
		return out, err
	}
	if err := ValidatePagination(page, size); err != nil {
		return out, err
	}
	// A repeatable-read snapshot keeps total and page consistent during mutation.
	tx, err := r.pool.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		return out, err
	}
	defer tx.Rollback(ctx)
	err = tx.QueryRow(ctx, `SELECT count(*) FROM boss_loot_runs WHERE device_id=$1 AND boss=$2 AND profile_key=$3 AND profile_version=$4`, device, p.Boss, p.ProfileKey, p.ProfileVersion).Scan(&out.Total)
	if err != nil {
		return out, err
	}
	rows, err := tx.Query(ctx, `SELECT `+runColumns+` FROM boss_loot_runs WHERE device_id=$1 AND boss=$2 AND profile_key=$3 AND profile_version=$4 ORDER BY date DESC NULLS LAST,run_number DESC LIMIT $5 OFFSET $6`, device, p.Boss, p.ProfileKey, p.ProfileVersion, size, (page-1)*size)
	if err != nil {
		return out, err
	}
	defer rows.Close()
	for rows.Next() {
		run, e := scanRun(rows)
		if e != nil {
			return out, e
		}
		out.Items = append(out.Items, run)
	}
	if err = rows.Err(); err != nil {
		return out, err
	}
	err = tx.Commit(ctx)
	return out, err
}

// StatsCumulative counts eligibility per observation, independent of today's
// settings. nextRun always belongs to the requesting device, even in Collective.
func (r *Repository) StatsCumulative(ctx context.Context, device string, p Profile, collective bool) (Stats, error) {
	out := Stats{NextRun: 1, Items: map[string]ItemStats{}}
	if err := p.Validate(); err != nil {
		return out, err
	}
	var raw []byte
	err := r.pool.QueryRow(ctx, `WITH profile AS (
 SELECT * FROM boss_loot_runs WHERE boss=$2 AND profile_key=$3 AND profile_version=$4
 ), selected AS (SELECT * FROM profile WHERE $5 OR device_id=$1),
 item_totals AS (
 SELECT key, SUM(COALESCE((items->'quantities'->>key)::bigint,0)) AS drops, COUNT(*) AS eligible
 FROM selected CROSS JOIN LATERAL jsonb_array_elements_text(items->'counted') AS keys(key) GROUP BY key
 ) SELECT (SELECT count(*) FROM selected),(SELECT count(DISTINCT device_id) FROM selected),
 (SELECT COALESCE(MAX(run_number),0)+1 FROM profile WHERE device_id=$1),
 COALESCE((SELECT jsonb_object_agg(key,jsonb_build_object('drops',drops,'eligibleRuns',eligible)) FROM item_totals),'{}'::jsonb)`, device, p.Boss, p.ProfileKey, p.ProfileVersion, collective).Scan(&out.Runs, &out.Devices, &out.NextRun, &raw)
	if err != nil {
		return out, err
	}
	err = json.Unmarshal(raw, &out.Items)
	return out, err
}
