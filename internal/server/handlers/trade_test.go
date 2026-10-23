package handlers

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/go-chi/chi/v5/middleware"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// makeCachedResult builds a minimal TradeLookupResult for cache tests.
func makeCachedResult(gem, variant string) *trade.TradeLookupResult {
	return &trade.TradeLookupResult{
		Gem:          gem,
		Variant:      variant,
		Total:        42,
		PriceFloor:   10.0,
		PriceCeiling: 50.0,
		PriceSpread:  40.0,
		MedianTop10:  25.0,
		Listings:     []trade.TradeListingDetail{{Price: 10.0, Currency: "chaos", Account: "test"}},
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
	r.Post("/api/trade/lookup", TradeLookup(gate, cache, league.Historical("Mirage"), syncTimeout, nil, nil))
	return r
}

func TestTradeLookup_CacheHit(t *testing.T) {
	cache := trade.NewTradeCache(10, league.Historical("Mirage"))
	result := makeCachedResult("Vaal Grace", "21")
	cache.Set(trade.CacheKey("Vaal Grace", "21"), result)

	// Gate is non-nil but we never expect it to be called because the cache
	// serves the request. We use a real gate with nil dependencies — if the
	// handler incorrectly submits to it, the test will hang or panic, which is
	// the behaviour we want to detect.
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, league.Historical("Mirage"), trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

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
	cache := trade.NewTradeCache(10, league.Historical("Mirage"))
	cfg := trade.TradeConfig{MaxQueueWait: 5 * time.Second}
	gate := trade.NewGate(cfg, league.Historical("Mirage"), trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

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
	cache := trade.NewTradeCache(10, league.Historical("Mirage"))
	cfg := trade.TradeConfig{MaxQueueWait: 5 * time.Second}
	gate := trade.NewGate(cfg, league.Historical("Mirage"), trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

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
	cache := trade.NewTradeCache(10, league.Historical("Mirage"))
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, league.Historical("Mirage"), trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

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
	cache := trade.NewTradeCache(10, league.Historical("Mirage"))
	cfg := trade.TradeConfig{MaxQueueWait: time.Second}
	gate := trade.NewGate(cfg, league.Historical("Mirage"), trade.NewRateLimiter(cfg), nil, nil, cache, func() float64 { return 212.0 }, nil)

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

// recordingTradeSubmitter answers synchronously so validation tests observe the
// submission boundary without a worker, HTTP upstream, or wall-clock waiting.
type recordingTradeSubmitter struct {
	requests []*trade.GateRequest
}

func (s *recordingTradeSubmitter) Submit(req *trade.GateRequest) {
	s.requests = append(s.requests, req)
	req.Result <- &trade.GateResponse{Data: &trade.TradeLookupResult{
		Gem: req.Gem, Variant: req.Variant, Total: 1, PriceFloor: 42,
	}}
}

func tradeValidationRequest(t *testing.T, handler http.HandlerFunc, gem, variant string, force bool) *httptest.ResponseRecorder {
	t.Helper()
	body, err := json.Marshal(tradeLookupRequest{Gem: gem, Variant: variant, Force: force})
	if err != nil {
		t.Fatalf("marshal lookup request: %v", err)
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, httptest.NewRequest(http.MethodPost, "/api/trade/lookup", strings.NewReader(string(body))))
	return w
}

func warmTradeDictionary() *lab.Cache {
	cache := lab.NewCache(league.Historical("Mirage"))
	cache.SetGemDictionary([]string{"Arc", "Vaal Grace"}, []string{"Spark of Nova"})
	return cache
}

func TestTradeLookup_UnknownGemNeverReachesGate(t *testing.T) {
	gate := &recordingTradeSubmitter{}
	handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, warmTradeDictionary(),
		func(context.Context, league.Scope, string) (bool, error) { return false, nil })
	for _, name := range []string{"junk1", "junk2", "junk3", "junk4", "junk5"} {
		w := tradeValidationRequest(t, handler, name, "20/20", true)
		var body map[string]string
		if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
			t.Fatalf("decode rejection for %q: %v", name, err)
		}
		if w.Code != http.StatusBadRequest || w.Header().Get("Content-Type") != "application/json" || body["error"] != "unknown gem name" {
			t.Fatalf("junk %q: status=%d headers=%v body=%s, want JSON unknown-gem rejection", name, w.Code, w.Header(), w.Body.String())
		}
	}
	if len(gate.requests) != 0 {
		t.Fatalf("junk names reached the gate: %+v", gate.requests)
	}
}

func TestTradeLookup_KnownNamesReachGate(t *testing.T) {
	for _, tc := range []struct {
		gem     string
		variant string
	}{
		{"Arc", "1"},
		{"Arc", "1/0"},
		{"Spark of Nova", "20/20"},
		{"Vaal Grace", "21/20"},
		{"Empower Support", "4"},
	} {
		t.Run(tc.gem+"/"+tc.variant, func(t *testing.T) {
			gate := &recordingTradeSubmitter{}
			handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, warmTradeDictionary(),
				func(_ context.Context, scope league.Scope, name string) (bool, error) {
					if scope.ID() != "Mirage" {
						t.Fatalf("name membership checked under %q, want Mirage", scope.ID())
					}
					return name == "Empower Support", nil
				})
			w := tradeValidationRequest(t, handler, tc.gem, tc.variant, true)
			if len(gate.requests) != 1 || gate.requests[0].Gem != tc.gem || gate.requests[0].Variant != tc.variant {
				t.Fatalf("submitted=%+v, want exactly %s/%s", gate.requests, tc.gem, tc.variant)
			}
			var body trade.TradeLookupResult
			if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
				t.Fatalf("decode lookup response: %v", err)
			}
			if w.Code != http.StatusOK || body.Gem != tc.gem || body.Variant != tc.variant || body.PriceFloor != 42 {
				t.Fatalf("status=%d body=%+v, want accepted known lookup", w.Code, body)
			}
		})
	}
}

