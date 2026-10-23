package harvest

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
)

// UpdatedTopic notifies clients only after Harvest has stored both horizons.
const UpdatedTopic = "poe/harvest/updated"

// loadSpanHours is the clock hours one recompute reads: the 72 hours ending at
// the newest stored hour, inclusive. It covers the 24-hour horizon and the
// newest-hour-before-window lookback that LastSeenChaos needs (D5, D7).
const loadSpanHours = 72

// horizons are the windows every recompute prices, in the order they are
// computed. Both are read from the same loaded rows (D2).
var horizons = []struct {
	horizon exchange.Horizon
	hours   int
}{
	{horizon: exchange.HorizonRecent, hours: 6},
	{horizon: exchange.HorizonDay, hours: 24},
}

// Snapshot is one horizon's Harvest answer: the prices and the tiers computed
// from the same rows, stamped with the feed hour they describe. Serving both
// from one Snapshot is what keeps prices and tiers from straddling a recompute.
type Snapshot struct {
	League          string
	Horizon         exchange.Horizon
	PriceHour       time.Time
	DivineChaosRate float64
	Items           map[string]ItemPrice
	Lifeforce       map[string]LifeforcePrice
	// Tiers is keyed by family id.
	Tiers map[string]FamilyTiers
}

// Cache holds the newest Snapshot per horizon.
//
// A horizon is COLD until a recompute has found a newest feed hour and stored a
// snapshot for it (D8). A league with no rows therefore stays COLD. This differs
// from exchange.Cache, which warms an empty league. Snapshot reports warmth as an
// explicit flag next to the value, so the zero Snapshot is never mistaken for an
// answer.
type Cache struct {
	mu    sync.RWMutex
	snaps map[exchange.Horizon]Snapshot
}

// NewCache creates an empty, COLD cache.
func NewCache() *Cache {
	return &Cache{snaps: make(map[exchange.Horizon]Snapshot)}
}

