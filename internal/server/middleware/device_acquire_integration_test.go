//go:build integration

package middleware

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/device"
)

// The POE-158 cache audit rests on one claim: a cache-served route never
// acquires a database connection, which is what keeps it out of the contended
// set when the pool is saturated. That claim was measured with curl and no
// device header, and it did not hold for the desktop — every request carrying
// X-Device-ID went through an upsert in this middleware. Measured on prod
// 2026-08-04: the same cache-served route took 0 acquires without the header
// and 1 with it, and against a saturated pool it went from 0.0 ms to 1805.7 ms.
//
// These tests pin the acquire count per request so that regression fails the
// build. They exercise DeviceMiddleware — the production constructor — rather
// than the cache directly, so removing the cache from the middleware's wiring
// is itself caught.

func acquirePool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(func() { pool.Close() })

	if _, err := pool.Exec(ctx, "SELECT 1 FROM devices LIMIT 1"); err != nil {
		t.Fatalf("devices table not reachable: %v", err)
	}
	return pool
}

// probeFingerprint returns a random valid fingerprint and removes its device row
// when the test ends.
func probeFingerprint(t *testing.T, pool *pgxpool.Pool) string {
	t.Helper()

	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		t.Fatalf("random fingerprint: %v", err)
	}
	fp := hex.EncodeToString(b)

	t.Cleanup(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if _, err := pool.Exec(ctx, "DELETE FROM devices WHERE fingerprint = $1", fp); err != nil {
			t.Logf("cleanup device %s: %v", fp, err)
		}
	})
	return fp
}

// acquireRouter mounts DeviceMiddleware exactly as internal/server/server.go
// does, in front of a handler that performs no database work — the shape of
// every lab.Cache-served read under /api/analysis/*.
func acquireRouter(pool *pgxpool.Pool) http.Handler {
	r := chi.NewRouter()
	r.Use(DeviceMiddleware(device.NewRepository(pool)))
	r.Get("/api/analysis/status", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"cached":true}`))
	})
	return r
}

func doRequest(t *testing.T, h http.Handler, fingerprint string) {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, "/api/analysis/status", nil)
	if fingerprint != "" {
		req.Header.Set("X-Device-ID", fingerprint)
	}
	w := httptest.NewRecorder()
	h.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", w.Code)
	}
}

func TestDeviceMiddleware_FirstRequestFromDevice_AcquiresOneConnection(t *testing.T) {
	pool := acquirePool(t)
	fp := probeFingerprint(t, pool)
	router := acquireRouter(pool)

	before := pool.Stat().AcquireCount()
	doRequest(t, router, fp)
	got := pool.Stat().AcquireCount() - before

	// One acquire, for the registering upsert. An unknown fingerprint has to be
	// read: it is the only chance to record the device and observe a ban.
	if got != 1 {
		t.Errorf("acquires for a first request from an unknown device = %d, want 1", got)
	}
}

func TestDeviceMiddleware_RepeatRequestsFromDevice_AcquireNoConnections(t *testing.T) {
	pool := acquirePool(t)
	fp := probeFingerprint(t, pool)
	router := acquireRouter(pool)

	doRequest(t, router, fp) // register

	const n = 20
	before := pool.Stat().AcquireCount()
	for i := 0; i < n; i++ {
		doRequest(t, router, fp)
	}
	got := pool.Stat().AcquireCount() - before

	// This is the guard. A cache-served route must cost zero connections for an
	// already-known device, whatever headers the desktop attaches — otherwise
	// the route rejoins the set that stalls when the pool is saturated.
	if got != 0 {
		t.Errorf("acquires for %d repeat requests from a known device = %d, want 0", n, got)
	}
}

func TestDeviceMiddleware_RequestWithoutDeviceHeader_AcquiresNoConnections(t *testing.T) {
	pool := acquirePool(t)
	router := acquireRouter(pool)

	before := pool.Stat().AcquireCount()
	for i := 0; i < 5; i++ {
		doRequest(t, router, "")
	}
	got := pool.Stat().AcquireCount() - before

	// The browser path, unchanged: no header, no device work, no connection.
	if got != 0 {
		t.Errorf("acquires for 5 header-less requests = %d, want 0", got)
	}
}
