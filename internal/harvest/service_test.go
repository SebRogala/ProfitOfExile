package harvest

import (
	"context"
	"errors"
	"io"
	"log/slog"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
)

const (
	lightningItem = "Metadata/Items/Currency/CurrencyDelveCraftingLightning"
	abyssItem     = "Metadata/Items/Currency/CurrencyDelveCraftingAbyss"
	physicalItem  = "Metadata/Items/Currency/CurrencyDelveCraftingPhysical"
	gemLevelItem  = "Metadata/Items/Currency/CurrencyDelveCraftingGemLevel"
	lifeItem      = "Metadata/Items/Currency/CurrencyDelveCraftingLife"
)

var testScope = league.Historical("Allflame")

var errBoom = errors.New("boom")

// fakeRows is an exchange.RowSource with fixed rows. It records every LoadRows
// window, and when gate is set the first LoadRows announces itself on entered
// and then blocks until gate is closed.
type fakeRows struct {
	mu      sync.Mutex
	newest  time.Time
	found   bool
	rows    []exchange.StoredRow
	loadErr error
	loads   []loadSpan
	gate    chan struct{}
	entered chan struct{}
}

type loadSpan struct {
	from, to time.Time
}

func newFake(rows ...exchange.StoredRow) *fakeRows {
	return &fakeRows{newest: newest, found: true, rows: rows}
}

func (f *fakeRows) NewestHour(context.Context, league.Scope) (time.Time, bool, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.newest, f.found, nil
}

func (f *fakeRows) LoadRows(_ context.Context, _ league.Scope, from, to time.Time) ([]exchange.StoredRow, error) {
	f.mu.Lock()
	first := len(f.loads) == 0
	f.loads = append(f.loads, loadSpan{from: from, to: to})
	gate, entered := f.gate, f.entered
	f.mu.Unlock()

	if first && gate != nil {
		entered <- struct{}{}
		<-gate
	}

	f.mu.Lock()
	defer f.mu.Unlock()
	if f.loadErr != nil {
		return nil, f.loadErr
	}
	return f.rows, nil
}

func (f *fakeRows) setNewest(h time.Time) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.newest = h
}

func (f *fakeRows) loadCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return len(f.loads)
}

func newTestService(f *fakeRows) (*Service, *Cache) {
	cache := NewCache()
	discard := slog.New(slog.NewTextHandler(io.Discard, nil))
	return NewService(f, testScope, cache, discard), cache
}

func mustWarm(t *testing.T, cache *Cache, h exchange.Horizon) Snapshot {
	t.Helper()
	s, ok := cache.Snapshot(h)
	if !ok {
		t.Fatalf("%s snapshot is cold, want warm", h)
	}
	return s
}

func TestRecompute_coldWhenNoRows(t *testing.T) {
	f := &fakeRows{newest: newest, found: false}
	svc, cache := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v, want nil for a league with no rows", err)
	}
	if _, ok := cache.Snapshot(exchange.HorizonRecent); ok {
		t.Fatal("recent snapshot is warm after a recompute that found no rows")
	}
	if _, ok := cache.Snapshot(exchange.HorizonDay); ok {
		t.Fatal("day snapshot is warm after a recompute that found no rows")
	}
	if n := f.loadCount(); n != 0 {
		t.Fatalf("LoadRows called %d times, want 0 when there is no newest hour", n)
	}
}

func TestRecompute_errorLeavesCacheUntouched(t *testing.T) {
	f := newFake()
	f.loadErr = errBoom
	svc, cache := newTestService(f)

	prior := Snapshot{League: "prior", Horizon: exchange.HorizonRecent, PriceHour: hoursBefore(1)}
	cache.Set(exchange.HorizonRecent, prior)

	err := svc.Recompute(context.Background())
	if !errors.Is(err, errBoom) {
		t.Fatalf("Recompute() error = %v, want it to wrap the load error", err)
	}
	got := mustWarm(t, cache, exchange.HorizonRecent)
	if got.League != prior.League || !got.PriceHour.Equal(prior.PriceHour) {
		t.Fatalf("recent snapshot = {League %q, PriceHour %v}, want the prior {%q, %v} after a failed recompute",
			got.League, got.PriceHour, prior.League, prior.PriceHour)
	}
	if _, ok := cache.Snapshot(exchange.HorizonDay); ok {
		t.Fatal("day snapshot warmed by a failed recompute")
	}
}

