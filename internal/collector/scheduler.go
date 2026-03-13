package collector

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"math/rand/v2"
	"runtime/debug"
	"sync"
	"time"

	"profitofexile/internal/price/gemcolor"
)

// perSourceSemaphoreCapacity is the number of concurrent requests allowed per
// source (e.g., "ninja"). Limits concurrent requests to the same API to avoid
// rate-limiting.
const perSourceSemaphoreCapacity = 3

// Scheduler orchestrates price data collection with independent goroutines per
// endpoint. Each endpoint runs its own fetch-sleep loop with cache-aware sleep
// calculation. Endpoints sharing a Source field share a rate-limit semaphore.
type Scheduler struct {
	endpoints     []EndpointConfig
	resolver      *gemcolor.Resolver
	league        string
	mercureURL    string
	mercureSecret string
	logger        *slog.Logger
	semaphores    map[string]chan struct{}
}

// NewScheduler creates a scheduler that runs each endpoint in its own goroutine.
// At least one endpoint is required.
func NewScheduler(
	endpoints []EndpointConfig,
	resolver *gemcolor.Resolver,
	league string,
	mercureURL string,
	mercureSecret string,
	logger *slog.Logger,
) (*Scheduler, error) {
	if len(endpoints) == 0 {
		return nil, fmt.Errorf("scheduler: at least one endpoint is required")
	}

	// Validate each endpoint configuration at startup to surface
	// misconfiguration early rather than as a runtime panic inside a goroutine.
	for i, ep := range endpoints {
		if err := ep.Validate(); err != nil {
			return nil, fmt.Errorf("scheduler: endpoint %d: %w", i, err)
		}
		if ep.Source == "" {
			logger.Warn("endpoint has empty Source, rate limiting will be skipped",
				"endpoint", ep.Name,
			)
		}
	}

	// Build per-source semaphores (limits concurrent requests to the same API
	// to avoid rate-limiting).
	sems := make(map[string]chan struct{})
	for _, ep := range endpoints {
		if ep.Source != "" {
			if _, ok := sems[ep.Source]; !ok {
				sems[ep.Source] = make(chan struct{}, perSourceSemaphoreCapacity)
			}
		}
	}

	// Defensive copy to prevent caller mutations from affecting the scheduler.
	eps := make([]EndpointConfig, len(endpoints))
	copy(eps, endpoints)

	return &Scheduler{
		endpoints:     eps,
		resolver:      resolver,
		league:        league,
		mercureURL:    mercureURL,
		mercureSecret: mercureSecret,
		logger:        logger,
		semaphores:    sems,
	}, nil
}

// Run launches a goroutine per endpoint with startup jitter, then blocks until
// ctx is cancelled. All goroutines are waited on for clean shutdown.
func (s *Scheduler) Run(ctx context.Context) error {
	var wg sync.WaitGroup

	for i := range s.endpoints {
		ep := s.endpoints[i]
		wg.Add(1)
		go func() {
			defer wg.Done()
			defer func() {
				if r := recover(); r != nil {
					s.logger.Error("endpoint goroutine panicked — this endpoint will stop collecting permanently",
						"endpoint", ep.Name,
						"panic", r,
						"stack", string(debug.Stack()),
					)
				}
			}()

			// Startup jitter: random delay between JitterMin and JitterMax.
			jitter := s.startupJitter(ep)
			if jitter > 0 {
				s.logger.Info("endpoint startup jitter",
					"endpoint", ep.Name,
					"jitter", jitter.Round(time.Millisecond).String(),
				)
				select {
				case <-ctx.Done():
					return
				case <-time.After(jitter):
				}
			}

			s.runEndpoint(ctx, ep)
		}()
	}

	<-ctx.Done()
	s.logger.Info("scheduler stopping, waiting for endpoints to finish")
	wg.Wait()
	s.logger.Info("scheduler stopped")
	return nil
}

// startupJitter returns a random duration between ep.JitterMin and ep.JitterMax.
// Returns 0 if JitterMax <= JitterMin or JitterMax is 0.
func (s *Scheduler) startupJitter(ep EndpointConfig) time.Duration {
	if ep.JitterMax <= ep.JitterMin || ep.JitterMax == 0 {
		return 0
	}
	spread := ep.JitterMax - ep.JitterMin
	return ep.JitterMin + time.Duration(rand.Int64N(int64(spread)))
}

// endpointState holds per-goroutine mutable state for a running endpoint.
// Encapsulates the ETag and retry counter that persist across fetch cycles.
type endpointState struct {
	lastETag   string
	retryCount int
}

// runEndpoint is the core loop for a single endpoint. It handles startup
// staleness checks, fetching, storing, and cache-aware sleep calculation.
func (s *Scheduler) runEndpoint(ctx context.Context, ep EndpointConfig) {
	state := &endpointState{}

	// Startup staleness check: if recent data exists, skip the first fetch.
	if sleep := s.checkStaleness(ctx, ep); sleep > 0 {
		s.logger.Info("recent snapshot exists, sleeping before first fetch",
			"endpoint", ep.Name,
			"sleep", sleep.Round(time.Second).String(),
		)
		select {
		case <-ctx.Done():
			return
		case <-time.After(sleep):
		}
	}

	for {
		if ctx.Err() != nil {
			return
		}

		sleep := s.fetchAndStore(ctx, ep, state)

		select {
		case <-ctx.Done():
			return
		case <-time.After(sleep):
		}
	}
}

