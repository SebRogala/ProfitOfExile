package collector

import (
	"context"

	"profitofexile/internal/mercure"
)

// PublishMercureEvent forwards to the shared mercure package. This wrapper
// preserves backward compatibility for collector and server code that already
// references collector.PublishMercureEvent.
func PublishMercureEvent(ctx context.Context, mercureURL, publisherSecret, topic, payload string) error {
	return mercure.PublishMercureEvent(ctx, mercureURL, publisherSecret, topic, payload)
}
