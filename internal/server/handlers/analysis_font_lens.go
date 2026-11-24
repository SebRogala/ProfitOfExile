package handlers

import (
	"context"
	"encoding/json"
	"net/url"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// Legacy Feeder and EV remain the trade quote and its EV for older desktops.
type lensCraftResponse struct {
	Outcome    lensFeeder         `json:"outcome"`
	Feeder     lensFeeder         `json:"feeder"`
	EV         *float64           `json:"ev"`
	Feeders    []lensFeederChoice `json:"feeders"`
	Cheapest   *int               `json:"cheapest"`
	EVCheapest *float64           `json:"evCheapest"`
}

type lensFeeder struct {
	trade.Quote
	TradeURL string `json:"tradeUrl"`
}

type lensFeederChoice struct {
	Source    string     `json:"source"`
	Name      string     `json:"name"`
	Chaos     *float64   `json:"chaos"`
	FetchedAt *time.Time `json:"fetchedAt"`
	TradeURL  string     `json:"tradeUrl"`
}

func lensCraft(ctx context.Context, tradeCache *trade.TradeCache, feeders exceptionalSupportPricer, scope league.Scope, now time.Time) (lensCraftResponse, error) {
	var snap lab.ExceptionalSupportSnapshot
	var loadErr error
	if feeders != nil {
		snap, loadErr = feeders.ExceptionalSupportPrices(ctx, scope)
		if loadErr != nil {
			snap = lab.ExceptionalSupportSnapshot{}
		}
	}
	var resp lensCraftResponse
	for _, target := range trade.FixedTargets() {
		quote := lensFeeder{TradeURL: target.TradeURL(scope.ID())}
		if tradeCache != nil {
			result, _ := tradeCache.For(scope).Get(target.Key())
			quote.Quote = trade.UsableQuote(result, now)
		}
		switch target.Role {
		case trade.FixedTargetRoleOutcome:
			resp.Outcome = quote
		case trade.FixedTargetRoleFeeder:
			resp.Feeder = quote
		}
	}
	resp.EV = lab.LensCraftEV(resp.Outcome.Chaos, resp.Feeder.Chaos)
	resp.Feeders = []lensFeederChoice{{
		Source: "trade", Name: "Exceptional Support Gem lv 3+",
		Chaos: resp.Feeder.Chaos, FetchedAt: resp.Feeder.FetchedAt, TradeURL: resp.Feeder.TradeURL,
	}}
	// A failed snapshot leaves the combined comparison unpriced; legacy trade quotes remain available.
	if loadErr != nil {
		resp.Feeders[0].Chaos = nil
	}
	var asOf *time.Time
	if !snap.AsOf.IsZero() {
		at := snap.AsOf.UTC()
		asOf = &at
	}
	for _, price := range snap.Prices() {
		search, err := lensExceptionalTradeURL(price.Name, scope)
		if err != nil {
			return lensCraftResponse{}, err
		}
		resp.Feeders = append(resp.Feeders, lensFeederChoice{
			Source: lab.ExceptionalSupportSource, Name: price.Name,
			Chaos: price.Chaos, FetchedAt: asOf, TradeURL: search,
		})
	}
	for i, feeder := range resp.Feeders {
		if feeder.Chaos != nil && (resp.Cheapest == nil || *feeder.Chaos < *resp.Feeders[*resp.Cheapest].Chaos) {
			index := i
			resp.Cheapest = &index
		}
	}
	if resp.Cheapest != nil {
		resp.EVCheapest = lab.LensCraftEV(resp.Outcome.Chaos, resp.Feeders[*resp.Cheapest].Chaos)
	}
	return resp, loadErr
}

func lensExceptionalTradeURL(name string, scope league.Scope) (string, error) {
	body, err := json.Marshal(map[string]any{
		"query": map[string]any{
			"type":   name,
			"status": map[string]string{"option": "securable"},
			"filters": map[string]any{
				"misc_filters": map[string]any{"filters": map[string]any{
					"gem_level": map[string]int{"min": 3, "max": 3},
				}},
				"trade_filters": map[string]any{"filters": map[string]any{
					"sale_type": map[string]string{"option": "priced"},
				}},
			},
		},
		"sort": map[string]string{"price": "asc"},
	})
	if err != nil {
		return "", err
	}
	return "https://www.pathofexile.com/trade/search/" + url.PathEscape(scope.ID()) + "?q=" + url.QueryEscape(string(body)), nil
}