// checkStaleness checks whether a recent snapshot exists for the endpoint. If
// the data is fresh enough (within MaxAge), it returns the calculated sleep
// duration. Returns 0 to indicate an immediate fetch is needed.
//
// On StalenessFunc error, falls back to immediate fetch. This assumes errors
// are transient (e.g., container startup order). Permanent DB errors (missing
// table, wrong permissions) will cause repeated fetch-store-fail cycles at
// MinSleep intervals until the DB issue is resolved.
func (s *Scheduler) checkStaleness(ctx context.Context, ep EndpointConfig) time.Duration {
	if ep.StalenessFunc == nil {
		return 0
	}

	last, err := ep.StalenessFunc(ctx)
	if err != nil {
		s.logger.Error("startup staleness check failed, fetching immediately",
			"endpoint", ep.Name,
			"error", err,
		)
		return 0
	}

	if last.IsZero() {
		return 0
	}

	age := time.Since(last)
	if age >= ep.MaxAge {
		return 0
	}

	// Data is fresh — calculate how long to wait before the next fetch.
	sleep := ep.MaxAge - age + 5*time.Second
	if sleep < ep.MinSleep {
		sleep = ep.MinSleep
	}
	return sleep
}

// fetchAndStore executes one fetch-store cycle and returns the sleep duration
// before the next cycle.
func (s *Scheduler) fetchAndStore(ctx context.Context, ep EndpointConfig, state *endpointState) time.Duration {
	// Acquire source semaphore; defer release to guard against panics.
	sem := s.semaphores[ep.Source]
	if sem != nil {
		select {
		case sem <- struct{}{}:
			defer func() { <-sem }()
		case <-ctx.Done():
			return 0
		}
	}

	result, err := ep.FetchFunc(ctx, s.league, state.lastETag)

	if err != nil {
		s.logger.Error("fetch failed",
			"endpoint", ep.Name,
			"error", err,
		)
		return ep.FallbackInterval
	}

	if err := result.Validate(); err != nil {
		s.logger.Error("invalid fetch result",
			"endpoint", ep.Name,
			"error", err,
		)
		return ep.FallbackInterval
	}

	// 304 Not Modified — data hasn't changed.
	if result.NotModified {
		state.retryCount++
		s.logger.Info("source returned 304 Not Modified",
			"endpoint", ep.Name,
			"retries", state.retryCount,
		)
		if state.retryCount > ep.MaxRetries {
			s.logger.Info("max 304 retries exceeded, falling back",
				"endpoint", ep.Name,
				"fallback", ep.FallbackInterval.String(),
			)
			state.retryCount = 0
			return ep.FallbackInterval
		}
		return ep.MinSleep
	}

	// 200 OK — new data available.
	state.retryCount = 0
	if result.ETag != "" {
		state.lastETag = result.ETag
	}

	snapTime := time.Now().UTC()

	if ep.StoreFunc != nil {
		inserted, err := ep.StoreFunc(ctx, snapTime, result)
		if err != nil {
			s.logger.Error("store failed, skipping post-collect and retrying sooner",
				"endpoint", ep.Name,
				"error", err,
			)
			return ep.MinSleep
		}
		s.logger.Info("snapshot stored",
			"endpoint", ep.Name,
			"inserted", inserted,
		)

		// Post-collect: conditional gem color upsert (gems only) + Mercure publish.
		s.postCollect(ctx, ep.Name, snapTime)
	} else {
		s.logger.Warn("no StoreFunc configured, fetch result discarded",
			"endpoint", ep.Name,
		)
	}

	// Calculate sleep from cache headers.
	return s.calculateSleep(ep, result.Age)
}

// calculateSleep computes the optimal sleep duration based on the endpoint's
// MaxAge and the response's Age header. Clamps to [MinSleep, FallbackInterval].
// Logs at debug level when the CDN-reported age exceeds MaxAge (stale-while-revalidate).
func (s *Scheduler) calculateSleep(ep EndpointConfig, ageSeconds int) time.Duration {
	maxAgeSec := int(ep.MaxAge.Seconds())
	if maxAgeSec > 0 && ageSeconds > maxAgeSec {
		s.logger.Debug("response age exceeds max-age (stale-while-revalidate)",
			"endpoint", ep.Name,
			"age", ageSeconds,
			"maxAge", maxAgeSec,
		)
	}

	ageDur := time.Duration(ageSeconds) * time.Second
	sleep := ep.MaxAge - ageDur + 5*time.Second

	if sleep < ep.MinSleep {
		sleep = ep.MinSleep
	}
	if sleep > ep.FallbackInterval {
		sleep = ep.FallbackInterval
	}

	return sleep
}

// postCollect handles actions after a successful fetch: Mercure publish for all
// endpoints, gem color upsert for the gems endpoint only.
func (s *Scheduler) postCollect(ctx context.Context, endpointName string, snapTime time.Time) {
	// Gem color resolver — only for the gems endpoint.
	if endpointName == EndpointNinjaGems && s.resolver != nil {
		if err := s.resolver.UpsertDiscoveries(ctx); err != nil {
			s.logger.Error("upsert gem color discoveries failed",
				"endpoint", endpointName,
				"error", err,
			)
		}
	}

	// Mercure publish (non-fatal on failure).
	payload, err := json.Marshal(map[string]string{
		"league":    s.league,
		"endpoint":  endpointName,
		"timestamp": snapTime.Format(time.RFC3339),
	})
	if err != nil {
		s.logger.Error("marshal mercure payload",
			"endpoint", endpointName,
			"error", err,
		)
		return
	}

	if err := PublishMercureEvent(ctx, s.mercureURL, s.mercureSecret, "prices-updated", string(payload)); err != nil {
		s.logger.Warn("mercure publish failed",
			"endpoint", endpointName,
			"error", err,
		)
	}
}
