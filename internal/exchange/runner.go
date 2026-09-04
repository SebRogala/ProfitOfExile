package exchange

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"time"

	"profitofexile/internal/league"
)

// Topic is the Mercure topic the runner publishes one event per stored hour on.
const Topic = "poe/collector/currency-exchange"

// EndpointName is the value of the event's "endpoint" field. The feed is not a
// poe.ninja endpoint, but downstream subscribers key on this field for every
// collector event, so it carries the same name shape.
const EndpointName = "currency-exchange"

// secondsPerHour is the cursor step: the feed publishes one immutable snapshot
// per unix hour, so the walk moves in whole hours of unix seconds.
const secondsPerHour int64 = 3600

// Runner defaults.
const (
	defaultTick            = 5 * time.Minute
	defaultMaxHoursPerTick = 48
	defaultBootstrap       = 24 * time.Hour
)

// HourFetcher fetches one hour of the feed. *Client satisfies it.
type HourFetcher interface {
	FetchHour(ctx context.Context, hour int64, realm string) (*HourPayload, error)
}

// Store persists an hour and tracks the walk position. *Repository satisfies it.
type Store interface {
	Cursor(ctx context.Context, scope league.Scope) (int64, bool, error)
	InsertHour(ctx context.Context, scope league.Scope, hour time.Time, rows []Row, nextHour int64) (int, error)
}

// EventPublisher publishes one already-built payload map on a topic. The
// implementation lives in cmd/collector, because stamping the league identity
// needs internal/collector and this package must not import it.
type EventPublisher interface {
	Publish(ctx context.Context, topic string, payload map[string]any) error
}

// RunnerConfig configures a Runner. Every field except Scope has a default, and
// Now plus Tick exist so tests drive the walk without waiting on real time.
type RunnerConfig struct {
	// Scope is the league whose rows are kept and whose cursor is advanced. It
	// is required and validated when the Runner is built.
	Scope league.Scope
	// Realm is the feed realm path segment; RealmPC (the empty string) is both
	// the default and the PC realm's own value.
	Realm string
	// Tick is how often the walk runs. Default defaultTick.
	Tick time.Duration
	// MaxHoursPerTick bounds one catch-up pass. Default defaultMaxHoursPerTick.
	MaxHoursPerTick int
	// Bootstrap is how far back the walk starts when the league has no cursor
	// row. Default defaultBootstrap.
	Bootstrap time.Duration
	// PerHourDelay is slept between consecutive hour fetches of one pass, so a
	// 48-hour backlog does not pull 48 payloads of roughly 1.7 MB from GGG's CDN
	// back to back. Zero means no pacing — deliberately NOT defaulted here: the
	// production value comes from the cmd/collector wiring (EXCHANGE_PER_HOUR_DELAY,
	// 250ms), which keeps an unpaced walk available to callers and to tests
	// without a sentinel value.
	PerHourDelay time.Duration
	// Now supplies the clock. Default time.Now.
	Now func() time.Time
	// Logger receives the walk's log lines. Default slog.Default().
	Logger *slog.Logger
}

// Runner walks the currency-exchange feed hour by hour, stores the active
// league's rows and publishes one event per stored hour.
//
// It has no retries and no backoff of its own: any failure stops the pass
// without advancing the cursor, and the next tick retries the same hour. The
// cursor lives in the database, so a restart resumes the walk where it stopped.
type Runner struct {
	fetcher   HourFetcher
	store     Store
	publisher EventPublisher
	cfg       RunnerConfig
}

// NewRunner builds a Runner, filling in every unset RunnerConfig default.
//
// An invalid Scope is not rejected here — Run returns it as an error instead, so
// wiring code has one error path rather than a panic to guard.
func NewRunner(f HourFetcher, s Store, p EventPublisher, cfg RunnerConfig) *Runner {
	if cfg.Tick <= 0 {
		cfg.Tick = defaultTick
	}
	if cfg.MaxHoursPerTick <= 0 {
		cfg.MaxHoursPerTick = defaultMaxHoursPerTick
	}
	if cfg.Bootstrap <= 0 {
		cfg.Bootstrap = defaultBootstrap
	}
	if cfg.Now == nil {
		cfg.Now = time.Now
	}
	if cfg.Logger == nil {
		cfg.Logger = slog.Default()
	}
	// Realm needs no default branch: RealmPC is the empty string.
	return &Runner{fetcher: f, store: s, publisher: p, cfg: cfg}
}

