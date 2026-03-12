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
}

// Health returns an HTTP handler that responds with the server health status.
// When a database pool is provided, it pings the database and returns 503 on
// failure. When pool is nil (e.g. in tests), the DB check is skipped.
func Health(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if pool != nil {
			if err := pool.Ping(r.Context()); err != nil {
				slog.Error("health: database ping failed", "error", err)
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusServiceUnavailable)
				w.Write([]byte(`{"status":"error","version":"` + Version + `"}`))
				return
			}
		}

		resp := healthResponse{
			Status:  "ok",
			Version: Version,
		}

		data, err := json.Marshal(resp)
		if err != nil {
			slog.Error("health: marshal response", "error", err)
			http.Error(w, `{"status":"error"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		w.Write(data)
	}
}
