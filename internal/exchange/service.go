package exchange

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"profitofexile/internal/league"
)

// UpdatedTopic is the Mercure topic the server publishes on after a recompute
// has refreshed the cache. It is deliberately distinct from Topic, which the
// collector publishes on when an hour was INGESTED: one says "new data exists",
// the other says "the served answer changed", and a client that listened to the
// ingest topic would refetch before the recompute had run.
const UpdatedTopic = "poe/currency-exchange/updated"

// DefaultUpdateDebounce is the quiet period a Debouncer waits before publishing
// UpdatedTopic. A catch-up pass stores several hours back to back and triggers a
// recompute per hour; two seconds is long enough to collapse that burst into one
// client refetch and short enough that a single stored hour still lands promptly.
const DefaultUpdateDebounce = 2 * time.Second

// Cache holds the newest computed Result for instant serving.
//
// It follows the cache-state contract written down in internal/lab/cache.go:
// COLD (nothing stored yet — the recompute has not run or it failed) and
// WARM-AND-EMPTY (the recompute ran and its honest answer was no plays) are
// different states that the value alone cannot tell apart, because an empty
// Result is what BOTH of them hold. Snapshot therefore reports warmth as an
// explicit flag next to the value, and Set stores the recompute's answer even
// when it is empty — a writer that skipped empty results would leave the cache
// COLD forever on a league with no qualifying plays.
//
// The handler reads warm == false as "no answer yet" and renders lastUpdated as
// null; it never falls back to the database, because this package's read side is
// the recompute and a handler query would only repeat it.
type Cache struct {
	mu     sync.RWMutex
	result Result
	warm   bool
}

// NewCache creates an empty, COLD cache.
func NewCache() *Cache {
	return &Cache{}
}

