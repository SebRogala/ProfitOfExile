package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"

	"profitofexile/internal/collector"
	"profitofexile/internal/db"
	"profitofexile/internal/price/gemcolor"
)

func main() {
	// Required env vars.
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		slog.Error("DATABASE_URL is required")
		fmt.Fprintln(os.Stderr, "DATABASE_URL environment variable must be set")
		os.Exit(1)
	}

	// Optional env vars with defaults.
	port := os.Getenv("PORT")
	if port == "" {
		port = "8090"
	}
	if p, err := strconv.Atoi(port); err != nil || p < 1 || p > 65535 {
		slog.Error("invalid PORT value", "port", port)
		fmt.Fprintf(os.Stderr, "PORT must be a number between 1 and 65535, got %q\n", port)
		os.Exit(1)
	}

	league := os.Getenv("LEAGUE")
	if league == "" {
		league = "Mirage"
	}

	// Build endpoint configuration from defaults + env var overrides.
	ninjaDefaults := collector.DefaultNinjaConfig()
	ninjaOverrides := collector.ParseEndpointOverrides("NINJA")
	ninjaCfg := collector.MergeEndpointConfig(ninjaDefaults, ninjaOverrides)

	// NINJA_INTERVAL is a legacy alias for NINJA_FALLBACK_INTERVAL.
	// If NINJA_FALLBACK_INTERVAL was not set via overrides but NINJA_INTERVAL
	// exists, use it and log a deprecation warning.
	ninjaIntervalStr := os.Getenv("NINJA_INTERVAL")
	if ninjaOverrides.FallbackInterval == 0 && ninjaIntervalStr != "" {
		slog.Warn("NINJA_INTERVAL is deprecated, use NINJA_FALLBACK_INTERVAL instead",
			"value", ninjaIntervalStr,
		)
		parsed, err := time.ParseDuration(ninjaIntervalStr)
		if err != nil {
			slog.Error("invalid NINJA_INTERVAL", "value", ninjaIntervalStr, "error", err)
			fmt.Fprintf(os.Stderr, "NINJA_INTERVAL must be a valid duration (e.g. 15m), got %q\n", ninjaIntervalStr)
			os.Exit(1)
		}
		ninjaCfg.FallbackInterval = parsed
	}

	mercureURL := os.Getenv("MERCURE_URL")
	if mercureURL == "" {
		mercureURL = "http://mercure/.well-known/mercure"
	}

	mercureJWTSecret := os.Getenv("MERCURE_JWT_SECRET")

	// Database setup.
	ctx := context.Background()

	pool, err := db.NewPool(ctx, databaseURL)
	if err != nil {
		slog.Error("failed to connect to database", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to connect to database. Check DATABASE_URL and ensure PostgreSQL is running.")
		os.Exit(1)
	}
	defer pool.Close()
	slog.Info("database connected")

	// Migrations are handled by the server — the collector only reads/writes
	// data and should not manage schema. This avoids crash loops when the
	// collector image is rebuilt separately from the server.

	// Initialize components.
	resolver, err := gemcolor.NewResolver(ctx, pool)
	if err != nil {
		slog.Error("failed to initialize gem color resolver", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to load gem colors from database.")
		os.Exit(1)
	}

	fetcher := collector.NewNinjaFetcher(resolver)
	repo := collector.NewRepository(pool)
	logger := slog.Default()

	if mercureJWTSecret == "" {
		slog.Warn("MERCURE_JWT_SECRET not set, Mercure publishing disabled")
	}

	// Build per-endpoint configs for the goroutine-per-endpoint scheduler.
	gemEndpoint := ninjaCfg
	gemEndpoint.Name = collector.EndpointNinjaGems
	gemEndpoint.FetchFunc = fetcher.FetchGemsEndpoint
	gemEndpoint.StoreFunc = func(ctx context.Context, snapTime time.Time, result *collector.FetchResult) (int, error) {
		if len(result.GemData) == 0 {
			return 0, fmt.Errorf("gem endpoint returned 200 with empty data for league %q — check LEAGUE env var or possible transient API issue", league)
		}
		return repo.InsertGemSnapshots(ctx, snapTime, result.GemData)
	}
	gemEndpoint.StalenessFunc = func(ctx context.Context) (time.Time, error) {
		return repo.LastGemSnapshotTime(ctx)
	}

	currencyEndpoint := ninjaCfg
	currencyEndpoint.Name = collector.EndpointNinjaCurrency
	currencyEndpoint.FetchFunc = fetcher.FetchCurrencyEndpoint
	currencyEndpoint.StoreFunc = func(ctx context.Context, snapTime time.Time, result *collector.FetchResult) (int, error) {
		if len(result.CurrencyData) == 0 {
			return 0, fmt.Errorf("currency endpoint returned 200 with empty data for league %q — check LEAGUE env var or possible transient API issue", league)
		}
		return repo.InsertCurrencySnapshots(ctx, snapTime, result.CurrencyData)
	}
	currencyEndpoint.StalenessFunc = func(ctx context.Context) (time.Time, error) {
		return repo.LastCurrencySnapshotTime(ctx)
	}

	fragmentEndpoint := ninjaCfg
	fragmentEndpoint.Name = collector.EndpointNinjaFragments
	fragmentEndpoint.FetchFunc = fetcher.FetchFragmentEndpoint
	fragmentEndpoint.StoreFunc = func(ctx context.Context, snapTime time.Time, result *collector.FetchResult) (int, error) {
		if len(result.FragmentData) == 0 {
			return 0, fmt.Errorf("fragment endpoint returned 200 with empty data for league %q — check LEAGUE env var or possible transient API issue", league)
		}
		return repo.InsertFragmentSnapshots(ctx, snapTime, result.FragmentData)
	}
	fragmentEndpoint.StalenessFunc = func(ctx context.Context) (time.Time, error) {
		return repo.LastFragmentSnapshotTime(ctx)
	}

	scheduler, err := collector.NewScheduler(
		[]collector.EndpointConfig{gemEndpoint, currencyEndpoint, fragmentEndpoint},
		resolver,
		league,
		mercureURL,
		mercureJWTSecret,
		logger,
	)
	if err != nil {
		slog.Error("failed to create scheduler", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to create scheduler. Check configuration.")
		os.Exit(1)
	}

	slog.Info("collector starting",
		"league", league,
		"fallbackInterval", ninjaCfg.FallbackInterval.String(),
		"port", port,
	)

	// Start scheduler in background.
	schedulerCtx, schedulerCancel := context.WithCancel(ctx)
	defer schedulerCancel()

	errCh := make(chan error, 1)
	go func() {
		errCh <- scheduler.Run(schedulerCtx)
	}()

	// Trade refresh scheduler — publishes a Mercure tick on each interval.
	// The server subscribes to poe/collector/trade-tick and runs the
	// gem-pick + lookup logic on its side. Alternates between tier-filtered
	// (MID+) and any-gem refreshes.
	if mercureJWTSecret != "" {
		tradeInterval := 45 * time.Second
		if d, err := time.ParseDuration(os.Getenv("TRADE_REFRESH_INTERVAL")); err == nil && d > 0 {
			tradeInterval = d
		}
		go runTradeRefresher(schedulerCtx, mercureURL, mercureJWTSecret, tradeInterval)
		slog.Info("trade refresh scheduler started", "interval", tradeInterval)
	} else {
		slog.Info("trade refresh scheduler disabled (MERCURE_JWT_SECRET not set)")
	}

	// Lab layout daily reset — publish Mercure event at 00:00 UTC so desktop
	// clients clear stale layouts and re-fetch when the new one is uploaded.
	if mercureJWTSecret != "" {
		go runLayoutResetTicker(schedulerCtx, mercureURL, mercureJWTSecret)
		slog.Info("lab layout reset ticker started (fires at 00:00 UTC daily)")
	}

	// Health/debug HTTP server.
	startedAt := time.Now()
	mux := http.NewServeMux()

	mux.HandleFunc("GET /health", func(w http.ResponseWriter, r *http.Request) {
		summary, err := repo.LatestSnapshot(r.Context())
		if err != nil {
			slog.Error("health endpoint: failed to fetch latest snapshot", "error", err)
			http.Error(w, `{"status":"error"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"status":       "ok",
			"lastSnapshot": summary.LastGemTime.Format(time.RFC3339),
			"uptime":       time.Since(startedAt).Round(time.Second).String(),
		}); err != nil {
			slog.Error("health endpoint: encode response", "error", err)
		}
	})

	mux.HandleFunc("GET /latest", func(w http.ResponseWriter, r *http.Request) {
		summary, err := repo.LatestSnapshot(r.Context())
		if err != nil {
			slog.Error("latest endpoint failed", "error", err)
			http.Error(w, `{"error":"failed to fetch latest snapshot"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"lastGemTime":      summary.LastGemTime.Format(time.RFC3339),
			"gemCount":         summary.GemCount,
			"lastCurrencyTime": summary.LastCurrencyTime.Format(time.RFC3339),
			"currencyCount":    summary.CurrencyCount,
		}); err != nil {
			slog.Error("latest endpoint: encode response", "error", err)
		}
	})

	mux.HandleFunc("GET /snapshots", func(w http.ResponseWriter, r *http.Request) {
		stats, err := repo.GetCollectionStats(r.Context())
		if err != nil {
			slog.Error("snapshots endpoint failed", "error", err)
			http.Error(w, `{"error":"failed to query stats"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(stats); err != nil {
			slog.Error("snapshots endpoint: encode response", "error", err)
		}
	})

	srv := &http.Server{
		Addr:         ":" + port,
		Handler:      mux,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	go func() {
		slog.Info("health server starting", "addr", srv.Addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			slog.Error("health server failed", "error", err)
		}
	}()

	// Wait for SIGINT/SIGTERM or scheduler error.
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	select {
	case err := <-errCh:
		if err != nil {
			slog.Error("scheduler failed", "error", err)
		}
	case sig := <-quit:
		slog.Info("shutting down", "signal", sig.String())
	}

	// Graceful shutdown.
	schedulerCancel()

	shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer shutdownCancel()

	if err := srv.Shutdown(shutdownCtx); err != nil {
		slog.Error("health server forced to shutdown", "error", err)
	}

	slog.Info("collector stopped")
}

// runLayoutResetTicker sleeps until 00:00 UTC each day, then publishes a
// layout reset event on the Mercure hub. Desktop clients listen for this
// and clear their cached lab layout so they fetch the new daily layout.
func runLayoutResetTicker(ctx context.Context, mercureURL, mercureSecret string) {
	for {
		now := time.Now().UTC()
		midnight := time.Date(now.Year(), now.Month(), now.Day()+1, 0, 0, 0, 0, time.UTC)
		sleepDur := midnight.Sub(now)
		slog.Info("layout reset: sleeping until midnight UTC", "duration", sleepDur.Round(time.Second))

		select {
		case <-ctx.Done():
			return
		case <-time.After(sleepDur):
		}

		payload := `{"action":"reset","source":"collector"}`
		if err := collector.PublishMercureEvent(ctx, mercureURL, mercureSecret, "poe/lab/layout", payload); err != nil {
			slog.Error("layout reset: mercure publish failed", "error", err)
		} else {
			slog.Info("layout reset: published reset event at midnight UTC")
		}
	}
}

// runTradeRefresher periodically publishes poe/collector/trade-tick Mercure
// events. The server subscribes and performs the gem-pick + GGG lookup.
// Alternates between tier-filtered (MID+ for 20/20) and any-gem refreshes —
// the server interprets the payload to know which mode is requested.
//
// Outcomes (gem chosen, lookup result) are logged on the server side. The
// collector is fire-and-forget here; it just drives the cadence.
//
// Transient publish failures stay at Warn so log volume is bounded. After
// publishErrorEscalateAfter consecutive failures the next failure is logged
// at Error so monitoring can page on a sustained Mercure outage; the level
// drops back to Warn once a publish succeeds.
func runTradeRefresher(ctx context.Context, mercureURL, mercureSecret string, interval time.Duration) {
	const publishErrorEscalateAfter = 3

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	tierTick := true
	consecFailures := 0
	escalated := false

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			var payload string
			if tierTick {
				payload = `{"variant":"20/20","minTier":"MID","minAge":"5m"}`
			} else {
				payload = `{"variant":"20/20","minAge":"5m"}`
			}
			tierTick = !tierTick

			if err := collector.PublishMercureEvent(ctx, mercureURL, mercureSecret, "poe/collector/trade-tick", payload); err != nil {
				if ctx.Err() != nil {
					return
				}
				consecFailures++
				if consecFailures >= publishErrorEscalateAfter && !escalated {
					slog.Error("trade tick: publish failing repeatedly", "error", err, "consecutive_failures", consecFailures)
					escalated = true
				} else {
					slog.Warn("trade tick: publish failed", "error", err, "consecutive_failures", consecFailures)
				}
				continue
			}

			if escalated {
				slog.Info("trade tick: publish recovered", "after_failures", consecFailures)
			}
			consecFailures = 0
			escalated = false
		}
	}
}
