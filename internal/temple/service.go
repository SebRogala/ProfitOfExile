package temple

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
	"strings"
	"sync"

	"profitofexile/internal/league"
)

// THE CACHE
//
// Its own type rather than a field on lab.Cache, because it is filled by a
// different event: the gems tick fills lab.Cache, and the seven item ticks fill
// this. The concurrency contract is the same one docs/ANALYSIS-CACHE.md and the
// header of internal/lab/cache.go set out — compute outside the lock, replace
// wholesale, never mutate stored data in place — and so is the cache-state
// contract: COLD (nothing computed yet) and WARM-AND-EMPTY (the recompute ran
// and found no lines) are different states that the stored value alone cannot
// tell apart, so Snapshot reports warmth as an explicit flag and Set stores the
// recompute's answer even when it is empty.
//
// The handler reads warm == false as "no answer yet" and serves EmptyMarket. It
// never falls back to the database, because the read side of item_snapshots IS
// this recompute — a handler query would repeat it on the request path, which is
// what this cache exists to avoid.

// Cache holds the newest computed temple market for one league.
type Cache struct {
	mu     sync.RWMutex
	scope  league.Scope
	market Market
	warm   bool
}

// NewCache creates an empty, COLD cache bound to scope for the process's
// lifetime.
func NewCache(scope league.Scope) *Cache {
	return &Cache{scope: scope}
}

// For asserts tenancy and returns the receiver.
//
// It is not a lookup. One cache serves exactly one league (ADR-009, and Cache.For
// in internal/lab), so a mismatch is a wiring defect that would otherwise serve
// one league's prices under another league's scope with nothing to show for it —
// it panics rather than returning an empty answer.
//
// A nil *Cache passes through unchecked, so a server started without the temple
// pillar still registers the route and Snapshot answers COLD.
func (c *Cache) For(scope league.Scope) *Cache {
	if c == nil {
		return nil
	}
	if scope.ID() != c.scope.ID() {
		panic(fmt.Sprintf("temple: cache bound to league %q accessed under league %q", c.scope.ID(), scope.ID()))
	}
	return c
}

