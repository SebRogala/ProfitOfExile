package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/exchange"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/server"
	"profitofexile/internal/temple"
)

// recomputeAnalyzer is the production analyzer surface used by startup and
// event recomputes. Keeping this command-local interface makes the dispatch
// and lifecycle seams testable without changing the analyzer package's owner.
type recomputeAnalyzer interface {
	RunTransfigure(context.Context, league.Scope) error
	RunQuality(context.Context, league.Scope) error
	RecomputeLatestV2(context.Context, league.Scope) error
	RunV2(context.Context, league.Scope) error
	RunFont(context.Context, league.Scope) error
	RunDoubleCorrupt(context.Context, league.Scope) error
	RunDedication(context.Context, league.Scope) error
}

type recomputeService interface {
	Trigger(context.Context)
	HandleEvent(context.Context, []byte)
}

type runtimeSubscriber interface {
	Run(context.Context)
}

type runtimeTimer interface {
	Stop() bool
}

type runtimeLock interface {
	Release()
}

type runtimePrepareDelayed func(context.Context, league.Scope) (runtimeLock, string, error)
type runtimeTimerFactory func(time.Duration, func()) runtimeTimer
type runtimeSubscriberFactory func(string, []string, string, func(server.MercureEvent)) runtimeSubscriber

// adaptDelayedLockResult keeps a nil *league.ProcessLock nil when it crosses
// into the runtime's interface seam. A typed-nil interface would make a clean
// lock/league skip look like the proceed path and panic when released.
func adaptDelayedLockResult(lock *league.ProcessLock, skip string, err error) (runtimeLock, string, error) {
	if lock == nil {
		return nil, skip, err
	}
	return lock, skip, err
}

type recomputeRuntime struct {
	ctx           context.Context
	scope         league.Scope
	mercureURL    string
	subscriberKey string

	pool                  *pgxpool.Pool
	labCache              *lab.Cache
	throttler             *lab.Throttler
	analyzer              recomputeAnalyzer
	exchange              recomputeService
	temple                recomputeService
	tradeTick             func(context.Context, []byte)
	updateRate            func(context.Context)
	refreshOfferingTiming func(context.Context)

	subscriberFactory runtimeSubscriberFactory
	timerFactory      runtimeTimerFactory
	prepareDelayed    runtimePrepareDelayed
	runV2             func(context.Context, league.Scope) error
	warnTradeDisabled func()
	eventGuard        *server.LeagueEventGuard

	tradeDisabledWarn sync.Once

	subCtx     context.Context
	subCancel  context.CancelFunc
	subscriber runtimeSubscriber

	// The timer fires 15 minutes after the last ninja_gems event so V2 picks up
	// trade data accumulated since the snapshot. Mercure scheduling and Close can
	// access the timer from different goroutines, so pointer access and replacement
	// are protected by this mutex.
	timerMu               sync.Mutex
	delayedRecomputeTimer runtimeTimer
}

func newRecomputeRuntime(ctx context.Context, scope league.Scope) *recomputeRuntime {
	r := &recomputeRuntime{
		ctx:        ctx,
		scope:      scope,
		eventGuard: server.NewLeagueEventGuard(scope),
		timerFactory: func(delay time.Duration, fn func()) runtimeTimer {
			return time.AfterFunc(delay, fn)
		},
	}
	r.warnTradeDisabled = func() {
		slog.Warn("mercure: trade-tick received but trade subsystem is disabled; further ticks will be silently dropped")
	}
	return r
}

