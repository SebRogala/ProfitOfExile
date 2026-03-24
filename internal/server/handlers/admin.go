package handlers

import (
	"context"
	"encoding/json"
	"log/slog"
	"net/http"

	"profitofexile/internal/lab"
)

// AdminRecalculate triggers a full recomputation of all analysis pipelines.
// Deletes stale v2 data for the latest snapshot and re-runs everything.
// POST /api/admin/recalculate
func AdminRecalculate(analyzer *lab.Analyzer) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		slog.Info("admin: recalculate triggered")

		go func() {
			ctx := context.Background()

			if err := analyzer.RunTransfigure(ctx); err != nil {
				slog.Error("admin recalculate: transfigure failed", "error", err)
			}
			if err := analyzer.RunFont(ctx); err != nil {
				slog.Error("admin recalculate: font failed", "error", err)
			}
			if err := analyzer.RunQuality(ctx); err != nil {
				slog.Error("admin recalculate: quality failed", "error", err)
			}
			if err := analyzer.RunTrends(ctx); err != nil {
				slog.Error("admin recalculate: trends failed", "error", err)
			}
			if err := analyzer.RecomputeLatestV2(ctx); err != nil {
				slog.Error("admin recalculate: v2 failed", "error", err)
			}

			slog.Info("admin: recalculate complete")
		}()

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]string{"status": "recalculation started"})
	}
}
