package main

import (
	"context"
	"encoding/json"
	"fmt"

	"profitofexile/internal/collector"
	"profitofexile/internal/league"
	"profitofexile/internal/mercure"
)

// exchangeUpdatePublisher posts the currency-exchange "answer changed" event to
// the Mercure hub.
//
// It lives here rather than in internal/exchange for the same reason its
// collector-side twin (cmd/collector/exchange_publisher.go) does: the stamp
// comes from collector.StampScope, and internal/exchange must import neither
// internal/collector nor internal/lab (see internal/exchange/doc.go and its
// boundary test). The stamp is mandatory — the server's own LeagueEventGuard
// drops any event lacking league and leagueRevision, so an unstamped update
// would be invisible to every subscriber that checks.
type exchangeUpdatePublisher struct {
	scope         league.Scope
	mercureURL    string
	mercureSecret string
}

// Publish stamps the league identity onto the payload and posts it to the hub.
// mercure.PublishMercureEvent is a no-op when the URL or the secret is empty, so
// a server without a hub costs one marshal per recompute burst and nothing else.
func (p exchangeUpdatePublisher) Publish(ctx context.Context, topic string, payload map[string]any) error {
	body, err := json.Marshal(collector.StampScope(payload, p.scope))
	if err != nil {
		return fmt.Errorf("exchange update publisher: marshal %s payload: %w", topic, err)
	}
	return mercure.PublishMercureEvent(ctx, p.mercureURL, p.mercureSecret, topic, string(body))
}