func TestRecompute_storesBothHorizonsAtTheNewestHour(t *testing.T) {
	f := newFake(
		row(hoursBefore(20), lightningItem, exchange.ChaosID, 1, 100),
		row(newest, lightningItem, exchange.ChaosID, 3, 30),
	)
	svc, cache := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v", err)
	}

	// The recent window (6 hours) holds only the newest row: 30 / 3.
	recent := mustWarm(t, cache, exchange.HorizonRecent)
	assertPrice(t, "recent lightning chaos", recent.Items[lightningItem].Chaos, 10)

	// The day window (24 hours) also holds the 20-hours-old row: (100 + 30) / (1 + 3).
	day := mustWarm(t, cache, exchange.HorizonDay)
	assertPrice(t, "day lightning chaos", day.Items[lightningItem].Chaos, 32.5)

	for name, s := range map[string]Snapshot{"recent": recent, "day": day} {
		if !s.PriceHour.Equal(newest) {
			t.Fatalf("%s PriceHour = %v, want the newest hour %v", name, s.PriceHour, newest)
		}
		if s.League != testScope.ID() {
			t.Fatalf("%s League = %q, want %q", name, s.League, testScope.ID())
		}
	}
}

func TestRecompute_tiersOnlyPricedTypes(t *testing.T) {
	f := newFake(
		row(newest, lightningItem, exchange.ChaosID, 2, 20),
		row(newest, abyssItem, exchange.ChaosID, 1, 50),
	)
	svc, cache := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v", err)
	}

	fossil, ok := mustWarm(t, cache, exchange.HorizonRecent).Tiers["fossil"]
	if !ok {
		t.Fatal("fossil family has no tiers, want them from the two priced types")
	}
	if len(fossil.ByItem) != 2 {
		t.Fatalf("fossil ByItem has %d keys %v, want exactly the two priced types", len(fossil.ByItem), fossil.ByItem)
	}
	for _, id := range []string{lightningItem, abyssItem} {
		if _, ok := fossil.ByItem[id]; !ok {
			t.Fatalf("fossil ByItem missing priced type %s", id)
		}
	}
	if _, ok := fossil.ByItem[physicalItem]; ok {
		t.Fatal("fossil ByItem tiers an unpriced type, want it left out")
	}
}

func TestRecompute_unpricedTypeKeepsItsTierFromLastSeen(t *testing.T) {
	f := newFake(
		row(hoursBefore(30), abyssItem, exchange.ChaosID, 1, 400),
		row(newest, lightningItem, exchange.ChaosID, 1, 9),
	)
	svc, cache := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v", err)
	}

	recent := mustWarm(t, cache, exchange.HorizonRecent)
	abyss := recent.Items[abyssItem]
	if abyss.Chaos != nil {
		t.Fatalf("abyss Chaos = %v, want nil: no trade inside the recent window", *abyss.Chaos)
	}
	assertPrice(t, "abyss LastSeenChaos", abyss.LastSeenChaos, 400)

	fossil := recent.Tiers["fossil"]
	if _, ok := fossil.ByItem[abyssItem]; !ok {
		t.Fatal("abyss is untiered, want it tiered at its last-seen chaos value")
	}
}

func TestRecompute_loadsSeventyTwoHours(t *testing.T) {
	f := newFake(row(newest, lightningItem, exchange.ChaosID, 1, 9))
	svc, _ := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v", err)
	}

	f.mu.Lock()
	defer f.mu.Unlock()
	if len(f.loads) != 1 {
		t.Fatalf("LoadRows called %d times, want exactly 1", len(f.loads))
	}
	got := f.loads[0]
	if want := newest.Add(-71 * time.Hour); !got.from.Equal(want) {
		t.Fatalf("LoadRows from = %v, want %v (newest - 71h)", got.from, want)
	}
	if want := newest.Add(time.Hour); !got.to.Equal(want) {
		t.Fatalf("LoadRows to = %v, want %v (newest + 1h, exclusive)", got.to, want)
	}
}

