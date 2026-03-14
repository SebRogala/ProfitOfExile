package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"profitofexile/internal/collector"
)

// DebugTrigger returns a handler that publishes a fake Mercure event for
// testing the event pipeline locally. Only mount this in dev mode.
func DebugTrigger(mercureURL, mercureSecret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		endpoint := r.URL.Query().Get("endpoint")
		if endpoint == "" {
			endpoint = "gems"
		}

		topic := "poe/collector/" + endpoint
		payload, _ := json.Marshal(map[string]any{
			"league":    "Mirage",
			"endpoint":  "ninja_" + endpoint,
			"timestamp": time.Now().UTC().Format(time.RFC3339),
			"inserted":  42,
			"debug":     true,
		})

		if err := collector.PublishMercureEvent(r.Context(), mercureURL, mercureSecret, topic, string(payload)); err != nil {
			http.Error(w, "publish failed: "+err.Error(), http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"status":"published","topic":"` + topic + `"}`))
	}
}
