package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/trade"
)

// submitRouter builds a chi router with the trade submit route, matching the
// production wiring in server.go. The route is only registered when cache is
// non-nil (same nil-guard as server.go line 114-116).
func submitRouter(cache *trade.TradeCache, repo *trade.Repository) http.Handler {
	r := chi.NewRouter()
	if cache != nil {
		r.Post("/api/trade/submit", TradeSubmit(cache, repo))
	}
	return r
}

// validSubmitBody returns a JSON string representing a valid TradeLookupResult.
func validSubmitBody(gem, variant string) string {
	result := trade.TradeLookupResult{
		Gem:         gem,
		Variant:     variant,
		Total:       15,
		PriceFloor:  42.5,
		PriceCeiling: 120.0,
		PriceSpread: 77.5,
		MedianTop10: 65.0,
		Listings: []trade.TradeListingDetail{
			{Price: 42.5, Currency: "chaos", ChaosPrice: 42.5, Account: "seller1", GemLevel: 20, GemQuality: 20},
			{Price: 55.0, Currency: "chaos", ChaosPrice: 55.0, Account: "seller2", GemLevel: 20, GemQuality: 20},
		},
		Signals: trade.TradeSignals{
			SellerConcentration: "NORMAL",
			CheapestStaleness:   "FRESH",
			PriceOutlier:        false,
			UniqueAccounts:      2,
		},
		DivinePrice: 212.0,
		TradeURL:    "https://www.pathofexile.com/trade/search/Mirage/abc123",
		FetchedAt:   time.Now(),
	}
	b, _ := json.Marshal(result)
	return string(b)
}

func TestTradeSubmit_ValidBody(t *testing.T) {
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	body := validSubmitBody("Vaal Grace of Phasing", "21/20")
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusNoContent, w.Body.String())
	}

	// Verify the response body is empty (204 No Content).
	if w.Body.Len() != 0 {
		t.Errorf("body = %q, want empty for 204", w.Body.String())
	}

	// Verify the result was cached under the correct key.
	key := trade.CacheKey("Vaal Grace of Phasing", "21/20")
	cached, ok := cache.Get(key)
	if !ok {
		t.Fatal("expected result to be cached after submit, but cache miss")
	}
	if cached.Gem != "Vaal Grace of Phasing" {
		t.Errorf("cached Gem = %q, want %q", cached.Gem, "Vaal Grace of Phasing")
	}
	if cached.Variant != "21/20" {
		t.Errorf("cached Variant = %q, want %q", cached.Variant, "21/20")
	}
	if cached.Total != 15 {
		t.Errorf("cached Total = %d, want 15", cached.Total)
	}
	if cached.PriceFloor != 42.5 {
		t.Errorf("cached PriceFloor = %f, want 42.5", cached.PriceFloor)
	}
	if len(cached.Listings) != 2 {
		t.Errorf("cached Listings count = %d, want 2", len(cached.Listings))
	}
	if cached.DivinePrice != 212.0 {
		t.Errorf("cached DivinePrice = %f, want 212.0", cached.DivinePrice)
	}
}

func TestTradeSubmit_MissingFields(t *testing.T) {
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	tests := []struct {
		name      string
		body      string
		wantError string
	}{
		{
			name:      "missing gem (empty string)",
			body:      `{"gem":"","variant":"21/20","total":1,"priceFloor":10,"listings":[],"signals":{"sellerConcentration":"NORMAL","cheapestStaleness":"FRESH","priceOutlier":false,"uniqueAccounts":0},"divinePrice":212,"tradeUrl":"","fetchedAt":"2026-03-29T00:00:00Z"}`,
			wantError: "gem and variant are required",
		},
		{
			name:      "missing variant (empty string)",
			body:      `{"gem":"Vaal Grace","variant":"","total":1,"priceFloor":10,"listings":[],"signals":{"sellerConcentration":"NORMAL","cheapestStaleness":"FRESH","priceOutlier":false,"uniqueAccounts":0},"divinePrice":212,"tradeUrl":"","fetchedAt":"2026-03-29T00:00:00Z"}`,
			wantError: "gem and variant are required",
		},
		{
			name:      "both gem and variant empty",
			body:      `{"gem":"","variant":"","total":1}`,
			wantError: "gem and variant are required",
		},
		{
			name:      "missing fields entirely (empty object)",
			body:      `{}`,
			wantError: "gem and variant are required",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(tt.body))
			req.Header.Set("Content-Type", "application/json")
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
			}

			var got map[string]string
			if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
				t.Fatalf("decode error response: %v", err)
			}
			if got["error"] != tt.wantError {
				t.Errorf("error = %q, want %q", got["error"], tt.wantError)
			}
		})
	}

	// Verify nothing was cached from invalid requests.
	if cache.Len() != 0 {
		t.Errorf("cache size = %d, want 0 after invalid requests", cache.Len())
	}
}

