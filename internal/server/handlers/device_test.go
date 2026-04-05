package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/device"
	"profitofexile/internal/server/middleware"
)

// mockLister implements device.Lister for testing AdminDevices.
type mockLister struct {
	ListFn func(ctx context.Context) ([]device.Device, error)
}

func (m *mockLister) List(ctx context.Context) ([]device.Device, error) {
	return m.ListFn(ctx)
}

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

// newTestDevice creates a device with the given fingerprint for test assertions.
func newTestDevice(fingerprint string) device.Device {
	now := time.Date(2026, 4, 5, 12, 0, 0, 0, time.UTC)
	alias := "test-alias"
	version := "0.3.1"
	return device.Device{
		Fingerprint: fingerprint,
		Alias:       &alias,
		Role:        "user",
		Banned:      false,
		AppVersion:  &version,
		FirstSeen:   now,
		LastSeen:    now,
	}
}

// adminRouter builds a chi router with the admin devices endpoint, matching the
// production wiring in server.go.
func adminRouter(repo device.Lister, secret string) http.Handler {
	r := chi.NewRouter()
	r.Get("/api/admin/devices", AdminDevices(repo, secret))
	return r
}

// identifyRouter builds a chi router with device middleware and the identify
// endpoint, matching the production wiring in server.go.
func identifyRouter(upserter device.Upserter, setter device.AliasSetter) http.Handler {
	r := chi.NewRouter()
	r.Use(middleware.DeviceMiddleware(upserter))
	r.Post("/api/device/identify", DeviceIdentify(setter))
	return r
}

// --- AdminDevices tests ---

func TestAdminDevices_WithoutInternalSecret_Returns401(t *testing.T) {
	repo := &mockLister{
		ListFn: func(_ context.Context) ([]device.Device, error) {
			t.Error("List should not be called when auth fails")
			return nil, nil
		},
	}

	router := adminRouter(repo, "my-secret-token")

	req := httptest.NewRequest(http.MethodGet, "/api/admin/devices", nil)
	// No X-Internal-Token header.
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusUnauthorized, w.Body.String())
	}
}

func TestAdminDevices_WrongInternalSecret_Returns401(t *testing.T) {
	repo := &mockLister{
		ListFn: func(_ context.Context) ([]device.Device, error) {
			t.Error("List should not be called when auth fails")
			return nil, nil
		},
	}

	router := adminRouter(repo, "correct-secret")

	req := httptest.NewRequest(http.MethodGet, "/api/admin/devices", nil)
	req.Header.Set("X-Internal-Token", "wrong-secret")
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusUnauthorized {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusUnauthorized, w.Body.String())
	}
}

func TestAdminDevices_WithInternalSecret_ReturnsList(t *testing.T) {
	devices := []device.Device{
		newTestDevice("fingerprint-aaa111"),
		newTestDevice("fingerprint-bbb222"),
	}

	repo := &mockLister{
		ListFn: func(_ context.Context) ([]device.Device, error) {
			return devices, nil
		},
	}

	secret := "test-secret-123"
	router := adminRouter(repo, secret)

	req := httptest.NewRequest(http.MethodGet, "/api/admin/devices", nil)
	req.Header.Set("X-Internal-Token", secret)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var body struct {
		Devices []device.Device `json:"devices"`
		Count   int             `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Count != 2 {
		t.Errorf("count = %d, want 2", body.Count)
	}
	if len(body.Devices) != 2 {
		t.Fatalf("devices length = %d, want 2", len(body.Devices))
	}
	if body.Devices[0].Fingerprint != "fingerprint-aaa111" {
		t.Errorf("first device fingerprint = %q, want %q", body.Devices[0].Fingerprint, "fingerprint-aaa111")
	}
	if body.Devices[1].Fingerprint != "fingerprint-bbb222" {
		t.Errorf("second device fingerprint = %q, want %q", body.Devices[1].Fingerprint, "fingerprint-bbb222")
	}
}

func TestAdminDevices_EmptySecretConfig_NoAuth(t *testing.T) {
	// When InternalSecret is empty, the handler should not enforce auth
	// (used in dev mode).
	repo := &mockLister{
		ListFn: func(_ context.Context) ([]device.Device, error) {
			return []device.Device{}, nil
		},
	}

	router := adminRouter(repo, "")

	req := httptest.NewRequest(http.MethodGet, "/api/admin/devices", nil)
	// No auth header needed when secret is empty.
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}

	var body struct {
		Devices []device.Device `json:"devices"`
		Count   int             `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body.Count != 0 {
		t.Errorf("count = %d, want 0", body.Count)
	}
}

func TestAdminDevices_ListError_Returns500(t *testing.T) {
	repo := &mockLister{
		ListFn: func(_ context.Context) ([]device.Device, error) {
			return nil, fmt.Errorf("database connection lost")
		},
	}

	secret := "test-secret"
	router := adminRouter(repo, secret)

	req := httptest.NewRequest(http.MethodGet, "/api/admin/devices", nil)
	req.Header.Set("X-Internal-Token", secret)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusInternalServerError, w.Body.String())
	}

	var resp map[string]string
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp["error"] != "failed to list devices" {
		t.Errorf("error = %q, want %q", resp["error"], "failed to list devices")
	}
}

// --- DeviceIdentify tests ---

func TestDeviceIdentify_WithAlias_UpdatesDevice(t *testing.T) {
	fingerprint := "abc123def456"
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
	fingerprint := "abc123def456"
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
	fingerprint := "abc123def456"
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
	fingerprint := "abc123def456"
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
	fingerprint := "abc123def456"

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
	fingerprint := "abc123def456"
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
