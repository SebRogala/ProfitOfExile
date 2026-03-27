package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/go-chi/chi/v5"
)

// desktopRouter builds a chi router with the desktop gems route, matching the
// production wiring in server.go. Empty mercure credentials cause the publish
// call to silently skip (see internal/mercure/publisher.go lines 75-76).
func desktopRouter() http.Handler {
	r := chi.NewRouter()
	r.Post("/api/desktop/gems", DesktopGems("", ""))
	return r
}

func TestDesktopGems_ValidRequest(t *testing.T) {
	router := desktopRouter()

	body := `{"pair":"Ab12","gems":["Vaal Grace","Empower Support"],"variant":"21/20"}`
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/gems", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var got map[string]bool
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if !got["published"] {
		t.Errorf("published = %v, want true", got["published"])
	}
}

func TestDesktopGems_ValidRequestWithoutVariant(t *testing.T) {
	router := desktopRouter()

	body := `{"pair":"xY9z","gems":["Vaal Grace"]}`
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/gems", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var got map[string]bool
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if !got["published"] {
		t.Errorf("published = %v, want true", got["published"])
	}
}

func TestDesktopGems_InvalidRequests(t *testing.T) {
	router := desktopRouter()

	tests := []struct {
		name      string
		body      string
		wantError string
	}{
		{
			name:      "missing pair (empty string)",
			body:      `{"pair":"","gems":["Vaal Grace"],"variant":"21/20"}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "pair too short",
			body:      `{"pair":"Ab1","gems":["Vaal Grace"]}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "pair too long",
			body:      `{"pair":"Ab123","gems":["Vaal Grace"]}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "pair with special characters",
			body:      `{"pair":"Ab!2","gems":["Vaal Grace"]}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "pair with spaces",
			body:      `{"pair":"Ab 2","gems":["Vaal Grace"]}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "empty gems array",
			body:      `{"pair":"Ab12","gems":[]}`,
			wantError: "gems must contain 1-5 items",
		},
		{
			name:      "too many gems (6)",
			body:      `{"pair":"Ab12","gems":["a","b","c","d","e","f"]}`,
			wantError: "gems must contain 1-5 items",
		},
		{
			name:      "empty gem name",
			body:      `{"pair":"Ab12","gems":[""]}`,
			wantError: "each gem name must be non-empty",
		},
		{
			name:      "one empty gem among valid ones",
			body:      `{"pair":"Ab12","gems":["Vaal Grace","","Empower Support"]}`,
			wantError: "each gem name must be non-empty",
		},
		{
			name:      "missing body (empty string)",
			body:      ``,
			wantError: "invalid JSON body",
		},
		{
			name:      "malformed JSON",
			body:      `{not json`,
			wantError: "invalid JSON body",
		},
		{
			name:      "missing pair field entirely",
			body:      `{"gems":["Vaal Grace"]}`,
			wantError: "pair must be exactly 4 alphanumeric characters",
		},
		{
			name:      "missing gems field entirely",
			body:      `{"pair":"Ab12"}`,
			wantError: "gems must contain 1-5 items",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodPost, "/api/desktop/gems", strings.NewReader(tt.body))
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
}

func TestDesktopGems_MaxGemsAllowed(t *testing.T) {
	router := desktopRouter()

	// Exactly 5 gems should be accepted (the maximum).
	body := `{"pair":"Ab12","gems":["a","b","c","d","e"]}`
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/gems", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var got map[string]bool
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if !got["published"] {
		t.Errorf("published = %v, want true", got["published"])
	}
}
