package lab

import (
	"context"
	"log/slog"
	"sync"
)

// Analyzer orchestrates analysis runs triggered by Mercure events.
// Each analysis type has its own mutex so independent analyses run in parallel.
type Analyzer struct {
	repo           *Repository
	logger         *slog.Logger
	muTransfigure  sync.Mutex
	muFont         sync.Mutex
	muQuality      sync.Mutex
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
	a.muTransfigure.Lock()
	defer a.muTransfigure.Unlock()

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

// RunFont fetches the latest gem snapshot and computes Font of Divine Skill EV.
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunFont(ctx context.Context) error {
	a.muFont.Lock()
	defer a.muFont.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("font: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("font: no gem snapshots available yet, skipping")
		return nil
	}

	results := AnalyzeFont(snapTime, gems)

	inserted, err := a.repo.SaveFontResults(ctx, results)
	if err != nil {
		a.logger.Error("font: failed to save results", "error", err)
		return err
	}

	a.logger.Info("font analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
	return nil
}

// RunQuality fetches the latest gem snapshot and computes quality-roll ROI.
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunQuality(ctx context.Context) error {
	a.muQuality.Lock()
	defer a.muQuality.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("quality: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("quality: no gem snapshots available yet, skipping")
		return nil
	}

	gcpPrice, err := a.repo.LatestGCPPrice(ctx)
	if err != nil {
		gcpPrice = 4.0
		a.logger.Warn("quality: using default GCP price", "default", gcpPrice, "error", err)
	}

	results := AnalyzeQuality(snapTime, gems, gcpPrice)

	inserted, err := a.repo.SaveQualityResults(ctx, results)
	if err != nil {
		a.logger.Error("quality: failed to save results", "error", err)
		return err
	}

	a.logger.Info("quality analysis complete",
		"snapTime", snapTime,
		"gcpPrice", gcpPrice,
		"results", len(results),
		"inserted", inserted,
	)
	return nil
}
