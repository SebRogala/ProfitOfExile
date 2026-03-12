package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
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
func Health() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
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
