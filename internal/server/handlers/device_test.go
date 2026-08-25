package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/device"
	"profitofexile/internal/server/middleware"
)

// mockAliasSetter implements device.AliasSetter for testing DeviceIdentify.
type mockAliasSetter struct {
	SetAliasFn func(ctx context.Context, fingerprint, alias string) error
}

func (m *mockAliasSetter) SetAlias(ctx context.Context, fingerprint, alias string) error {
	return m.SetAliasFn(ctx, fingerprint, alias)
}

// mockUpserter implements device.Upserter for the middleware in integration-style tests.
type mockUpserter struct {
	UpsertFn func(ctx context.Context, fingerprint, appVersion string) (*device.Device, error)
}

func (m *mockUpserter) Upsert(ctx context.Context, fingerprint, appVersion string) (*device.Device, error) {
	return m.UpsertFn(ctx, fingerprint, appVersion)
}

// identifyRouter builds a chi router with device middleware and the identify
// endpoint, matching the production wiring in server.go.
func identifyRouter(upserter device.Upserter, setter device.AliasSetter) http.Handler {
	r := chi.NewRouter()
	r.Use(middleware.DeviceMiddleware(upserter))
	r.Post("/api/device/identify", DeviceIdentify(setter))
	return r
}

// --- DeviceIdentify tests ---

func TestDeviceIdentify_WithAlias_UpdatesDevice(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	var capturedFingerprint, capturedAlias string

	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, fp, alias string) error {
			capturedFingerprint = fp
			capturedAlias = alias
			return nil
		},
	}

	router := identifyRouter(upserter, setter)

	body := `{"alias":"Seb's PC"}`
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	if capturedFingerprint != fingerprint {
		t.Errorf("SetAlias fingerprint = %q, want %q", capturedFingerprint, fingerprint)
	}
	if capturedAlias != "Seb's PC" {
		t.Errorf("SetAlias alias = %q, want %q", capturedAlias, "Seb's PC")
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["status"] != "identified" {
		t.Errorf("status = %q, want %q", resp["status"], "identified")
	}
	if resp["fingerprint"] != fingerprint {
		t.Errorf("fingerprint = %q, want %q", resp["fingerprint"], fingerprint)
	}
	if resp["alias"] != "Seb's PC" {
		t.Errorf("alias = %q, want %q", resp["alias"], "Seb's PC")
	}
}

func TestDeviceIdentify_WithoutDeviceID_Returns400(t *testing.T) {
	// When no X-Device-ID header is sent, the middleware does not set a device
	// in context, so the handler returns 400.
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, _, _ string) error {
			t.Error("SetAlias should not be called when device is missing from context")
			return nil
		},
	}

	router := identifyRouter(upserter, setter)

	body := `{"alias":"My PC"}`
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	// No X-Device-ID header.
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["error"] != "X-Device-ID header required" {
		t.Errorf("error = %q, want %q", resp["error"], "X-Device-ID header required")
	}
}

func TestDeviceIdentify_EmptyAlias_Returns400(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, _, _ string) error {
			t.Error("SetAlias should not be called with empty alias")
			return nil
		},
	}

	router := identifyRouter(upserter, setter)

	body := `{"alias":""}`
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["error"] != "alias is required" {
		t.Errorf("error = %q, want %q", resp["error"], "alias is required")
	}
}

func TestDeviceIdentify_AliasTooLong_Returns400(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, _, _ string) error {
			t.Error("SetAlias should not be called with too-long alias")
			return nil
		},
	}

	router := identifyRouter(upserter, setter)

	longAlias := strings.Repeat("a", 65) // 65 chars > 64 max
	body := fmt.Sprintf(`{"alias":"%s"}`, longAlias)
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["error"] != "alias too long (max 64 characters)" {
		t.Errorf("error = %q, want %q", resp["error"], "alias too long (max 64 characters)")
	}
}

func TestDeviceIdentify_MaxLengthAlias_Accepted(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setAliasCalled := false
	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, _, _ string) error {
			setAliasCalled = true
			return nil
		},
	}

	router := identifyRouter(upserter, setter)

	maxAlias := strings.Repeat("a", 64) // exactly 64 chars — should be accepted
	body := fmt.Sprintf(`{"alias":"%s"}`, maxAlias)
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	if !setAliasCalled {
		t.Error("SetAlias should have been called for a valid max-length alias")
	}
}

func TestDeviceIdentify_InvalidJSON_Returns400(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"

	tests := []struct {
		name string
		body string
	}{
		{"malformed JSON", `{not json`},
		{"empty body", ``},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			upserter := &mockUpserter{
				UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
					return &device.Device{Fingerprint: fp, Role: "user"}, nil
				},
			}

			setter := &mockAliasSetter{
				SetAliasFn: func(_ context.Context, _, _ string) error {
					t.Error("SetAlias should not be called with invalid JSON")
					return nil
				},
			}

			router := identifyRouter(upserter, setter)

			req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(tt.body))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("X-Device-ID", fingerprint)
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
			}

			var resp map[string]string
			if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
				t.Fatalf("decode response: %v", err)
			}
			if resp["error"] != "invalid JSON body" {
				t.Errorf("error = %q, want %q", resp["error"], "invalid JSON body")
			}
		})
	}
}

