package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
)

// Pinger abstracts the database ping operation for health checking.
// *pgxpool.Pool satisfies this interface.
type Pinger interface {
	Ping(ctx context.Context) error
}

// NopPinger is a Pinger that always succeeds. Use in tests that don't
// require database access.
type NopPinger struct{}

// Ping always returns nil.
func (NopPinger) Ping(context.Context) error { return nil }

// Version is set at build time via ldflags:
//
//	go build -ldflags "-X profitofexile/internal/server/handlers.Version=<sha>"
var Version = "dev"

// Health status constants for the health endpoint.
const (
	statusOK       = "ok"
	statusDegraded = "degraded"
)

// DB status constants for the health endpoint.
const (
	dbStatusOK          = "ok"
	dbStatusUnavailable = "unavailable"
	dbStatusError       = "error"
)

type healthResponse struct {
	Status  string `json:"status"`
	Version string `json:"version"`
	DB      string `json:"db"`
}

// Health returns an HTTP handler that responds with the server health status.
// It pings the database and returns 503 on failure. The pinger must not be nil;
// use NopPinger in tests that don't require database access.
func Health(pinger Pinger) http.HandlerFunc {
	if pinger == nil {
		panic("handlers.Health: pinger must not be nil (use NopPinger for tests)")
	}
	return func(w http.ResponseWriter, r *http.Request) {
		resp := healthResponse{
			Status:  statusOK,
			Version: Version,
		}
		httpStatus := http.StatusOK

		if err := pinger.Ping(r.Context()); err != nil {
			slog.Error("health: database ping failed", "error", err)
			resp.Status = statusDegraded
			resp.DB = dbStatusError
			httpStatus = http.StatusServiceUnavailable
		} else {
			resp.DB = dbStatusOK
		}

		data, err := json.Marshal(resp)
		if err != nil {
			slog.Error("health: marshal response", "error", err)
			http.Error(w, `{"status":"error"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(httpStatus)
		if _, err := w.Write(data); err != nil {
			slog.Error("health: write response", "error", err)
		}
	}
}