// Run walks once immediately, then on every tick until ctx is done, and returns
// ctx.Err().
//
// A failed pass is logged by RunOnce and never stops the loop: the feed being
// unreachable for one tick must not take the ingest goroutine down for the rest
// of the process lifetime.
func (r *Runner) Run(ctx context.Context) error {
	if err := r.cfg.Scope.Validate(); err != nil {
		return fmt.Errorf("exchange runner: run: %w", err)
	}

	ticker := time.NewTicker(r.cfg.Tick)
	defer ticker.Stop()

	// An already-cancelled ctx skips the immediate pass rather than spending a
	// fetch on a walk that cannot finish.
	if ctx.Err() == nil {
		_, _ = r.RunOnce(ctx)
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
			_, _ = r.RunOnce(ctx)
		}
	}
}

// RunOnce walks from the stored cursor forward and returns how many hours were
// stored in this pass.
//
// The walk stops without advancing on three conditions, all of which leave the
// cursor on the hour that failed:
//
//   - *ErrNotPublished — the feed has not published that hour yet. This is the
//     normal end of a caught-up pass, so it returns a nil error.
//   - any other fetch error — transport, decode or an unexpected status.
//   - an hour whose every market was skipped as malformed, which reads as feed
//     schema drift rather than one bad row.
//
// It also stops after MaxHoursPerTick hours, so a long outage catches up over
// several ticks instead of pulling a day of 1.7 MB payloads in one burst.
func (r *Runner) RunOnce(ctx context.Context) (processed int, err error) {
	if err := r.cfg.Scope.Validate(); err != nil {
		return 0, fmt.Errorf("exchange runner: run once: %w", err)
	}

	cursor, err := r.resolveCursor(ctx)
	if err != nil {
		return 0, err
	}

	for processed < r.cfg.MaxHoursPerTick {
		// Pace between hours, never before the first: a caught-up walk that
		// stores one hour per tick must not pay the delay at all.
		if processed > 0 && !r.pace(ctx) {
			return processed, nil
		}

		payload, err := r.fetcher.FetchHour(ctx, cursor, r.cfg.Realm)
		if err != nil {
			// Shutdown cancels the fetch in flight. That is an orderly stop, not
			// a feed failure, so it leaves the cursor alone without a warning
			// operators would have to learn to ignore on every restart.
			if ctx.Err() != nil {
				return processed, nil
			}
			var notPublished *ErrNotPublished
			if errors.As(err, &notPublished) {
				// The 404 body names the hour the feed wants next. More than one
				// hour past our cursor means the hours in between are not coming
				// — aged out of retention, or never published — and the walk is
				// stuck on one of them until someone moves the cursor.
				if gap := notPublished.NextChangeID - cursor; gap > secondsPerHour {
					r.cfg.Logger.Warn("currency-exchange: feed moved past the cursor",
						"hour", cursor, "nextChangeID", notPublished.NextChangeID,
						"gapHours", gap/secondsPerHour, "processed", processed)
					return processed, nil
				}
				r.cfg.Logger.Debug("currency-exchange: hour not published yet",
					"hour", cursor, "nextChangeID", notPublished.NextChangeID, "processed", processed)
				return processed, nil
			}
			r.cfg.Logger.Warn("currency-exchange: fetch hour failed",
				"hour", cursor, "processed", processed, "error", err)
			return processed, fmt.Errorf("exchange runner: fetch hour %d: %w", cursor, err)
		}

		rows, stats := Normalize(payload)

		// Every market malformed is a schema change, not a bad row: storing the
		// resulting empty hour would advance the cursor past data that is
		// actually there, and the hour is never re-fetched.
		if len(payload.Markets) > 0 && stats.Skipped == len(payload.Markets) {
			err := fmt.Errorf("exchange runner: hour %d: all %d markets skipped as malformed", cursor, len(payload.Markets))
			r.cfg.Logger.Error("currency-exchange: every market skipped, not advancing",
				"hour", cursor, "skipped", stats.Skipped, "processed", processed, "error", err)
			return processed, err
		}

		// A partly skipped hour is not the schema drift the gate above catches,
		// but it is still silent data loss: the hour stores fewer markets than
		// the feed carried, and the Info line below reports only what was kept.
		if stats.Skipped > 0 {
			r.cfg.Logger.Warn("currency-exchange: some markets skipped as malformed",
				"hour", cursor, "skipped", stats.Skipped, "markets", len(payload.Markets))
		}

		leagueRows, otherLeagueRows := r.filterScope(rows)

		hour := time.Unix(cursor, 0).UTC()
		nextCursor := cursor + secondsPerHour

		inserted, err := r.store.InsertHour(ctx, r.cfg.Scope, hour, leagueRows, nextCursor)
		if err != nil {
			if ctx.Err() != nil {
				// Shutdown landed between the fetch and the store: the hour was
				// not advanced, the next pass re-fetches it. Not worth a warning.
				return processed, nil
			}
			r.cfg.Logger.Warn("currency-exchange: store hour failed",
				"hour", cursor, "rows", len(leagueRows), "processed", processed, "error", err)
			return processed, fmt.Errorf("exchange runner: store hour %d: %w", cursor, err)
		}

		r.cfg.Logger.Info("currency-exchange: hour stored",
			"hour", cursor,
			"nextCursor", nextCursor,
			"rows", inserted,
			"skipped", stats.Skipped,
			"invalid", stats.Invalid,
			"non_reduced", stats.NonReduced,
			"otherLeagueRows", otherLeagueRows,
		)

		// The hour is committed at this point, cursor included, so a failed
		// publish must not stop the walk — it would re-fetch an hour that is
		// already stored and never make progress.
		r.publish(ctx, cursor, nextCursor, inserted)

		cursor = nextCursor
		processed++
	}

	return processed, nil
}

