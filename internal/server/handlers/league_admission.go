package handlers

import (
	"encoding/json"
	"errors"
	"net/http"

	"profitofexile/internal/league"
)

// admitLeague maps the shared admission rule to the contribution HTTP contract.
func admitLeague(w http.ResponseWriter, scope league.Scope, claimed string) bool {
	err := scope.Admit(claimed)
	if err == nil {
		return true
	}
	switch {
	case errors.Is(err, league.ErrLeagueAbsent):
		jsonError(w, http.StatusBadRequest, "league_absent")
	case errors.Is(err, league.ErrLeagueMismatch):
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]string{"error": "league_mismatch", "league": scope.ID()})
	default:
		jsonError(w, http.StatusServiceUnavailable, "league_unavailable")
	}
	return false
}
