package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// exceptionalSupportPricer reads the exceptional-support prices.
// *lab.Repository implements it.
type exceptionalSupportPricer interface {
	ExceptionalSupportPrices(ctx context.Context, scope league.Scope) (lab.ExceptionalSupportSnapshot, error)
}

type exceptionalSupportsResponse struct {
	Source     string                  `json:"source"`
	Variant    string                  `json:"variant"`
	AsOf       *time.Time              `json:"asOf"`
	DivineRate float64                 `json:"divineRate"`
	Gems       []exceptionalSupportGem `json:"gems"`
}

type exceptionalSupportGem struct {
	Name  string   `json:"name"`
	Chaos *float64 `json:"chaos"`
}

// ExceptionalSupportPrices serves the three Divine Font exceptional supports'
// level-1 uncorrupted poe.ninja prices in fixed order, with the divine rate
// (0 when unknown). The shared accessor of POE-328 and POE-329.
// GET /api/analysis/exceptional-supports
func ExceptionalSupportPrices(repo exceptionalSupportPricer, cache *lab.Cache, scope league.Scope) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		snap, err := repo.ExceptionalSupportPrices(r.Context(), scope)
		if err != nil {
			slog.Error("exceptional supports: query failed", "error", err)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}

		resp := exceptionalSupportsResponse{
			Source:  lab.ExceptionalSupportSource,
			Variant: lab.ExceptionalSupportVariant,
		}
		if !snap.AsOf.IsZero() {
			asOf := snap.AsOf.UTC()
			resp.AsOf = &asOf
		}
		if cache != nil {
			resp.DivineRate = cache.For(scope).DivineRate()
		}
		for _, p := range snap.Prices() {
			resp.Gems = append(resp.Gems, exceptionalSupportGem{Name: p.Name, Chaos: p.Chaos})
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(resp); err != nil {
			slog.Error("exceptional supports: encode response failed", "error", err)
		}
	}
}
