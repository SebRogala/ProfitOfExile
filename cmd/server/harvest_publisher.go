package main

import (
	"context"
	"log/slog"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/harvest"
)

// newHarvestUpdateDebouncer publishes the newest stored Harvest hour after a
// recompute burst. Only Harvest's notify signals it: Exchange completing first
// says nothing about whether the Harvest cache is ready.
func newHarvestUpdateDebouncer(ctx context.Context, cache *harvest.Cache, publisher marketUpdatePublisher) *exchange.Debouncer {
	return exchange.NewDebouncer(exchange.DefaultUpdateDebounce, func() {
		snapshot, warm := cache.Snapshot(exchange.DefaultHorizon)
		if !warm {
			return
		}
		payload := map[string]any{
			"topic":     harvest.UpdatedTopic,
			"priceHour": snapshot.PriceHour.UTC().Format(time.RFC3339),
			"timestamp": time.Now().UTC().Format(time.RFC3339),
		}
		if err := publisher.Publish(ctx, harvest.UpdatedTopic, payload); err != nil {
			slog.Warn("harvest: publishing the update event failed", "error", err)
		}
	})
}
