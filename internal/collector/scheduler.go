package collector

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"time"

	"profitofexile/internal/price/gemcolor"
)

// Scheduler orchestrates periodic price data collection from external sources.
type Scheduler struct {
	repo          SnapshotStore
	fetchers      []Fetcher
	resolver      *gemcolor.Resolver
	interval      time.Duration
	league        string
	mercureURL    string
	mercureSecret string
	logger        *slog.Logger
}

// NewScheduler creates a scheduler that runs collection at the given interval.
// interval must be positive and fetchers must not be empty.
func NewScheduler(
	repo SnapshotStore,
	fetchers []Fetcher,
	resolver *gemcolor.Resolver,
	interval time.Duration,
	league string,
	mercureURL string,
	mercureSecret string,
	logger *slog.Logger,
) (*Scheduler, error) {
	if interval <= 0 {
		return nil, fmt.Errorf("scheduler: interval must be positive, got %s", interval)
	}
	if len(fetchers) == 0 {
		return nil, fmt.Errorf("scheduler: at least one fetcher is required")
	}

	return &Scheduler{
		repo:          repo,
		fetchers:      fetchers,
		resolver:      resolver,
		interval:      interval,
		league:        league,
		mercureURL:    mercureURL,
		mercureSecret: mercureSecret,
		logger:        logger,
	}, nil
}

// Run starts the collection loop. It checks for recent snapshots on startup to
// avoid redundant API calls on rapid redeploys, then ticks at the configured
// interval. Run blocks until ctx is cancelled.
func (s *Scheduler) Run(ctx context.Context) error {
	// Startup: check if a recent snapshot already exists.
	last, err := s.repo.LastGemSnapshotTime(ctx)
	if err != nil {
		// DB check failed — fall through to collect anyway. If the DB is truly
		// unreachable, collect() will independently report its own errors.
		s.logger.Error("startup: failed to check last snapshot time (degraded startup, attempting collect)", "error", err)
		s.collect(ctx)
	} else if !last.IsZero() && time.Since(last) < s.interval {
		s.logger.Info("recent snapshot exists, waiting for next tick",
			"last", last.Format(time.RFC3339),
			"age", time.Since(last).Round(time.Second).String(),
		)
	} else {
		s.collect(ctx)
	}

	ticker := time.NewTicker(s.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			s.logger.Info("scheduler stopping")
			return nil
		case <-ticker.C:
			s.collect(ctx)
		}
	}
}

// collect runs a single collection cycle across all fetchers.
func (s *Scheduler) collect(ctx context.Context) {
	if ctx.Err() != nil {
		return
	}

	start := time.Now().UTC()
	snapTime := start

	totalGems := 0
	totalCurrencies := 0
	errorCount := 0

	for i, fetcher := range s.fetchers {
		// Fetch gems.
		gems, err := fetcher.FetchGems(ctx, s.league)
		if err != nil {
			s.logger.Error("fetch gems failed", "fetcher", i, "error", err)
			errorCount++
		} else if len(gems) > 0 {
			inserted, err := s.repo.InsertGemSnapshots(ctx, snapTime, gems)
			if err != nil {
				s.logger.Error("insert gem snapshots failed", "fetcher", i, "error", err)
				errorCount++
			} else {
				totalGems += inserted
			}
		}

		// Fetch currency.
		currencies, err := fetcher.FetchCurrency(ctx, s.league)
		if err != nil {
			s.logger.Error("fetch currency failed", "fetcher", i, "error", err)
			errorCount++
		} else if len(currencies) > 0 {
			inserted, err := s.repo.InsertCurrencySnapshots(ctx, snapTime, currencies)
			if err != nil {
				s.logger.Error("insert currency snapshots failed", "fetcher", i, "error", err)
				errorCount++
			} else {
				totalCurrencies += inserted
			}
		}
	}

	// Persist newly resolved gem colors.
	if s.resolver != nil {
		if err := s.resolver.UpsertDiscoveries(ctx); err != nil {
			s.logger.Error("upsert gem color discoveries failed", "error", err)
			errorCount++
		}
	}

	// Publish Mercure event (non-fatal on failure).
	// Marshal failure = programming bug (Error); publish failure = transient infra issue (Warn).
	payload, err := json.Marshal(map[string]string{
		"league":    s.league,
		"timestamp": snapTime.Format(time.RFC3339),
	})
	if err != nil {
		s.logger.Error("marshal mercure payload", "error", err)
	} else if err := PublishMercureEvent(ctx, s.mercureURL, s.mercureSecret, "prices-updated", string(payload)); err != nil {
		s.logger.Warn("mercure publish failed", "error", err)
	}

	elapsed := time.Since(start)
	logLevel := slog.LevelInfo
	if errorCount > 0 && totalGems == 0 && totalCurrencies == 0 {
		logLevel = slog.LevelError
	}
	s.logger.Log(ctx, logLevel, "snapshot complete",
		"gems", totalGems,
		"currencies", totalCurrencies,
		"errors", errorCount,
		"duration", elapsed.Round(time.Millisecond).String(),
	)
}