func TestTradeLookup_InvalidVariantNeverReachesGate(t *testing.T) {
	for _, variant := range []string{"0", "-1", "01", "20/", "/20", "20//20", "20/020", "20/-1", "1.0", "20/20c", "21c", "20/20junk", "20/100", " 20"} {
		t.Run(variant, func(t *testing.T) {
			gate := &recordingTradeSubmitter{}
			handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, nil, nil)
			w := tradeValidationRequest(t, handler, "Arc", variant, true)
			var body map[string]string
			if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
				t.Fatalf("decode variant rejection: %v", err)
			}
			if w.Code != http.StatusBadRequest || body["error"] != "invalid variant: expected level or level/quality" || len(gate.requests) != 0 {
				t.Fatalf("variant %q: status=%d body=%v submissions=%+v, want rejection before gate", variant, w.Code, body, gate.requests)
			}
		})
	}
}

func TestTradeLookup_ColdDictionaryAllowsLookup(t *testing.T) {
	cold := lab.NewCache(league.Historical("Mirage"))
	empty := lab.NewCache(league.Historical("Mirage"))
	empty.SetGemDictionary(nil, nil)
	for _, tc := range []struct {
		name       string
		dictionary *lab.Cache
	}{
		{"missing", nil},
		{"cold", cold},
		{"warm-empty", empty},
	} {
		t.Run(tc.name, func(t *testing.T) {
			gate := &recordingTradeSubmitter{}
			handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, tc.dictionary,
				func(context.Context, league.Scope, string) (bool, error) { return false, nil })
			w := tradeValidationRequest(t, handler, "Unknown at startup", "20/20", true)
			if w.Code != http.StatusOK || len(gate.requests) != 1 || gate.requests[0].Gem != "Unknown at startup" {
				t.Fatalf("cold dictionary: status=%d submissions=%+v, want lookup allowed", w.Code, gate.requests)
			}
		})
	}
}