// Set replaces one horizon's snapshot and marks that horizon warm. The cache
// keeps its own copy of the maps, so the caller may keep mutating its values.
// A nil Cache is a no-op.
func (c *Cache) Set(h exchange.Horizon, s Snapshot) {
	if c == nil {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.snaps == nil {
		c.snaps = make(map[exchange.Horizon]Snapshot)
	}
	c.snaps[h] = cloneSnapshot(s)
}

// Snapshot returns one horizon's snapshot and whether that horizon is warm.
// warm == false means COLD and the returned Snapshot is the zero value.
//
// The Items, Lifeforce and Tiers maps are copied down to their slices and
// pointers, so a caller that mutates the result cannot change what the next
// reader sees. A nil Cache reads as COLD.
func (c *Cache) Snapshot(h exchange.Horizon) (Snapshot, bool) {
	if c == nil {
		return Snapshot{}, false
	}
	c.mu.RLock()
	defer c.mu.RUnlock()
	s, ok := c.snaps[h]
	if !ok {
		return Snapshot{}, false
	}
	return cloneSnapshot(s), true
}

// Service recomputes the Harvest prices and tiers and keeps the Cache current.
//
// It mirrors exchange.Service: the exchange service serves plays from the rows
// the collector stores, and this one serves Harvest prices and tiers from the
// same rows, for the same league scope.
type Service struct {
	rows   exchange.RowSource
	scope  league.Scope
	cache  *Cache
	notify func()
	logger *slog.Logger

	// mu guards running and dirty only. It is never held across a Recompute, so a
	// second trigger returns immediately while the first is still reading.
	mu      sync.Mutex
	running bool
	dirty   bool
}

// NewService builds a recompute service for one league scope. A nil logger
// falls back to slog.Default; a nil notify is a no-op.
func NewService(rows exchange.RowSource, scope league.Scope, cache *Cache, notify func(), logger *slog.Logger) *Service {
	if logger == nil {
		logger = slog.Default()
	}
	if notify == nil {
		notify = func() {}
	}
	return &Service{
		rows:   rows,
		scope:  scope,
		cache:  cache,
		notify: notify,
		logger: logger,
	}
}

// Recompute reads the newest 72 hours from storage, prices every Harvest item
// and lifeforce colour per horizon, classifies the families, and stores both
// horizons in the cache before signaling notify.
//
// A league with no rows returns nil and leaves the cache untouched, so a cold
// cache stays cold. On any error the cache is left untouched and the error is
// returned wrapped. Neither case signals notify.
func (s *Service) Recompute(ctx context.Context) error {
	if err := s.scope.Validate(); err != nil {
		return fmt.Errorf("harvest service: recompute: %w", err)
	}

	newest, found, err := s.rows.NewestHour(ctx, s.scope)
	if err != nil {
		return fmt.Errorf("harvest service: recompute: newest hour: %w", err)
	}
	if !found {
		return nil
	}

	from := newest.Add(-time.Duration(loadSpanHours-1) * time.Hour)
	to := newest.Add(time.Hour)
	rows, err := s.rows.LoadRows(ctx, s.scope, from, to)
	if err != nil {
		return fmt.Errorf("harvest service: recompute: load rows: %w", err)
	}

	itemIDs := ItemIDs()
	for _, hz := range horizons {
		prices := ComputePrices(rows, newest, hz.hours, itemIDs)
		tiers := ClassifyFamilies(tierInput(prices))
		s.cache.Set(hz.horizon, Snapshot{
			League:          s.scope.ID(),
			Horizon:         hz.horizon,
			PriceHour:       newest.UTC(),
			DivineChaosRate: prices.DivineChaosRate,
			Items:           prices.Items,
			Lifeforce:       prices.Lifeforce,
			Tiers:           tiers,
		})

		priced := 0
		for _, ip := range prices.Items {
			if ip.Chaos != nil {
				priced++
			}
		}
		s.logger.Info("harvest: prices recomputed",
			"horizon", string(hz.horizon),
			"items", len(prices.Items),
			"priced", priced,
			"families", len(tiers),
			"newestHour", newest.UTC().Format(time.RFC3339),
		)
	}
	s.notify()
	return nil
}

// tierInput builds the per-family item prices the tiers are classified from.
// Each weighted type is priced at its chaos value, else its last-seen chaos
// value (D12). A type with neither is left out, so it has no tier slot yet
// its served chaos stays null.
func tierInput(p Prices) map[string]map[string]float64 {
	out := make(map[string]map[string]float64)
	for _, f := range Families() {
		if f.Weights == nil {
			continue
		}
		for _, t := range f.Weights.Types {
			ip, ok := p.Items[t.ItemID]
			if !ok {
				continue
			}
			price, ok := tierPrice(ip)
			if !ok {
				continue
			}
			if out[f.ID] == nil {
				out[f.ID] = make(map[string]float64)
			}
			out[f.ID][t.ItemID] = price
		}
	}
	return out
}

// tierPrice is the chaos value a type is tiered at: its current chaos VWAP, or
// its last-seen chaos VWAP when the window has none.
func tierPrice(ip ItemPrice) (float64, bool) {
	if ip.Chaos != nil {
		return *ip.Chaos, true
	}
	if ip.LastSeenChaos != nil {
		return *ip.LastSeenChaos, true
	}
	return 0, false
}

// Trigger runs a recompute, coalescing concurrent requests.
//
// While a recompute is in flight, further triggers only set dirty and return
// immediately, and the running recompute repeats once for however many arrived.
// The rerun keeps the last event from being lost: the in-flight read may have
// started before its rows were committed.
//
// Errors are logged at Warn rather than returned, because callers are
// fire-and-forget. Call Recompute directly when the error matters.
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
		if err := s.Recompute(ctx); err != nil {
			s.logger.Warn("harvest: recompute failed", "error", err)
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

// HandleEvent recomputes in response to a collector event. The payload is
// ignored: a recompute reads storage, not the event, so a replayed event costs
// one coalesced recompute and changes no state.
//
// The caller is responsible for the league guard; this method assumes the event
// already belongs to the scope.
func (s *Service) HandleEvent(ctx context.Context, raw []byte) {
	_ = raw
	s.Trigger(ctx)
}

func cloneSnapshot(s Snapshot) Snapshot {
	s.Items = cloneItems(s.Items)
	s.Lifeforce = cloneLifeforce(s.Lifeforce)
	s.Tiers = cloneTiers(s.Tiers)
	return s
}

// cloneItems copies the map and each price's pointers. A nil map stays nil.
func cloneItems(in map[string]ItemPrice) map[string]ItemPrice {
	if in == nil {
		return nil
	}
	out := make(map[string]ItemPrice, len(in))
	for k, v := range in {
		out[k] = ItemPrice{
			Chaos:         cloneFloat(v.Chaos),
			Divine:        cloneFloat(v.Divine),
			LastSeenChaos: cloneFloat(v.LastSeenChaos),
		}
	}
	return out
}

func cloneLifeforce(in map[string]LifeforcePrice) map[string]LifeforcePrice {
	if in == nil {
		return nil
	}
	out := make(map[string]LifeforcePrice, len(in))
	for k, v := range in {
		out[k] = v
	}
	return out
}

// cloneTiers copies every family's slices and maps. A nil map or slice stays
// nil, so the copy has the same shape as the original.
func cloneTiers(in map[string]FamilyTiers) map[string]FamilyTiers {
	if in == nil {
		return nil
	}
	out := make(map[string]FamilyTiers, len(in))
	for k, v := range in {
		out[k] = FamilyTiers{
			BoundariesChaos:  cloneFloats(v.BoundariesChaos),
			ByItem:           cloneStrings(v.ByItem),
			Names:            cloneStringSlice(v.Names),
			TopBoundaryChaos: cloneFloat(v.TopBoundaryChaos),
		}
	}
	return out
}

func cloneFloat(p *float64) *float64 {
	if p == nil {
		return nil
	}
	v := *p
	return &v
}

func cloneFloats(in []float64) []float64 {
	if in == nil {
		return nil
	}
	out := make([]float64, len(in))
	copy(out, in)
	return out
}

func cloneStringSlice(in []string) []string {
	if in == nil {
		return nil
	}
	out := make([]string, len(in))
	copy(out, in)
	return out
}

func cloneStrings(in map[string]string) map[string]string {
	if in == nil {
		return nil
	}
	out := make(map[string]string, len(in))
	for k, v := range in {
		out[k] = v
	}
	return out
}
