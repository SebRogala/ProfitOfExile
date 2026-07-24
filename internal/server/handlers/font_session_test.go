package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// TestFontSession_NoDevice_Returns401 asserts that a font-session submission
// without an authenticated device is rejected. The device is taken from the
// request context (set by DeviceMiddleware), never from the body — so a client
// cannot attribute crowd-sourced font data to an arbitrary device id. The body
// below is otherwise valid: it passes every field validation and reaches the
// device-identity check, which is the behavior under test.
//
// Mutation check: removing the `dev == nil` guard in FontSession makes this test
// fail (the handler then dereferences the nil device instead of returning 401).
func TestFontSession_NoDevice_Returns401(t *testing.T) {
	body := `{"lab_type":"Uber","total_crafts":6,"variant":"20/20","rounds":[{"option_chosen":"a","gems_offered":["Spark"],"gem_picked":"Spark"}]}`
	req := httptest.NewRequest(http.MethodPost, "/api/desktop/font-session", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()

	// Pool is nil: the device check must reject before any DB access is attempted.
	FontSession(nil).ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusUnauthorized, w.Body.String())
	}

	var got map[string]string
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode error response: %v", err)
	}
	if got["error"] != "device identification required" {
		t.Errorf("error = %q, want %q", got["error"], "device identification required")
	}
}
