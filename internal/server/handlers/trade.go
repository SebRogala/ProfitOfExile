package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/trade"
)

// tradeLookupRequest is the expected JSON body for POST /api/trade/lookup.
type tradeLookupRequest struct {
	Gem     string `json:"gem"`
	Variant string `json:"variant"`
	Force   bool   `json:"force"`
}

// TradeLookup handles POST /api/trade/lookup. It checks the LRU cache first
// (unless force=true), then submits a GateRequest and either returns the result
// synchronously (if it arrives within syncTimeout) or responds with 202 and a
// requestId for the client to await via Mercure.
func TradeLookup(gate *trade.Gate, cache *trade.TradeCache, syncTimeout time.Duration) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var body tradeLookupRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON body"})
			return
		}

		if body.Gem == "" || body.Variant == "" {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "gem and variant are required"})
			return
		}

		// Cache-first path (skip when force-refreshing).
		if !body.Force && cache != nil {
			if result, ok := cache.Get(trade.CacheKey(body.Gem, body.Variant)); ok {
				w.Header().Set("Content-Type", "application/json")
				if err := json.NewEncoder(w).Encode(result); err != nil {
					slog.Error("trade lookup: encode cached response", "error", err)
				}
				return
			}
		}

		requestID := middleware.GetReqID(r.Context())

		req := &trade.GateRequest{
			Gem:         body.Gem,
			Variant:     body.Variant,
			RequestID:   requestID,
			Priority:    trade.PriorityHigh,
			SubmittedAt: time.Now(),
			Result:      make(chan *trade.GateResponse, 1),
		}

		gate.Submit(req)

		// Wait up to syncTimeout for a fast-path response.
		select {
		case res := <-req.Result:
			if res.Error != nil {
				slog.Warn("trade lookup: gate error", "error", res.Error, "gem", body.Gem, "variant", body.Variant)
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadGateway)
				json.NewEncoder(w).Encode(map[string]string{"error": res.Error.Error()})
				return
			}
			w.Header().Set("Content-Type", "application/json")
			if err := json.NewEncoder(w).Encode(res.Data); err != nil {
				slog.Error("trade lookup: encode response", "error", err)
			}
		case <-time.After(syncTimeout):
			// Result not yet available — tell the client to listen on Mercure.
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusAccepted)
			json.NewEncoder(w).Encode(map[string]string{"requestId": requestID})
		}
	}
}
