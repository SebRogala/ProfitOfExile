package middleware

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"sync"
	"time"

	"profitofexile/internal/device"
)

const (
	// deviceCacheTTL bounds how stale a cached device record may be. It is the
	// single knob trading DB writes against freshness, and it prices two things:
	//
	//   - last_seen precision. Every consumer is coarse: Repository.Stats buckets
	//     activity at 8h/24h/7d, List/ListIdentified only ORDER BY it, and the
	//     promote CLI prints it. A minute of skew is invisible to all of them.
	//   - ban propagation. A device with a warm cache entry keeps its cached
	//     Banned flag until the entry expires, so a ban takes effect within one
	//     TTL rather than on the next request. Bans are applied by hand in SQL —
	//     the repository has no setter and no CLI verb — so this was never an
	//     instantaneous path.
	deviceCacheTTL = 60 * time.Second

	// deviceCacheMaxEntries caps the cache so a flood of distinct well-formed
	// fingerprints cannot grow the map without bound. Prod carries 375 devices
	// total, so this is a safety valve rather than a tuning parameter.
	deviceCacheMaxEntries = 8192

	// deviceRefreshTimeout bounds a background refresh. The request that
	// triggered it has already been served, so this only stops a stalled
	// refresh from pinning a pool slot indefinitely.
	deviceRefreshTimeout = 10 * time.Second
)

// deviceEntry is one cached device record. Its mutex serializes refreshes for a
// single fingerprint so concurrent requests from one device collapse into at
// most one upsert instead of racing for the same row lock.
type deviceEntry struct {
	mu         sync.Mutex
	dev        *device.Device
	expiresAt  time.Time
	refreshing bool
}

// deviceCache fronts a device.Upserter with a TTL cache so a warm device costs
// zero pool acquires.
//
// Why this exists: the middleware runs before every handler, and the upsert it
// performed on each request meant a request carrying X-Device-ID acquired a
// connection and wrote a row no matter what the handler did. Measured against
// prod (POE-159): a cache-served route took 0 acquires without the header and 1
// with it, and with the pool saturated the same request went from 0.0 ms
// without the header to 1805.7 ms with it. The write was cheap; joining the set
// of requests that wait on a connection was not.
type deviceCache struct {
	repo device.Upserter
	ttl  time.Duration
	max  int

	mu      sync.Mutex
	entries map[string]*deviceEntry
}

func newDeviceCache(repo device.Upserter, ttl time.Duration, max int) *deviceCache {
	return &deviceCache{
		repo:    repo,
		ttl:     ttl,
		max:     max,
		entries: make(map[string]*deviceEntry),
	}
}

// entryFor returns the cache entry for a fingerprint, creating it if absent.
func (c *deviceCache) entryFor(fingerprint string) *deviceEntry {
	c.mu.Lock()
	defer c.mu.Unlock()

	if e, ok := c.entries[fingerprint]; ok {
		return e
	}
	if len(c.entries) >= c.max {
		c.evictLocked()
	}
	e := &deviceEntry{}
	c.entries[fingerprint] = e
	return e
}

// evictLocked drops expired entries, falling back to dropping an arbitrary one
// when every entry is still live. Callers must hold c.mu.
//
// It probes each entry with TryLock and skips the ones it cannot take: a held
// entry lock means a cold upsert is in flight for that fingerprint, and blocking
// on it here would stall every other fingerprint's lookup behind c.mu for the
// duration of a database round trip.
func (c *deviceCache) evictLocked() {
	now := time.Now()
	evicted := false
	for fp, e := range c.entries {
		if !e.mu.TryLock() {
			continue
		}
		expired := e.dev == nil || now.After(e.expiresAt)
		e.mu.Unlock()
		if expired {
			delete(c.entries, fp)
			evicted = true
		}
	}
	if evicted {
		return
	}
	for fp := range c.entries {
		delete(c.entries, fp)
		return
	}
}

