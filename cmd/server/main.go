package main

import (
	"context"
	"embed"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/device"
	"profitofexile/internal/lab"
	"profitofexile/internal/mercure"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
	"profitofexile/internal/trade"
)

//go:embed all:frontend_build
var frontendEmbed embed.FS

// corsOrigins returns allowed CORS origins from the CORS_ORIGINS env var.
// Comma-separated list, e.g. "http://localhost:1420,tauri://localhost".
// Returns nil (no CORS) when unset.
func corsOrigins() []string {
	raw := os.Getenv("CORS_ORIGINS")
	if raw == "" {
		return nil
	}
	var origins []string
	for _, o := range strings.Split(raw, ",") {
		if o = strings.TrimSpace(o); o != "" {
			origins = append(origins, o)
		}
	}
	return origins
}

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	if p, err := strconv.Atoi(port); err != nil || p < 1 || p > 65535 {
		slog.Error("invalid PORT value", "port", port)
		fmt.Fprintf(os.Stderr, "PORT must be a number between 1 and 65535, got %q\n", port)
		os.Exit(1)
	}

	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		slog.Error("DATABASE_URL is required")
		fmt.Fprintln(os.Stderr, "DATABASE_URL environment variable must be set")
		os.Exit(1)
	}

	ctx, ctxCancel := context.WithCancel(context.Background())
	defer ctxCancel()

	pool, err := db.NewPool(ctx, databaseURL)
	if err != nil {
		slog.Error("failed to connect to database", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to connect to database. Check DATABASE_URL and ensure PostgreSQL is running.")
		os.Exit(1)
	}
	defer pool.Close()
	slog.Info("database connected")

	// Auto-migrate: apply pending migrations before binding the server.
	// Fail-fast on error per ADR-004.
	if err := db.MigrateUp(db.MigrationsFS, databaseURL); err != nil {
		slog.Error("auto-migrate failed", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to apply database migrations. Check migration files and database state.")
		os.Exit(1)
	}

	frontendFS, err := fs.Sub(frontendEmbed, "frontend_build")
	if err != nil {
		slog.Error("failed to load embedded frontend", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to load embedded frontend assets.")
		os.Exit(1)
	}

	mercureURL := os.Getenv("MERCURE_URL")
	mercureSecret := os.Getenv("MERCURE_JWT_SECRET")
	devMode := os.Getenv("APP_ENV") == "dev"

	if mercureURL != "" && mercureSecret == "" {
		slog.Warn("MERCURE_URL is set but MERCURE_JWT_SECRET is empty — publish operations will be skipped")
	}

	labRepo := lab.NewRepository(pool)
	layoutRepo := lab.NewLayoutRepository(pool)
	labCache := lab.NewCache()
	throttler := lab.NewThrottler(mercureURL, mercureSecret, 2*time.Second, labCache)

	// Trade cache — created before analyzer so the v2 pipeline can use it.
	tradeCacheMax := 200
	if v := os.Getenv("TRADE_CACHE_MAX"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			tradeCacheMax = n
		}
	}
	tradeCache := trade.NewTradeCache(tradeCacheMax)

	analyzer := lab.NewAnalyzer(labRepo, throttler, labCache, tradeCache)
	var tradeRepo *trade.Repository
	if pool != nil {
		tradeRepo = trade.NewRepository(pool)

		// Warm trade cache from DB — load latest lookup per gem+variant (last 24h).
		warmCtx, warmCancel := context.WithTimeout(ctx, 10*time.Second)
		results, err := tradeRepo.LatestLookups(warmCtx, 24)
		warmCancel()
		if err != nil {
			slog.Warn("trade cache warm failed", "error", err)
		} else if len(results) > 0 {
			loaded := tradeCache.Warm(results)
			slog.Info("trade cache warmed from DB", "entries", loaded)
		}
	}

	// Trade Gate (server-side GGG lookups) — optional, requires TRADE_ENABLED=true.
	var tradeGate *trade.Gate
	var tradeSyncTimeout time.Duration

	if os.Getenv("TRADE_ENABLED") == "true" {
		tradeCeiling := 0.65
		if v := os.Getenv("TRADE_CEILING"); v != "" {
			if f, err := strconv.ParseFloat(v, 64); err == nil && f > 0 && f <= 1 {
				tradeCeiling = f
			}
		}
		tradeLatencyPad := 1 * time.Second
		if v := os.Getenv("TRADE_LATENCY_PAD"); v != "" {
			if d, err := time.ParseDuration(v); err == nil {
				tradeLatencyPad = d
			}
		}
		tradeMaxWait := 30 * time.Second
		if v := os.Getenv("TRADE_MAX_WAIT"); v != "" {
			if d, err := time.ParseDuration(v); err == nil {
				tradeMaxWait = d
			}
		}
		tradeSyncTimeout = 500 * time.Millisecond
		if v := os.Getenv("TRADE_SYNC_WAIT"); v != "" {
			if d, err := time.ParseDuration(v); err == nil {
				tradeSyncTimeout = d
			}
		}

		tradeCfg := trade.TradeConfig{
			Enabled:           true,
			LeagueName:        os.Getenv("LEAGUE"),
			CeilingFactor:     tradeCeiling,
			LatencyPadding:    tradeLatencyPad,
			DefaultSearchRate: 1,
			DefaultFetchRate:  1,
			MaxQueueWait:      tradeMaxWait,
			CacheMaxEntries:   tradeCacheMax,
			UserAgent:         getEnvDefault("TRADE_USER_AGENT", "profitofexile/0.1.0"),
			SyncWaitBudget:    tradeSyncTimeout,
		}

		tradeLimiter := trade.NewRateLimiter(tradeCfg)
		tradeClient := trade.NewClient(tradeCfg)
		tradePub := &mercure.HubPublisher{URL: mercureURL, Secret: mercureSecret}

		// Divine rate function: queries latest currency snapshot from DB.
		// Cached in a goroutine-safe variable, refreshed on each call (query is fast).
		divineRateFn := func() float64 {
			qCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()
			var rate float64
			err := pool.QueryRow(qCtx,
				`SELECT chaos FROM currency_snapshots WHERE currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
			).Scan(&rate)
			if err != nil {
				slog.Warn("trade: failed to get divine rate, using 0", "error", err)
				return 0
			}
			return rate
		}

		tradeGate = trade.NewGate(tradeCfg, tradeLimiter, tradeClient, tradePub, tradeCache, divineRateFn, tradeRepo)

		go tradeGate.Run(ctx)
		slog.Info("trade gate started", "league", tradeCfg.LeagueName, "cacheMax", tradeCacheMax)
		// Trade refresh scheduling moved to collector — server exposes
		// POST /api/internal/trade/refresh for collector to trigger.
	}

	deviceRepo := device.NewRepository(pool)

	routerCfg := server.RouterConfig{
		MercureURL:           mercureURL,
		MercureSecret:        mercureSecret,
		DevMode:              devMode,
		Pool:                 pool,
		LabRepo:              labRepo,
		LayoutRepo:           layoutRepo,
		LabCache:             labCache,
		MercureSubscriberKey: os.Getenv("MERCURE_SUBSCRIBER_KEY"),
		MercurePublicURL:     os.Getenv("MERCURE_PUBLIC_URL"),
		TradeGate:            tradeGate,
		TradeCache:           tradeCache,
		TradeRepo:            tradeRepo,
		TradeSyncTimeout:     tradeSyncTimeout,
		InternalSecret:       os.Getenv("INTERNAL_SECRET"),
		League:               os.Getenv("LEAGUE"),
		Analyzer:             analyzer,
		AllowedOrigins:       corsOrigins(),
		DeviceRepo:           deviceRepo,
	}

	router := server.NewRouter(pool, frontendFS, routerCfg)

	// Seed cache from DB on startup.
	go func() {
		qCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
		defer cancel()

		// Estimate next fetch from the interval between last two gem snapshots.
		var lastSnap, prevSnap time.Time
		if err := pool.QueryRow(qCtx,
			`SELECT time FROM gem_snapshots ORDER BY time DESC LIMIT 1`,
		).Scan(&lastSnap); err == nil && !lastSnap.IsZero() {
			// Find the previous distinct snapshot time.
			_ = pool.QueryRow(qCtx,
				`SELECT time FROM gem_snapshots WHERE time < $1 ORDER BY time DESC LIMIT 1`, lastSnap,
			).Scan(&prevSnap)

			var interval time.Duration
			if !prevSnap.IsZero() {
				interval = lastSnap.Sub(prevSnap)
			}
			if interval < 10*time.Minute || interval > 2*time.Hour {
				interval = 30 * time.Minute // sane fallback
			}
			nextFetch := lastSnap.Add(interval)
			labCache.SetNextFetch(nextFetch)
			slog.Info("startup: seeded nextFetch", "lastSnap", lastSnap, "interval", interval, "nextFetch", nextFetch)
		}

		// Seed divine rate.
		var divRate float64
		if err := pool.QueryRow(qCtx,
			`SELECT chaos FROM currency_snapshots WHERE currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
		).Scan(&divRate); err == nil && divRate > 0 {
			labCache.SetDivineRate(divRate)
			slog.Info("startup: seeded divine rate", "rate", divRate)
		}
	}()

	// Recompute latest v2 snapshot on startup — ensures fresh computed data
	// after a deploy with new scoring logic (ON CONFLICT DO NOTHING would
	// otherwise keep stale data). Only deletes computed tables, not raw snapshots.
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("startup v2+font analysis panicked", "recover", r)
			}
		}()
		// Recompute V2: deletes latest computed data, then re-runs full pipeline.
		if err := analyzer.RecomputeLatestV2(ctx); err != nil {
			slog.Warn("startup v2 recompute failed (non-fatal)", "error", err)
		}
		// Font analysis second — needs GemFeatures from V2.
		if err := analyzer.RunFont(ctx); err != nil {
			slog.Warn("startup font analysis failed (non-fatal)", "error", err)
		}
		// Dedication runs after V2 for risk-adjustment features.
		if err := analyzer.RunDedication(ctx); err != nil {
			slog.Warn("startup dedication analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("transfigure analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunTransfigure(ctx); err != nil {
			slog.Warn("startup transfigure analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("quality analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunQuality(ctx); err != nil {
			slog.Warn("startup quality analysis failed (non-fatal)", "error", err)
		}
	}()
	// Delayed recompute timer — fires 15min after the last ninja_gems event
	// so that the v2 pipeline picks up trade data accumulated since the snapshot.
	// Protected by a mutex since the timer callback and Mercure handler run on
	// different goroutines.
	var (
		delayedRecomputeMu    sync.Mutex
		delayedRecomputeTimer *time.Timer
	)
	defer func() {
		delayedRecomputeMu.Lock()
		if delayedRecomputeTimer != nil {
			delayedRecomputeTimer.Stop()
		}
		delayedRecomputeMu.Unlock()
	}()

	// Start Mercure subscriber in background if configured.
	if mercureURL != "" {
		subCtx, subCancel := context.WithCancel(ctx)
		defer subCancel()

		topics := []string{"poe/collector/gems", "poe/collector/currency", "poe/collector/fragments"}
		mercureSubKey := os.Getenv("MERCURE_SUBSCRIBER_KEY")
		sub := server.NewMercureSubscriber(mercureURL, topics, mercureSubKey, func(ev server.MercureEvent) {
			var payload map[string]any
			if err := json.Unmarshal([]byte(ev.Data), &payload); err != nil {
				slog.Warn("mercure: invalid event payload", "error", err)
				return
			}
			slog.Info("mercure event received",
				"topic", ev.Topic,
				"endpoint", payload["endpoint"],
				"inserted", payload["inserted"],
			)

			// Parse nextFetch from collector payload so the throttler can
			// include it as "nextAny" in the analysis-updated event.
			var nextFetch time.Time
			if nf, ok := payload["nextFetch"].(string); ok {
				if parsed, err := time.Parse(time.RFC3339, nf); err == nil {
					nextFetch = parsed
				}
			}

			// Trigger analysis only on new gem data — currency/fragment updates
			// are not relevant for the lab dashboard.
			endpoint, ok := payload["endpoint"].(string)
			if !ok {
				slog.Warn("mercure: missing or non-string 'endpoint' in payload", "payload", payload)
				return
			}
			if endpoint == "ninja_currency" || endpoint == "ninja-currency" {
				// Update divine rate on cache from latest DB data.
				go func() {
					qCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
					defer cancel()
					var rate float64
					if err := pool.QueryRow(qCtx,
						`SELECT chaos FROM currency_snapshots WHERE currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
					).Scan(&rate); err != nil {
						slog.Warn("currency event: divine rate query failed", "error", err)
						return
					}
					labCache.SetDivineRate(rate)
					slog.Info("currency event: divine rate updated", "rate", rate)
				}()
			}

			if endpoint == "ninja_fragments" || endpoint == "ninja-fragments" {
				go func() {
					qCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
					defer cancel()
					offerings := handlers.ComputeOfferingTimings(qCtx, pool)
					if len(offerings) > 0 {
						if data, err := json.Marshal(offerings); err == nil {
							labCache.SetOfferingTiming(data)
							slog.Info("fragment event: offering timing updated", "offerings", len(offerings))
						}
					}
				}()
			}

			if endpoint == "ninja_gems" || endpoint == "ninja-gems" {
				// Always signal throttler on gem events; nextFetch is optional enrichment.
				throttler.Signal(nextFetch)
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("transfigure analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunTransfigure(subCtx); err != nil {
						slog.Warn("transfigure analysis failed", "error", err)
					}
				}()
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("quality analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunQuality(subCtx); err != nil {
						slog.Warn("quality analysis failed", "error", err)
					}
				}()
				// RunV2 must complete before RunFont — font reads GemFeatures
				// from cache (tier classification). Running them concurrently
				// causes font to read stale tiers from the previous cycle.
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("v2/font analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunV2(subCtx); err != nil {
						slog.Warn("v2 analysis failed", "error", err)
						return
					}
					// Font runs after V2 so it reads fresh GemFeatures with current tier classification.
					if err := analyzer.RunFont(subCtx); err != nil {
						slog.Warn("font analysis failed", "error", err)
					}
					// Dedication runs after V2 for risk-adjustment features.
					if err := analyzer.RunDedication(subCtx); err != nil {
						slog.Warn("dedication analysis failed", "error", err)
					}
				}()

				// Schedule a delayed recompute T+15min after each ninja_gems event.
				// This picks up trade data accumulated since the snapshot.
				// A new ninja event cancels any pending delayed recompute.
				delayedRecomputeMu.Lock()
				if delayedRecomputeTimer != nil {
					delayedRecomputeTimer.Stop()
				}
				delayedRecomputeTimer = time.AfterFunc(15*time.Minute, func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("delayed v2 recompute panicked", "recover", r)
						}
					}()
					slog.Info("delayed recompute: running v2 with accumulated trade data")
					if err := analyzer.RunV2(context.Background()); err != nil {
						slog.Warn("delayed v2 recompute failed", "error", err)
					}
				})
				delayedRecomputeMu.Unlock()
			}
		})
		go sub.Run(subCtx)
		slog.Info("mercure subscriber started", "topics", topics)
	}

	srv := &http.Server{
		Addr:         ":" + port,
		Handler:      router,
		ReadTimeout:  10 * time.Second,
		WriteTimeout: 30 * time.Second,
		IdleTimeout:  120 * time.Second,
	}

	// Start server in a goroutine, sending errors back to the main goroutine
	// so deferred cleanup can run before exit.
	errCh := make(chan error, 1)
	go func() {
		slog.Info("server starting", "addr", srv.Addr)
		errCh <- srv.ListenAndServe()
	}()

	// Wait for SIGINT/SIGTERM or a server startup error.
	quit := make(chan os.Signal, 1)
	signal.Notify(quit, syscall.SIGINT, syscall.SIGTERM)

	select {
	case err := <-errCh:
		if !errors.Is(err, http.ErrServerClosed) {
			slog.Error("server failed to start", "error", err)
			os.Exit(1)
		}
	case sig := <-quit:
		slog.Info("shutting down server", "signal", sig.String())
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := srv.Shutdown(ctx); err != nil {
		slog.Error("server forced to shutdown", "error", err)
		os.Exit(1)
	}

	slog.Info("server stopped")
}

func getEnvDefault(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