func TestTradeLookup_DictionaryWarmupRejectsPreviouslyUncheckedName(t *testing.T) {
	dictionary := lab.NewCache(league.Historical("Mirage"))
	gate := &recordingTradeSubmitter{}
	handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, dictionary,
		func(context.Context, league.Scope, string) (bool, error) { return false, nil })
	tradeValidationRequest(t, handler, "junk", "20/20", true)
	dictionary.SetGemDictionary([]string{"Arc"}, nil)
	w := tradeValidationRequest(t, handler, "junk", "20/20", true)
	var body map[string]string
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode warm rejection: %v", err)
	}
	if w.Code != http.StatusBadRequest || body["error"] != "unknown gem name" || len(gate.requests) != 1 {
		t.Fatalf("warmup: status=%d body=%v submissions=%d, want only the cold lookup submitted", w.Code, body, len(gate.requests))
	}
}

func TestTradeLookup_MissingNameCheckerAllowsLookup(t *testing.T) {
	gate := &recordingTradeSubmitter{}
	handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, warmTradeDictionary(), nil)
	w := tradeValidationRequest(t, handler, "Unknown without name checker", "20/20", true)
	if w.Code != http.StatusOK || len(gate.requests) != 1 || gate.requests[0].Gem != "Unknown without name checker" {
		t.Fatalf("missing name checker: status=%d submissions=%+v, want lookup allowed", w.Code, gate.requests)
	}
}

func TestTradeLookup_DictionaryQueryFailureNeverReachesGate(t *testing.T) {
	for _, lookupErr := range []error{errors.New("dictionary unavailable"), context.Canceled, context.DeadlineExceeded} {
		t.Run(lookupErr.Error(), func(t *testing.T) {
			gate := &recordingTradeSubmitter{}
			handler := TradeLookup(gate, nil, league.Historical("Mirage"), time.Second, warmTradeDictionary(),
				func(context.Context, league.Scope, string) (bool, error) { return false, lookupErr })
			w := tradeValidationRequest(t, handler, "Unknown during dictionary query failure", "20/20", true)
			var body map[string]string
			if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
				t.Fatalf("decode dictionary failure: %v", err)
			}
			if w.Code != http.StatusServiceUnavailable || body["error"] != "gem dictionary unavailable" || len(gate.requests) != 0 {
				t.Fatalf("dictionary failure: status=%d body=%v submissions=%+v, want 503 before gate", w.Code, body, gate.requests)
			}
		})
	}
}

func submitValidationRequest(t *testing.T, handler http.HandlerFunc, result trade.TradeLookupResult) *httptest.ResponseRecorder {
	t.Helper()
	body, err := json.Marshal(result)
	if err != nil {
		t.Fatalf("marshal contribution: %v", err)
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(string(body))))
	return w
}

func TestTradeSubmit_UnknownNameNeverReachesCacheOrPersistence(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := trade.NewTradeCache(10, scope)
	var persisted []trade.TradeLookupResult
	handler := tradeSubmitHandler(cache, scope, func(_ context.Context, result *trade.TradeLookupResult) bool {
		persisted = append(persisted, *result)
		return true
	}, tradeSubmitOptions{
		dictionary: warmTradeDictionary(),
		checker:    func(context.Context, league.Scope, string) (bool, error) { return false, nil },
	})
	for _, gem := range []string{"Junk of 1", "Junk of 2", "Junk of 3", "Junk of 4", "Junk of 5"} {
		w := submitValidationRequest(t, handler, trade.TradeLookupResult{
			Gem: gem, Variant: "20/20", FetchedAt: time.Date(2000, 1, 1, 0, 0, 0, 0, time.UTC),
		})
		var body map[string]string
		if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
			t.Fatalf("decode contribution rejection: %v", err)
		}
		if w.Code != http.StatusBadRequest || body["error"] != "unknown gem name" || cache.Len() != 0 || len(persisted) != 0 {
			t.Fatalf("junk %q: status=%d body=%v cache=%v persisted=%+v, want rejection before either write", gem, w.Code, body, cache.GetSnapshot(), persisted)
		}
	}
}