// runFullRecompute runs every analysis step in the order required by
// RecomputeLatestV2. V2 must complete before Font — Font reads GemFeatures
// for tier classification, so running them concurrently leaves Font with
// stale tiers. Each step is run sequentially; an error in one step is logged
// and the next step still runs (matches the pre-Mercure handler's behavior).
//
// Triggered by the poe/admin/recompute Mercure event from cmd/recalculate.
func runFullRecompute(ctx context.Context, analyzer recomputeAnalyzer, scope league.Scope) {
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

func (r *recomputeRuntime) Start() {
	r.startup()
	if r.mercureURL == "" || r.subscriberFactory == nil {
		return
	}

	r.subCtx, r.subCancel = context.WithCancel(r.ctx)
	topics := r.topics()
	r.subscriber = r.subscriberFactory(r.mercureURL, topics, r.subscriberKey, r.dispatch)
	go r.subscriber.Run(r.subCtx)
	slog.Info("mercure subscriber started", "topics", topics)
}

// Close preserves the former defer order: subscriber cancellation happens
// before the pending timer is stopped. A timer callback already handed to its
// goroutine is not cancelled by Stop and is intentionally allowed to finish.
func (r *recomputeRuntime) Close() {
	if r.subCancel != nil {
		r.subCancel()
	}
	r.timerMu.Lock()
	if r.delayedRecomputeTimer != nil {
		r.delayedRecomputeTimer.Stop()
	}
	r.timerMu.Unlock()
}

func (r *recomputeRuntime) startup() {
	if r.pool != nil && r.labCache != nil {
		go r.seedCaches()
	}
	if r.analyzer != nil {
		go r.startupV2AndFont()
		go r.startupTransfigure()
		go r.startupQuality()
	}
	if r.exchange != nil {
		// Exchange plays live in memory. Rebuild them at boot instead of leaving
		// the cache COLD until the collector stores its next hour, which may be
		// minutes away; Trigger logs failures and never blocks serving.
		go r.exchange.Trigger(r.ctx)
	}
	if r.temple != nil {
		// The temple market is also held in memory only. Rebuild it at boot rather
		// than waiting for the next stored item tick, which may be a full poe.ninja
		// cache cycle away; Trigger logs failures and never blocks serving.
		go r.temple.Trigger(r.ctx)
	}
}

func (r *recomputeRuntime) seedCaches() {
	qCtx, cancel := context.WithTimeout(r.ctx, 5*time.Second)
	defer cancel()

	// Estimate next fetch from the interval between last two gem snapshots.
	var lastSnap, prevSnap time.Time
	if err := r.pool.QueryRow(qCtx,
		`SELECT time FROM gem_snapshots WHERE league = $1 ORDER BY time DESC LIMIT 1`, r.scope.ID(),
	).Scan(&lastSnap); err == nil && !lastSnap.IsZero() {
		// Find the previous distinct snapshot time.
		_ = r.pool.QueryRow(qCtx,
			`SELECT time FROM gem_snapshots WHERE league = $1 AND time < $2 ORDER BY time DESC LIMIT 1`, r.scope.ID(), lastSnap,
		).Scan(&prevSnap)

		var interval time.Duration
		if !prevSnap.IsZero() {
			interval = lastSnap.Sub(prevSnap)
		}
		if interval < 10*time.Minute || interval > 2*time.Hour {
			interval = 30 * time.Minute // sane fallback
		}
		nextFetch := lastSnap.Add(interval)
		r.labCache.For(r.scope).SetNextFetch(nextFetch)
		slog.Info("startup: seeded nextFetch", "lastSnap", lastSnap, "interval", interval, "nextFetch", nextFetch)
	}

	// Seed divine rate.
	var divRate float64
	if err := r.pool.QueryRow(qCtx,
		`SELECT chaos FROM currency_snapshots WHERE league = $1 AND currency_id = 'divine' ORDER BY time DESC LIMIT 1`,
		r.scope.ID(),
	).Scan(&divRate); err == nil && divRate > 0 {
		r.labCache.For(r.scope).SetDivineRate(divRate)
		slog.Info("startup: seeded divine rate", "rate", divRate)
	}
}

func (r *recomputeRuntime) startupV2AndFont() {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("startup v2+font analysis panicked", "recover", recovered)
		}
	}()
	// RecomputeLatestV2 attempts to refresh computed data for the latest raw
	// snapshot on startup after a deploy with new scoring logic. It deletes only
	// computed tables, not raw snapshots; failures remain non-fatal below.
	if err := r.analyzer.RecomputeLatestV2(r.ctx, r.scope); err != nil {
		slog.Warn("startup v2 recompute failed (non-fatal)", "error", err)
	}
	// Font reads GemFeatures from V2, so it must run after the V2 rebuild.
	if err := r.analyzer.RunFont(r.ctx, r.scope); err != nil {
		slog.Warn("startup font analysis failed (non-fatal)", "error", err)
	}
	// Double corrupt uses its EV as the tiebreaker when no Font candidate wins
	// on 20/20 value, so its comparison corpus follows Font.
	if err := r.analyzer.RunDoubleCorrupt(r.ctx, r.scope); err != nil {
		slog.Warn("startup double corrupt analysis failed (non-fatal)", "error", err)
	}
	// Dedication follows V2 for its risk-adjustment features.
	if err := r.analyzer.RunDedication(r.ctx, r.scope); err != nil {
		slog.Warn("startup dedication analysis failed (non-fatal)", "error", err)
	}
}

