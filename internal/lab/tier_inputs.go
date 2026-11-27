package lab

import (
	"context"
	"fmt"
	"math"
	"sort"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

// labTrailingSnapshots is how many consecutive snapshots, ending at asOf, the
// Font classification price and depth are taken over.
const labTrailingSnapshots = 3

// labSnapshotGapMax is the widest gap between two snapshots that still counts
// as consecutive. It bounds the trailing chain and the previous tier's age.
const labSnapshotGapMax = 45 * time.Minute

// LoadTierInputs returns gems with the Font classification inputs attached:
// the trailing price and depth from history, the previous snapshot's tiers and
// each variant's last stored HIGH, all read strictly before asOf.
func LoadTierInputs(ctx context.Context, repo *Repository, scope league.Scope, gems []GemPrice, history []GemPriceHistory, asOf time.Time) ([]GemPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab: tier inputs: %w", err)
	}
	prev, prevAt, found, err := repo.previousGemTiers(ctx, scope, asOf)
	if err != nil {
		return nil, fmt.Errorf("lab: tier inputs: %w", err)
	}
	bounds, err := repo.previousTierBoundaries(ctx, scope, asOf)
	if err != nil {
		return nil, fmt.Errorf("lab: tier inputs: %w", err)
	}
	return deriveTierInputs(gems, history, prev, prevAt, found, bounds, asOf), nil
}

// deriveTierInputs attaches the classification inputs to each Font outcome.
// prev is the snapshot at prevAt (found reports whether one exists); a Font gem
// missing from it, or stored without a tier, was outside the tiers. prevBounds
// holds each variant's last stored HIGH at any age.
func deriveTierInputs(gems []GemPrice, history []GemPriceHistory, prev GemClassificationMap, prevAt time.Time, found bool, prevBounds map[string]tier.Boundaries, asOf time.Time) []GemPrice {
	points := make(map[GemClassificationKey][]PricePoint, len(history))
	for _, h := range history {
		points[GemClassificationKey{Name: h.Name, Variant: h.Variant}] = h.Points
	}
	recent := found && prevAt.Before(asOf) && asOf.Sub(prevAt) <= labSnapshotGapMax
	out := append([]GemPrice(nil), gems...)
	for i := range out {
		g := &out[i]
		g.tierPrice, g.tierDepth, g.prevTier, g.prevBoundaries = 0, 0, "", tier.Boundaries{}
		if !isFontOutcome(*g) {
			continue
		}
		key := GemClassificationKey{Name: g.Name, Variant: g.Variant}
		if chain := trailingChain(points[key], asOf); len(chain) > 0 {
			prices := make([]float64, len(chain))
			depths := make([]int, len(chain))
			for j, p := range chain {
				prices[j], depths[j] = p.Chaos, p.Listings
			}
			g.tierPrice, g.tierDepth = medianPrice(prices), lowerMedianDepth(depths)
			if g.hold != nil {
				g.tierPrice = math.Min(g.tierPrice, g.hold.LastSolidPrice)
			}
		}
		if recent {
			g.prevTier = tier.Excluded
			if c, ok := prev[key]; ok && c.Tier != "" {
				g.prevTier = c.Tier
			}
		}
		g.prevBoundaries = prevBounds[g.Variant]
	}
	return out
}

// trailingChain returns up to labTrailingSnapshots consecutive points ending
// at the point at asOf, oldest first, or nil without a point at asOf.
func trailingChain(points []PricePoint, asOf time.Time) []PricePoint {
	end := -1
	for i, p := range points {
		if p.Time.Equal(asOf) {
			end = i
		}
	}
	if end < 0 {
		return nil
	}
	start := end
	for start > 0 && end-start+1 < labTrailingSnapshots {
		gap := points[start].Time.Sub(points[start-1].Time)
		if gap <= 0 || gap > labSnapshotGapMax {
			break
		}
		start--
	}
	return points[start : end+1]
}

func medianPrice(prices []float64) float64 {
	sorted := append([]float64(nil), prices...)
	sort.Float64s(sorted)
	n := len(sorted)
	if n%2 == 1 {
		return sorted[n/2]
	}
	return (sorted[n/2-1] + sorted[n/2]) / 2
}

func lowerMedianDepth(depths []int) int {
	sorted := append([]int(nil), depths...)
	sort.Ints(sorted)
	return sorted[(len(sorted)-1)/2]
}
