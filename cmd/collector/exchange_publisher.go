package main

import (
	"context"
	"encoding/json"
	"fmt"

	"profitofexile/internal/collector"
	"profitofexile/internal/league"
)

// stampedPublisher adapts exchange.EventPublisher onto the collector's Mercure
// publish path.
//
// It lives here rather than in internal/exchange because it needs
// collector.StampScope, and internal/exchange must not import internal/collector
// (see internal/exchange/doc.go). The stamp is mandatory: the server's league
// event guard drops any event without league and leagueRevision.
type stampedPublisher struct {
	scope         league.Scope
	mercureURL    string
	mercureSecret string
}

// Publish stamps the league identity onto the payload and posts it to the hub.
// collector.PublishMercureEvent is a no-op when the URL or the secret is empty,
// so an unconfigured hub costs a marshal and nothing else.
func (p stampedPublisher) Publish(ctx context.Context, topic string, payload map[string]any) error {
	body, err := json.Marshal(collector.StampScope(payload, p.scope))
	if err != nil {
		return fmt.Errorf("stamped publisher: marshal %s payload: %w", topic, err)
	}
	return collector.PublishMercureEvent(ctx, p.mercureURL, p.mercureSecret, topic, string(body))
}
