package lab

import (
	"context"
	"fmt"
	"time"

	"profitofexile/internal/league"
)

func (r *Repository) LensExceptionalFeeders(ctx context.Context, scope league.Scope) (ExceptionalSupportSnapshot, error) {
	if err := scope.Validate(); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: lens exceptional feeders: %w", err)
	}
	var snapTime *time.Time
	if err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots WHERE league = $1", scope.ID()).Scan(&snapTime); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: lens feeder snapshot time: %w", err)
	}
	if snapTime == nil {
		return ExceptionalSupportSnapshot{}, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, MIN(chaos)
		FROM gem_snapshots
		WHERE league = $1 AND time = $2 AND variant IN ('3', '3c')
		  AND name = ANY($3) AND chaos IS NOT NULL
		GROUP BY name`, scope.ID(), *snapTime, ExceptionalSupportNames[:])
	if err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: query lens exceptional feeders: %w", err)
	}
	defer rows.Close()

	snap := ExceptionalSupportSnapshot{AsOf: *snapTime, Chaos: make(map[string]float64, len(ExceptionalSupportNames))}
	for rows.Next() {
		var name string
		var chaos float64
		if err := rows.Scan(&name, &chaos); err != nil {
			return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: scan lens exceptional feeder: %w", err)
		}
		snap.Chaos[name] = chaos
	}
	if err := rows.Err(); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: lens exceptional feeders rows iteration: %w", err)
	}
	return snap, nil
}
