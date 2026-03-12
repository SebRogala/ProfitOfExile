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

// Version is set at build time via ldflags:
//
//	go build -ldflags "-X profitofexile/internal/server/handlers.Version=<sha>"
var Version = "dev"

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
// When a Pinger is provided, it pings the database and returns 503 on
// failure. When pinger is nil (e.g. in tests), DB is reported as "unavailable"
// but the overall status remains "ok" with HTTP 200 — this path exists for
// test convenience and should not occur in production.
func Health(pinger Pinger) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		resp := healthResponse{
			Status:  "ok",
			Version: Version,
		}
		httpStatus := http.StatusOK

		if pinger == nil {
			slog.Warn("health: database pool is nil, reporting DB as unavailable")
			resp.DB = dbStatusUnavailable
		} else if err := pinger.Ping(r.Context()); err != nil {
			slog.Error("health: database ping failed", "error", err)
			resp.Status = "degraded"
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
