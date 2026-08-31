package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"
	"unicode/utf8"

	"profitofexile/internal/device"
	"profitofexile/internal/server/middleware"
)

// identifyRequest is the expected JSON body for POST /api/device/identify.
type identifyRequest struct {
	Alias string `json:"alias"`
}

// DeviceIdentify handles POST /api/device/identify. Reads the device from
// request context (set by DeviceMiddleware) and updates its alias.
// Returns 400 if no device is in context (no X-Device-ID header).
func DeviceIdentify(repo device.AliasSetter) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		d := middleware.DeviceFromContext(r.Context())
		if d == nil {
			jsonError(w, http.StatusBadRequest, "X-Device-ID header required")
			return
		}

		r.Body = http.MaxBytesReader(w, r.Body, 4096)
		var body identifyRequest
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			jsonError(w, http.StatusBadRequest, "invalid JSON body")
			return
		}

		body.Alias = strings.TrimSpace(body.Alias)

		if body.Alias == "" {
			jsonError(w, http.StatusBadRequest, "alias is required")
			return
		}

		if utf8.RuneCountInString(body.Alias) > 64 {
			jsonError(w, http.StatusBadRequest, "alias too long (max 64 characters)")
			return
		}

		if err := repo.SetAlias(r.Context(), d.Fingerprint, body.Alias); err != nil {
			slog.Error("device identify: set alias failed",
				"fingerprint", d.Fingerprint,
				"alias", body.Alias,
				"error", err,
			)
			jsonError(w, http.StatusInternalServerError, "failed to update alias")
			return
		}

		slog.Info("device identified",
			"fingerprint", d.Fingerprint,
			"alias", body.Alias,
		)

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{
			"status":      "identified",
			"fingerprint": d.Fingerprint,
			"alias":       body.Alias,
		})
	}
}

// deviceMeResponse is the JSON body of GET /api/device/me. Features is always
// encoded, and always as an array — see device.Entitlements. Alias is the
// device's registered name (null while unregistered, and always null for an
// anonymous request) so the identify dialog can say whether THIS device is
// known to the server, not only what it is entitled to.
type deviceMeResponse struct {
	Role     string   `json:"role"`
	Alias    *string  `json:"alias"`
	Channel  string   `json:"channel"`
	Features []string `json:"features"`
}

// DeviceMe handles GET /api/device/me. It reports the calling device's role
// together with the update channel and hidden features that role entitles it
// to, so the desktop app can gate its beta module and updater on one answer.
//
// It needs no repository: the device record is whatever DeviceMiddleware
// attached to the context. A request with no X-Device-ID header — and a server
// started without a device repository, where the middleware is not installed at
// all — therefore reads as no device, which maps to the same stable channel and
// empty feature list an unrecognised role gets. That is deliberate: the handler
// itself has no error path, so a missing or unknown device is answered rather
// than rejected. DeviceMiddleware can still reject the request before the
// handler runs — 400 for a malformed X-Device-ID, 403 for a banned device (see
// internal/server/middleware/device.go).
//
// The answer is per-device, so it is sent Cache-Control: no-store to keep a
// proxy from handing one device's entitlements to another.
func DeviceMe() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		role := ""
		var alias *string
		if d := middleware.DeviceFromContext(r.Context()); d != nil {
			role = d.Role
			// The middleware serves a cached record, so an alias set by
			// /api/device/identify can read stale here for up to the cache
			// TTL — the dialog shows the identify response's own answer for
			// that window, so this is display data, never a gate.
			alias = d.Alias
		}

		channel, features := device.Entitlements(role)

		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		json.NewEncoder(w).Encode(deviceMeResponse{
			Role:     role,
			Alias:    alias,
			Channel:  channel,
			Features: features,
		})
	}
}
