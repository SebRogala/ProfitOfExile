package lab

import (
	"context"
	"log/slog"
)

// Analyzer orchestrates analysis runs triggered by Mercure events.
type Analyzer struct {
	repo   *Repository
	logger *slog.Logger
}

// NewAnalyzer creates an analyzer wired to the given repository.
func NewAnalyzer(repo *Repository) *Analyzer {
	return &Analyzer{
		repo:   repo,
		logger: slog.Default(),
	}
}

// RunTransfigure fetches the latest gem snapshot and computes transfigure ROI.
func (a *Analyzer) RunTransfigure(ctx context.Context) {
	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("transfigure: failed to load gem prices", "error", err)
		return
	}

	results := AnalyzeTransfigure(snapTime, gems)

	inserted, err := a.repo.SaveTransfigureResults(ctx, results)
	if err != nil {
		a.logger.Error("transfigure: failed to save results", "error", err)
		return
	}

	a.logger.Info("transfigure analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
}