func (r *recomputeRuntime) startupTransfigure() {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("transfigure analysis panicked on startup", "recover", recovered)
		}
	}()
	if err := r.analyzer.RunTransfigure(r.ctx, r.scope); err != nil {
		slog.Warn("startup transfigure analysis failed (non-fatal)", "error", err)
	}
}

func (r *recomputeRuntime) startupQuality() {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("quality analysis panicked on startup", "recover", recovered)
		}
	}()
	if err := r.analyzer.RunQuality(r.ctx, r.scope); err != nil {
		slog.Warn("startup quality analysis failed (non-fatal)", "error", err)
	}
}

func (r *recomputeRuntime) topics() []string {
	topics := []string{
		"poe/collector/gems",
		"poe/collector/currency",
		"poe/collector/fragments",
		"poe/collector/trade-tick", // collector schedules trade refresh ticks
		"poe/admin/recompute",      // operator-triggered full recompute
		exchange.Topic,             // collector stored a currency-exchange hour
	}
	// The seven item-overview topics come from temple.Topics rather than being
	// repeated here, so subscription and endpoint dispatch cannot drift apart.
	return append(topics, temple.Topics()...)
}

// MercureSubscriber.Run reconnects without cancelling subCtx; only Close
// cancels it, just before main cancels r.ctx. The two contexts therefore differ
// only during shutdown.
func (r *recomputeRuntime) eventContext() context.Context {
	if r.subCtx != nil {
		return r.subCtx
	}
	return r.ctx
}

func (r *recomputeRuntime) dispatch(ev server.MercureEvent) {
	if ev.Topic == "poe/admin/recompute" {
		slog.Info("mercure: admin recompute requested")
		if r.analyzer != nil {
			// Runs on the service context r.ctx, as before the extraction.
			go runFullRecompute(r.ctx, r.analyzer, r.scope)
		}
		return
	}

	if ev.Topic == "poe/collector/trade-tick" {
		if r.tradeTick == nil {
			// A misconfigured deployment may publish ticks with trade disabled;
			// warn once because repeating the warning every tick is noise.
			r.tradeDisabledWarn.Do(func() { r.warnTradeDisabled() })
			return
		}
		if !r.eventGuard.AcceptRaw([]byte(ev.Data)) {
			return
		}
		// Runs on the service context r.ctx so the gate request is not tied to the
		// subscriber context.
		go r.tradeTick(r.ctx, []byte(ev.Data))
		return
	}

	if ev.Topic == exchange.Topic {
		// Exchange events use their own topic and do not carry a poe.ninja
		// endpoint. Return before generic endpoint dispatch, or this stored hour
		// would produce a misleading missing-endpoint warning. The exchange
		// service owns replay-safe coalescing. It runs on the service context r.ctx.
		if r.eventGuard.AcceptRaw([]byte(ev.Data)) && r.exchange != nil {
			go r.exchange.HandleEvent(r.ctx, []byte(ev.Data))
		}
		return
	}

	if !r.eventGuard.AcceptRaw([]byte(ev.Data)) {
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

	// Parse nextFetch so the throttler can include it as nextAny in the
	// analysis-updated event.
	var nextFetch time.Time
	if nf, ok := payload["nextFetch"].(string); ok {
		if parsed, err := time.Parse(time.RFC3339, nf); err == nil {
			nextFetch = parsed
		}
	}

	// Only gem snapshots trigger the lab analysis chain; currency and fragment
	// events update their own cache-side data without recomputing the dashboard.
	endpoint, ok := payload["endpoint"].(string)
	if !ok {
		slog.Warn("mercure: missing or non-string 'endpoint' in payload", "payload", payload)
		return
	}
	if endpoint == "ninja_currency" || endpoint == "ninja-currency" {
		if r.updateRate != nil {
			go r.updateRate(context.Background())
		}
	}
	if endpoint == "ninja_fragments" || endpoint == "ninja-fragments" {
		if r.refreshOfferingTiming != nil {
			go r.refreshOfferingTiming(context.Background())
		}
	}
	if temple.IsFeedEndpoint(endpoint) {
		// The temple service coalesces the seven feed events into a bounded
		// recompute. It runs on the service context r.ctx.
		if r.temple != nil {
			go r.temple.HandleEvent(r.ctx, []byte(ev.Data))
		}
		return
	}
	if endpoint != "ninja_gems" && endpoint != "ninja-gems" {
		return
	}

	if r.throttler != nil {
		// Signal on every gem event; nextFetch is optional enrichment, so a missing or
		// invalid timestamp remains the zero value rather than suppressing the update.
		r.throttler.Signal(nextFetch)
	}
	eventCtx := r.eventContext()
	if r.analyzer != nil {
		go r.runGemTransfigure(eventCtx)
		go r.runGemQuality(eventCtx)
		go r.runGemV2Chain(eventCtx)
	}
	r.scheduleDelayedRecompute()
}

func (r *recomputeRuntime) runGemTransfigure(ctx context.Context) {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("transfigure analysis panicked", "recover", recovered)
		}
	}()
	if err := r.analyzer.RunTransfigure(ctx, r.scope); err != nil {
		slog.Warn("transfigure analysis failed", "error", err)
	}
}

