package collector

import (
	"context"
	"encoding/json"
	"log/slog"
	"time"

	"profitofexile/internal/price/gemcolor"
)

// Scheduler orchestrates periodic price data collection from external sources.
type Scheduler struct {
	repo          *Repository
	fetchers      []Fetcher
	resolver      *gemcolor.Resolver
	interval      time.Duration
	league        string
	mercureURL    string
	mercureSecret string
	logger        *slog.Logger
}

// NewScheduler creates a scheduler that runs collection at the given interval.
func NewScheduler(
	repo *Repository,
	fetchers []Fetcher,
	resolver *gemcolor.Resolver,
	interval time.Duration,
	league string,
	mercureURL string,
	mercureSecret string,
	logger *slog.Logger,
) *Scheduler {
	return &Scheduler{
		repo:          repo,
		fetchers:      fetchers,
		resolver:      resolver,
		interval:      interval,
		league:        league,
		mercureURL:    mercureURL,
		mercureSecret: mercureSecret,
		logger:        logger,
	}
}

// Run starts the collection loop. It checks for recent snapshots on startup to
// avoid redundant API calls on rapid redeploys, then ticks at the configured
// interval. Run blocks until ctx is cancelled.
func (s *Scheduler) Run(ctx context.Context) error {
	// Startup: check if a recent snapshot already exists.
	last, err := s.repo.LastGemSnapshotTime(ctx)
	if err != nil {
		s.logger.Error("startup: failed to check last snapshot time", "error", err)
	}

	if err == nil && !last.IsZero() && time.Since(last) < s.interval {
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
	start := time.Now()
	now := time.Now().UTC()

	totalGems := 0
	totalCurrencies := 0

	for i, fetcher := range s.fetchers {
		// Fetch gems.
		gems, err := fetcher.FetchGems(ctx, s.league)
		if err != nil {
			s.logger.Error("fetch gems failed", "fetcher", i, "error", err)
		} else if len(gems) > 0 {
			inserted, err := s.repo.InsertGemSnapshots(ctx, now, gems)
			if err != nil {
				s.logger.Error("insert gem snapshots failed", "fetcher", i, "error", err)
			} else {
				totalGems += inserted
			}
		}

		// Fetch currency.
		currencies, err := fetcher.FetchCurrency(ctx, s.league)
		if err != nil {
			s.logger.Error("fetch currency failed", "fetcher", i, "error", err)
		} else if len(currencies) > 0 {
			inserted, err := s.repo.InsertCurrencySnapshots(ctx, now, currencies)
			if err != nil {
				s.logger.Error("insert currency snapshots failed", "fetcher", i, "error", err)
			} else {
				totalCurrencies += inserted
			}
		}
	}

	// Persist newly resolved gem colors.
	if s.resolver != nil {
		if err := s.resolver.UpsertDiscoveries(ctx); err != nil {
			s.logger.Error("upsert gem color discoveries failed", "error", err)
		}

		if unresolved := s.resolver.UnresolvedGems(); len(unresolved) > 0 {
			s.logger.Warn("unresolved gem colors", "count", len(unresolved), "gems", unresolved)
		}
	}

	// Publish Mercure event (non-fatal on failure).
	payload, _ := json.Marshal(map[string]string{
		"league":    s.league,
		"timestamp": now.Format(time.RFC3339),
	})
	if err := PublishMercureEvent(ctx, s.mercureURL, s.mercureSecret, "prices-updated", string(payload)); err != nil {
		s.logger.Warn("mercure publish failed", "error", err)
	}

	elapsed := time.Since(start)
	s.logger.Info("snapshot complete",
		"gems", totalGems,
		"currencies", totalCurrencies,
		"duration", elapsed.Round(time.Millisecond).String(),
	)
}
