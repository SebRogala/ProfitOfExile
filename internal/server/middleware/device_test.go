package middleware

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"

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

func TestDeviceMiddleware_InvalidFingerprint_Returns400(t *testing.T) {
	upsertCalled := false
	repo := &mockUpserter{
		UpsertFn: func(_ context.Context, _, _ string) (*device.Device, error) {
			upsertCalled = true
			return testDevice("x"), nil
		},
	}

	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("inner handler should not be called for invalid fingerprint")
	})

	handler := DeviceMiddleware(repo)(inner)

	tests := []struct {
		name        string
		fingerprint string
	}{
		{"too short", "abc123"},
		{"too long", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},     // 67 chars
		{"wrong length between 36 and 64", "aabbccddeeff00112233445566778899aabbccddeeff"},      // 44 chars
		{"uppercase hex", "AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899AA"}, // 64 uppercase
		{"special characters", "abc!@#$%^&*()_+abc!@#$%^&*()_+abcabc"},                          // 37 chars with specials
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
			req.Header.Set("X-Device-ID", tt.fingerprint)
			w := httptest.NewRecorder()

			handler.ServeHTTP(w, req)

			if w.Code != http.StatusBadRequest {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusBadRequest, w.Body.String())
			}

			var body map[string]string
			if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
				t.Fatalf("decode response: %v", err)
			}
			if body["error"] != "invalid device fingerprint format" {
				t.Errorf("error = %q, want %q", body["error"], "invalid device fingerprint format")
			}
		})
	}

	if upsertCalled {
		t.Error("Upsert should not be called for invalid fingerprints")
	}
}

func TestDeviceMiddleware_ValidFingerprints(t *testing.T) {
	tests := []struct {
		name        string
		fingerprint string
	}{
		{"64-char hex (SHA-256)", "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"},
		{"36-char UUID", "550e8400-e29b-41d4-a716-446655440000"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			repo := &mockUpserter{
				UpsertFn: func(_ context.Context, fp, _ string) (*device.Device, error) {
					return testDevice(fp), nil
				},
			}

			inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(http.StatusOK)
			})

			handler := DeviceMiddleware(repo)(inner)

			req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
			req.Header.Set("X-Device-ID", tt.fingerprint)
			w := httptest.NewRecorder()

			handler.ServeHTTP(w, req)

			if w.Code != http.StatusOK {
				t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
			}
		})
	}
}

func TestDeviceMiddleware_WithDeviceID(t *testing.T) {
	fingerprint := "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
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
			req.Header.Set("X-Device-ID", "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899")
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
	req.Header.Set("X-Device-ID", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
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
	req.Header.Set("X-Device-ID", "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc")
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
	req.Header.Set("X-Device-ID", "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd")
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

// --- deviceCache (POE-159) ---
//
// The middleware used to upsert on every request carrying X-Device-ID, so every
// desktop request acquired a pool connection and wrote a row regardless of what
// the handler did. These tests pin the cache that removed that cost, and the
// behaviour it must not lose along the way: registration, ban enforcement, and
// device identity surviving a database failure.

// countingUpserter records how many times Upsert was called and what it was
// asked for, and lets a test swap the response mid-run.
type countingUpserter struct {
	mu     sync.Mutex
	calls  int
	dev    *device.Device
	err    error
	notify chan struct{}
}

func (c *countingUpserter) Upsert(_ context.Context, fp, _ string) (*device.Device, error) {
	c.mu.Lock()
	c.calls++
	dev, err := c.dev, c.err
	notify := c.notify
	c.mu.Unlock()

	if notify != nil {
		select {
		case notify <- struct{}{}:
		default:
		}
	}
	if err != nil {
		return nil, err
	}
	if dev == nil {
		dev = testDevice(fp)
	}
	return dev, nil
}

func (c *countingUpserter) callCount() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.calls
}

func (c *countingUpserter) failWith(err error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.err = err
}

const testFingerprint = "1111111111111111111111111111111111111111111111111111111111111111"

// cachedHandler builds the middleware over a cache with the given TTL, wrapping
// a handler that performs no work of its own — the cache-served shape.
func cachedHandler(t *testing.T, repo device.Upserter, ttl time.Duration) http.Handler {
	t.Helper()
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
	})
	return deviceMiddleware(newDeviceCache(repo, ttl, 8192))(inner)
}

func deviceRequest(handler http.Handler, fingerprint string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(http.MethodGet, "/api/analysis/status", nil)
	if fingerprint != "" {
		req.Header.Set("X-Device-ID", fingerprint)
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, req)
	return w
}

func TestDeviceCache_RepeatRequestsWithinTTL_UpsertOnce(t *testing.T) {
	repo := &countingUpserter{}
	handler := cachedHandler(t, repo, time.Minute)

	// A gem scan is four requests from one device; without the cache that was
	// four upserts against one row.
	for i := 0; i < 4; i++ {
		if w := deviceRequest(handler, testFingerprint); w.Code != http.StatusOK {
			t.Fatalf("request %d: status = %d, want 200", i, w.Code)
		}
	}

	if got := repo.callCount(); got != 1 {
		t.Errorf("upserts for 4 requests from one device = %d, want 1", got)
	}
}

func TestDeviceCache_DistinctDevices_UpsertPerDevice(t *testing.T) {
	repo := &countingUpserter{}
	handler := cachedHandler(t, repo, time.Minute)

	deviceRequest(handler, testFingerprint)
	deviceRequest(handler, "2222222222222222222222222222222222222222222222222222222222222222")
	deviceRequest(handler, "3333333333333333333333333333333333333333333333333333333333333333")

	if got := repo.callCount(); got != 3 {
		t.Errorf("upserts for 3 distinct devices = %d, want 3 (cache must be keyed per fingerprint)", got)
	}
}

