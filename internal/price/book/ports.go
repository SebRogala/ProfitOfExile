package book

import (
	"context"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

// TradeObservations reads stored trade lookups as native observations.
type TradeObservations interface {
	ObservationsAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]resolve.Observation, error)
}

// NinjaGemObservations reads the poe.ninja gem snapshot as observations.
type NinjaGemObservations interface {
	GemObservationsAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]resolve.Observation, error)
}

// DivineRate returns the app-wide divine→chaos answer at call time.
type DivineRate func() resolve.Rate
