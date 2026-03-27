package handlers

import (
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/trade"
)

// makeCachedResult builds a minimal TradeLookupResult for cache tests.
func makeCachedResult(gem, variant string) *trade.TradeLookupResult {
	return &trade.TradeLookupResult{
		Gem:         gem,
		Variant:     variant,
		Total:       42,
		PriceFloor:  10.0,
		PriceCeiling: 50.0,
		PriceSpread: 40.0,
		MedianTop10: 25.0,
		Listings:    []trade.TradeListingDetail{{Price: 10.0, Currency: "chaos", Account: "test"}},
		Signals: trade.TradeSignals{
			SellerConcentration: "NORMAL",
			CheapestStaleness:   "FRESH",
			UniqueAccounts:      1,
		},
		FetchedAt: time.Now(),
	}
}

// tradeRouter builds a chi router with the RequestID middleware and the trade
// lookup route, matching the production wiring in server.go.
func tradeRouter(gate *trade.Gate, cache *trade.TradeCache, syncTimeout time.Duration) http.Handler {
	r := chi.NewRouter()
	r.Use(middleware.RequestID)
	r.Post("/api/trade/lookup", TradeLookup(gate, cache, syncTimeout))
	return r
}

func TestTradeLookup_CacheHit(t *testing.T) {
	cache := trade.NewTradeCache(10)
	result := makeCachedResult("Vaal Grace", "21")
	cache.Set(trade.CacheKey("Vaal Grace", "21"), result)

	// Gate is non-nil but we never expect it to be called because the cache
	// serves the request. We use a real gate with nil dependencies — if the
	// handler incorrectly submits to it, the test will hang or panic, which is
	// the behaviour we want to detect.
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

	router := tradeRouter(gate, cache, 50*time.Millisecond)

	body := `{"gem":"Vaal Grace","variant":"21"}`
	req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var got trade.TradeLookupResult
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if got.Gem != "Vaal Grace" {
		t.Errorf("Gem = %q, want %q", got.Gem, "Vaal Grace")
	}
	if got.Total != 42 {
		t.Errorf("Total = %d, want 42", got.Total)
	}
}

func TestTradeLookup_FastPath(t *testing.T) {
	cache := trade.NewTradeCache(10)
	cfg := trade.TradeConfig{MaxQueueWait: 5 * time.Second}
	gate := trade.NewGate(cfg, trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

	// Simulate the gate delivering a result by intercepting Submit. We cannot
	// easily mock Submit on a concrete Gate, so instead we pre-populate the
	// result channel in a goroutine that watches for submissions.
	router := tradeRouter(gate, cache, 2*time.Second)

	body := `{"gem":"Empower Support","variant":"4","force":true}`
	req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	// The handler calls gate.Submit which puts the request on the high channel.
	// We drain it from the channel and deliver a result before the sync timeout.
	go func() {
		gateReq := <-gate.HighChan()
		gateReq.Result <- &trade.GateResponse{
			Data: makeCachedResult("Empower Support", "4"),
		}
	}()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var got trade.TradeLookupResult
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if got.Gem != "Empower Support" {
		t.Errorf("Gem = %q, want %q", got.Gem, "Empower Support")
	}
}

func TestTradeLookup_WaitPath(t *testing.T) {
	cache := trade.NewTradeCache(10)
	cfg := trade.TradeConfig{MaxQueueWait: 5 * time.Second}
	gate := trade.NewGate(cfg, trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

	// Use a very short sync timeout so the handler gives up quickly.
	router := tradeRouter(gate, cache, 10*time.Millisecond)

	body := `{"gem":"Empower Support","variant":"4"}`
	req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	// Drain the gate channel so the goroutine from Submit does not leak.
	go func() {
		select {
		case r := <-gate.HighChan():
			r.Result <- &trade.GateResponse{Error: nil, Data: makeCachedResult("x", "1")}
		case <-time.After(time.Second):
		}
	}()

	if w.Code != http.StatusAccepted {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusAccepted, w.Body.String())
	}

	var got map[string]string
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if got["requestId"] == "" {
		t.Error("expected non-empty requestId in 202 response")
	}
}

func TestTradeLookup_InvalidJSON(t *testing.T) {
	cache := trade.NewTradeCache(10)
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

	router := tradeRouter(gate, cache, 50*time.Millisecond)

	req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(`{not json`))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusBadRequest)
	}

	var got map[string]string
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if !strings.Contains(got["error"], "invalid JSON") {
		t.Errorf("error = %q, want mention of invalid JSON", got["error"])
	}
}

func TestTradeLookup_EmptyGem(t *testing.T) {
	cache := trade.NewTradeCache(10)
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

	router := tradeRouter(gate, cache, 50*time.Millisecond)

	tests := []struct {
		name string
		body string
	}{
		{"empty gem", `{"gem":"","variant":"4"}`},
		{"empty variant", `{"gem":"Empower Support","variant":""}`},
		{"both empty", `{"gem":"","variant":""}`},
		{"missing fields", `{}`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(tt.body))
			req.Header.Set("Content-Type", "application/json")
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
			}
		})
	}
}

func TestTradeLookup_TradeDisabled(t *testing.T) {
	// When TradeGate is nil, the route is not registered. chi returns 405 for
	// a POST to an unregistered path.
	r := chi.NewRouter()
	r.Use(middleware.RequestID)
	// No trade route registered — simulates trade disabled.

	body := `{"gem":"Vaal Grace","variant":"21"}`
	req := httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	// chi returns 405 Method Not Allowed when the path exists for other methods,
	// or 404 when it does not exist at all. With no routes at /api/trade/lookup,
	// we expect 404.
	if w.Code != http.StatusNotFound && w.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want 404 or 405", w.Code)
	}
}

func TestMercureToken_IncludesTradeTopics(t *testing.T) {
	subscriberKey := "test-secret-key-for-mercure-subscriber"
	publicURL := "https://mercure.example.com/.well-known/mercure"

	router := chi.NewRouter()
	router.Get("/api/mercure/token", MercureToken(subscriberKey, publicURL))

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

	token := resp["token"]
	if token == "" {
		t.Fatal("expected non-empty token")
	}

	// Decode JWT payload (second segment).
	parts := strings.Split(token, ".")
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

	// Verify explicit subscribe topics (analysis, trade, desktop).
	wantTopics := []string{
		"poe/analysis/updated",
		"poe/trade/results",
		"poe/desktop/{pair}",
	}
	for _, want := range wantTopics {
		found := false
		for _, got := range claims.Mercure.Subscribe {
			if got == want {
				found = true
				break
			}
		}
		if !found {
			t.Errorf("subscriber topics %v missing %q", claims.Mercure.Subscribe, want)
		}
	}
}