// Set replaces the stored market wholesale and marks the cache warm.
//
// An empty market is stored like any other: a writer that skipped empty answers
// would leave the cache COLD forever on a league whose feeds serve nothing, and
// every request would then take the cold path with no way to tell.
func (c *Cache) Set(m Market) {
	if c == nil {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	c.market = m
	c.warm = true
}

// Snapshot returns the stored market and whether the cache is warm.
//
// warm == false means COLD — nothing has been computed yet — and the returned
// Market is the zero value; the caller serves EmptyMarket. warm == true means
// the Market is authoritative INCLUDING when it carries no rooms and no priced
// items.
//
// The returned Market carries fresh Rooms, Items and Recipes slices, and a fresh
// Price behind every priced item, so a caller that sorts or rewrites what it was
// given cannot reach what the next reader sees. A nil *Cache reads as COLD
// rather than panicking, so a server started without the temple pillar can still
// register the route.
func (c *Cache) Snapshot() (Market, bool) {
	if c == nil {
		return Market{}, false
	}
	c.mu.RLock()
	defer c.mu.RUnlock()

	if !c.warm {
		return Market{}, false
	}
	return copyMarket(c.market), true
}

func copyMarket(m Market) Market {
	out := m
	if m.AsOf != nil {
		at := *m.AsOf
		out.AsOf = &at
	}
	// make(..., 0, len) rather than append to a nil slice: appending nothing to
	// nil yields nil, so a WARM-AND-EMPTY market — a rollover, or a league whose
	// IncursionTemple feed 404s inside the lookback while the others collect —
	// would marshal as "rooms": null and break a client that deserializes into a
	// non-nullable list (the desktop's Rust Vec, POE-258).
	out.CategoriesSeen = append(make([]string, 0, len(m.CategoriesSeen)), m.CategoriesSeen...)
	out.Rooms = append(make([]Room, 0, len(m.Rooms)), m.Rooms...)
	out.Recipes = append(make([]Recipe, 0, len(m.Recipes)), m.Recipes...)
	out.Items = make([]Item, len(m.Items))
	for i, item := range m.Items {
		out.Items[i] = item
		if item.Price != nil {
			price := *item.Price
			out.Items[i].Price = &price
		}
	}
	return out
}

// Source is the storage a recompute reads. *Repository satisfies it.
//
// The service depends on this two-method interface rather than on *Repository so
// the lifecycle — the two-phase read, the coalescing, the cache write — is
// testable without a database.
type Source interface {
	NewestLines(ctx context.Context, scope league.Scope) ([]Line, error)
	WindowPrices(ctx context.Context, scope league.Scope, lines []Line) (map[string][]WindowPrice, error)
}

// Service recomputes the temple market and keeps the cache filled.
type Service struct {
	source Source
	cache  *Cache
	scope  league.Scope
	logger *slog.Logger

	mu      sync.Mutex
	running bool
	dirty   bool

	// lastUntiered is the previously logged untiered set, joined on a byte no
	// room name can contain. Guarded by mu.
	lastUntiered string
}

// NewService wires a recompute over source into cache for scope. A nil logger
// falls back to the default.
func NewService(source Source, cache *Cache, scope league.Scope, logger *slog.Logger) *Service {
	if logger == nil {
		logger = slog.Default()
	}
	return &Service{source: source, cache: cache, scope: scope, logger: logger}
}

// Recompute reads the newest snapshot of each category, window-prices the thin
// lines, and stores the result.
//
// Two queries, in order: the second one's name list comes out of the first one's
// answer (ThinLines), so a confident market costs one query and a thin one costs
// two — never one per item.
func (s *Service) Recompute(ctx context.Context) (Market, error) {
	if err := s.scope.Validate(); err != nil {
		return Market{}, fmt.Errorf("temple service: recompute: %w", err)
	}

	newest, err := s.source.NewestLines(ctx, s.scope)
	if err != nil {
		return Market{}, fmt.Errorf("temple service: recompute: %w", err)
	}

	thin := ThinLines(newest)
	window, err := s.source.WindowPrices(ctx, s.scope, thin)
	if err != nil {
		return Market{}, fmt.Errorf("temple service: recompute: %w", err)
	}

	market := Build(s.scope.ID(), newest, window)
	s.logUntieredRooms(market.Rooms)
	s.cache.Set(market)

	asOf := "none"
	if market.AsOf != nil {
		asOf = market.AsOf.Format("2006-01-02T15:04:05Z07:00")
	}
	s.logger.Info("temple: market recomputed",
		"league", market.League,
		"asOf", asOf,
		"categories", len(market.CategoriesSeen),
		"rooms", len(market.Rooms),
		"floor", market.Floor,
		"windowPriced", len(thin),
	)
	return market, nil
}

// Trigger runs a recompute, coalescing concurrent callers.
//
// The seven item feeds publish within seconds of each other, so seven events can
// arrive while one recompute is in flight. A caller that finds one running marks
// the work dirty and returns; the running recompute then repeats ONCE for
// however many arrived. The rerun is what keeps the last event from being lost —
// the in-flight read may have started before that event's rows were committed.
//
// This is a mutex and a rerun, deliberately not a debounce or a timer: there is
// no quiet period to wait out and nothing to schedule, so a burst costs two
// bounded queries rather than seven, and the answer served after the burst was
// computed after the last event.
//
// Errors are logged at Warn rather than returned: the callers are the Mercure
// subscriber and the startup goroutine, both fire-and-forget, and the next
// stored tick triggers another attempt. Call Recompute directly when the error
// matters.
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
			s.logger.Warn("temple: recompute failed", "error", err)
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

// HandleEvent recomputes in response to one of the seven collector item events.
//
// The raw payload is deliberately ignored. Its "inserted" field counts what that
// pass wrote, not what the snapshot holds, so a replayed tick publishes
// inserted: 0 while being fully populated — a content check would skip exactly
// the recompute a catch-up needs. Reading nothing from it also makes this
// replay-safe: a duplicate delivery costs one coalesced recompute and changes no
// state.
//
// The caller owns the league guard (server.LeagueEventGuard) and the endpoint
// test (IsFeedEndpoint); this method assumes the event already belongs to the
// scope and to one of the seven feeds.
func (s *Service) HandleEvent(ctx context.Context, raw []byte) {
	_ = raw
	s.Trigger(ctx)
}

// logUntieredRooms reports the room lines whose name carries no "(Tier n)"
// suffix, once per distinct set.
//
// On CHANGE, not once for the process: the boot set is stable — Apex of Atzoatl
// and the ten connector rooms — so a line per tick would be pure noise, but the
// reason to log at all is to make a NEW unparseable name visible, and a
// once-for-the-process guard fires on that stable boot set and then never
// reports the new name at all. Comparing the sorted set against the last one
// logged keeps the boot set to a single line and still reports the tick a name
// joins or leaves it. They are served with tier 0, never dropped.
func (s *Service) logUntieredRooms(rooms []Room) {
	var untiered []string
	for _, room := range rooms {
		if room.Tier == 0 {
			untiered = append(untiered, room.Name)
		}
	}
	sort.Strings(untiered)
	set := strings.Join(untiered, "\x00")

	s.mu.Lock()
	changed := set != s.lastUntiered
	s.lastUntiered = set
	s.mu.Unlock()

	// An empty set is remembered but not logged: there is nothing to report, and
	// remembering it is what makes the name's return a change again.
	if !changed || len(untiered) == 0 {
		return
	}
	s.logger.Info("temple: room lines with no tier suffix, served with tier 0",
		"count", len(untiered), "names", untiered)
}
