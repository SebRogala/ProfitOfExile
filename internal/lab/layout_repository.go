package lab

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// LayoutRepository handles persistence of daily lab layouts.
type LayoutRepository struct {
	pool *pgxpool.Pool
}

// NewLayoutRepository creates a LayoutRepository backed by the given connection pool.
func NewLayoutRepository(pool *pgxpool.Pool) *LayoutRepository {
	return &LayoutRepository{pool: pool}
}

// GetLayout retrieves the lab layout for the given difficulty and date.
// Returns nil if no layout exists for that combination.
func (r *LayoutRepository) GetLayout(ctx context.Context, difficulty string, date string) (json.RawMessage, error) {
	var layout json.RawMessage
	err := r.pool.QueryRow(ctx,
		`SELECT layout FROM lab_layouts WHERE difficulty = $1 AND date = $2`,
		difficulty, date,
	).Scan(&layout)
	if err == pgx.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("lab layout repo: get layout: %w", err)
	}
	return layout, nil
}

// SaveLayout stores a lab layout. Returns true if the layout was inserted,
// false if a layout already existed for that difficulty+date (ON CONFLICT DO NOTHING).
func (r *LayoutRepository) SaveLayout(ctx context.Context, difficulty string, date string, layout json.RawMessage) (bool, error) {
	tag, err := r.pool.Exec(ctx,
		`INSERT INTO lab_layouts (difficulty, date, layout) VALUES ($1, $2, $3)
		 ON CONFLICT (difficulty, date) DO NOTHING`,
		difficulty, date, layout,
	)
	if err != nil {
		return false, fmt.Errorf("lab layout repo: save layout: %w", err)
	}
	return tag.RowsAffected() > 0, nil
}
