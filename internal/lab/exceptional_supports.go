package lab

import (
	"context"
	"fmt"
	"time"

	"profitofexile/internal/league"
)

// ExceptionalSupportNames is the one list of the three Divine Font
// exceptional-support outcomes, in their fixed display order. Prices are read
// at level 1 because these gems list at levels 1-4 only.
var ExceptionalSupportNames = [3]string{"Enhance Support", "Enlighten Support", "Empower Support"}

// The exceptional-support price is a poe.ninja-only definition, apart from
// ADR-027's trade-first gem price; a trade-first swap changes this file.
const (
	ExceptionalSupportSource  = "poe.ninja"
	ExceptionalSupportVariant = "1"
)

// ExceptionalSupportSnapshot holds the uncorrupted level-1 chaos prices of the
// exceptional supports from a league's latest gem snapshot. A zero AsOf means
// the league has no snapshot; a name absent from Chaos has no price.
type ExceptionalSupportSnapshot struct {
	AsOf  time.Time
	Chaos map[string]float64
}

// ExceptionalSupportPrice is one exceptional support's price; Chaos is nil
// when the snapshot has none.
type ExceptionalSupportPrice struct {
	Name  string
	Chaos *float64
}

// Prices returns exactly the three exceptional supports in
// ExceptionalSupportNames order.
func (s ExceptionalSupportSnapshot) Prices() []ExceptionalSupportPrice {
	prices := make([]ExceptionalSupportPrice, 0, len(ExceptionalSupportNames))
	for _, name := range ExceptionalSupportNames {
		p := ExceptionalSupportPrice{Name: name}
		if chaos, ok := s.Chaos[name]; ok {
			p.Chaos = &chaos
		}
		prices = append(prices, p)
	}
	return prices
}

// ExceptionalSupportPrices reads the exceptional supports' prices from the
// league's latest gem snapshot. Returns a zero snapshot and nil error when the
// league has no snapshot.
func (r *Repository) ExceptionalSupportPrices(ctx context.Context, scope league.Scope) (ExceptionalSupportSnapshot, error) {
	if err := scope.Validate(); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: exceptional support prices: %w", err)
	}
	var snapTime *time.Time
	if err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots WHERE league = $1", scope.ID()).Scan(&snapTime); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: exceptional support snapshot time: %w", err)
	}
	if snapTime == nil {
		return ExceptionalSupportSnapshot{}, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, chaos
		FROM gem_snapshots
		WHERE league = $1 AND time = $2 AND variant = $3 AND is_corrupted = false
		  AND name = ANY($4) AND chaos IS NOT NULL`,
		scope.ID(), *snapTime, ExceptionalSupportVariant, ExceptionalSupportNames[:])
	if err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: query exceptional supports: %w", err)
	}
	defer rows.Close()

	snap := ExceptionalSupportSnapshot{AsOf: *snapTime, Chaos: make(map[string]float64, len(ExceptionalSupportNames))}
	for rows.Next() {
		var name string
		var chaos float64
		if err := rows.Scan(&name, &chaos); err != nil {
			return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: scan exceptional support: %w", err)
		}
		snap.Chaos[name] = chaos
	}
	if err := rows.Err(); err != nil {
		return ExceptionalSupportSnapshot{}, fmt.Errorf("lab repo: exceptional supports rows iteration: %w", err)
	}
	return snap, nil
}
