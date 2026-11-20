package lab

import (
	"context"

	"profitofexile/internal/league"
)

// ExceptionalSupportCache serves the exceptional-support snapshot.
type ExceptionalSupportCache struct {
	load func(context.Context, league.Scope) (ExceptionalSupportSnapshot, error)
}

// NewExceptionalSupportCache serves load's snapshot.
func NewExceptionalSupportCache(load func(context.Context, league.Scope) (ExceptionalSupportSnapshot, error), cache *Cache) *ExceptionalSupportCache {
	return &ExceptionalSupportCache{load: load}
}

// ExceptionalSupportPrices returns the league's exceptional-support snapshot.
func (c *ExceptionalSupportCache) ExceptionalSupportPrices(ctx context.Context, scope league.Scope) (ExceptionalSupportSnapshot, error) {
	return c.load(ctx, scope)
}
