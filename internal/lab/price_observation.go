package lab

import (
	"context"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

// GemObservations turns one poe.ninja gem snapshot into engine observations:
// one native chaos price per line, keyed to the line's market. Lines without a
// positive price carry no observation.
func GemObservations(leagueID string, snapTime time.Time, gems []GemPrice) []resolve.Observation {
	var obs []resolve.Observation
	for _, g := range gems {
		if g.Chaos <= 0 {
			continue
		}
		key := resolve.GemKey(leagueID, g.Name, g.Variant)
		key.Corrupted = g.IsCorrupted
		obs = append(obs, resolve.Observation{
			Key:         key,
			Source:      resolve.SourceNinja,
			At:          snapTime,
			Prices:      []resolve.Money{{Amount: g.Chaos, Currency: "chaos"}},
			SampleCount: g.SampleCount,
		})
	}
	return obs
}

// GemObservationsAsOf reads the newest gem snapshot at or before asOf as
// engine observations.
func (r *Repository) GemObservationsAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]resolve.Observation, error) {
	gems, snapTime, err := r.GemPricesAsOf(ctx, scope, asOf)
	if err != nil {
		return nil, err
	}
	return GemObservations(scope.ID(), snapTime, gems), nil
}