// Upsert returns the device for a fingerprint, hitting the database only when
// the fingerprint is unknown to this process. A known-but-stale record is
// served immediately and refreshed in the background, so the steady-state
// request path performs no database work at all.
//
// It satisfies device.Upserter, so the middleware is unaware of the caching.
func (c *deviceCache) Upsert(ctx context.Context, fingerprint, appVersion string) (*device.Device, error) {
	e := c.entryFor(fingerprint)

	e.mu.Lock()
	if e.dev != nil {
		d := e.dev
		if time.Now().After(e.expiresAt) && !e.refreshing {
			e.refreshing = true
			go c.refresh(e, fingerprint, appVersion)
		}
		e.mu.Unlock()
		return d, nil
	}

	// Unknown fingerprint: the record has to be read synchronously. This is the
	// only path that can register the device and the only one that can observe a
	// ban for a device this process has not seen before. The lock is held across
	// the call so a burst from one new device performs a single upsert.
	d, err := c.repo.Upsert(ctx, fingerprint, appVersion)
	if err != nil {
		e.mu.Unlock()
		return nil, err
	}
	e.dev = d
	e.expiresAt = time.Now().Add(c.ttl)
	e.mu.Unlock()
	return d, nil
}

// refresh writes last_seen and re-reads the record out of band. It runs on its
// own context because the request that scheduled it has already been answered.
// On failure the previously cached record stays in place: serving a record that
// is a minute stale beats dropping device identity, which would make
// /api/lab/runs and /api/desktop/font-session reject the request outright.
func (c *deviceCache) refresh(e *deviceEntry, fingerprint, appVersion string) {
	ctx, cancel := context.WithTimeout(context.Background(), deviceRefreshTimeout)
	defer cancel()

	d, err := c.repo.Upsert(ctx, fingerprint, appVersion)

	e.mu.Lock()
	defer e.mu.Unlock()
	e.refreshing = false
	if err != nil {
		// Back off for one TTL rather than retrying on every request.
		e.expiresAt = time.Now().Add(c.ttl)
		slog.Warn("device middleware: background refresh failed, serving cached record",
			"fingerprint", fingerprint,
			"error", err,
		)
		return
	}
	e.dev = d
	e.expiresAt = time.Now().Add(c.ttl)
}

// validFingerprint checks that the fingerprint is either a 64-char lowercase
// hex string (SHA-256 from desktop app) or a 36-char UUID (fallback format).
// Only lowercase hex digits and hyphens are accepted.
func validFingerprint(fp string) bool {
	n := len(fp)
	if n != 36 && n != 64 {
		return false
	}
	for _, c := range fp {
		if !((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || c == '-') {
			return false
		}
	}
	return true
}

// deviceCtxKey is the context key for the device record set by DeviceMiddleware.
type deviceCtxKey struct{}

// DeviceFromContext returns the *device.Device from the request context, or nil
// if no device header was provided (web/curl requests).
func DeviceFromContext(ctx context.Context) *device.Device {
	d, _ := ctx.Value(deviceCtxKey{}).(*device.Device)
	return d
}

// DeviceMiddleware reads X-Device-ID and X-App-Version headers. When a device
// ID is present it resolves the device record (auto-registering on first
// sight), attaches it to the request context, and enforces bans. Requests
// without X-Device-ID pass through untracked.
//
// Resolution goes through a per-process TTL cache (deviceCacheTTL), so only the
// first request from a given device costs a pool acquire and a write; the rest
// are served from memory. See deviceCache for the measurement that motivated it.
func DeviceMiddleware(repo device.Upserter) func(http.Handler) http.Handler {
	return deviceMiddleware(newDeviceCache(repo, deviceCacheTTL, deviceCacheMaxEntries))
}

// deviceMiddleware is the uncached core, split out so tests can supply a cache
// with a controlled TTL.
func deviceMiddleware(repo device.Upserter) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			fingerprint := r.Header.Get("X-Device-ID")
			if fingerprint == "" {
				next.ServeHTTP(w, r)
				return
			}

			if !validFingerprint(fingerprint) {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				json.NewEncoder(w).Encode(map[string]string{"error": "invalid device fingerprint format"})
				return
			}

			appVersion := r.Header.Get("X-App-Version")

			d, err := repo.Upsert(r.Context(), fingerprint, appVersion)
			if err != nil {
				slog.Error("device middleware: upsert failed",
					"fingerprint", fingerprint,
					"error", err,
				)
				// Fail open: pass the request through WITHOUT attaching a device to
				// context. This means banned-device checks can't fire (no record to
				// inspect), but any handler that requires a device (e.g. DeviceIdentify)
				// will get "no device" and return 400. Acceptable tradeoff: a DB outage
				// is temporary and a banned device may slip through for seconds.
				next.ServeHTTP(w, r)
				return
			}

			if d.Banned {
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusForbidden)
				json.NewEncoder(w).Encode(map[string]string{"error": "device is banned"})
				return
			}

			ctx := context.WithValue(r.Context(), deviceCtxKey{}, d)
			next.ServeHTTP(w, r.WithContext(ctx))
		})
	}
}