func TestTradeSubmit_InvalidJSON(t *testing.T) {
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	tests := []struct {
		name string
		body string
	}{
		{"malformed JSON", `{not json`},
		{"empty body", ``},
		{"array instead of object", `[1,2,3]`},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(tt.body))
			req.Header.Set("Content-Type", "application/json")
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
			}

			var got map[string]string
			if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
				t.Fatalf("decode error response: %v", err)
			}
			if !strings.Contains(got["error"], "invalid JSON") {
				t.Errorf("error = %q, want mention of 'invalid JSON'", got["error"])
			}
		})
	}
}

func TestTradeSubmit_NilCacheGuard(t *testing.T) {
	// When TradeCache is nil, the route is not registered (same nil-guard as
	// server.go). chi returns 404 for a POST to an unregistered path.
	router := submitRouter(nil, nil)

	body := validSubmitBody("Vaal Grace", "21/20")
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusNotFound && w.Code != http.StatusMethodNotAllowed {
		t.Fatalf("status = %d, want 404 or 405 when cache is nil", w.Code)
	}
}

func TestTradeSubmit_NilRepoGuard(t *testing.T) {
	// When Repository is nil, the handler should still work — it just skips
	// the async DB persist. This is the normal desktop-only flow.
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	body := validSubmitBody("Empower Support", "4")
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusNoContent, w.Body.String())
	}

	// Verify the result was cached despite nil repo.
	key := trade.CacheKey("Empower Support", "4")
	cached, ok := cache.Get(key)
	if !ok {
		t.Fatal("expected result to be cached after submit with nil repo, but cache miss")
	}
	if cached.Gem != "Empower Support" {
		t.Errorf("cached Gem = %q, want %q", cached.Gem, "Empower Support")
	}
}

func TestTradeSubmit_CacheKeyMatchesCompareEnrichment(t *testing.T) {
	// This test verifies the end-to-end flow: a trade result submitted via
	// TradeSubmit is stored under the same cache key that CompareAnalysis uses
	// for trade enrichment (trade.CacheKey(transfiguredName, variant)).
	//
	// CompareAnalysis enriches rows at collective.go line 443-446:
	//   if tradeResult, ok := tradeCache.Get(trade.CacheKey(cr.TransfiguredName, cr.Variant)); ok {
	//       rw.Trade = tradeResult
	//   }
	//
	// The gem field in the submitted result must match the TransfiguredName
	// from lab analysis. This test confirms the cache key alignment.
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	// Submit a trade result for a transfigured gem.
	gem := "Vaal Grace of Phasing"
	variant := "21/20"
	body := validSubmitBody(gem, variant)
	req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusNoContent {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusNoContent)
	}

	// Verify the cache key used by TradeSubmit matches the key CompareAnalysis
	// would use to look up trade data for this gem.
	enrichmentKey := trade.CacheKey(gem, variant)
	result, ok := cache.Get(enrichmentKey)
	if !ok {
		t.Fatalf("cache miss for enrichment key %q — TradeSubmit and CompareAnalysis use different key formats", enrichmentKey)
	}
	if result.Gem != gem {
		t.Errorf("cached result.Gem = %q, want %q", result.Gem, gem)
	}
	if result.Variant != variant {
		t.Errorf("cached result.Variant = %q, want %q", result.Variant, variant)
	}
	if result.PriceFloor != 42.5 {
		t.Errorf("cached PriceFloor = %f, want 42.5 — data integrity lost in submit pipeline", result.PriceFloor)
	}
	if result.Signals.SellerConcentration != "NORMAL" {
		t.Errorf("cached Signals.SellerConcentration = %q, want %q", result.Signals.SellerConcentration, "NORMAL")
	}
}

