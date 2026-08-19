package handlers

import (
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/exchange"
)

// subscribeClaims issues a token through the handler and returns the topics its
// mercure.subscribe claim grants. The JWT is not verified here — the signature
// is the hub's business; what this file asserts is which topics were claimed.
func subscribeClaims(t *testing.T) []string {
	t.Helper()

	router := chi.NewRouter()
	router.Get("/api/mercure/token", MercureToken("test-secret-key-for-mercure-subscriber", "https://mercure.example.com/.well-known/mercure"))

	req := httptest.NewRequest(http.MethodGet, "/api/mercure/token", nil)
	w := httptest.NewRecorder()
	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}

	parts := strings.Split(resp["token"], ".")
	if len(parts) != 3 {
		t.Fatalf("JWT has %d parts, want 3", len(parts))
	}
	payloadJSON, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		t.Fatalf("decode JWT payload: %v", err)
	}

	var claims struct {
		Mercure struct {
			Subscribe []string `json:"subscribe"`
		} `json:"mercure"`
	}
	if err := json.Unmarshal(payloadJSON, &claims); err != nil {
		t.Fatalf("unmarshal JWT claims: %v", err)
	}
	return claims.Mercure.Subscribe
}

func TestMercureToken_grantsTheCurrencyExchangeUpdateTopic(t *testing.T) {
	// The browser subscribes with exactly this token: a topic missing from the
	// claim is refused by the hub, so the plays page would never learn that a
	// recompute changed the answer and would sit on its first fetch forever.
	got := subscribeClaims(t)

	for _, topic := range got {
		if topic == exchange.UpdatedTopic {
			return
		}
	}
	t.Errorf("subscriber topics %v missing %q", got, exchange.UpdatedTopic)
}

func TestMercureToken_grantsTheCurrencyExchangeUpdateTopicNotTheIngestTopic(t *testing.T) {
	// poe/collector/currency-exchange says "an hour was ingested" and is the
	// server's own input; a client that subscribed to it would refetch before
	// the recompute had run. Only the server-side update topic is claimed.
	for _, topic := range subscribeClaims(t) {
		if topic == exchange.Topic {
			t.Errorf("subscriber topics grant %q, want only the server-side %q", exchange.Topic, exchange.UpdatedTopic)
		}
	}
}
