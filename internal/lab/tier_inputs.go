package lab

import (
	"context"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

// Zero-valued declarations for the POE-406 red-test phase; the implementation seat supplies behavior.
func LoadTierInputs(context.Context, *Repository, league.Scope, []GemPrice, []GemPriceHistory, time.Time) ([]GemPrice, error) {
	return nil, nil
}

func deriveTierInputs([]GemPrice, []GemPriceHistory, GemClassificationMap, time.Time, bool, map[string]tier.Boundaries, time.Time) []GemPrice {
	return nil
}

func (r *Repository) previousGemTiers(context.Context, league.Scope, time.Time) (GemClassificationMap, time.Time, bool, error) {
	return nil, time.Time{}, false, nil
}

func (r *Repository) previousTierBoundaries(context.Context, league.Scope, time.Time) (map[string]tier.Boundaries, error) {
	return nil, nil
}