// resolveCursor reads the stored cursor, or bootstraps at Now()-Bootstrap
// truncated to the hour when the league has no cursor row yet.
func (r *Runner) resolveCursor(ctx context.Context) (int64, error) {
	cursor, found, err := r.store.Cursor(ctx, r.cfg.Scope)
	if err != nil {
		r.cfg.Logger.Warn("currency-exchange: read cursor failed", "error", err)
		return 0, fmt.Errorf("exchange runner: read cursor: %w", err)
	}
	if found {
		return cursor, nil
	}

	bootstrap := r.cfg.Now().Add(-r.cfg.Bootstrap).UTC().Truncate(time.Hour).Unix()
	r.cfg.Logger.Info("currency-exchange: no cursor, bootstrapping",
		"hour", bootstrap, "bootstrap", r.cfg.Bootstrap.String())
	return bootstrap, nil
}

// pace sleeps PerHourDelay and reports whether the walk may continue. It returns
// false when ctx ends first, so a shutdown during the delay stops the pass
// instead of sleeping out the rest of the backlog.
func (r *Runner) pace(ctx context.Context) bool {
	if r.cfg.PerHourDelay <= 0 {
		return true
	}
	timer := time.NewTimer(r.cfg.PerHourDelay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return false
	case <-timer.C:
		return true
	}
}

// filterScope keeps the rows of the configured league and counts the rest. The
// feed carries every league in one payload, so this is where the other leagues
// are dropped; the repository rejects a foreign row as a defensive backstop.
func (r *Runner) filterScope(rows []Row) (kept []Row, otherLeagueRows int) {
	id := r.cfg.Scope.ID()
	kept = make([]Row, 0, len(rows))
	for i := range rows {
		if rows[i].League == id {
			kept = append(kept, rows[i])
			continue
		}
		otherLeagueRows++
	}
	return kept, otherLeagueRows
}

// publish emits the per-hour event. An hour that stored zero rows is published
// too, because downstream recompute needs to know the hour passed.
//
// The "rows" field counts the rows this pass INSERTED, not the rows the hour
// holds: a crash-replayed hour re-inserts nothing and publishes rows: 0 over a
// fully populated hour. Consumers must read it as "how much is new" and go
// through Repository.LoadRows when they need the hour's contents.
func (r *Runner) publish(ctx context.Context, hour, nextCursor int64, rows int) {
	if r.publisher == nil {
		return
	}
	payload := map[string]any{
		"topic":      Topic,
		"endpoint":   EndpointName,
		"hour":       hour,
		"nextCursor": nextCursor,
		"rows":       rows,
		"timestamp":  r.cfg.Now().UTC().Format(time.RFC3339),
	}
	if err := r.publisher.Publish(ctx, Topic, payload); err != nil {
		r.cfg.Logger.Warn("currency-exchange: publish failed",
			"hour", hour, "nextCursor", nextCursor, "rows", rows, "error", err)
	}
}
