package lab

import (
	"context"
	"time"

	"profitofexile/internal/league"
)

// LatestTwoSnapshotTimes returns the newest gem_snapshots time for scope and
// the newest distinct time before it. Only the first query reports an error;
// a failed or empty second query yields a zero prev, which callers treat as
// "interval unknown".
func (r *Repository) LatestTwoSnapshotTimes(ctx context.Context, scope league.Scope) (last, prev time.Time, err error) {
	if err = r.pool.QueryRow(ctx,
		`SELECT time FROM gem_snapshots WHERE league = $1 ORDER BY time DESC LIMIT 1`, scope.ID(),
	).Scan(&last); err != nil {
		return time.Time{}, time.Time{}, err
	}
	_ = r.pool.QueryRow(ctx,
		`SELECT time FROM gem_snapshots WHERE league = $1 AND time < $2 ORDER BY time DESC LIMIT 1`, scope.ID(), last,
	).Scan(&prev)
	return last, prev, nil
}
