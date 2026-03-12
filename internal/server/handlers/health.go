package handlers

import (
	"encoding/json"
	"log/slog"
	"net/http"
)

type healthResponse struct {
	Status  string `json:"status"`
	Version string `json:"version"`
}

// Health returns an HTTP handler that responds with the server health status.
func Health() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")

		resp := healthResponse{
			Status:  "ok",
			Version: "dev",
		}

		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("health: encode response", "error", err)
		}
	}
}
