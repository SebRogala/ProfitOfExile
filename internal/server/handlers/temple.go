package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"

	"profitofexile/internal/league"
	"profitofexile/internal/temple"
)

// TempleMarket returns the aggregated temple market for the server's resolved
// league (POE-255).
//
// One object, no query parameters — the same shape of endpoint as MarketOverview
// beside it, and for the same reason: the desktop temple module polls one
// derived object rather than assembling several, and the raw price exports that
// would have let it do otherwise were deliberately removed (POE-150/157, see the
// comment above the analysis routes in internal/server/server.go).
//
// The league is the process-active scope and is not a parameter. A request
// cannot ask for another one: the cache is bound to this league for the
// process's lifetime (temple.Cache.For, ADR-009) and per-request league
// selection is POE-121.
//
// Served from memory, always. There is no repository fallback here, and its
// absence is the design rather than an omission: this package's read side of
// item_snapshots IS the recompute behind the cache, so a fallback query on the
// request path would repeat the recompute rather than answer something the cache
// could not. On a COLD cache — the window after every deploy, while the warm-up
// goroutine runs (docs/ANALYSIS-CACHE.md) — the answer is 200 with
// temple.EmptyMarket, matching MarketOverview's cold behaviour: the static
// recipe table and the floor rule, no observations, and asOf null so a client
// can tell that apart from a warm answer.
func TempleMarket(cache *temple.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		market, warm := cache.For(scope).Snapshot()
		if !warm {
			market = temple.EmptyMarket(scope.ID())
		}

		if err := json.NewEncoder(w).Encode(market); err != nil {
			slog.Error("temple market: encode response", "error", err)
		}
	}
}
