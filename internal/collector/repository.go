package collector

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Repository handles snapshot persistence in TimescaleDB.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a snapshot repository backed by the given connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// LastGemSnapshotTime returns the most recent gem snapshot timestamp.
// Returns the zero time if no snapshots exist.
func (r *Repository) LastGemSnapshotTime(ctx context.Context) (time.Time, error) {
	var t *time.Time
	err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots").Scan(&t)
	if err != nil {
		return time.Time{}, fmt.Errorf("repo: last gem snapshot time: %w", err)
	}
	if t == nil {
		return time.Time{}, nil
	}
	return *t, nil
}

// InsertGemSnapshots batch-inserts gem snapshots in a single transaction.
// All rows share the provided timestamp for snapshot coherence.
// Returns the number of rows actually inserted (excludes conflicts).
func (r *Repository) InsertGemSnapshots(ctx context.Context, snapTime time.Time, snapshots []GemSnapshot) (int, error) {
	if len(snapshots) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("repo: insert gem snapshots: begin tx: %w", err)
	}
	defer tx.Rollback(ctx) //nolint:errcheck

	inserted := 0
	for _, s := range snapshots {
		var gemColor *string
		if s.GemColor != "" {
			gemColor = &s.GemColor
		}

		tag, err := tx.Exec(ctx,
			`INSERT INTO gem_snapshots (time, name, variant, chaos, listings, is_transfigured, gem_color)
			 VALUES ($1, $2, $3, $4, $5, $6, $7)
			 ON CONFLICT DO NOTHING`,
			snapTime, s.Name, s.Variant, s.Chaos, s.Listings, s.IsTransfigured, gemColor,
		)
		if err != nil {
			return 0, fmt.Errorf("repo: insert gem snapshot %q/%q: %w", s.Name, s.Variant, err)
		}
		inserted += int(tag.RowsAffected())
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("repo: insert gem snapshots: commit: %w", err)
	}

	return inserted, nil
}

// InsertCurrencySnapshots batch-inserts currency snapshots in a single transaction.
// All rows share the provided timestamp for snapshot coherence.
// Returns the number of rows actually inserted (excludes conflicts).
func (r *Repository) InsertCurrencySnapshots(ctx context.Context, snapTime time.Time, snapshots []CurrencySnapshot) (int, error) {
	if len(snapshots) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("repo: insert currency snapshots: begin tx: %w", err)
	}
	defer tx.Rollback(ctx) //nolint:errcheck

	inserted := 0
	for _, s := range snapshots {
		tag, err := tx.Exec(ctx,
			`INSERT INTO currency_snapshots (time, currency_id, chaos, volume, sparkline_change)
			 VALUES ($1, $2, $3, $4, $5)
			 ON CONFLICT DO NOTHING`,
			snapTime, s.CurrencyID, s.Chaos, s.Volume, s.SparklineChange,
		)
		if err != nil {
			return 0, fmt.Errorf("repo: insert currency snapshot %q: %w", s.CurrencyID, err)
		}
		inserted += int(tag.RowsAffected())
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("repo: insert currency snapshots: commit: %w", err)
	}

	return inserted, nil
}

// LatestSnapshot returns a summary of the most recent snapshot data across
// both gem and currency tables. Used by debug/health endpoints.
func (r *Repository) LatestSnapshot(ctx context.Context) (*SnapshotSummary, error) {
	summary := &SnapshotSummary{}

	// Gem stats.
	err := r.pool.QueryRow(ctx,
		`SELECT COALESCE(MAX(time), '1970-01-01'::timestamptz), COUNT(*)
		 FROM gem_snapshots
		 WHERE time = (SELECT MAX(time) FROM gem_snapshots)`,
	).Scan(&summary.LastGemTime, &summary.GemCount)
	if err != nil {
		return nil, fmt.Errorf("repo: latest snapshot gems: %w", err)
	}

	// Currency stats.
	err = r.pool.QueryRow(ctx,
		`SELECT COALESCE(MAX(time), '1970-01-01'::timestamptz), COUNT(*)
		 FROM currency_snapshots
		 WHERE time = (SELECT MAX(time) FROM currency_snapshots)`,
	).Scan(&summary.LastCurrencyTime, &summary.CurrencyCount)
	if err != nil {
		return nil, fmt.Errorf("repo: latest snapshot currency: %w", err)
	}

	return summary, nil
}

// QueryGemSnapshots returns gem snapshots from the last N hours, ordered by time
// descending. Used by debug endpoints.
func (r *Repository) QueryGemSnapshots(ctx context.Context, hours int) ([]GemSnapshot, error) {
	rows, err := r.pool.Query(ctx,
		`SELECT time, name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		        is_transfigured, COALESCE(gem_color, '')
		 FROM gem_snapshots
		 WHERE time > NOW() - make_interval(hours => $1)
		 ORDER BY time DESC, name, variant`,
		hours,
	)
	if err != nil {
		return nil, fmt.Errorf("repo: query gem snapshots: %w", err)
	}
	defer rows.Close()

	var snapshots []GemSnapshot
	for rows.Next() {
		var s GemSnapshot
		if err := rows.Scan(&s.Time, &s.Name, &s.Variant, &s.Chaos, &s.Listings, &s.IsTransfigured, &s.GemColor); err != nil {
			return nil, fmt.Errorf("repo: scan gem snapshot: %w", err)
		}
		snapshots = append(snapshots, s)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("repo: iterate gem snapshots: %w", err)
	}

	return snapshots, nil
}
