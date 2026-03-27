package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"regexp"

	"profitofexile/internal/mercure"
)

// desktopGemsRequest is the expected JSON body for POST /api/desktop/gems.
type desktopGemsRequest struct {
	Pair    string   `json:"pair"`
	Gems    []string `json:"gems"`
	Variant string   `json:"variant"`
}

// pairPattern matches exactly 4 alphanumeric characters.
var pairPattern = regexp.MustCompile(`^[A-Za-z0-9]{4}$`)

// DesktopGems handles POST /api/desktop/gems. It validates the request, then
// publishes a Mercure event on topic "poe/desktop/{pair}" so that the web
// comparator auto-fills the detected gems via SSE.
func DesktopGems(mercureURL, mercureSecret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		r.Body = http.MaxBytesReader(w, r.Body, 4096)

		var body desktopGemsRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "invalid JSON body"})
			return
		}

		// Validate pair: required, exactly 4 alphanumeric characters.
		if !pairPattern.MatchString(body.Pair) {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "pair must be exactly 4 alphanumeric characters"})
			return
		}

		// Validate gems: required, 1-5 non-empty strings.
		if len(body.Gems) == 0 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "gems must contain 1-5 items"})
			return
		}
		if len(body.Gems) > 5 {
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusBadRequest)
			json.NewEncoder(w).Encode(map[string]string{"error": "gems must contain 1-5 items"})
			return
		}
		for _, g := range body.Gems {
			if g == "" {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "each gem name must be non-empty"})
				return
			}
		}

		// Build Mercure event payload.
		topic := "poe/desktop/" + body.Pair
		eventPayload := map[string]any{
			"type": "gems-detected",
			"gems": body.Gems,
		}
		if body.Variant != "" {
			eventPayload["variant"] = body.Variant
		}

		payloadJSON, err := json.Marshal(eventPayload)
		if err != nil {
			slog.Error("desktop gems: marshal payload", "error", err)
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusInternalServerError)
			json.NewEncoder(w).Encode(map[string]string{"error": "failed to build event payload"})
			return
		}

		if err := mercure.PublishMercureEvent(r.Context(), mercureURL, mercureSecret, topic, string(payloadJSON)); err != nil {
			slog.Error("desktop gems: publish failed", "error", err, "pair", body.Pair)
			w.Header().Set("Content-Type", "application/json")
			w.WriteHeader(http.StatusInternalServerError)
			json.NewEncoder(w).Encode(map[string]string{"error": "failed to publish event"})
			return
		}

		slog.Info("desktop gems: published", "pair", body.Pair, "gems", len(body.Gems), "topic", topic)

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]bool{"published": true})
	}
}
