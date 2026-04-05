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

// AdminDevices returns an HTTP handler that lists all registered devices.
// Protected by INTERNAL_SECRET.
// GET /api/admin/devices
func AdminDevices(repo device.Lister, internalSecret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if internalSecret != "" && r.Header.Get("X-Internal-Token") != internalSecret {
			http.Error(w, "Unauthorized", http.StatusUnauthorized)
			return
		}

		devices, err := repo.List(r.Context())
		if err != nil {
			slog.Error("admin devices: list failed", "error", err)
			jsonError(w, http.StatusInternalServerError, "failed to list devices")
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"devices": devices,
			"count":   len(devices),
		})
	}
}

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
