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

// LivenessChecker reports whether an auxiliary process invariant still holds —
// specifically the server's held advisory-lock connection. If that connection
// drops, Postgres auto-releases the fence and a second server could silently
// acquire it, so the health handler treats a non-nil error as unhealthy. A nil
// LivenessChecker is skipped. *league.ProcessLock satisfies this interface.
type LivenessChecker interface {
	CheckHeld(ctx context.Context) error
}

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
// use NopPinger in tests that don't require database access. The fence may be
// nil; when set, a failed liveness check (dropped advisory-lock connection)
// also degrades the response so a fenceless server drops out of the load
// balancer.
func Health(pinger Pinger, fence LivenessChecker) http.HandlerFunc {
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

		if fence != nil {
			if err := fence.CheckHeld(r.Context()); err != nil {
				slog.Error("health: league fence liveness check failed", "error", err)
				resp.Status = statusDegraded
				httpStatus = http.StatusServiceUnavailable
			}
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
