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

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
	"profitofexile/internal/device"
	"profitofexile/internal/exchange"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/mercenary"
	"profitofexile/internal/mercure"
	"profitofexile/internal/server"
	"profitofexile/internal/server/handlers"
	"profitofexile/internal/trade"
)

//go:embed all:frontend_build
var frontendEmbed embed.FS

// runFullRecompute runs every analysis step in the order required by
// RecomputeLatestV2. V2 must complete before Font — Font reads GemFeatures
// for tier classification, so running them concurrently leaves Font with
// stale tiers. Each step is run sequentially; an error in one step is logged
// and the next step still runs (matches the pre-Mercure handler's behavior).
//
// Triggered by the poe/admin/recompute Mercure event from cmd/recalculate.
func runFullRecompute(ctx context.Context, analyzer *lab.Analyzer, scope league.Scope) {
	defer func() {
		if r := recover(); r != nil {
			slog.Error("admin recompute panicked", "recover", r)
		}
	}()
	slog.Info("admin recompute: started")
	failures := 0
	if err := analyzer.RunTransfigure(ctx, scope); err != nil {
		slog.Error("admin recompute: transfigure failed", "error", err)
		failures++
	}
	if err := analyzer.RunQuality(ctx, scope); err != nil {
		slog.Error("admin recompute: quality failed", "error", err)
		failures++
	}
	if err := analyzer.RecomputeLatestV2(ctx, scope); err != nil {
		slog.Error("admin recompute: v2 failed", "error", err)
		failures++
	}
	if err := analyzer.RunFont(ctx, scope); err != nil {
		slog.Error("admin recompute: font failed", "error", err)
		failures++
	}
	if err := analyzer.RunDoubleCorrupt(ctx, scope); err != nil {
		slog.Error("admin recompute: double corrupt failed", "error", err)
		failures++
	}
	if err := analyzer.RunDedication(ctx, scope); err != nil {
		slog.Error("admin recompute: dedication failed", "error", err)
		failures++
	}
	if failures == 0 {
		slog.Info("admin recompute: complete")
	} else {
		slog.Warn("admin recompute: finished with failures", "failed_steps", failures, "total_steps", 6)
	}
}

// delayedRecomputeLockTimeout bounds the delayed-recompute timer's attempt to
// take the league data lock and re-resolve the active league. The timer fires
// on context.Background(), and both pool.Acquire (advisory-lock connection) and
// league.Resolve block on pool exhaustion, so without an independent deadline a
// stuck decision would leak a goroutine every timer cycle. It is deliberately
// separate from the context passed to RunV2, which keeps the pre-existing
// unbounded background context so a long recompute is not truncated.
const delayedRecomputeLockTimeout = 5 * time.Second

// fenceAcquireMaxWait bounds how long the server boot fence retries a held
// advisory lock before giving up. It absorbs a deploy handoff where the
// orchestrator starts the new server before the previous instance has released
// its fence, without blocking startup indefinitely on a genuinely concurrent
// second writer. This is the boot fence only; the delayed-recompute path keeps
// its fail-fast acquire because it wants to skip the cycle, not wait.
const fenceAcquireMaxWait = 15 * time.Second

