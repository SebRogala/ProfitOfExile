package main

import (
	"context"
	"embed"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/device"
	"profitofexile/internal/exchange"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/mercenary"
	"profitofexile/internal/mercure"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
	"profitofexile/internal/temple"
	"profitofexile/internal/trade"
)

//go:embed all:frontend_build
var frontendEmbed embed.FS

// fenceAcquireMaxWait bounds how long the server boot fence retries a held
// advisory lock before giving up. It absorbs a deploy handoff where the
// orchestrator starts the new server before the previous instance has released
// its fence, without blocking startup indefinitely on a genuinely concurrent
// second writer. This is the boot fence only; the delayed-recompute path keeps
// its fail-fast acquire because it wants to skip the cycle, not wait.
const fenceAcquireMaxWait = 15 * time.Second

func main() {
	port, portErr := loadPort()
	if portErr != nil {
		slog.Error("invalid PORT value", "port", port)
		fmt.Fprintln(os.Stderr, portErr)
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

	// Resolve the process-active league scope once from the runtime SSOT. Every
	// scoped repository read/write and analysis run uses this single value for the
	// process lifetime (per-request selection is POE-121).
	scope, err := league.Resolve(ctx, pool)
	if err != nil {
		slog.Error("failed to resolve active league scope", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to resolve active league from runtime_config. Ensure league control migrations ran and runtime_config is seeded.")
		os.Exit(1)
	}
	slog.Info("active league resolved", "league", scope.ID(), "revision", scope.Revision())

	// EXPECTED_LEAGUE is an optional deploy-time assertion: if the operator
	// pins the league they believe this process should serve, a mismatch with
	// the runtime-resolved scope is a misconfiguration that must not reach
	// readiness (e.g. pointing a Standard-configured deploy at a Mirage DB).
	if expected := os.Getenv("EXPECTED_LEAGUE"); expected != "" && expected != scope.ID() {
		slog.Error("EXPECTED_LEAGUE does not match the resolved active league",
			"expected", expected, "resolved", scope.ID())
		fmt.Fprintf(os.Stderr, "EXPECTED_LEAGUE=%q but runtime resolved league %q; refusing to start.\n", expected, scope.ID())
		os.Exit(1)
	}

	// Server fence: the server is a writer, not a read-only consumer — it owns
	// 9 of the 12 league-scoped tables (written on every gem event and on
	// startup) and makes live GGG trade calls. Two servers on the same league
	// double-write and double-call GGG (the 1616s-ban failure mode), so hold a
	// process-lifetime advisory lock on a key distinct from the collector's
	// RuntimeLockKey — co-located server+collector must fence independently, not
	// deadlock on a shared key. On contention the peer is the legitimate owner:
	// refuse readiness and exit non-zero before binding routes or subscribing.
	//
	// The server needs at least THREE connections, not two. The fence parks one
	// for the process lifetime. The delayed-recompute path then nests: it holds
	// the DataLockKey connection AND, while holding it, runs RunV2 on an unbounded
	// background context — RunV2's pool.Acquire needs a working connection. At
	// max_conns=2 the fence takes #1 and the data lock takes #2, so RunV2 blocks
	// forever waiting for a third that never frees, wedging the pool and never
	// releasing the lock. Three is the floor: fence + data-lock + ≥1 working conn.
	if maxConns := pool.Config().MaxConns; maxConns < 3 {
		slog.Error("server fence requires at least 3 DB connections", "max_conns", maxConns)
		fmt.Fprintf(os.Stderr, "POE_DB_MAX_CONNS must be >= 3: the server parks one connection for the process-lifetime fence and the delayed recompute holds a second (DataLockKey) while RunV2 needs a third to make progress (max_conns=%d).\n", maxConns)
		os.Exit(1)
	}

	serverLock, err := league.AcquireProcessLockWait(ctx, pool, league.ServerLockKey(), fenceAcquireMaxWait)
	if err != nil {
		if errors.Is(err, league.ErrLockHeld) {
			slog.Error("another server still holds the league fence after wait; refusing to start", "league", scope.ID(), "wait", fenceAcquireMaxWait)
			fmt.Fprintf(os.Stderr, "Another server still holds the server fence after %s — is a previous instance still shutting down? Refusing to start a second writer for the same database.\n", fenceAcquireMaxWait)
			os.Exit(1)
		}
		slog.Error("failed to acquire server fence", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to acquire the server advisory lock. Check database connectivity.")
		os.Exit(1)
	}
	defer serverLock.Release()
	slog.Info("server fence acquired", "league", scope.ID())

	frontendFS, err := fs.Sub(frontendEmbed, "frontend_build")
	if err != nil {
		slog.Error("failed to load embedded frontend", "error", err)
		fmt.Fprintln(os.Stderr, "Failed to load embedded frontend assets.")
		os.Exit(1)
	}

	serviceCfg := loadSharedServiceConfig()
	mercureURL := serviceCfg.MercureURL
	mercureSecret := serviceCfg.MercureSecret
	devMode := serviceCfg.DevMode

	labRepo := lab.NewRepository(pool)
	layoutRepo := lab.NewLayoutRepository(pool)
	labCache := lab.NewCache(scope)
	throttler := lab.NewThrottler(mercureURL, mercureSecret, 2*time.Second, labCache)

	// Currency exchange: its own repository, cache and recompute service. This
	// pillar shares nothing with the lab/gem stack above — separate tables,
	// separate collector, separate Mercure topics (see ADR-008 and
	// internal/exchange/doc.go).
	exchangeRepo := exchange.NewRepository(pool)
	exchangeCache := exchange.NewCache()

	// Temple market: the read side of item_snapshots (POE-255). Its own
	// repository, cache and recompute service for the same reason the exchange
	// has its own (ADR-008) — separate tables, separate collector endpoints,
	// separate Mercure topics — and its own cache type rather than a field on
	// labCache because a different event fills it.
	templeCache := temple.NewCache(scope)
	templeService := temple.NewService(temple.NewRepository(pool), templeCache, scope, slog.Default())

	exchangeCfg := loadExchangeConfig()

	// Publishing "the served answer changed" is debounced: a catch-up pass
	// stores several hours back to back and triggers one recompute per hour, and
	// clients only need to be told once, about the final state.
	exchangePublisher := exchangeUpdatePublisher{scope: scope, mercureURL: mercureURL, mercureSecret: mercureSecret}
	// Captured explicitly: the closure runs on a timer goroutine long after this
	// line, so it must hold the process-lifetime context by value.
	publishCtx := ctx
	exchangeDebounce := exchange.NewDebouncer(exchange.DefaultUpdateDebounce, func() {
		res, _ := exchangeCache.Snapshot(exchange.DefaultHorizon)
		if err := exchangePublisher.Publish(publishCtx, exchange.UpdatedTopic, exchange.UpdatePayload(res, time.Now())); err != nil {
			slog.Warn("currency-exchange: publishing the update event failed", "error", err)
		}
	})
	// Pairs with ctxCancel above: a pending publish is dropped rather than sent
	// from a process on its way out, and clients refetch on their next poll.
	defer exchangeDebounce.Stop()
	exchangeService := exchange.NewService(exchangeRepo, scope, exchangeCfg, exchangeCache, exchangeDebounce.Signal, slog.Default())
	slog.Info("currency exchange service configured",
		"minVolumePerHour", exchangeCfg.MinVolumePerHour,
		"minEdge", exchangeCfg.MinEdge,
		"minTurnoverChaos", exchangeCfg.MinTurnoverChaos,
		"maxTick", exchangeCfg.MaxTick,
		"minEdgeTickRatio", exchangeCfg.MinEdgeTickRatio,
		"minROIChaos", exchangeCfg.MinROIChaos,
		"maxPlays", exchangeCfg.MaxPlays,
	)
	for _, horizon := range exchangeCfg.Horizons {
		slog.Info("currency exchange horizon configured",
			"horizon", horizon.Horizon,
			"windowHours", horizon.WindowHours,
			"minHoursSeen", horizon.MinHoursSeen,
		)
	}

	// Trade cache — created before analyzer so the v2 pipeline can use it.
	tradeCacheMax := loadTradeCacheMax()
	tradeCache := trade.NewTradeCache(tradeCacheMax, scope)

	analyzer := lab.NewAnalyzer(labRepo, throttler, labCache, tradeCache)
	var tradeRepo *trade.Repository
	if pool != nil {
		tradeRepo = trade.NewRepository(pool)

		// Warm trade cache from DB — load latest lookup per gem+variant (last 24h).
		warmCtx, warmCancel := context.WithTimeout(ctx, 10*time.Second)
		results, err := tradeRepo.LatestLookups(warmCtx, scope, 24)
		warmCancel()
		if err != nil {
			slog.Warn("trade cache warm failed", "error", err)
		} else if len(results) > 0 {
			loaded := tradeCache.For(scope).Warm(results)
			slog.Info("trade cache warmed from DB", "entries", loaded)
		}
	}
	tradeCfg := loadTradeConfig(scope.ID(), tradeCacheMax)

	// Trade Gate (server-side GGG lookups) — optional, requires TRADE_ENABLED=true.
	var tradeGate *trade.Gate
	var tradeSyncTimeout time.Duration

	if tradeCfg.Enabled {
		tradeSyncTimeout = tradeCfg.SyncWaitBudget

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
				`SELECT chaos FROM currency_snapshots WHERE league = $1 AND currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
				scope.ID(),
			).Scan(&rate)
			if err != nil {
				slog.Warn("trade: failed to get divine rate, using 0", "error", err)
				return 0
			}
			return rate
		}

		tradeGate = trade.NewGate(tradeCfg, scope, tradeLimiter, tradeClient, tradePub, tradeCache, divineRateFn, tradeRepo)

		go tradeGate.Run(ctx)
		slog.Info("trade gate started", "league", tradeCfg.LeagueName, "cacheMax", tradeCacheMax)
		// Trade refresh scheduling: collector publishes poe/collector/trade-tick
		// Mercure events; the server subscriber routes them to HandleTradeTick.
	}

	deviceRepo := device.NewRepository(pool)
	mercTemplateRepo := mercenary.NewRepository(pool)

	routerCfg := server.RouterConfig{
		MercureURL:           mercureURL,
		MercureSecret:        mercureSecret,
		DevMode:              devMode,
		Pool:                 pool,
		LabRepo:              labRepo,
		LayoutRepo:           layoutRepo,
		LabCache:             labCache,
		ExchangeCache:        exchangeCache,
		TempleCache:          templeCache,
		MercureSubscriberKey: serviceCfg.MercureSubscriberKey,
		MercurePublicURL:     serviceCfg.MercurePublicURL,
		TradeGate:            tradeGate,
		TradeCache:           tradeCache,
		TradeRepo:            tradeRepo,
		TradeSyncTimeout:     tradeSyncTimeout,
		League:               scope,
		Analyzer:             analyzer,
		AllowedOrigins:       serviceCfg.AllowedOrigins,
		DeviceRepo:           deviceRepo,
		MercTemplateRepo:     mercTemplateRepo,
		FenceChecker:         serverLock,
		// PROD: this must be a persistent Coolify volume (/data/icons-cache),
		// else every redeploy starts empty and re-fetches every icon from
		// poewiki — which on the VPS is not a slow path but a dead one
		// (ADR-012: poewiki 403s datacenter IPs, so an unseeded icon 502s
		// forever). ONE volume holds every icon set: internal/server derives a
		// sub-directory per set beneath this root, because the sets share the
		// cache-filename scheme and one flat directory would let a gem name and
		// an item id reduce to the same file — and only if they also share a
		// source URL, since the filename carries a hash of that URL too.
		IconCacheDir: serviceCfg.IconCacheDir,
	}

	router := server.NewRouter(pool, frontendFS, routerCfg)

	runtime := newRecomputeRuntime(ctx, scope)
	runtime.pool = pool
	runtime.labCache = labCache
	runtime.throttler = throttler
	runtime.analyzer = analyzer
	runtime.exchange = exchangeService
	runtime.temple = templeService
	runtime.mercureURL = mercureURL
	runtime.subscriberKey = serviceCfg.MercureSubscriberKey
	runtime.subscriberFactory = func(hubURL string, topics []string, subscriberKey string, handler func(server.MercureEvent)) runtimeSubscriber {
		return server.NewMercureSubscriber(hubURL, topics, subscriberKey, handler)
	}
	if tradeGate != nil {
		runtime.tradeTick = func(eventCtx context.Context, raw []byte) {
			server.HandleTradeTick(eventCtx, tradeGate, tradeCache, labCache, scope, raw)
		}
	}
	runtime.updateRate = func(_ context.Context) {
		qCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		var rate float64
		if err := pool.QueryRow(qCtx,
			`SELECT chaos FROM currency_snapshots WHERE league = $1 AND currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
			scope.ID(),
		).Scan(&rate); err != nil {
			slog.Warn("currency event: divine rate query failed", "error", err)
			return
		}
		labCache.For(scope).SetDivineRate(rate)
		slog.Info("currency event: divine rate updated", "rate", rate)
	}
	runtime.refreshOfferingTiming = func(_ context.Context) {
		qCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		// Stores the answer even when it is empty — see RefreshOfferingTimings and
		// the cache-state contract in internal/lab/cache.go. This used to store
		// only a non-empty result, the writer-side twin of the reader defect
		// POE-152 fixed.
		n, err := handlers.RefreshOfferingTimings(qCtx, pool, labCache, scope)
		if err != nil {
			slog.Warn("fragment event: offering timing refresh failed", "error", err)
			return
		}
		slog.Info("fragment event: offering timing updated", "offerings", n)
	}
	runtime.prepareDelayed = func(decCtx context.Context, captured league.Scope) (runtimeLock, string, error) {
		return adaptDelayedLockResult(prepareDelayedRecompute(decCtx, pool, captured))
	}
	runtime.runV2 = analyzer.RunV2
	defer runtime.Close()
	runtime.Start()

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

	// Named apart from the service ctx above: closures still running (the
	// exchange debounce publish, the subscriber branches) read that one
	// asynchronously, and shadowing it here would hand them a 10-second deadline.
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := srv.Shutdown(shutdownCtx); err != nil {
		slog.Error("server forced to shutdown", "error", err)
		os.Exit(1)
	}

	slog.Info("server stopped")
}
