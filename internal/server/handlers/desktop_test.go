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

// capturingHub stands in for the Mercure hub so the published payload — the
// handler's actual output — can be asserted. The tests above can only see the
// HTTP response, which says nothing about what the web view receives.
func capturingHub(t *testing.T, captured *map[string]any) *httptest.Server {
	t.Helper()
	return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Errorf("hub: parse form: %v", err)
			return
		}
		var payload map[string]any
		if err := json.Unmarshal([]byte(r.PostForm.Get("data")), &payload); err != nil {
			t.Errorf("hub: decode data: %v", err)
			return
		}
		*captured = payload
		w.WriteHeader(http.StatusOK)
	}))
}

func publishGems(t *testing.T, body string) map[string]any {
	t.Helper()
	var captured map[string]any
	hub := capturingHub(t, &captured)
	defer hub.Close()

	r := chi.NewRouter()
	r.Post("/api/desktop/gems", DesktopGems(hub.URL, "test-secret"))

	req := httptest.NewRequest(http.MethodPost, "/api/desktop/gems", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	return captured
}

// The market alone does not say which mode it belongs to — "20/20" is a real
// market in Normal and none at all in Dedication — so the web view needs the
// mode to tell an unknown market from another mode's.
func TestDesktopGems_RelaysMode(t *testing.T) {
	// Both modes, because dropping either one from the whitelist strands that
	// mode's scans on the no-mode path the field exists to replace.
	for _, mode := range []string{"dedication", "normal"} {
		t.Run(mode, func(t *testing.T) {
			payload := publishGems(t, `{"pair":"Ab12","gems":["Vaal Grace"],"variant":"20/20","mode":"`+mode+`"}`)

			if payload["mode"] != mode {
				t.Errorf("mode = %v, want %s", payload["mode"], mode)
			}
		})
	}
}

// An unrecognised mode is worse than none: the web view would switch itself to
// a mode that does not exist rather than fall back to its own.
func TestDesktopGems_DropsUnknownMode(t *testing.T) {
	payload := publishGems(t, `{"pair":"Ab12","gems":["Vaal Grace"],"variant":"20/20","mode":"uber"}`)

	if _, present := payload["mode"]; present {
		t.Errorf("mode = %v, want absent", payload["mode"])
	}
}

// Older desktop builds send no mode at all. The event must still carry the gems.
func TestDesktopGems_OmitsModeWhenAbsent(t *testing.T) {
	payload := publishGems(t, `{"pair":"Ab12","gems":["Vaal Grace"]}`)

	if _, present := payload["mode"]; present {
		t.Errorf("mode = %v, want absent", payload["mode"])
	}
	if payload["type"] != "gems-detected" {
		t.Errorf("type = %v, want gems-detected", payload["type"])
	}
}