func TestTrigger_rerunsAfterAnEventDuringLoad(t *testing.T) {
	gate := make(chan struct{})
	entered := make(chan struct{}, 1)
	f := newFake(row(newest, lightningItem, exchange.ChaosID, 1, 9))
	f.gate = gate
	f.entered = entered
	svc, cache := newTestService(f)

	var releaseOnce sync.Once
	release := func() { releaseOnce.Do(func() { close(gate) }) }
	t.Cleanup(release)

	ctx := context.Background()
	done := make(chan struct{})
	go func() {
		svc.Trigger(ctx)
		close(done)
	}()

	// The first pass is now inside LoadRows, so running is set.
	select {
	case <-entered:
	case <-time.After(2 * time.Second):
		t.Fatal("first Trigger never reached LoadRows")
	}

	// A second trigger during the load only marks the service dirty and returns.
	secondDone := make(chan struct{})
	go func() {
		svc.Trigger(ctx)
		close(secondDone)
	}()
	select {
	case <-secondDone:
	case <-time.After(2 * time.Second):
		t.Fatal("second Trigger blocked: coalescing regressed")
	}

	// The feed advances while the first pass is still reading its old rows.
	next := newest.Add(time.Hour)
	f.setNewest(next)
	release()

	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("first Trigger did not return after its load was released")
	}

	got := mustWarm(t, cache, exchange.HorizonRecent)
	if !got.PriceHour.Equal(next) {
		t.Fatalf("recent PriceHour = %v, want %v: the event during the load was not rerun", got.PriceHour, next)
	}
	if n := f.loadCount(); n != 2 {
		t.Fatalf("LoadRows called %d times, want 2 (the first pass and one coalesced rerun)", n)
	}
}

func TestRecompute_tiersClassifyTheServedPrices(t *testing.T) {
	f := newFake(
		row(hoursBefore(30), abyssItem, exchange.ChaosID, 1, 400), // last-seen only
		row(newest, gemLevelItem, exchange.ChaosID, 1, 190),
		row(newest, physicalItem, exchange.ChaosID, 1, 44),
		row(newest, lightningItem, exchange.ChaosID, 1, 9),
		row(newest, lifeItem, exchange.ChaosID, 1, 15),
	)
	svc, cache := newTestService(f)

	if err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute() error = %v", err)
	}

	fossil := mustWarm(t, cache, exchange.HorizonRecent).Tiers["fossil"]
	// Tiers come from the engine on these prices: the 400c last-seen Abyss is the
	// top price, so the first boundary snaps to it; 190c GemLevel falls under
	// that boundary; 9c Lightning falls under the FLOOR (about 8% of the top-5
	// average, here 10.53c).
	if got := fossil.ByItem[abyssItem]; got != "HIGH" {
		t.Fatalf("abyss (last-seen 400c) tier = %q, want HIGH", got)
	}
	if got := fossil.ByItem[gemLevelItem]; got != "LOW" {
		t.Fatalf("gem level (190c) tier = %q, want LOW", got)
	}
	if got := fossil.ByItem[lightningItem]; got != "FLOOR" {
		t.Fatalf("lightning (9c) tier = %q, want FLOOR", got)
	}
	if len(fossil.BoundariesChaos) == 0 {
		t.Fatal("fossil BoundariesChaos is empty, want the classified boundaries")
	}
	if got := fossil.BoundariesChaos[0]; got != 400 {
		t.Fatalf("fossil first boundary = %v, want 400 (the top price)", got)
	}
}

// populatedSnapshot returns a snapshot whose every nested pointer, slice and
// map is freshly allocated, so a test can mutate one copy without touching
// another.
func populatedSnapshot() Snapshot {
	chaos, divine, last := 10.0, 2.0, 400.0
	top := 500.0
	return Snapshot{
		League:    testScope.ID(),
		Horizon:   exchange.HorizonRecent,
		PriceHour: newest,
		Items: map[string]ItemPrice{lightningItem: {
			Chaos:         &chaos,
			Divine:        &divine,
			LastSeenChaos: &last,
		}},
		Lifeforce: map[string]LifeforcePrice{"Primal": {ItemID: LifeforcePrimal, Colour: "blue", Chaos: 1, PerDivine: 2}},
		Tiers: map[string]FamilyTiers{"fossil": {
			BoundariesChaos:  []float64{5, 1},
			ByItem:           map[string]string{lightningItem: "T1"},
			Names:            []string{"HIGH", "LOW"},
			TopBoundaryChaos: &top,
		}},
	}
}

// mutateNested changes every nested value reachable from s.
func mutateNested(s Snapshot) {
	ip := s.Items[lightningItem]
	*ip.Chaos = 999
	*ip.Divine = 999
	*ip.LastSeenChaos = 999
	fossil := s.Tiers["fossil"]
	fossil.ByItem[lightningItem] = "T9"
	fossil.BoundariesChaos[0] = 999
	fossil.Names[0] = "X"
	*fossil.TopBoundaryChaos = 999
}

