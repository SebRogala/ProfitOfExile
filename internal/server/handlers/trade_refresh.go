package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/trade"
)

// tradeRefreshRequest is the expected JSON body for POST /api/internal/trade/refresh.
type tradeRefreshRequest struct {
	Variant string `json:"variant"` // e.g. "20/20"
	MinTier string `json:"minTier"` // e.g. "MID-HIGH" — only gems at or above this tier
	MinAge  string `json:"minAge"`  // e.g. "5m" — skip if cache age < this (default: 5m)
}

// tradeRefreshResponse is the JSON response for a trade refresh.
type tradeRefreshResponse struct {
	Gem     string  `json:"gem,omitempty"`
	Variant string  `json:"variant,omitempty"`
	Total   int     `json:"total,omitempty"`
	Floor   float64 `json:"floor,omitempty"`
	Skipped bool    `json:"skipped"` // true if nothing needed refreshing
	Error   string  `json:"error,omitempty"`
}

// tierRank maps tier names to numeric rank for >= comparison.
var tierRank = map[string]int{
	"TOP": 6, "HIGH": 5, "MID-HIGH": 4, "MID": 3, "LOW": 2, "FLOOR": 1,
}

// TradeRefresh handles POST /api/internal/trade/refresh.
// Called by the collector to trigger a single trade lookup for the oldest stale
// gem matching the given variant + tier filter. The server picks the gem, the
// collector controls the schedule.
// Protected by INTERNAL_SECRET — requests without a valid X-Internal-Token are rejected.
func TradeRefresh(gate *trade.Gate, cache *trade.TradeCache, labCache *lab.Cache, syncTimeout time.Duration, internalSecret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if internalSecret != "" && r.Header.Get("X-Internal-Token") != internalSecret {
			w.WriteHeader(http.StatusForbidden)
			return
		}

		var body tradeRefreshRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeJSON(w, http.StatusBadRequest, tradeRefreshResponse{Error: "invalid JSON body"})
			return
		}

		if body.Variant == "" {
			body.Variant = "20/20"
		}
		minAge := 5 * time.Minute
		if body.MinAge != "" {
			d, err := time.ParseDuration(body.MinAge)
			if err != nil {
				writeJSON(w, http.StatusBadRequest, tradeRefreshResponse{Error: "invalid minAge: " + body.MinAge})
				return
			}
			minAge = d
		}

		minTierRank := 0
		if body.MinTier != "" {
			rank, ok := tierRank[body.MinTier]
			if !ok {
				writeJSON(w, http.StatusBadRequest, tradeRefreshResponse{Error: "unknown minTier: " + body.MinTier})
				return
			}
			minTierRank = rank
		}

		// Build tier set from lab cache.
		var tierSet map[string]bool
		if minTierRank > 0 {
			tierSet = make(map[string]bool)
			for _, t := range labCache.Trends() {
				if t.Variant != body.Variant {
					continue
				}
				if tierRank[t.PriceTier] >= minTierRank {
					tierSet[t.Name] = true
				}
			}
			if len(tierSet) == 0 {
				writeJSON(w, http.StatusOK, tradeRefreshResponse{Skipped: true})
				return
			}
		}

		// Find oldest stale gem matching filter.
		key, found := cache.OldestStale(minAge, func(key string, _ *trade.TradeLookupResult) bool {
			gem, variant := trade.ParseCacheKey(key)
			if variant != body.Variant {
				return false
			}
			// Must be a transfigured gem (contains " of ").
			if !strings.Contains(gem, " of ") {
				return false
			}
			if tierSet != nil && !tierSet[gem] {
				return false
			}
			return true
		})

		if !found {
			writeJSON(w, http.StatusOK, tradeRefreshResponse{Skipped: true})
			return
		}

		gem, variant := trade.ParseCacheKey(key)

		req := &trade.GateRequest{
			Gem:         gem,
			Variant:     variant,
			RequestID:   "collector-refresh-" + gem,
			Priority:    trade.PriorityLow,
			SubmittedAt: time.Now(),
			Result:      make(chan *trade.GateResponse, 1),
		}

		gate.Submit(req)

		select {
		case res := <-req.Result:
			if res.Error != nil {
				slog.Warn("trade refresh failed", "gem", gem, "variant", variant, "error", res.Error)
				writeJSON(w, http.StatusOK, tradeRefreshResponse{Gem: gem, Variant: variant, Error: res.Error.Error()})
				return
			}
			slog.Info("trade refresh complete", "gem", gem, "variant", variant,
				"total", res.Data.Total, "floor", res.Data.PriceFloor)
			writeJSON(w, http.StatusOK, tradeRefreshResponse{
				Gem:     gem,
				Variant: variant,
				Total:   res.Data.Total,
				Floor:   res.Data.PriceFloor,
			})
		case <-r.Context().Done():
			return // collector disconnected
		case <-time.After(syncTimeout):
			slog.Info("trade refresh: sync timeout", "gem", gem, "variant", variant)
			writeJSON(w, http.StatusAccepted, tradeRefreshResponse{Gem: gem, Variant: variant})
		}
	}
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(v); err != nil {
		slog.Warn("writeJSON: encode failed", "error", err)
	}
}