// prepareDelayedRecompute decides whether a fired delayed-recompute timer may
// run RunV2 for the league it was scheduled under, and if so returns the held
// league data lock the caller must Release.
//
// The 15-minute timer captures a scope at schedule time and fires much later on
// context.Background(), guarded only by an in-process mutex. That guard cannot
// see two hazards this function closes:
//   - concurrent delayed recompute: another delayed-recompute run may already be
//     writing the same league. DataLockKey serialises THIS delayed run against
//     that one (and against a second server's delayed run once >1 server is
//     permitted — today the ServerLockKey boot fence makes that cross-process
//     case vacuous). The IMMEDIATE recompute chain (the Mercure gem-event RunV2)
//     does NOT take this lock, so it is not serialised against — see POE-118.
//   - wrong-league: the active league (id or revision) may have changed while
//     the timer was in flight; committing RunV2 under the captured scope would
//     write and cache the previous league's data as if it were current.
//
// TODO(POE-118): bring the immediate RunV2 paths under DataLockKey so all writers
// of one league's computed data are serialised, not just the delayed timer.
//
// Return contract:
//   - lock != nil, skip == "": proceed; caller runs RunV2 then Release()s lock.
//   - lock == nil, skip != "": do not run; skip explains why (expected, log info).
//   - err  != nil:            an unexpected failure while deciding; do not run.
func prepareDelayedRecompute(ctx context.Context, pool *pgxpool.Pool, captured league.Scope) (lock *league.ProcessLock, skip string, err error) {
	// Bound the whole decision independently of the recompute context: the timer
	// fires on context.Background(), and both the lock acquire and the re-resolve
	// hit the pool, which blocks on exhaustion. An unbounded decision would leak
	// a goroutine per 15-minute cycle.
	decCtx, cancel := context.WithTimeout(ctx, delayedRecomputeLockTimeout)
	defer cancel()

	lock, err = league.AcquireProcessLock(decCtx, pool, league.DataLockKey(captured))
	if err != nil {
		switch {
		case errors.Is(err, league.ErrLockHeld):
			return nil, "league data lock held by another recompute", nil
		case errors.Is(err, context.DeadlineExceeded), errors.Is(err, context.Canceled):
			return nil, "timed out acquiring league data lock", nil
		default:
			return nil, "", err
		}
	}

	// Lock held: re-resolve the active league and refuse if it drifted (id OR
	// revision) since the timer was scheduled. Revision alone catches a same-name
	// league bumped mid rolling deploy (ADR-009), which is exactly what the
	// captured scope must not silently overwrite.
	active, rerr := league.Resolve(decCtx, pool)
	if rerr != nil {
		lock.Release()
		return nil, "", fmt.Errorf("re-resolve active league: %w", rerr)
	}
	if active.ID() != captured.ID() || active.Revision() != captured.Revision() {
		lock.Release()
		return nil, fmt.Sprintf("active league changed since timer was scheduled (was %s@%d, now %s@%d)",
			captured.ID(), captured.Revision(), active.ID(), active.Revision()), nil
	}

	return lock, "", nil
}

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

	mercureURL := os.Getenv("MERCURE_URL")
	mercureSecret := os.Getenv("MERCURE_JWT_SECRET")
	devMode := os.Getenv("APP_ENV") == "dev"

	if mercureURL != "" && mercureSecret == "" {
		slog.Warn("MERCURE_URL is set but MERCURE_JWT_SECRET is empty — publish operations will be skipped")
	}

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

	// Ranking knobs are overridable per deploy. Unlike the TRADE_* fallbacks
	// above, an unusable value here is logged loudly rather than swallowed: a
	// typo in a threshold silently changes which plays users are shown, with no
	// other symptom to notice it by.
	//
	// Since POE-191 the four quality gates below (turnover, tick, edge/tick,
	// chaos payout) ship OFF and the desktop applies them client-side, so
	// setting one here re-arms it for everyone and can only tighten what the
	// server serves — envPositiveFloat rejects a zero or negative value, which
	// is also the only way back to "off" (unset the variable).
	exchangeCfg := exchange.DefaultConfig()
	// WindowHours and MinHoursSeen are per horizon (below), not on the base
	// config: the base values would be overwritten by every horizon overlay, so
	// an override here would read as configured and do nothing.
	exchangeCfg.MinVolumePerHour = envPositiveFloat("EXCHANGE_MIN_VOLUME_PER_HOUR", exchangeCfg.MinVolumePerHour)
	exchangeCfg.MaxPlays = envPositiveInt("EXCHANGE_MAX_PLAYS", exchangeCfg.MaxPlays)
	exchangeCfg.MinTurnoverChaos = envPositiveFloat("EXCHANGE_MIN_TURNOVER_CHAOS", exchangeCfg.MinTurnoverChaos)
	exchangeCfg.MaxTick = envPositiveFloat("EXCHANGE_MAX_TICK", exchangeCfg.MaxTick)
	exchangeCfg.MinEdgeTickRatio = envPositiveFloat("EXCHANGE_MIN_EDGE_TICK_RATIO", exchangeCfg.MinEdgeTickRatio)
	exchangeCfg.MinROIChaos = envPositiveFloat("EXCHANGE_MIN_ROI_CHAOS", exchangeCfg.MinROIChaos)
	// The junk bands are fractions of an hour's VWAP, so both are positive and
	// the low one is meant to be under 1 while the high one is over it. Nothing
	// enforces that ordering here: a deploy that wants to widen or narrow either
	// side of "believable" is allowed to, and inverting them would flag every
	// leg rather than none, which is loud enough to notice.
	exchangeCfg.SuspectLowBand = envPositiveFloat("EXCHANGE_SUSPECT_LOW_BAND", exchangeCfg.SuspectLowBand)
	exchangeCfg.SuspectHighBand = envPositiveFloat("EXCHANGE_SUSPECT_HIGH_BAND", exchangeCfg.SuspectHighBand)
	// HideSuspect turns the flag into a filter. Default false: a flagged row can
	// be argued with, a missing one cannot.
	exchangeCfg.HideSuspect = envBool("EXCHANGE_HIDE_SUSPECT", exchangeCfg.HideSuspect)
	// MinEdge is where the engine FLAGS a play as having no spread worth taking
	// (Play.LowLiquidity), not a floor it drops below — since 2026-08-22 raising
	// this marks more rows and hides none, and an operator who wants rows GONE
	// arms EXCHANGE_MIN_EDGE_TICK_RATIO or EXCHANGE_MIN_ROI_CHAOS instead. It is
	// the one knob where a negative value is meaningful (it stops the small gains
	// from being marked), so only an exact 0 is rejected — the engine reads 0 as
	// "unset" and would restore the default behind the log line, making the
	// configured value a lie.
	if v := os.Getenv("EXCHANGE_MIN_EDGE"); v != "" {
		f, err := strconv.ParseFloat(v, 64)
		switch {
		case err != nil:
			slog.Warn("ignoring unparseable environment override; keeping the default",
				"var", "EXCHANGE_MIN_EDGE", "value", v, "default", exchangeCfg.MinEdge)
		case f == 0:
			slog.Warn("EXCHANGE_MIN_EDGE=0 reads as unset by the engine; keeping the default (pass a small negative value to stop flagging small gains)",
				"default", exchangeCfg.MinEdge)
		default:
			exchangeCfg.MinEdge = f
		}
	}

	// Both served horizons are recomputed from one read; each has its own span
	// and its own persistence demand. EXCHANGE_WINDOW_HOURS and
	// EXCHANGE_MIN_HOURS_SEEN predate the split and still work: they set the
	// RECENT horizon, which is the one an unqualified request gets, and the
	// EXCHANGE_RECENT_* names override them for anyone who wants to say so.
	horizons := append([]exchange.HorizonConfig(nil), exchangeCfg.Horizons...)
	for i, horizon := range horizons {
		switch horizon.Horizon {
		case exchange.HorizonRecent:
			horizon.WindowHours = envPositiveInt("EXCHANGE_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_MIN_HOURS_SEEN", horizon.MinHoursSeen)
			horizon.WindowHours = envPositiveInt("EXCHANGE_RECENT_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_RECENT_MIN_HOURS_SEEN", horizon.MinHoursSeen)
		case exchange.HorizonDay:
			horizon.WindowHours = envPositiveInt("EXCHANGE_DAY_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_DAY_MIN_HOURS_SEEN", horizon.MinHoursSeen)
		}
		horizons[i] = horizon
	}
	exchangeCfg.Horizons = horizons

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
	tradeCacheMax := 200
	if v := os.Getenv("TRADE_CACHE_MAX"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			tradeCacheMax = n
		}
	}
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
			LeagueName:        scope.ID(),
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
		MercureSubscriberKey: os.Getenv("MERCURE_SUBSCRIBER_KEY"),
		MercurePublicURL:     os.Getenv("MERCURE_PUBLIC_URL"),
		TradeGate:            tradeGate,
		TradeCache:           tradeCache,
		TradeRepo:            tradeRepo,
		TradeSyncTimeout:     tradeSyncTimeout,
		League:               scope,
		Analyzer:             analyzer,
		AllowedOrigins:       corsOrigins(),
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
		IconCacheDir: getEnvDefault("ICON_CACHE_DIR", server.DefaultIconCacheDir),
	}

	router := server.NewRouter(pool, frontendFS, routerCfg)

	// Seed cache from DB on startup.
	go func() {
		qCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
		defer cancel()

		// Estimate next fetch from the interval between last two gem snapshots.
		var lastSnap, prevSnap time.Time
		if err := pool.QueryRow(qCtx,
			`SELECT time FROM gem_snapshots WHERE league = $1 ORDER BY time DESC LIMIT 1`, scope.ID(),
		).Scan(&lastSnap); err == nil && !lastSnap.IsZero() {
			// Find the previous distinct snapshot time.
			_ = pool.QueryRow(qCtx,
				`SELECT time FROM gem_snapshots WHERE league = $1 AND time < $2 ORDER BY time DESC LIMIT 1`, scope.ID(), lastSnap,
			).Scan(&prevSnap)

			var interval time.Duration
			if !prevSnap.IsZero() {
				interval = lastSnap.Sub(prevSnap)
			}
			if interval < 10*time.Minute || interval > 2*time.Hour {
				interval = 30 * time.Minute // sane fallback
			}
			nextFetch := lastSnap.Add(interval)
			labCache.For(scope).SetNextFetch(nextFetch)
			slog.Info("startup: seeded nextFetch", "lastSnap", lastSnap, "interval", interval, "nextFetch", nextFetch)
		}

		// Seed divine rate.
		var divRate float64
		if err := pool.QueryRow(qCtx,
			`SELECT chaos FROM currency_snapshots WHERE league = $1 AND currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
			scope.ID(),
		).Scan(&divRate); err == nil && divRate > 0 {
			labCache.For(scope).SetDivineRate(divRate)
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
		if err := analyzer.RecomputeLatestV2(ctx, scope); err != nil {
			slog.Warn("startup v2 recompute failed (non-fatal)", "error", err)
		}
		// Font analysis second — needs GemFeatures from V2.
		if err := analyzer.RunFont(ctx, scope); err != nil {
			slog.Warn("startup font analysis failed (non-fatal)", "error", err)
		}
		// Double corrupt runs after Font: the compare path uses its EV as the
		// tiebreaker when no Font candidate wins on 20/20 value.
		if err := analyzer.RunDoubleCorrupt(ctx, scope); err != nil {
			slog.Warn("startup double corrupt analysis failed (non-fatal)", "error", err)
		}
		// Dedication runs after V2 for risk-adjustment features.
		if err := analyzer.RunDedication(ctx, scope); err != nil {
			slog.Warn("startup dedication analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("transfigure analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunTransfigure(ctx, scope); err != nil {
			slog.Warn("startup transfigure analysis failed (non-fatal)", "error", err)
		}
	}()
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("quality analysis panicked on startup", "recover", r)
			}
		}()
		if err := analyzer.RunQuality(ctx, scope); err != nil {
			slog.Warn("startup quality analysis failed (non-fatal)", "error", err)
		}
	}()
	// Currency-exchange plays are held in memory only, so a restart leaves the
	// cache COLD until something recomputes it. Rebuild at boot rather than
	// waiting for the collector's next stored hour, which can be minutes away.
	// Trigger logs its own failures and never blocks serving.
	go exchangeService.Trigger(ctx)
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

		topics := []string{
			"poe/collector/gems",
			"poe/collector/currency",
			"poe/collector/fragments",
			"poe/collector/trade-tick", // collector schedules trade refresh ticks
			"poe/admin/recompute",      // operator-triggered full recompute
			exchange.Topic,             // collector stored a currency-exchange hour
		}
		mercureSubKey := os.Getenv("MERCURE_SUBSCRIBER_KEY")
		// One-shot warning for misconfigured deploys where the collector is
		// publishing trade-ticks but the server has trade disabled. Repeated
		// every-tick warnings would just be noise.
		var tradeDisabledWarn sync.Once
		// Reject collector events stamped for a different league or a bumped
		// revision — a stale publisher lingering across a rolling deploy must not
		// have its data applied as current. Rejections are counted and logged.
		eventGuard := server.NewLeagueEventGuard(scope)
		sub := server.NewMercureSubscriber(mercureURL, topics, mercureSubKey, func(ev server.MercureEvent) {
			// Operator-triggered full recompute (from cmd/recalculate). Use parent
			// ctx so the pipeline survives a subscriber reconnect mid-run.
			if ev.Topic == "poe/admin/recompute" {
				slog.Info("mercure: admin recompute requested")
				go runFullRecompute(ctx, analyzer, scope)
				return
			}

			// Collector-driven trade refresh tick. Parent ctx for the same reason —
			// we want any in-flight gate request to complete and log its outcome
			// even if the subscriber temporarily drops.
			if ev.Topic == "poe/collector/trade-tick" {
				if tradeGate == nil {
					tradeDisabledWarn.Do(func() {
						slog.Warn("mercure: trade-tick received but trade subsystem is disabled; further ticks will be silently dropped")
					})
					return
				}
				if !eventGuard.AcceptRaw([]byte(ev.Data)) {
					return
				}
				go server.HandleTradeTick(ctx, tradeGate, tradeCache, labCache, scope, []byte(ev.Data))
				return
			}

			// A stored currency-exchange hour. This branch returns before the
			// generic dispatch below, which reads payload["endpoint"] and knows
			// only the poe.ninja endpoints — falling through would log a
			// "missing endpoint" warning per hour and do nothing useful.
			//
			// The payload itself is ignored on purpose (see Service.HandleEvent):
			// its "rows" field counts what that pass inserted, not what the hour
			// holds, so a replayed hour reports 0 while being fully populated.
			// Parent ctx, like the branches above, so a recompute survives a
			// subscriber reconnect. Recomputes coalesce, so a catch-up burst
			// costs one extra pass, not one per hour.
			if ev.Topic == exchange.Topic {
				if eventGuard.AcceptRaw([]byte(ev.Data)) {
					go exchangeService.HandleEvent(ctx, []byte(ev.Data))
				}
				return
			}

			// Reject collector data events stamped for a non-active league or
			// revision before they are processed as current data.
			if !eventGuard.AcceptRaw([]byte(ev.Data)) {
				return
			}

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
						`SELECT chaos FROM currency_snapshots WHERE league = $1 AND currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
						scope.ID(),
					).Scan(&rate); err != nil {
						slog.Warn("currency event: divine rate query failed", "error", err)
						return
					}
					labCache.For(scope).SetDivineRate(rate)
					slog.Info("currency event: divine rate updated", "rate", rate)
				}()
			}

			if endpoint == "ninja_fragments" || endpoint == "ninja-fragments" {
				go func() {
					qCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
					defer cancel()
					// Stores the answer even when it is empty — see
					// RefreshOfferingTimings and the cache-state contract in
					// internal/lab/cache.go. This used to store only a non-empty
					// result, the writer-side twin of the reader defect POE-152 fixed.
					n, err := handlers.RefreshOfferingTimings(qCtx, pool, labCache, scope)
					if err != nil {
						slog.Warn("fragment event: offering timing refresh failed", "error", err)
						return
					}
					slog.Info("fragment event: offering timing updated", "offerings", n)
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
					if err := analyzer.RunTransfigure(subCtx, scope); err != nil {
						slog.Warn("transfigure analysis failed", "error", err)
					}
				}()
				go func() {
					defer func() {
						if r := recover(); r != nil {
							slog.Error("quality analysis panicked", "recover", r)
						}
					}()
					if err := analyzer.RunQuality(subCtx, scope); err != nil {
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
					if err := analyzer.RunV2(subCtx, scope); err != nil {
						slog.Warn("v2 analysis failed", "error", err)
						return
					}
					// Font runs after V2 so it reads fresh GemFeatures with current tier classification.
					if err := analyzer.RunFont(subCtx, scope); err != nil {
						slog.Warn("font analysis failed", "error", err)
					}
					// Double corrupt runs after Font: the compare path uses its
					// EV as the tiebreaker when no Font candidate wins on 20/20
					// value, so its corpus must be warm by the time a compare
					// request lands.
					if err := analyzer.RunDoubleCorrupt(subCtx, scope); err != nil {
						slog.Warn("double corrupt analysis failed", "error", err)
					}
					// Dedication runs after V2 for risk-adjustment features.
					if err := analyzer.RunDedication(subCtx, scope); err != nil {
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

					// Take the league data lock and confirm the active league has
					// not changed since this timer was scheduled. Without this the
					// callback would commit/cache RunV2 output under a league that
					// may have drifted while the timer was in flight, and could
					// race a second server's recompute of the same dataset.
					lock, skip, err := prepareDelayedRecompute(context.Background(), pool, scope)
					if err != nil {
						slog.Warn("delayed recompute: skipped", "reason", "lock/league check failed", "error", err)
						return
					}
					if lock == nil {
						slog.Info("delayed recompute: skipped", "reason", skip)
						return
					}
					defer lock.Release()

					slog.Info("delayed recompute: running v2 with accumulated trade data")
					if err := analyzer.RunV2(context.Background(), scope); err != nil {
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

// envPositiveInt reads key as a positive integer, falling back to def when the
// variable is unset. An unparseable or non-positive value logs a Warn naming the
// variable and keeps def: the engine treats a non-positive count as "unset" and
// would restore the default anyway, so the log is the only way an operator finds
// out their override never took effect.
func envPositiveInt(key string, def int) int {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	n, err := strconv.Atoi(v)
	if err != nil || n <= 0 {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return n
}

// envPositiveFloat is envPositiveInt for a float knob, with the same
// keep-the-default-and-say-so contract.
func envPositiveFloat(key string, def float64) float64 {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	f, err := strconv.ParseFloat(v, 64)
	if err != nil || f <= 0 {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return f
}

// envBool is envPositiveInt for a switch, with the same
// keep-the-default-and-say-so contract. There is no "unset" value to inherit
// from — a bool that parses replaces the default outright — so an empty
// variable and an unparseable one are the only ways to keep it.
func envBool(key string, def bool) bool {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return b
}

func getEnvDefault(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
