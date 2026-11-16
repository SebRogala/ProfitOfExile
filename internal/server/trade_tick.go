package server

import (
	"context"
	"encoding/json"
	"log/slog"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

// TradeTickPayload retains legacy cadence hints for rolling deployments.
// LeagueEventGuard owns revision admission before runtime dispatch.
type TradeTickPayload struct {
	Variant  string `json:"variant"`
	MinTier  string `json:"minTier"`
	MinAge   string `json:"minAge"`
	League   string `json:"league"`
	Revision int64  `json:"leagueRevision"`
}

// HandleTradeTick validates cadence input; snapshot demand owns selection.
func HandleTradeTick(ctx context.Context, scope league.Scope, raw []byte, tick func(context.Context)) {
	var p TradeTickPayload
	if err := json.Unmarshal(raw, &p); err != nil {
		slog.Warn("trade tick: invalid payload", "error", err)
		return
	}
	if err := scope.Admit(p.League); err != nil {
		return
	}
	if p.Revision != 0 && p.Revision != scope.Revision() {
		return
	}
	if p.MinAge != "" {
		if d, err := time.ParseDuration(p.MinAge); err != nil || d < 0 {
			return
		}
	}
	if p.MinTier != "" {
		known := p.MinTier == tier.Chaotic
		for _, name := range tier.Order {
			if p.MinTier == name {
				known = true
				break
			}
		}
		if !known {
			return
		}
	}
	if ctx.Err() == nil && tick != nil {
		tick(ctx)
	}
}