// assertPopulatedOriginals fails unless s still holds populatedSnapshot's values.
func assertPopulatedOriginals(t *testing.T, s Snapshot) {
	t.Helper()
	ip := s.Items[lightningItem]
	assertPrice(t, "chaos", ip.Chaos, 10)
	assertPrice(t, "divine", ip.Divine, 2)
	assertPrice(t, "lastSeenChaos", ip.LastSeenChaos, 400)
	fossil := s.Tiers["fossil"]
	if got := fossil.ByItem[lightningItem]; got != "T1" {
		t.Fatalf("ByItem[lightning] = %q, want T1", got)
	}
	if got := fossil.BoundariesChaos[0]; got != 5 {
		t.Fatalf("BoundariesChaos[0] = %v, want 5", got)
	}
	if got := fossil.Names[0]; got != "HIGH" {
		t.Fatalf("Names[0] = %q, want HIGH", got)
	}
	if fossil.TopBoundaryChaos == nil || *fossil.TopBoundaryChaos != 500 {
		t.Fatalf("TopBoundaryChaos = %v, want 500", fossil.TopBoundaryChaos)
	}
}

func TestCacheSnapshot_isolatesNestedValues(t *testing.T) {
	cache := NewCache()
	cache.Set(exchange.HorizonRecent, populatedSnapshot())

	mutateNested(mustWarm(t, cache, exchange.HorizonRecent))

	assertPopulatedOriginals(t, mustWarm(t, cache, exchange.HorizonRecent))
}

func TestCacheSet_ownsItsInput(t *testing.T) {
	cache := NewCache()
	in := populatedSnapshot()
	cache.Set(exchange.HorizonRecent, in)

	mutateNested(in)

	assertPopulatedOriginals(t, mustWarm(t, cache, exchange.HorizonRecent))
}

func TestCacheSnapshot_returnsCopies(t *testing.T) {
	cache := NewCache()
	price := 10.0
	cache.Set(exchange.HorizonRecent, Snapshot{
		League:    testScope.ID(),
		Horizon:   exchange.HorizonRecent,
		PriceHour: newest,
		Items:     map[string]ItemPrice{lightningItem: {Chaos: &price}},
		Lifeforce: map[string]LifeforcePrice{"Primal": {ItemID: LifeforcePrimal, Colour: "blue", Chaos: 1, PerDivine: 2}},
		Tiers: map[string]FamilyTiers{"fossil": {
			BoundariesChaos: []float64{5},
			ByItem:          map[string]string{lightningItem: "T1"},
		}},
	})

	// Mutating the value the caller passed to Set must not reach the cache either.
	price = 77

	got := mustWarm(t, cache, exchange.HorizonRecent)
	*got.Items[lightningItem].Chaos = 999
	delete(got.Items, lightningItem)
	got.Items["added"] = ItemPrice{}
	delete(got.Lifeforce, "Primal")
	delete(got.Tiers, "fossil")

	again := mustWarm(t, cache, exchange.HorizonRecent)
	assertPrice(t, "lightning chaos after caller mutation", again.Items[lightningItem].Chaos, 10)
	if _, ok := again.Items["added"]; ok {
		t.Fatal("a key added to the returned Items appeared in the next Snapshot")
	}
	if _, ok := again.Lifeforce["Primal"]; !ok {
		t.Fatal("a Lifeforce key deleted from the returned map is gone from the next Snapshot")
	}
	fossil, ok := again.Tiers["fossil"]
	if !ok {
		t.Fatal("a Tiers key deleted from the returned map is gone from the next Snapshot")
	}
	if fossil.ByItem[lightningItem] != "T1" || fossil.BoundariesChaos[0] != 5 {
		t.Fatalf("tiers changed through the returned copy: ByItem %v, BoundariesChaos %v",
			fossil.ByItem, fossil.BoundariesChaos)
	}
}

func TestCache_nilReadsCold(t *testing.T) {
	var cache *Cache
	cache.Set(exchange.HorizonRecent, Snapshot{League: testScope.ID()})

	if _, ok := cache.Snapshot(exchange.HorizonRecent); ok {
		t.Fatal("nil cache reported a warm snapshot")
	}
}
