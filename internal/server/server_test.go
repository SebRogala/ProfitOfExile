package server

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
	"profitofexile/internal/server/handlers"
)

func TestNewRouter_HealthRoute(t *testing.T) {
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/health status = %d, want %d", w.Code, http.StatusOK)
	}

	contentType := w.Header().Get("Content-Type")
	if contentType != "application/json" {
		t.Errorf("Content-Type = %q, want %q", contentType, "application/json")
	}

	var body struct {
		Status  string `json:"status"`
		Version string `json:"version"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Status != "ok" {
		t.Errorf("status = %q, want %q", body.Status, "ok")
	}
	if body.Version != "dev" {
		t.Errorf("version = %q, want %q", body.Version, "dev")
	}
}

func TestNewRouter_HealthTakesPrecedenceOverStaticCatchAll(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>SPA</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/health status = %d, want %d", w.Code, http.StatusOK)
	}

	contentType := w.Header().Get("Content-Type")
	if contentType != "application/json" {
		t.Errorf("Content-Type = %q, want %q", contentType, "application/json")
	}

	var body struct {
		Status string `json:"status"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Status != "ok" {
		t.Errorf("status = %q, want %q", body.Status, "ok")
	}
}

func TestNewRouter_StaticCatchAllServesFiles(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>ProfitOfExile</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET / status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "ProfitOfExile") {
		t.Errorf("GET / body = %q, want it to contain %q", string(body), "ProfitOfExile")
	}
}

func TestNewRouter_StaticCatchAllFallbackForUnknownPaths(t *testing.T) {
	frontendFS := fstest.MapFS{
		"index.html": &fstest.MapFile{
			Data: []byte("<html><body>ProfitOfExile</body></html>"),
		},
	}
	router := NewRouter(handlers.NopPinger{}, frontendFS, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/strategies/lab", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /strategies/lab status = %d, want %d", w.Code, http.StatusOK)
	}

	body, err := io.ReadAll(w.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}
	if !strings.Contains(string(body), "ProfitOfExile") {
		t.Errorf("GET /strategies/lab body = %q, want SPA fallback with %q", string(body), "ProfitOfExile")
	}
}

func TestNewRouter_CurrencyExchangeRouteIsServedWithoutTheLabStack(t *testing.T) {
	// Currency exchange is a separate pillar: its own collector, tables and
	// cache, sharing nothing with LabRepo. Registering it inside the
	// `if cfg.LabRepo != nil` block would make a lab-less server 404 on it —
	// and the cache the route answers from is the one in the config, which is
	// what the league and the play count below prove.
	cache := exchange.NewCache()
	cache.Set(exchange.Result{
		League: "Mirage",
		Hours:  6,
		To:     time.Date(2026, 8, 19, 7, 0, 0, 0, time.UTC),
		Plays:  []exchange.Play{{Key: "direct:a", Mode: exchange.ModeDirect}},
	})
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{
		League:        league.Historical("Mirage"),
		ExchangeCache: cache,
	})

	req := httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/currency-exchange/plays status = %d, want %d (body: %s)",
			w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		League string `json:"league"`
		Warm   bool   `json:"warm"`
		Count  int    `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.League != "Mirage" {
		t.Errorf("league = %q, want %q — the route must read the configured cache", body.League, "Mirage")
	}
	if !body.Warm {
		t.Error("warm = false, want true")
	}
	if body.Count != 1 {
		t.Errorf("count = %d, want 1", body.Count)
	}
}

func TestNewRouter_CurrencyExchangeRouteIsRegisteredWithoutACache(t *testing.T) {
	// A server started without the pillar leaves ExchangeCache nil; the route
	// still answers, cold, rather than 404ing.
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("GET /api/currency-exchange/plays status = %d, want %d (body: %s)",
			w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		Warm  bool `json:"warm"`
		Count int  `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}
	if body.Warm {
		t.Error("warm = true, want false")
	}
	if body.Count != 0 {
		t.Errorf("count = %d, want 0", body.Count)
	}
}

func TestNewRouter_NilFrontendFSReturns404ForNonAPIPaths(t *testing.T) {
	router := NewRouter(handlers.NopPinger{}, nil, RouterConfig{})

	req := httptest.NewRequest(http.MethodGet, "/", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	// With no frontendFS, non-API paths should return 404 or 405.
	if w.Code == http.StatusOK {
		t.Errorf("GET / with nil frontendFS status = %d, want non-200 (404 or 405)", w.Code)
	}
}
