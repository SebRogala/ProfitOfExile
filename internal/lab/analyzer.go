package lab

import (
	"context"
	"fmt"
	"log/slog"
	"sync"

	"profitofexile/internal/trade"
)

// Analyzer orchestrates analysis runs triggered by Mercure events.
// Each analysis type has its own mutex so independent analyses run in parallel.
type Analyzer struct {
	repo           *Repository
	throttler      *Throttler
	cache          *Cache
	tradeCache     *trade.TradeCache // nil-safe: when nil, trade enrichment is skipped
	logger         *slog.Logger
	muTransfigure  sync.Mutex
	muFont         sync.Mutex
	muQuality      sync.Mutex
	muDedication   sync.Mutex
	muV2           sync.Mutex
}

// NewAnalyzer creates an analyzer wired to the given repository.
// The throttler may be nil — in that case no Mercure signals are emitted.
// The cache may be nil — in that case results are only persisted to the DB.
// The tradeCache may be nil — in that case trade enrichment is skipped in ComputeGemFeatures.
func NewAnalyzer(repo *Repository, throttler *Throttler, cache *Cache, tradeCache *trade.TradeCache) *Analyzer {
	return &Analyzer{
		repo:       repo,
		throttler:  throttler,
		cache:      cache,
		tradeCache: tradeCache,
		logger:     slog.Default(),
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
// It requires GemFeature data for tier-based winner classification.
// Features are loaded from cache first, then DB fallback.
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

	// Load gem features: try cache first, fall back to DB.
	var features []GemFeature
	if a.cache != nil {
		features = a.cache.GemFeatures()
	}
	if len(features) == 0 {
		features, err = a.repo.LatestGemFeatures(ctx, "", "", 50000)
		if err != nil {
			a.logger.Error("font: failed to load gem features", "error", err)
			return err
		}
	}
	if len(features) == 0 {
		a.logger.Info("font: no gem features available yet, skipping (run v2 pipeline first)")
		return nil
	}

	analysis := AnalyzeFont(snapTime, features)

	// Combine all three modes for DB persistence.
	allResults := make([]FontResult, 0, len(analysis.Safe)+len(analysis.Premium)+len(analysis.Jackpot))
	allResults = append(allResults, analysis.Safe...)
	allResults = append(allResults, analysis.Premium...)
	allResults = append(allResults, analysis.Jackpot...)

	inserted, err := a.repo.SaveFontResults(ctx, allResults)
	if err != nil {
		a.logger.Error("font: failed to save results", "error", err)
		return err
	}

	if a.cache != nil {
		a.cache.SetFont(analysis)
	}

	a.logger.Info("font analysis complete",
		"snapTime", snapTime,
		"safe", len(analysis.Safe),
		"premium", len(analysis.Premium),
		"jackpot", len(analysis.Jackpot),
		"inserted", inserted,
	)
	a.throttler.Signal()
	return nil
}

// RunDedication fetches the latest gem snapshot and computes Dedication lab EV
// for corrupted 21/23 gems (both skills and transfigured pools).
// It requires GemFeature data for risk-adjustment (sell probability, stability discount).
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunDedication(ctx context.Context) error {
	a.muDedication.Lock()
	defer a.muDedication.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("dedication: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("dedication: no gem snapshots available yet, skipping")
		return nil
	}

	// Load gem features: try cache first, fall back to DB.
	var features []GemFeature
	if a.cache != nil {
		features = a.cache.GemFeatures()
	}
	if len(features) == 0 {
		features, err = a.repo.LatestGemFeatures(ctx, "", "", 50000)
		if err != nil {
			a.logger.Error("dedication: failed to load gem features", "error", err)
			return err
		}
	}
	// Dedication can still run without features — risk adjustments use defaults.

	analysis := AnalyzeDedication(snapTime, gems, features)

	// Combine both pools for DB persistence.
	allResults := make([]DedicationResult, 0, len(analysis.Skills)+len(analysis.Transfigured))
	allResults = append(allResults, analysis.Skills...)
	allResults = append(allResults, analysis.Transfigured...)

	inserted, err := a.repo.SaveDedicationResults(ctx, allResults)
	if err != nil {
		a.logger.Error("dedication: failed to save results", "error", err)
		return err
	}

	if a.cache != nil {
		a.cache.SetDedication(analysis)

		// Also populate corrupted gem name caches for autocomplete.
		skillNames, err := a.repo.CorruptedGemNamesAutocomplete(ctx, false, 1000)
		if err != nil {
			a.logger.Warn("dedication: failed to load corrupted skill gem names", "error", err)
		}
		transfiguredNames, err := a.repo.CorruptedGemNamesAutocomplete(ctx, true, 1000)
		if err != nil {
			a.logger.Warn("dedication: failed to load corrupted transfigured gem names", "error", err)
		}
		a.cache.SetCorruptedGemNames(skillNames, transfiguredNames)
	}

	a.logger.Info("dedication analysis complete",
		"snapTime", snapTime,
		"skills", len(analysis.Skills),
		"transfigured", len(analysis.Transfigured),
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
		a.cache.SetGCPPrice(gcpPrice)
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

// RunV2 is the entry point for the v2 pre-computed analysis pipeline.
// Computes and persists MarketContext, GemFeatures, and GemSignals per snapshot.
// It is safe to call from multiple goroutines; concurrent runs are serialized.
func (a *Analyzer) RunV2(ctx context.Context) error {
	a.muV2.Lock()
	defer a.muV2.Unlock()

	gems, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		a.logger.Error("v2: failed to load gem prices", "error", err)
		return err
	}
	if len(gems) == 0 {
		a.logger.Info("v2: no gem snapshots available yet, skipping")
		return nil
	}

	// Fetch history for velocity computation.
	history, err := a.repo.GemPriceHistoryByVariant(ctx, "", 168)
	if err != nil {
		a.logger.Error("v2: failed to load gem price history", "error", err)
		return err
	}

	// Step 0: Unified gem classification (CASCADE → TOP → tiers).
	classification := ComputeGemClassification(gems)

	mc := ComputeMarketContext(snapTime, gems, history, classification)
	if err := a.repo.SaveMarketContext(ctx, mc); err != nil {
		a.logger.Error("v2: failed to save market context", "error", err)
		return err
	}
	if a.cache != nil {
		a.cache.SetMarketContext(&mc)
	}

	// Normalize history using temporal coefficients before computing features.
	depthMap := PrecomputeMarketDepth(gems, mc)
	normalizedHistory := NormalizeHistoryDepthGated(history, mc, depthMap)

	features := ComputeGemFeatures(snapTime, gems, normalizedHistory, mc, classification.Gems, a.tradeCache)
	inserted, err := a.repo.SaveGemFeatures(ctx, features)
	if err != nil {
		a.logger.Error("v2: failed to save gem features", "error", err)
		return err
	}
	if a.cache != nil {
		a.cache.SetGemFeatures(features)
	}
	a.logger.Info("v2 gem features computed",
		"snapTime", snapTime,
		"features", len(features),
		"inserted", inserted,
	)

	// Load base-side data needed for sellUrgency and windowSignal classifiers.
	baseHistory, err := a.repo.BasePriceHistory(ctx, "", 24)
	if err != nil {
		a.logger.Error("v2: failed to load base price history", "error", err)
		return err
	}
	marketAvgBaseLst, err := a.repo.MarketAvgBaseListings(ctx, "")
	if err != nil {
		a.logger.Warn("v2: failed to compute market avg base listings, using 0", "error", err)
		marketAvgBaseLst = 0
	}

	signals := ComputeGemSignals(snapTime, features, mc, gems, baseHistory, marketAvgBaseLst)
	insertedSig, err := a.repo.SaveGemSignals(ctx, signals)
	if err != nil {
		a.logger.Error("v2: failed to save gem signals", "error", err)
		return err
	}
	if a.cache != nil {
		a.cache.SetGemSignals(signals)
	}
	a.logger.Info("v2 gem signals computed",
		"snapTime", snapTime,
		"signals", len(signals),
		"inserted", insertedSig,
	)

	a.logger.Info("v2 analysis complete", "snapTime", snapTime, "gems", len(gems))
	a.throttler.Signal()
	return nil
}

// RecomputeLatestV2 deletes the latest snapshot's computed v2 data and re-runs
// the full pipeline. Use on startup to force recomputation after a deploy with
// new scoring logic — otherwise ON CONFLICT DO NOTHING would keep stale data.
func (a *Analyzer) RecomputeLatestV2(ctx context.Context) error {
	_, snapTime, err := a.repo.LatestGemPrices(ctx)
	if err != nil {
		return err
	}
	if snapTime.IsZero() {
		return nil
	}
	a.logger.Info("recomputing latest v2 snapshot", "snapTime", snapTime)
	if err := a.repo.DeleteV2ForSnapshot(ctx, snapTime); err != nil {
		return fmt.Errorf("recompute: delete old data: %w", err)
	}
	return a.RunV2(ctx)
}
