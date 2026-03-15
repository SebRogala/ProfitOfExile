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
	throttler      *Throttler
	cache          *Cache
	logger         *slog.Logger
	muTransfigure  sync.Mutex
	muFont         sync.Mutex
	muQuality      sync.Mutex
	muTrends       sync.Mutex
}

// NewAnalyzer creates an analyzer wired to the given repository.
// The throttler may be nil — in that case no Mercure signals are emitted.
// The cache may be nil — in that case results are only persisted to the DB.
func NewAnalyzer(repo *Repository, throttler *Throttler, cache *Cache) *Analyzer {
	return &Analyzer{
		repo:      repo,
		throttler: throttler,
		cache:     cache,
		logger:    slog.Default(),
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

	if a.cache != nil {
		a.cache.SetTransfigure(results)
	}

	a.logger.Info("transfigure analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
	a.throttler.Signal()
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

	if a.cache != nil {
		a.cache.SetFont(results)
	}

	a.logger.Info("font analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
	a.throttler.Signal()
	return nil
}

// RunTrends fetches the latest gem snapshot plus historical data and computes trend signals.
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunTrends(ctx context.Context) error {
	a.muTrends.Lock()
	defer a.muTrends.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("trends: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("trends: no gem snapshots available yet, skipping")
		return nil
	}

	// Fetch 7 days of history (168 hours) for CV and historical position.
	history, err := a.repo.GemPriceHistoryByVariant(ctx, "", 168)
	if err != nil {
		a.logger.Error("trends: failed to load gem price history", "error", err)
		return err
	}

	// Fetch base gem history (shorter window — velocity needs recent data).
	baseHistory, err := a.repo.BasePriceHistory(ctx, "", 24)
	if err != nil {
		a.logger.Error("trends: failed to load base price history", "error", err)
		return err
	}

	// Compute market-wide average base listings for relative liquidity.
	marketAvg, err := a.repo.MarketAvgBaseListings(ctx, "")
	if err != nil {
		a.logger.Warn("trends: failed to compute market avg base listings, using 0", "error", err)
		marketAvg = 0
	}

	results := AnalyzeTrends(snapTime, gems, history, baseHistory, marketAvg)

	inserted, err := a.repo.SaveTrendResults(ctx, results)
	if err != nil {
		a.logger.Error("trends: failed to save results", "error", err)
		return err
	}

	if a.cache != nil {
		a.cache.SetTrends(results)
	}

	a.logger.Info("trend analysis complete",
		"snapTime", snapTime,
		"results", len(results),
		"inserted", inserted,
	)
	a.throttler.Signal()
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

	if a.cache != nil {
		a.cache.SetQuality(results)
	}

	a.logger.Info("quality analysis complete",
		"snapTime", snapTime,
		"gcpPrice", gcpPrice,
		"results", len(results),
		"inserted", inserted,
	)
	a.throttler.Signal()
	return nil
}
