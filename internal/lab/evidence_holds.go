package lab

import (
	"context"
	"time"

	"profitofexile/internal/league"
)

// evidencePoint is one gem_snapshots row the evidence hold grades.
type evidencePoint struct {
	Name        string
	Variant     string
	Time        time.Time
	Chaos       float64
	Listings    int
	SampleCount *int
}

// LoadEvidenceHolds returns gems with the sold-through evidence hold attached
// to each held key. Red-phase scaffold: it attaches nothing.
func LoadEvidenceHolds(ctx context.Context, repo *Repository, scope league.Scope, gems []GemPrice, asOf time.Time) ([]GemPrice, error) {
	return gems, nil
}

// deriveEvidenceHolds attaches each key's newest floor-SOLID point to its gem.
// Red-phase scaffold: it attaches nothing.
func deriveEvidenceHolds(gems []GemPrice, points []evidencePoint, asOf time.Time) []GemPrice {
	return gems
}