func TestTradeSubmit_OverwritesExistingCacheEntry(t *testing.T) {
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	gem := "Empower Support"
	variant := "4"

	// Submit initial result.
	body1 := validSubmitBody(gem, variant)
	req1 := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body1))
	req1.Header.Set("Content-Type", "application/json")
	w1 := httptest.NewRecorder()
	router.ServeHTTP(w1, req1)

	if w1.Code != http.StatusNoContent {
		t.Fatalf("first submit: status = %d, want %d", w1.Code, http.StatusNoContent)
	}

	// Submit updated result with different price.
	updated := trade.TradeLookupResult{
		Gem:         gem,
		Variant:     variant,
		Total:       25,
		PriceFloor:  99.0,
		PriceCeiling: 200.0,
		Listings:    []trade.TradeListingDetail{},
		Signals: trade.TradeSignals{
			SellerConcentration: "CONCENTRATED",
			CheapestStaleness:   "AGING",
			UniqueAccounts:      3,
		},
		FetchedAt: time.Now(),
	}
	body2, _ := json.Marshal(updated)
	req2 := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(string(body2)))
	req2.Header.Set("Content-Type", "application/json")
	w2 := httptest.NewRecorder()
	router.ServeHTTP(w2, req2)

	if w2.Code != http.StatusNoContent {
		t.Fatalf("second submit: status = %d, want %d", w2.Code, http.StatusNoContent)
	}

	// Verify the cache contains the updated result, not the original.
	key := trade.CacheKey(gem, variant)
	cached, ok := cache.Get(key)
	if !ok {
		t.Fatal("expected cached result after second submit")
	}
	if cached.PriceFloor != 99.0 {
		t.Errorf("cached PriceFloor = %f, want 99.0 (updated value)", cached.PriceFloor)
	}
	if cached.Total != 25 {
		t.Errorf("cached Total = %d, want 25 (updated value)", cached.Total)
	}
	if cached.Signals.SellerConcentration != "CONCENTRATED" {
		t.Errorf("cached SellerConcentration = %q, want %q", cached.Signals.SellerConcentration, "CONCENTRATED")
	}

	// Cache should still have only 1 entry (overwrite, not duplicate).
	if cache.Len() != 1 {
		t.Errorf("cache size = %d, want 1 after overwrite", cache.Len())
	}
}

func TestTradeSubmit_MultipleGems(t *testing.T) {
	cache := trade.NewTradeCache(10)
	router := submitRouter(cache, nil)

	gems := []struct {
		gem     string
		variant string
	}{
		{"Vaal Grace of Phasing", "21/20"},
		{"Empower Support", "4"},
		{"Enlighten Support", "3"},
	}

	for _, g := range gems {
		body := validSubmitBody(g.gem, g.variant)
		req := httptest.NewRequest(http.MethodPost, "/api/trade/submit", strings.NewReader(body))
		req.Header.Set("Content-Type", "application/json")
		w := httptest.NewRecorder()
		router.ServeHTTP(w, req)

		if w.Code != http.StatusNoContent {
			t.Fatalf("submit %s/%s: status = %d, want %d", g.gem, g.variant, w.Code, http.StatusNoContent)
		}
	}

	// Verify all gems are cached independently.
	if cache.Len() != 3 {
		t.Errorf("cache size = %d, want 3", cache.Len())
	}

	for _, g := range gems {
		key := trade.CacheKey(g.gem, g.variant)
		result, ok := cache.Get(key)
		if !ok {
			t.Errorf("cache miss for %s/%s", g.gem, g.variant)
			continue
		}
		if result.Gem != g.gem {
			t.Errorf("cached Gem = %q, want %q", result.Gem, g.gem)
		}
		if result.Variant != g.variant {
			t.Errorf("cached Variant = %q, want %q", result.Variant, g.variant)
		}
	}
}
