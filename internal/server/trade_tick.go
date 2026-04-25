package server

import (
	"context"
	"encoding/json"
	"log/slog"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/trade"
)

// TradeTickPayload is the wire format for poe/collector/trade-tick events.
// Variant defaults to "20/20"; MinAge defaults to 5 minutes; MinTier is
// optional and limits the pick to gems classified at or above the named tier.
type TradeTickPayload struct {
	Variant string `json:"variant"`
	MinTier string `json:"minTier"`
	MinAge  string `json:"minAge"`
}

// tradeTickTierRank maps tier names to numeric rank for >= comparison.
var tradeTickTierRank = map[string]int{
	"TOP": 6, "HIGH": 5, "MID-HIGH": 4, "MID": 3, "LOW": 2, "FLOOR": 1,
}

// HandleTradeTick processes a single poe/collector/trade-tick event:
// pick the oldest stale transfigured gem matching the variant + tier
// filter and submit it to the trade gate. The submit outcome is logged
// inside trade.SubmitRefresh; there is no reply path here since Mercure
// is fire-and-forget.
func HandleTradeTick(ctx context.Context, gate *trade.Gate, cache *trade.TradeCache, labCache *lab.Cache, raw []byte) {
	var p TradeTickPayload
	if err := json.Unmarshal(raw, &p); err != nil {
		slog.Warn("trade tick: invalid payload", "error", err)
		return
	}

	if p.Variant == "" {
		p.Variant = "20/20"
	}
	minAge := 5 * time.Minute
	if p.MinAge != "" {
		d, err := time.ParseDuration(p.MinAge)
		if err != nil {
			slog.Warn("trade tick: invalid minAge", "minAge", p.MinAge, "error", err)
			return
		}
		minAge = d
	}

	minTierRank := 0
	if p.MinTier != "" {
		r, ok := tradeTickTierRank[p.MinTier]
		if !ok {
			slog.Warn("trade tick: unknown minTier", "minTier", p.MinTier)
			return
		}
		minTierRank = r
	}

	var tierSet map[string]bool
	if minTierRank > 0 {
		tierSet = make(map[string]bool)
		signals := labCache.GemSignals()
		for _, s := range signals {
			if s.Variant != p.Variant {
				continue
			}
			if tradeTickTierRank[s.Tier] >= minTierRank {
				tierSet[s.Name] = true
			}
		}
		if len(tierSet) == 0 {
			// Either signals haven't been computed yet (cold start), or no gem
			// for this variant currently meets the requested tier. Either way,
			// the operator probably wants to see this when investigating why
			// trade ticks aren't refreshing anything.
			slog.Debug("trade tick: no gems at or above tier",
				"variant", p.Variant,
				"minTier", p.MinTier,
				"signal_count", len(signals),
			)
			return
		}
	}

	key, found := cache.OldestStale(minAge, func(key string, _ *trade.TradeLookupResult) bool {
		gem, variant := trade.ParseCacheKey(key)
		if variant != p.Variant {
			return false
		}
		if !strings.Contains(gem, " of ") {
			return false
		}
		if tierSet != nil && !tierSet[gem] {
			return false
		}
		return true
	})
	if !found {
		// Steady-state outcome: cache fresh, nothing to refresh. Logged at
		// Debug to help diagnose unexpectedly empty caches.
		slog.Debug("trade tick: no stale entry to refresh",
			"variant", p.Variant,
			"minTier", p.MinTier,
			"minAge", minAge,
		)
		return
	}

	gem, variant := trade.ParseCacheKey(key)
	trade.SubmitRefresh(ctx, gate, gem, variant)
}
