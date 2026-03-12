package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Version is set at build time via ldflags:
//
//	go build -ldflags "-X profitofexile/internal/server/handlers.Version=<sha>"
var Version = "dev"

type healthResponse struct {
	Status  string `json:"status"`
	Version string `json:"version"`
	DB      string `json:"db"`
}

// Health returns an HTTP handler that responds with the server health status.
// When a database pool is provided, it pings the database and returns 503 on
// failure. When pool is nil (e.g. in tests), DB is reported as "unavailable".
func Health(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		resp := healthResponse{
			Status:  "ok",
			Version: Version,
		}
		httpStatus := http.StatusOK

		if pool == nil {
			resp.DB = "unavailable"
		} else if err := pool.Ping(r.Context()); err != nil {
			slog.Error("health: database ping failed", "error", err)
			resp.Status = "degraded"
			resp.DB = "error"
			httpStatus = http.StatusServiceUnavailable
		} else {
			resp.DB = "ok"
		}

		data, err := json.Marshal(resp)
		if err != nil {
			slog.Error("health: marshal response", "error", err)
			http.Error(w, `{"status":"error"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(httpStatus)
		w.Write(data)
	}
}
