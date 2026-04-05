package middleware

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"

	"profitofexile/internal/device"
)

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
// ID is present it upserts the device record (auto-registration), attaches it
// to the request context, and enforces bans. Requests without X-Device-ID pass
// through untracked.
func DeviceMiddleware(repo device.Upserter) func(http.Handler) http.Handler {
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
