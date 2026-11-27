package lab

import (
	"context"
	"fmt"
	"math"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/evidence"
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
// to each held key.
func LoadEvidenceHolds(ctx context.Context, repo *Repository, scope league.Scope, gems []GemPrice, asOf time.Time) ([]GemPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab: evidence holds: %w", err)
	}
	policy := GemEvidencePolicy()
	var keys []GemClassificationKey
	seen := make(map[GemClassificationKey]bool)
	for _, g := range gems {
		g.hold = nil
		depth := gemDepth(g)
		key := GemClassificationKey{Name: g.Name, Variant: g.Variant}
		if depth.PricedBy != evidence.SourceNone && evidence.Grade(depth, policy) != evidence.Solid && !seen[key] {
			keys = append(keys, key)
			seen[key] = true
		}
	}
	if len(keys) == 0 {
		return deriveEvidenceHolds(gems, nil, asOf), nil
	}
	points, err := repo.gemEvidenceHistory(ctx, scope, keys, asOf)
	if err != nil {
		return nil, fmt.Errorf("lab: evidence holds: %w", err)
	}
	return deriveEvidenceHolds(gems, points, asOf), nil
}

// deriveEvidenceHolds attaches each key's newest floor-SOLID point to its gem.
func deriveEvidenceHolds(gems []GemPrice, points []evidencePoint, asOf time.Time) []GemPrice {
	policy := GemEvidencePolicy()
	latest := make(map[GemClassificationKey]evidencePoint)
	for _, point := range points {
		age := asOf.Sub(point.Time)
		if age < 0 || age > policy.HoldWindow {
			continue
		}
		depth := gemDepth(GemPrice{Chaos: point.Chaos, Listings: point.Listings, SampleCount: point.SampleCount})
		if evidence.Grade(depth, policy) != evidence.Solid {
			continue
		}
		key := GemClassificationKey{Name: point.Name, Variant: point.Variant}
		if previous, ok := latest[key]; !ok || point.Time.After(previous.Time) {
			latest[key] = point
		}
	}
	held := append([]GemPrice(nil), gems...)
	for i := range held {
		g := &held[i]
		g.hold = nil
		g.holdRejected = false
		if g.Chaos <= 0 || evidence.Grade(gemDepth(*g), policy) == evidence.Solid {
			continue
		}
		point, ok := latest[GemClassificationKey{Name: g.Name, Variant: g.Variant}]
		if ok && g.Chaos > point.Chaos*policy.HoldBand {
			g.holdRejected = true
		}
		if ok && math.Max(g.Chaos, point.Chaos)/math.Min(g.Chaos, point.Chaos) <= policy.HoldBand {
			g.hold = &evidence.Hold{LastSolidPrice: point.Chaos, LastSolidAge: asOf.Sub(point.Time)}
			g.Chaos = math.Min(g.Chaos, point.Chaos)
		}
	}
	return held
}