func TestTradeSubmit_KnownNameReachesCacheAndPersistence(t *testing.T) {
	for _, gem := range []string{"Spark of Nova", "Empower Support"} {
		t.Run(gem, func(t *testing.T) {
			scope := league.Historical("Mirage")
			cache := trade.NewTradeCache(10, scope)
			now := time.Date(2026, 10, 23, 12, 0, 0, 0, time.UTC)
			var persisted []trade.TradeLookupResult
			handler := tradeSubmitHandler(cache, scope, func(_ context.Context, result *trade.TradeLookupResult) bool {
				persisted = append(persisted, *result)
				return true
			}, tradeSubmitOptions{
				dictionary: warmTradeDictionary(),
				checker: func(_ context.Context, _ league.Scope, name string) (bool, error) {
					return name == "Empower Support", nil
				},
				now: func() time.Time { return now },
			})
			w := submitValidationRequest(t, handler, trade.TradeLookupResult{
				Gem: gem, Variant: "20/20", Total: 1, PriceFloor: 42, FetchedAt: now.Add(-time.Minute),
				Listings: []trade.TradeListingDetail{{ChaosPrice: 42}},
			})
			cached, ok := cache.Get(trade.CacheKey(gem, "20/20"))
			if w.Code != http.StatusNoContent || !ok || cached.PriceFloor != 42 || cached.MedianTop3 != 42 || !cached.FetchedAt.Equal(now.Add(-time.Minute)) {
				t.Fatalf("status=%d cache=%+v, want accepted 42c contribution with original recent timestamp", w.Code, cached)
			}
			if len(persisted) != 1 || persisted[0].Gem != gem || persisted[0].Variant != "20/20" || persisted[0].PriceFloor != 42 || !persisted[0].FetchedAt.Equal(now.Add(-time.Minute)) {
				t.Fatalf("persisted=%+v, want the accepted contribution", persisted)
			}
		})
	}
}

func TestTradeSubmit_InvalidVariantNeverReachesCacheOrPersistence(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := trade.NewTradeCache(10, scope)
	persisted := 0
	handler := tradeSubmitHandler(cache, scope, func(context.Context, *trade.TradeLookupResult) bool {
		persisted++
		return true
	}, tradeSubmitOptions{dictionary: warmTradeDictionary()})
	w := submitValidationRequest(t, handler, trade.TradeLookupResult{Gem: "Arc", Variant: "20/20junk"})
	var body map[string]string
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode variant rejection: %v", err)
	}
	if w.Code != http.StatusBadRequest || body["error"] != "invalid variant: expected level or level/quality" || cache.Len() != 0 || persisted != 0 {
		t.Fatalf("status=%d body=%v cache=%v persisted=%d, want rejection before either write", w.Code, body, cache.GetSnapshot(), persisted)
	}
}

func TestTradeSubmit_ColdDictionaryAllowsContribution(t *testing.T) {
	empty := lab.NewCache(league.Historical("Mirage"))
	empty.SetGemDictionary(nil, nil)
	for _, dictionary := range []*lab.Cache{nil, lab.NewCache(league.Historical("Mirage")), empty} {
		scope := league.Historical("Mirage")
		cache := trade.NewTradeCache(10, scope)
		persisted := 0
		handler := tradeSubmitHandler(cache, scope, func(context.Context, *trade.TradeLookupResult) bool {
			persisted++
			return true
		}, tradeSubmitOptions{
			dictionary: dictionary,
			checker:    func(context.Context, league.Scope, string) (bool, error) { return false, nil },
		})
		w := submitValidationRequest(t, handler, trade.TradeLookupResult{Gem: "Unknown at startup", Variant: "20/20"})
		cached, ok := cache.Get(trade.CacheKey("Unknown at startup", "20/20"))
		if w.Code != http.StatusNoContent || !ok || cached.Gem != "Unknown at startup" || persisted != 1 {
			t.Fatalf("cold dictionary: status=%d cached=%+v persisted=%d, want accepted contribution", w.Code, cached, persisted)
		}
	}
}

