package lab

import (
	"context"
	"log/slog"
	"sync"
)

// Analyzer orchestrates analysis runs triggered by Mercure events.
// Only one RunTransfigure executes at a time; concurrent calls are dropped.
type Analyzer struct {
	repo   *Repository
	logger *slog.Logger
	mu     sync.Mutex
}

// NewAnalyzer creates an analyzer wired to the given repository.
func NewAnalyzer(repo *Repository) *Analyzer {
	return &Analyzer{
		repo:   repo,
		logger: slog.Default(),
	}
}

// RunTransfigure fetches the latest gem snapshot and computes transfigure ROI.
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunTransfigure(ctx context.Context) error {
	a.mu.Lock()
	defer a.mu.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("transfigure: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("transfigure: no gem snapshots available yet, skipping")
		return nil
	}

	results := AnalyzeTransfigure(snapTime, gems)

	inserted, err := a.repo.SaveTransfigureResults(ctx, results)
	if err != nil {
		a.logger.Error("transfigure: failed to save results", "error", err)
		return err
	}

	a.logger.Info("transfigure analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
	return nil
}