func TestDeviceCache_ConcurrentFirstRequests_UpsertOnce(t *testing.T) {
	repo := &countingUpserter{}
	handler := cachedHandler(t, repo, time.Minute)

	const n = 20
	var wg sync.WaitGroup
	codes := make([]int, n)
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			codes[i] = deviceRequest(handler, testFingerprint).Code
		}(i)
	}
	wg.Wait()

	for i, c := range codes {
		if c != http.StatusOK {
			t.Fatalf("concurrent request %d: status = %d, want 200", i, c)
		}
	}
	if got := repo.callCount(); got != 1 {
		t.Errorf("upserts for %d simultaneous first requests = %d, want 1", n, got)
	}
}

func TestDeviceCache_AfterTTL_RefreshesInBackground(t *testing.T) {
	repo := &countingUpserter{notify: make(chan struct{}, 4)}
	handler := cachedHandler(t, repo, 10*time.Millisecond)

	deviceRequest(handler, testFingerprint) // registers, upsert #1
	<-repo.notify

	time.Sleep(20 * time.Millisecond) // entry is now stale

	if w := deviceRequest(handler, testFingerprint); w.Code != http.StatusOK {
		t.Fatalf("post-expiry status = %d, want 200", w.Code)
	}

	select {
	case <-repo.notify:
	case <-time.After(2 * time.Second):
		t.Fatal("no background refresh after the entry expired; last_seen would never advance")
	}
	if got := repo.callCount(); got != 2 {
		t.Errorf("upserts after one expiry = %d, want 2", got)
	}
}

// awaitRefreshSettled blocks until no background refresh is in flight for the
// fingerprint. It is a synchronisation barrier, not an assertion: without it the
// test would observe the entry before the refresh goroutine has written to it.
func awaitRefreshSettled(t *testing.T, c *deviceCache, fingerprint string) {
	t.Helper()

	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		c.mu.Lock()
		e := c.entries[fingerprint]
		c.mu.Unlock()

		e.mu.Lock()
		settled := !e.refreshing
		e.mu.Unlock()
		if settled {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatal("background refresh never completed")
}

func TestDeviceCache_BackgroundRefreshFails_StillIdentifiesDevice(t *testing.T) {
	repo := &countingUpserter{notify: make(chan struct{}, 4)}

	var seen *device.Device
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		seen = DeviceFromContext(r.Context())
		w.WriteHeader(http.StatusOK)
	})
	cache := newDeviceCache(repo, 10*time.Millisecond, 8192)
	handler := deviceMiddleware(cache)(inner)

	deviceRequest(handler, testFingerprint) // registers
	<-repo.notify

	repo.failWith(errors.New("connection refused"))
	time.Sleep(20 * time.Millisecond) // entry is now stale

	deviceRequest(handler, testFingerprint) // schedules the refresh that will fail
	<-repo.notify
	awaitRefreshSettled(t, cache, testFingerprint)

	// Only now can the failed refresh's effect on the cached record be observed.
	seen = nil
	if w := deviceRequest(handler, testFingerprint); w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", w.Code)
	}

	// The refresh failed, but the request must still carry device identity —
	// /api/lab/runs and /api/desktop/font-session reject a request without it,
	// so discarding the record on a database blip would turn a transient outage
	// into 401s.
	if seen == nil {
		t.Fatal("device missing from context after a failed refresh; identity-gated routes would 401")
	}
	if seen.Fingerprint != testFingerprint {
		t.Errorf("fingerprint = %q, want %q", seen.Fingerprint, testFingerprint)
	}
}

func TestDeviceCache_BannedDeviceServedFromCache_Returns403(t *testing.T) {
	repo := &countingUpserter{dev: testBannedDevice(testFingerprint)}
	inner := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		t.Error("inner handler must not run for a banned device")
	})
	handler := deviceMiddleware(newDeviceCache(repo, time.Minute, 8192))(inner)

	for i := 0; i < 3; i++ {
		w := deviceRequest(handler, testFingerprint)
		if w.Code != http.StatusForbidden {
			t.Fatalf("request %d: status = %d, want 403", i, w.Code)
		}
	}

	// Caching must not weaken the ban: it is enforced from the cached record,
	// without going back to the database on every request.
	if got := repo.callCount(); got != 1 {
		t.Errorf("upserts while serving a banned device = %d, want 1", got)
	}
}

func TestDeviceCache_AtCapacity_DropsExpiredEntries(t *testing.T) {
	repo := &countingUpserter{}
	cache := newDeviceCache(repo, 10*time.Millisecond, 2)

	fps := []string{
		testFingerprint,
		"2222222222222222222222222222222222222222222222222222222222222222",
		"3333333333333333333333333333333333333333333333333333333333333333",
	}
	for _, fp := range fps[:2] {
		if _, err := cache.Upsert(context.Background(), fp, ""); err != nil {
			t.Fatalf("seed upsert: %v", err)
		}
	}

	time.Sleep(20 * time.Millisecond) // both seeded entries are now expired

	if _, err := cache.Upsert(context.Background(), fps[2], ""); err != nil {
		t.Fatalf("upsert past capacity: %v", err)
	}

	cache.mu.Lock()
	size := len(cache.entries)
	cache.mu.Unlock()

	// Without eviction the map grows unbounded on distinct fingerprints.
	if size > 2 {
		t.Errorf("cache holds %d entries with max=2; expired entries were not evicted", size)
	}
}