func (r *recomputeRuntime) runGemQuality(ctx context.Context) {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("quality analysis panicked", "recover", recovered)
		}
	}()
	if err := r.analyzer.RunQuality(ctx, r.scope); err != nil {
		slog.Warn("quality analysis failed", "error", err)
	}
}

func (r *recomputeRuntime) runGemV2Chain(ctx context.Context) {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("v2/font analysis panicked", "recover", recovered)
		}
	}()
	if err := r.analyzer.RunV2(ctx, r.scope); err != nil {
		slog.Warn("v2 analysis failed", "error", err)
		return
	}
	// Font reads the GemFeatures tier classification produced by V2; running
	// them concurrently would let Font observe the previous cycle's tiers.
	if err := r.analyzer.RunFont(ctx, r.scope); err != nil {
		slog.Warn("font analysis failed", "error", err)
	}
	// Double corrupt follows Font because its compare path uses EV as the
	// tiebreaker when no Font candidate wins on 20/20 value, so its corpus must be
	// warm by the time a compare request lands.
	if err := r.analyzer.RunDoubleCorrupt(ctx, r.scope); err != nil {
		slog.Warn("double corrupt analysis failed", "error", err)
	}
	// Dedication follows V2 for its risk-adjustment features.
	if err := r.analyzer.RunDedication(ctx, r.scope); err != nil {
		slog.Warn("dedication analysis failed", "error", err)
	}
}

func (r *recomputeRuntime) scheduleDelayedRecompute() {
	r.timerMu.Lock()
	defer r.timerMu.Unlock()
	// Coalesce pending gem events: each ninja_gems event moves the T+15m run to
	// 15 minutes after that event, allowing trade data accumulated since the latest
	// snapshot to be included. Holding timerMu serializes replacement with Close; Stop
	// does not cancel a callback already handed to its goroutine.
	if r.delayedRecomputeTimer != nil {
		r.delayedRecomputeTimer.Stop()
	}
	if r.timerFactory == nil {
		return
	}
	r.delayedRecomputeTimer = r.timerFactory(15*time.Minute, r.runDelayedRecompute)
}

func (r *recomputeRuntime) runDelayedRecompute() {
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("delayed v2 recompute panicked", "recover", recovered)
		}
	}()
	if r.prepareDelayed == nil || r.runV2 == nil {
		return
	}
	lock, skip, err := r.prepareDelayed(context.Background(), r.scope)
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
	if err := r.runV2(context.Background(), r.scope); err != nil {
		slog.Warn("delayed v2 recompute failed", "error", err)
	}
}