func TestTradeSubmit_TimestampClamping(t *testing.T) {
	now := time.Date(2026, 10, 23, 12, 0, 0, 0, time.UTC)
	for _, tc := range []struct {
		name      string
		fetchedAt time.Time
		want      time.Time
	}{
		{"ancient", time.Date(2000, 1, 1, 0, 0, 0, 0, time.UTC), now},
		{"future", now.Add(time.Nanosecond), now},
		{"missing", time.Time{}, now},
		{"stale-boundary", now.Add(-5 * time.Minute), now},
		{"just-fresh", now.Add(-5*time.Minute + time.Nanosecond), now.Add(-5*time.Minute + time.Nanosecond)},
		{"recent", now.Add(-time.Minute), now.Add(-time.Minute)},
		{"receipt", now, now},
	} {
		t.Run(tc.name, func(t *testing.T) {
			scope := league.Historical("Mirage")
			cache := trade.NewTradeCache(10, scope)
			var persisted *trade.TradeLookupResult
			handler := tradeSubmitHandler(cache, scope, func(_ context.Context, result *trade.TradeLookupResult) bool {
				snapshot := *result
				persisted = &snapshot
				return true
			}, tradeSubmitOptions{dictionary: warmTradeDictionary(), now: func() time.Time { return now }})
			w := submitValidationRequest(t, handler, trade.TradeLookupResult{Gem: "Spark of Nova", Variant: "20/20", FetchedAt: tc.fetchedAt})
			cached, ok := cache.Get(trade.CacheKey("Spark of Nova", "20/20"))
			if w.Code != http.StatusNoContent || !ok || !cached.FetchedAt.Equal(tc.want) || persisted == nil || !persisted.FetchedAt.Equal(tc.want) {
				t.Fatalf("status=%d cached=%+v persisted=%+v, want timestamp %v in both writes", w.Code, cached, persisted, tc.want)
			}
		})
	}
}

func TestTradeSubmit_AncientTimestampDoesNotLeadStalePicker(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := trade.NewTradeCache(10, scope)
	// Freeze the receipt clock. The existing entry is ten minutes old, so it
	// remains eligible regardless of how long this synchronous test takes.
	now := time.Now()
	otherKey := trade.CacheKey("Arc", "20/20")
	cache.Set(otherKey, &trade.TradeLookupResult{Gem: "Arc", Variant: "20/20", FetchedAt: now.Add(-10 * time.Minute)})
	handler := tradeSubmitHandler(cache, scope, nil, tradeSubmitOptions{
		dictionary: warmTradeDictionary(), now: func() time.Time { return now },
	})
	w := submitValidationRequest(t, handler, trade.TradeLookupResult{
		Gem: "Spark of Nova", Variant: "20/20", FetchedAt: now.Add(-24 * time.Hour),
	})
	key, ok := cache.OldestStale(5*time.Minute, nil)
	if w.Code != http.StatusNoContent || !ok || key != otherKey {
		t.Fatalf("status=%d oldest=%q found=%v, want the existing Arc rather than contributed ancient timestamp", w.Code, key, ok)
	}
}

func TestTradeLookup_CachedUnknownNameStillRejected(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := trade.NewTradeCache(10, scope)
	cache.Set(trade.CacheKey("Junk of cached", "20/20"), makeCachedResult("Junk of cached", "20/20"))
	gate := &recordingTradeSubmitter{}
	handler := TradeLookup(gate, cache, scope, time.Second, warmTradeDictionary(),
		func(context.Context, league.Scope, string) (bool, error) { return false, nil })
	w := tradeValidationRequest(t, handler, "Junk of cached", "20/20", false)
	var body map[string]string
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode cached-name rejection: %v", err)
	}
	if w.Code != http.StatusBadRequest || body["error"] != "unknown gem name" || len(gate.requests) != 0 {
		t.Fatalf("status=%d body=%v submitted=%+v, want rejection before cache-first response", w.Code, body, gate.requests)
	}
}

func TestTradeSubmit_PublicHandlerUsesDictionary(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := trade.NewTradeCache(10, scope)
	handler := TradeSubmit(cache, nil, scope, warmTradeDictionary(),
		func(context.Context, league.Scope, string) (bool, error) { return false, nil })
	w := submitValidationRequest(t, handler, trade.TradeLookupResult{Gem: "Junk of public", Variant: "20/20"})
	var body map[string]string
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode public-handler rejection: %v", err)
	}
	if w.Code != http.StatusBadRequest || body["error"] != "unknown gem name" || cache.Len() != 0 {
		t.Fatalf("status=%d body=%v cache=%v, want public handler to reject the unknown name", w.Code, body, cache.GetSnapshot())
	}
}
