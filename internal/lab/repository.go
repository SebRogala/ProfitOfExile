package lab

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Repository handles analysis data persistence.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates an analysis repository.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// LatestGemPrices returns all gem prices from the most recent snapshot.
func (r *Repository) LatestGemPrices(ctx context.Context) ([]GemPrice, time.Time, error) {
	var snapTime time.Time
	err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots").Scan(&snapTime)
	if err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: latest snapshot time: %w", err)
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		       is_transfigured, is_corrupted, COALESCE(gem_color, '')
		FROM gem_snapshots
		WHERE time = $1`, snapTime)
	if err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: query latest gems: %w", err)
	}
	defer rows.Close()

	var gems []GemPrice
	for rows.Next() {
		var g GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings,
			&g.IsTransfigured, &g.IsCorrupted, &g.GemColor); err != nil {
			return nil, time.Time{}, fmt.Errorf("lab repo: scan gem: %w", err)
		}
		gems = append(gems, g)
	}
	if err := rows.Err(); err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: rows iteration: %w", err)
	}

	return gems, snapTime, nil
}

// SaveTransfigureResults batch-inserts transfigure analysis results.
func (r *Repository) SaveTransfigureResults(ctx context.Context, results []TransfigureResult) (int, error) {
	if len(results) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, r := range results {
		batch.Queue(
			`INSERT INTO transfigure_results
			 (time, base_name, transfigured_name, variant, base_price, transfigured_price,
			  roi, roi_pct, base_listings, transfigured_listings, gem_color, confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.BaseName, r.TransfiguredName, r.Variant,
			r.BasePrice, r.TransfiguredPrice, r.ROI, r.ROIPct,
			r.BaseListings, r.TransfiguredListings, r.GemColor, r.Confidence,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert transfigure result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit transfigure results: %w", err)
	}

	return inserted, nil
}

// LatestTransfigureResults returns the most recent analysis results, optionally filtered by variant.
func (r *Repository) LatestTransfigureResults(ctx context.Context, variant string, limit int) ([]TransfigureResult, error) {
	query := `
		SELECT time, base_name, transfigured_name, variant, base_price, transfigured_price,
		       roi, roi_pct, base_listings, transfigured_listings, COALESCE(gem_color, ''), confidence
		FROM transfigure_results
		WHERE time = (SELECT MAX(time) FROM transfigure_results)`
	args := []any{}

	if variant != "" {
		query += ` AND variant = $1 ORDER BY roi DESC LIMIT $2`
		args = append(args, variant, limit)
	} else {
		query += ` ORDER BY roi DESC LIMIT $1`
		args = append(args, limit)
	}

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query transfigure results: %w", err)
	}
	defer rows.Close()

	var results []TransfigureResult
	for rows.Next() {
		var r TransfigureResult
		if err := rows.Scan(&r.Time, &r.BaseName, &r.TransfiguredName, &r.Variant,
			&r.BasePrice, &r.TransfiguredPrice, &r.ROI, &r.ROIPct,
			&r.BaseListings, &r.TransfiguredListings, &r.GemColor, &r.Confidence); err != nil {
			return nil, fmt.Errorf("lab repo: scan transfigure result: %w", err)
		}
		results = append(results, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: transfigure rows iteration: %w", err)
	}

	return results, nil
}
