package handlers

import (
	"encoding/json"
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
		w.WriteHeader(http.StatusOK)

		resp := healthResponse{
			Status:  "ok",
			Version: "dev",
		}

		json.NewEncoder(w).Encode(resp)
	}
}
