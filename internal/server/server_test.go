package server

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"testing/fstest"

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