func TestDeviceIdentify_SetAliasError_Returns500(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	setter := &mockAliasSetter{
		SetAliasFn: func(_ context.Context, _, _ string) error {
			return fmt.Errorf("database timeout")
		},
	}

	router := identifyRouter(upserter, setter)

	body := `{"alias":"My PC"}`
	req := httptest.NewRequest(http.MethodPost, "/api/device/identify", strings.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusInternalServerError, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["error"] != "failed to update alias" {
		t.Errorf("error = %q, want %q", resp["error"], "failed to update alias")
	}
}

// --- DeviceMe tests ---

// meRouter builds a chi router with device middleware and the me endpoint,
// matching the production wiring in server.go for a server that has a device
// repository.
func meRouter(upserter device.Upserter) http.Handler {
	r := chi.NewRouter()
	r.Use(middleware.DeviceMiddleware(upserter))
	r.Get("/api/device/me", DeviceMe())
	return r
}

// meResponse mirrors the documented response body of GET /api/device/me.
type meResponse struct {
	Role     string   `json:"role"`
	Channel  string   `json:"channel"`
	Features []string `json:"features"`
}

func TestDeviceMe_RoleDeterminesChannelAndFeatures(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"

	tests := []struct {
		name         string
		role         string
		wantChannel  string
		wantFeatures []string
	}{
		{"editor is a beta device with every hidden feature", "editor", "beta", []string{"merc", "exchange", "temple"}},
		{"admin is a beta device with every hidden feature", "admin", "beta", []string{"merc", "exchange", "temple"}},
		{"user is a stable device with no features", "user", "stable", []string{}},
		{"unset role is a stable device with no features", "", "stable", []string{}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			upserter := &mockUpserter{
				UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
					return &device.Device{Fingerprint: fp, Role: tt.role}, nil
				},
			}

			req := httptest.NewRequest(http.MethodGet, "/api/device/me", nil)
			req.Header.Set("X-Device-ID", fingerprint)
			w := httptest.NewRecorder()

			meRouter(upserter).ServeHTTP(w, req)

			if w.Code != http.StatusOK {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
			}

			var resp meResponse
			if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
				t.Fatalf("decode response: %v", err)
			}
			if resp.Role != tt.role {
				t.Errorf("role = %q, want %q", resp.Role, tt.role)
			}
			if resp.Channel != tt.wantChannel {
				t.Errorf("channel = %q, want %q", resp.Channel, tt.wantChannel)
			}
			if !reflect.DeepEqual(resp.Features, tt.wantFeatures) {
				t.Errorf("features = %#v, want %#v", resp.Features, tt.wantFeatures)
			}
		})
	}
}

func TestDeviceMe_WithoutDeviceID_ReturnsStableAndNoFeatures(t *testing.T) {
	// No X-Device-ID header means the middleware attaches no device, so the
	// handler must fall back to the unentitled answer rather than reject the
	// request the way DeviceIdentify does.
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, _, _ string) (*device.Device, error) {
			t.Error("Upsert should not be called without an X-Device-ID header")
			return nil, nil
		},
	}

	req := httptest.NewRequest(http.MethodGet, "/api/device/me", nil)
	w := httptest.NewRecorder()

	meRouter(upserter).ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var resp meResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp.Role != "" {
		t.Errorf("role = %q, want empty", resp.Role)
	}
	if resp.Channel != "stable" {
		t.Errorf("channel = %q, want %q", resp.Channel, "stable")
	}
	if len(resp.Features) != 0 {
		t.Errorf("features = %#v, want empty", resp.Features)
	}
}

func TestDeviceMe_WithoutDeviceMiddleware_ReturnsStableAndNoFeatures(t *testing.T) {
	// Mirrors a server started with cfg.DeviceRepo == nil: the middleware is
	// never installed, but the route is registered unconditionally and must
	// still answer rather than 404/500.
	r := chi.NewRouter()
	r.Get("/api/device/me", DeviceMe())

	req := httptest.NewRequest(http.MethodGet, "/api/device/me", nil)
	req.Header.Set("X-Device-ID", "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899")
	w := httptest.NewRecorder()

	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var resp meResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp.Channel != "stable" {
		t.Errorf("channel = %q, want %q", resp.Channel, "stable")
	}
	if len(resp.Features) != 0 {
		t.Errorf("features = %#v, want empty", resp.Features)
	}
}

func TestDeviceMe_UnentitledDeviceEncodesFeaturesAsEmptyArray(t *testing.T) {
	// The desktop client reads features with an includes() check, so an unentitled
	// device must get [] on the wire — a null would be a different type there.
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
	upserter := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return &device.Device{Fingerprint: fp, Role: "user"}, nil
		},
	}

	req := httptest.NewRequest(http.MethodGet, "/api/device/me", nil)
	req.Header.Set("X-Device-ID", fingerprint)
	w := httptest.NewRecorder()

	meRouter(upserter).ServeHTTP(w, req)

	if got := w.Header().Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q, want %q", got, "application/json")
	}

	// The answer is per-device: a shared cache must not serve one device's
	// entitlements to another.
	if got := w.Header().Get("Cache-Control"); got != "no-store" {
		t.Errorf("Cache-Control = %q, want %q", got, "no-store")
	}

	body := strings.TrimSpace(w.Body.String())
	if !strings.Contains(body, `"features":[]`) {
		t.Errorf("body = %s, want it to encode features as an empty array", body)
	}
}