// Set replaces the stored result wholesale and marks the cache warm. Results are
// whole-corpus answers, so there is no merge: the newest recompute is the truth.
//
// An empty Result is stored like any other — see the cache-state contract above.
func (c *Cache) Set(r Result) {
	if c == nil {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	c.result = r
	c.warm = true
}

// Snapshot returns the stored result and whether the cache is warm.
//
// warm == false means COLD: nothing has been computed yet and the returned
// Result is the zero value. warm == true means the Result is authoritative
// INCLUDING when it holds no plays.
//
// The returned Result carries a fresh Plays slice, so a caller that filters or
// sorts it in place cannot corrupt what the next reader sees. The Play values
// inside are copied with it; their Legs slices are shared with the cached
// result, which is safe because a computed Play is never mutated after BestPlays
// returns it — a caller that needs to rewrite legs must copy them first.
//
// A nil *Cache reads as COLD rather than panicking, so a server started without
// the currency-exchange pillar can still register the route.
func (c *Cache) Snapshot() (Result, bool) {
	if c == nil {
		return Result{}, false
	}
	c.mu.RLock()
	defer c.mu.RUnlock()

	snapshot := c.result
	if c.result.Plays != nil {
		snapshot.Plays = make([]Play, len(c.result.Plays))
		copy(snapshot.Plays, c.result.Plays)
	}
	return snapshot, c.warm
}

// RowSource is the storage the recompute reads. *Repository satisfies it.
//
// The service depends on this two-method interface rather than on *Repository so
// the lifecycle logic — window arithmetic, coalescing, cache writes — is
// testable without a database.
type RowSource interface {
	// NewestHour returns the newest stored feed hour, or found == false when the
	// league has no rows.
	NewestHour(ctx context.Context, scope league.Scope) (time.Time, bool, error)
	// LoadRows returns the stored rows for the half-open window [from, to).
	LoadRows(ctx context.Context, scope league.Scope, from, to time.Time) ([]StoredRow, error)
}

// The repository is the production RowSource. Asserting it here means a change
// to either signature fails at this line rather than in cmd/server's wiring.
var _ RowSource = (*Repository)(nil)

// Service recomputes the league's best plays and keeps the Cache current.
//
// It is the server-side counterpart of the collector's Runner: the Runner writes
// hours, the Service reads them back and republishes the derived answer. Nothing
// here is persisted — BestPlays is cheap enough to re-run from stored rows, and
// storing plays is POE-180's call.
//
// # Why lab.Throttler is not reused
//
// The debounce-then-publish shape below duplicates internal/lab/throttler.go on
// purpose. That type hard-codes the topic "poe/analysis/updated", holds a
// *lab.Cache to write SetNextFetch into, and builds a payload out of the gem
// stack's vocabulary — none of which this pillar has. Reusing it would also mean
// importing internal/lab, which boundary_test.go forbids and ADR-008 explains:
// the currency engine scores its feed on its own terms. Debouncer is the
// twenty-line generic core of the same idea, with the topic and the payload left
// to the caller in cmd/server.
type Service struct {
	rows   RowSource
	scope  league.Scope
	cfg    Config
	cache  *Cache
	notify func()
	logger *slog.Logger

	// mu guards running and dirty only. It is never held across a Recompute:
	// the whole point of the flags is to let a second trigger return
	// immediately while the first one is still in the database.
	mu      sync.Mutex
	running bool
	dirty   bool
}

// NewService builds a recompute service for one league scope.
//
// cfg is normalized once here, so every recompute uses the same window and the
// same floors as the values logged at startup. A nil logger falls back to
// slog.Default and a nil notify to a no-op, which is what makes a Service usable
// in a test or in a server started without a Mercure hub.
func NewService(rows RowSource, scope league.Scope, cfg Config, cache *Cache, notify func(), logger *slog.Logger) *Service {
	if logger == nil {
		logger = slog.Default()
	}
	if notify == nil {
		notify = func() {}
	}
	return &Service{
		rows:   rows,
		scope:  scope,
		cfg:    cfg.withDefaults(),
		cache:  cache,
		notify: notify,
		logger: logger,
	}
}

// Recompute reads the newest window from storage, ranks it, stores the answer in
// the cache and signals notify. It returns what it stored.
//
// The window is anchored on the newest hour that actually HAS rows, not on the
// clock and not on the ingest cursor: [newest − WindowHours·1h, newest + 1h).
// A feed that stalls therefore keeps serving its last real hours instead of
// sliding into an empty window, and the upper bound includes the newest hour
// while excluding the next one.
//
// That span covers WindowHours+1 CLOCK hours, which is deliberate rather than
// an off-by-one: the extra oldest hour is a hedge against gaps, since the engine
// keeps only the newest WindowHours DISTINCT hours it finds and a feed that
// missed a poll would otherwise rank one hour short.
//
// A league with no rows yields Result{League, Plays: []Play{}} — warm, empty and
// honest — rather than an error, because "nothing ingested yet" is the normal
// state of a fresh league.
//
// On any error the cache is left untouched and notify is NOT called: a failed
// read must not turn a good cached answer into an empty one, and must not tell
// clients that something changed.
func (s *Service) Recompute(ctx context.Context) (Result, error) {
	if err := s.scope.Validate(); err != nil {
		return Result{}, fmt.Errorf("exchange service: recompute: %w", err)
	}

	newest, found, err := s.rows.NewestHour(ctx, s.scope)
	if err != nil {
		return Result{}, fmt.Errorf("exchange service: recompute: %w", err)
	}

	res := Result{League: s.scope.ID(), Plays: []Play{}}
	newestAttr := "none"
	if found {
		newestAttr = newest.Format(time.RFC3339)
		from := newest.Add(-time.Duration(s.cfg.WindowHours) * time.Hour)
		to := newest.Add(time.Hour)

		rows, loadErr := s.rows.LoadRows(ctx, s.scope, from, to)
		if loadErr != nil {
			return Result{}, fmt.Errorf("exchange service: recompute: %w", loadErr)
		}
		res = BestPlays(s.scope.ID(), rows, s.cfg)
	}

	s.cache.Set(res)
	s.notify()
	s.logger.Info("currency-exchange: plays recomputed",
		"hours", res.Hours,
		"plays", len(res.Plays),
		"newestHour", newestAttr,
	)
	return res, nil
}

// Trigger runs a recompute, coalescing concurrent requests.
//
// A catch-up pass stores several hours in a row and the subscriber fires one
// Trigger per stored hour. Running them all would mean N full window reads for
// one answer, so: while a recompute is in flight, further triggers only set
// dirty and return immediately, and the running recompute then repeats ONCE for
// however many arrived. The rerun is what keeps the last event from being lost —
// the in-flight read may have started before its rows were committed.
//
// Errors are logged at Warn rather than returned: callers are the subscriber and
// the startup goroutine, both fire-and-forget, and the next stored hour triggers
// another attempt. Call Recompute directly when the error matters.
func (s *Service) Trigger(ctx context.Context) {
	s.mu.Lock()
	if s.running {
		s.dirty = true
		s.mu.Unlock()
		return
	}
	s.running = true
	s.mu.Unlock()

	for {
		if _, err := s.Recompute(ctx); err != nil {
			s.logger.Warn("currency-exchange: recompute failed", "error", err)
		}

		s.mu.Lock()
		if s.dirty {
			s.dirty = false
			s.mu.Unlock()
			continue
		}
		s.running = false
		s.mu.Unlock()
		return
	}
}

// HandleEvent recomputes in response to a collector event. The raw payload is
// deliberately ignored.
//
// The event's "rows" field counts what that pass INSERTED, not what the hour
// holds, so a replayed hour publishes rows: 0 while being fully populated — an
// event-content check would skip exactly the recompute a crash recovery needs.
// Reading nothing from the payload also makes the handler replay-safe: a
// duplicate delivery costs one coalesced recompute and changes no state.
//
// The caller is responsible for the league guard (server.LeagueEventGuard); this
// method assumes the event already belongs to the scope.
func (s *Service) HandleEvent(ctx context.Context, raw []byte) {
	_ = raw
	s.Trigger(ctx)
}

// Debouncer collapses a burst of signals into a single trailing-edge call.
//
// Trailing edge, not leading: fn runs once after d of quiet, so a catch-up pass
// that stores six hours publishes one update carrying the FINAL state rather
// than one per hour or one describing the first.
//
// The trailing edge is also why Stop DROPS a pending call at shutdown rather
// than flushing it: the pending fn publishes "the served answer changed" to a
// process that is going away, and clients refetch the endpoint on their next
// poll or reconnect anyway.
type Debouncer struct {
	mu    sync.Mutex
	d     time.Duration
	fn    func()
	timer *time.Timer
}

// NewDebouncer creates a debouncer that calls fn after d of quiet. A
// non-positive d fires as soon as the runtime gets to it, which disables the
// coalescing rather than the call.
func NewDebouncer(d time.Duration, fn func()) *Debouncer {
	return &Debouncer{d: d, fn: fn}
}

// Signal restarts the quiet period. fn runs on a timer goroutine once d passes
// with no further signal.
//
// A nil Debouncer or a nil fn makes this a no-op, so a server wired without a
// Mercure hub can pass Signal as the Service's notify without a branch.
func (b *Debouncer) Signal() {
	if b == nil || b.fn == nil {
		return
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	if b.timer != nil {
		b.timer.Stop()
	}
	b.timer = time.AfterFunc(b.d, b.fn)
}

// Stop cancels a pending call and leaves the debouncer usable: a later Signal
// starts a fresh quiet period.
//
// The pending fn is DROPPED, not flushed — see the type comment. A nil
// Debouncer is a no-op, so a caller may defer Stop on a debouncer it may never
// have built.
//
// A call already handed to the runtime may still run: Stop reports nothing
// about that race because the caller (a shutting-down server) has nothing to do
// with the answer.
func (b *Debouncer) Stop() {
	if b == nil {
		return
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	if b.timer != nil {
		b.timer.Stop()
		b.timer = nil
	}
}

// LastUpdated reports the newest feed hour the result covers, and whether the
// result covers any hour at all.
//
// Result.To is the newest hour PLUS one (the half-open window's exclusive
// bound), so the hour itself is To − 1h. This is data freshness, not compute
// freshness: it answers "how new is the feed" and moves only when a new hour is
// ingested, so a recompute over unchanged rows leaves it where it was. Showing
// the recompute's wall clock here would read as fresh while the feed was hours
// stale.
//
// ok is false when the result covers no hours, in which case From and To are
// zero rather than a real interval and the caller must render null.
func LastUpdated(r Result) (time.Time, bool) {
	return r.To.Add(-time.Hour), r.Hours > 0
}

// UpdatePayload builds the Mercure body published on UpdatedTopic.
//
// It is a notification, not the data: clients read the counts to decide whether
// to refetch GET /api/currency-exchange/plays, which is the one place the plays
// themselves are served. now is passed in rather than read from the clock so the
// caller controls it.
//
// Keys: topic, league, lastUpdated (RFC3339 or null — the newest feed hour, see
// LastUpdated), hours, plays (the count, not the plays) and timestamp (when this
// event was built, RFC3339). cmd/server adds league and leagueRevision on top
// via collector.StampScope; the server drops unstamped events.
func UpdatePayload(r Result, now time.Time) map[string]any {
	payload := map[string]any{
		"topic":       UpdatedTopic,
		"league":      r.League,
		"lastUpdated": nil,
		"hours":       r.Hours,
		"plays":       len(r.Plays),
		"timestamp":   now.UTC().Format(time.RFC3339),
	}
	if last, ok := LastUpdated(r); ok {
		payload["lastUpdated"] = last.UTC().Format(time.RFC3339)
	}
	return payload
}
