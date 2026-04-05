package middleware

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"profitofexile/internal/device"
)

// mockUpserter implements device.Upserter for unit testing.
type mockUpserter struct {
	UpsertFn func(ctx context.Context, fingerprint, appVersion string) (*device.Device, error)
}

func (m *mockUpserter) Upsert(ctx context.Context, fingerprint, appVersion string) (*device.Device, error) {
	return m.UpsertFn(ctx, fingerprint, appVersion)
}

// testDevice returns a non-banned device with the given fingerprint.
func testDevice(fingerprint string) *device.Device {
	return &device.Device{
		Fingerprint: fingerprint,
		Role:        "user",
		Banned:      false,
	}
}

// testBannedDevice returns a banned device with the given fingerprint.
func testBannedDevice(fingerprint string) *device.Device {
	return &device.Device{
		Fingerprint: fingerprint,
		Role:        "user",
		Banned:      true,
	}
}

func TestDeviceMiddleware_WithDeviceID(t *testing.T) {
	fingerprint := "abc123def456"
	appVersion := "0.3.1"

	var capturedFingerprint, capturedVersion string
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, av string) (*device.Device, error) {
			capturedFingerprint = fp
			capturedVersion = av
			return testDevice(fp), nil
		},
	}

	var ctxDevice *device.Device
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctxDevice = DeviceFromContext(r.Context())
		w.WriteHeader(http.StatusOK)
	})

	handler := DeviceMiddleware(repo)(inner)

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	req.Header.Set("X-Device-ID", fingerprint)
	req.Header.Set("X-App-Version", appVersion)
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}

	// Verify Upsert was called with correct arguments.
	if capturedFingerprint != fingerprint {
		t.Errorf("upsert fingerprint = %q, want %q", capturedFingerprint, fingerprint)
	}
	if capturedVersion != appVersion {
		t.Errorf("upsert appVersion = %q, want %q", capturedVersion, appVersion)
	}

	// Verify device was attached to context.
	if ctxDevice == nil {
		t.Fatal("expected device in context, got nil")
	}
	if ctxDevice.Fingerprint != fingerprint {
		t.Errorf("context device fingerprint = %q, want %q", ctxDevice.Fingerprint, fingerprint)
	}
}

func TestDeviceMiddleware_WithoutDeviceID(t *testing.T) {
	upsertCalled := false
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, _, _ string) (*device.Device, error) {
			upsertCalled = true
			return testDevice("should-not-be-called"), nil
		},
	}

	var ctxDevice *device.Device
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ctxDevice = DeviceFromContext(r.Context())
		w.WriteHeader(http.StatusOK)
	})

	handler := DeviceMiddleware(repo)(inner)

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	// No X-Device-ID header set.
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}

	if upsertCalled {
		t.Error("Upsert should not be called when X-Device-ID is absent")
	}

	if ctxDevice != nil {
		t.Errorf("expected nil device in context, got %+v", ctxDevice)
	}
}

func TestDeviceMiddleware_AppVersionStored(t *testing.T) {
	tests := []struct {
		name       string
		appVersion string
	}{
		{"with version", "0.3.1"},
		{"without version header", ""},
		{"with long version", "1.2.3-beta.4+build.567"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var capturedVersion string
			repo := &mockUpserter{
				UpsertFn: func(_ context.Context, fp, av string) (*device.Device, error) {
					capturedVersion = av
					return testDevice(fp), nil
				},
			}

			inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(http.StatusOK)
			})

			handler := DeviceMiddleware(repo)(inner)

			req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
			req.Header.Set("X-Device-ID", "test-fingerprint")
			if tt.appVersion != "" {
				req.Header.Set("X-App-Version", tt.appVersion)
			}
			w := httptest.NewRecorder()

			handler.ServeHTTP(w, req)

			if capturedVersion != tt.appVersion {
				t.Errorf("upsert appVersion = %q, want %q", capturedVersion, tt.appVersion)
			}
		})
	}
}

func TestDeviceMiddleware_BannedDevice(t *testing.T) {
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return testBannedDevice(fp), nil
		},
	}

	innerCalled := false
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		innerCalled = true
		w.WriteHeader(http.StatusOK)
	})

	handler := DeviceMiddleware(repo)(inner)

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	req.Header.Set("X-Device-ID", "banned-device-fp")
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusForbidden {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusForbidden)
	}

	if innerCalled {
		t.Error("inner handler should not be called for banned devices")
	}

	var body map[string]string
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if body["error"] != "device is banned" {
		t.Errorf("error = %q, want %q", body["error"], "device is banned")
	}
}

func TestDeviceMiddleware_NonBannedDevice(t *testing.T) {
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
			return testDevice(fp), nil
		},
	}

	innerCalled := false
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		innerCalled = true
		w.WriteHeader(http.StatusOK)
	})

	handler := DeviceMiddleware(repo)(inner)

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	req.Header.Set("X-Device-ID", "good-device-fp")
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}

	if !innerCalled {
		t.Error("inner handler should be called for non-banned devices")
	}
}

func TestDeviceMiddleware_UpsertError_FailsOpen(t *testing.T) {
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, _, _ string) (*device.Device, error) {
			return nil, fmt.Errorf("database connection refused")
		},
	}

	innerCalled := false
	var ctxDevice *device.Device
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		innerCalled = true
		ctxDevice = DeviceFromContext(r.Context())
		w.WriteHeader(http.StatusOK)
	})

	handler := DeviceMiddleware(repo)(inner)

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	req.Header.Set("X-Device-ID", "some-fingerprint")
	w := httptest.NewRecorder()

	handler.ServeHTTP(w, req)

	// Middleware fails open — request passes through even when upsert fails.
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (fail-open)", w.Code, http.StatusOK)
	}

	if !innerCalled {
		t.Error("inner handler should be called even when upsert fails (fail-open)")
	}

	// No device should be in context since upsert failed.
	if ctxDevice != nil {
		t.Errorf("expected nil device in context after upsert error, got %+v", ctxDevice)
	}
}

func TestDeviceFromContext_NilWhenMissing(t *testing.T) {
	ctx := context.Background()
	d := DeviceFromContext(ctx)
	if d != nil {
		t.Errorf("expected nil from empty context, got %+v", d)
	}
}
