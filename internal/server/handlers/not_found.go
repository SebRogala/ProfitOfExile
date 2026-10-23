package handlers

import "net/http"

// APINotFound keeps missing API endpoints on the JSON error contract.
func APINotFound(w http.ResponseWriter, r *http.Request) {
	jsonError(w, http.StatusNotFound, "not found")
}
