package trade

import (
	"context"
	"log/slog"
	"strings"
	"time"
)

// TierProvider returns gem names at or above a given tier for a specific variant.
type TierProvider func(variant string) map[string]bool

// Refresher periodically refreshes stale trade data via the Gate.
// It alternates between two strategies each tick:
//   - Tier tick: refresh the oldest MID-HIGH+ gem (20/20 only)
//   - Stale tick: refresh the oldest ANY gem (20/20 only)
type Refresher struct {
	gate     *Gate
	cache    *TradeCache
	tiers    TierProvider
	interval time.Duration
	minAge   time.Duration
	variant  string
}

// RefresherConfig holds tunables for the periodic trade refresher.
type RefresherConfig struct {
	Gate     *Gate
	Cache    *TradeCache
	Tiers    TierProvider
	Interval time.Duration // tick interval (default: 45s)
	MinAge   time.Duration // skip if cache age < this (default: 5min)
	Variant  string        // only refresh this variant (default: "20/20")
}

// NewRefresher creates a periodic trade refresher.
// Panics if Gate, Cache, or Tiers are nil.
func NewRefresher(cfg RefresherConfig) *Refresher {
	if cfg.Gate == nil {
		panic("trade.NewRefresher: Gate is required")
	}
	if cfg.Cache == nil {
		panic("trade.NewRefresher: Cache is required")
	}
	if cfg.Tiers == nil {
		panic("trade.NewRefresher: Tiers is required")
	}
	if cfg.Interval == 0 {
		cfg.Interval = 45 * time.Second
	}
	if cfg.MinAge == 0 {
		cfg.MinAge = 5 * time.Minute
	}
	if cfg.Variant == "" {
		cfg.Variant = "20/20"
	}
	return &Refresher{
		gate:     cfg.Gate,
		cache:    cfg.Cache,
		tiers:    cfg.Tiers,
		interval: cfg.Interval,
		minAge:   cfg.MinAge,
		variant:  cfg.Variant,
	}
}

// Run starts the refresh loop. Blocks until ctx is cancelled.
func (r *Refresher) Run(ctx context.Context) {
	ticker := time.NewTicker(r.interval)
	defer ticker.Stop()

	tierTick := true // alternate: true=tier, false=stale

	slog.Info("trade refresher started",
		"interval", r.interval,
		"minAge", r.minAge,
		"variant", r.variant,
	)

	for {
		select {
		case <-ctx.Done():
			slog.Info("trade refresher stopped")
			return
		case <-ticker.C:
			var key string
			var found bool

			if tierTick {
				key, found = r.pickTiered()
				if found {
					slog.Info("trade refresh: tier tick", "key", key)
				}
			} else {
				key, found = r.pickOldest()
				if found {
					slog.Info("trade refresh: stale tick", "key", key)
				}
			}
			tierTick = !tierTick

			if !found {
				continue
			}

			gem, variant := ParseCacheKey(key)
			r.submit(ctx, gem, variant)
		}
	}
}

// pickTiered finds the oldest MID-HIGH+ gem for the target variant.
func (r *Refresher) pickTiered() (string, bool) {
	highTierGems := r.tiers(r.variant)
	if len(highTierGems) == 0 {
		return "", false
	}

	return r.cache.OldestStale(r.minAge, func(key string, _ *TradeLookupResult) bool {
		gem, variant := ParseCacheKey(key)
		return variant == r.variant && highTierGems[gem]
	})
}

// pickOldest finds the oldest cached transfigured gem for the target variant.
func (r *Refresher) pickOldest() (string, bool) {
	return r.cache.OldestStale(r.minAge, func(key string, _ *TradeLookupResult) bool {
		gem, variant := ParseCacheKey(key)
		return variant == r.variant && strings.Contains(gem, " of ")
	})
}

// submit sends a low-priority gate request and waits for the result.
func (r *Refresher) submit(ctx context.Context, gem, variant string) {
	req := &GateRequest{
		Gem:         gem,
		Variant:     variant,
		RequestID:   "refresh-" + gem,
		Priority:    PriorityLow,
		SubmittedAt: time.Now(),
		Result:      make(chan *GateResponse, 1),
	}

	r.gate.Submit(req)

	select {
	case <-ctx.Done():
		return
	case resp := <-req.Result:
		if resp.Error != nil {
			slog.Warn("trade refresh failed", "gem", gem, "variant", variant, "error", resp.Error)
		} else {
			slog.Info("trade refresh complete", "gem", gem, "variant", variant,
				"total", resp.Data.Total, "floor", resp.Data.PriceFloor)
		}
	}
}

