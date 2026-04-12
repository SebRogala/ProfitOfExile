package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"

	"profitofexile/internal/lab"
)

// AdminRecalculate triggers a full recomputation of all analysis pipelines.
// V2 runs before Font so Font reads fresh features with current tier classification.
// POST /api/internal/recalculate (protected by INTERNAL_SECRET)
func AdminRecalculate(analyzer *lab.Analyzer, internalSecret string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if internalSecret != "" && r.Header.Get("X-Internal-Token") != internalSecret {
			http.Error(w, "Unauthorized", http.StatusUnauthorized)
			return
		}

		slog.Info("admin: recalculate triggered")

		go func() {
			ctx := context.Background()

			if err := analyzer.RunTransfigure(ctx); err != nil {
				slog.Error("admin recalculate: transfigure failed", "error", err)
			}
			if err := analyzer.RunQuality(ctx); err != nil {
				slog.Error("admin recalculate: quality failed", "error", err)
			}
			// V2 must complete before Font — Font reads GemFeatures for tier classification.
			if err := analyzer.RecomputeLatestV2(ctx); err != nil {
				slog.Error("admin recalculate: v2 failed", "error", err)
			}
			if err := analyzer.RunFont(ctx); err != nil {
				slog.Error("admin recalculate: font failed", "error", err)
			}
			if err := analyzer.RunDedication(ctx); err != nil {
				slog.Error("admin recalculate: dedication failed", "error", err)
			}

			slog.Info("admin: recalculate complete")
		}()

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "recalculation started"})
	}
}
