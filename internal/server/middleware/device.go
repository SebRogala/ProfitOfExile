package middleware

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"

	"profitofexile/internal/device"
)

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
func DeviceMiddleware(repo *device.Repository) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			fingerprint := r.Header.Get("X-Device-ID")
			if fingerprint == "" {
				next.ServeHTTP(w, r)
				return
			}

			appVersion := r.Header.Get("X-App-Version")

			d, err := repo.Upsert(r.Context(), fingerprint, appVersion)
			if err != nil {
				slog.Error("device middleware: upsert failed",
					"fingerprint", fingerprint,
					"error", err,
				)
				// Fail open — don't block the request if the DB is having issues.
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
