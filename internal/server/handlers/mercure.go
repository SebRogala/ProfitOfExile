package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/harvest"
	"profitofexile/internal/mercure"
)

// MercureToken returns a short-lived subscriber JWT and the public Mercure URL.
// The frontend uses this to connect to Mercure SSE directly from the browser.
func MercureToken(subscriberKey, publicURL string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if subscriberKey == "" || publicURL == "" {
			http.Error(w, `{"error":"Mercure not configured"}`, http.StatusServiceUnavailable)
			return
		}

		token, err := signSubscriberToken(subscriberKey)
		if err != nil {
			slog.Error("mercure token: sign failed", "error", err)
			http.Error(w, `{"error":"token generation failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]string{
			"token": token,
			"url":   publicURL,
		}); err != nil {
			slog.Error("mercure token: encode response", "error", err)
		}
	}
}

func signSubscriberToken(secret string) (string, error) {
	topics := []string{
		mercure.LeagueActiveTopic,
		mercure.AnalysisUpdatedTopic,
		mercure.TradeResultsTopic,
		mercure.DesktopTopicPattern,
		mercure.LabLayoutTopic,
		// The currency-exchange pillar publishes here after a recompute
		// changed the served answer; without the claim the browser's
		// subscription is rejected and the page never refetches.
		exchange.UpdatedTopic,
		// Harvest finishes independently of the exchange recompute.
		harvest.UpdatedTopic,
	}
	return mercure.SignJWT(secret, map[string]any{"subscribe": topics}, 30*time.Minute)
}
