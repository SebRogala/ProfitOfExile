package temple

import (
	"bytes"
	"context"
	"errors"
	"io"
	"log/slog"
	"strings"
	"sync"
	"testing"

	"profitofexile/internal/league"
)

var testScope = league.Historical("Allflame")

func quietLogger() *slog.Logger {
	return slog.New(slog.NewTextHandler(io.Discard, nil))
}

// fakeSource records what each recompute asked for so the two-phase read can be
// asserted without a database.
type fakeSource struct {
	mu sync.Mutex

	newest    []Line
	newestErr error

	window    map[string][]WindowPrice
	windowErr error

	calls      int
	windowedBy [][]Line

	// beforeNewest runs at the top of NewestLines, which is what lets a test
	// hold one recompute open while it triggers more.
	beforeNewest func()
}

func (f *fakeSource) NewestLines(ctx context.Context, scope league.Scope) ([]Line, error) {
	if f.beforeNewest != nil {
		f.beforeNewest()
	}
	f.mu.Lock()
	f.calls++
	f.mu.Unlock()
	return f.newest, f.newestErr
}

func (f *fakeSource) WindowPrices(ctx context.Context, scope league.Scope, lines []Line) (map[string][]WindowPrice, error) {
	f.mu.Lock()
	f.windowedBy = append(f.windowedBy, lines)
	f.mu.Unlock()
	return f.window, f.windowErr
}

func (f *fakeSource) callCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.calls
}

func TestServiceRecompute_storesTheBuiltMarketInTheCache(t *testing.T) {
	source := &fakeSource{newest: []Line{
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
		room("Locus of Corruption (Tier 3)", 853.6, 2489),
	}}
	cache := NewCache(testScope)
	svc := NewService(source, cache, testScope, quietLogger())

	if _, err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	got, warm := cache.Snapshot()
	if !warm {
		t.Fatalf("cache still COLD after a successful recompute")
	}
	if got.Floor != 10 || len(got.Rooms) != 3 {
		t.Errorf("cached market = floor %v, %d rooms; want floor 10 and 3 rooms", got.Floor, len(got.Rooms))
	}
}

func TestServiceRecompute_asksForAWindowOnlyForTheThinLines(t *testing.T) {
	// The window read is the second query, and its cost is the number of names it
	// carries. Handing it every served line would query 31 names on every tick.
	source := &fakeSource{newest: []Line{
		item("Vial", "Vial of the Ghost", 1689, 3),
		item("Vial", "Vial of Fate", 1, 10),
		item("Vial", "Vial of Dominance", 1, 10),
		item("Vial", "Vial of Awakening", 9, 100),
	}}
	svc := NewService(source, NewCache(testScope), testScope, quietLogger())

	if _, err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if len(source.windowedBy) != 1 {
		t.Fatalf("WindowPrices called %d times, want once", len(source.windowedBy))
	}
	asked := source.windowedBy[0]
	if len(asked) != 1 || asked[0].Name != "Vial of the Ghost" {
		t.Errorf("window read asked for %+v, want only the thin Vial of the Ghost line", asked)
	}
}

func TestServiceRecompute_failedNewestReadLeavesTheCacheCold(t *testing.T) {
	// Storing a partial or empty market on a failed read would mark the cache
	// warm, and every later request would serve that empty answer with no
	// fallback until the next tick.
	source := &fakeSource{newestErr: errors.New("database is down")}
	cache := NewCache(testScope)
	svc := NewService(source, cache, testScope, quietLogger())

	if _, err := svc.Recompute(context.Background()); err == nil {
		t.Fatalf("Recompute returned no error on a failed read")
	}
	if _, warm := cache.Snapshot(); warm {
		t.Errorf("cache reported warm after a failed recompute")
	}
}

func TestServiceRecompute_failedWindowReadLeavesTheCacheCold(t *testing.T) {
	source := &fakeSource{
		newest:    []Line{item("Vial", "Vial of the Ghost", 1689, 3), item("Vial", "Vial of Fate", 1, 10), item("Vial", "Vial of Dominance", 1, 10)},
		windowErr: errors.New("statement timeout"),
	}
	cache := NewCache(testScope)
	svc := NewService(source, cache, testScope, quietLogger())

	if _, err := svc.Recompute(context.Background()); err == nil {
		t.Fatalf("Recompute returned no error when the window read failed")
	}
	if _, warm := cache.Snapshot(); warm {
		t.Errorf("cache reported warm after a failed window read; thin items would be served at their newest print with windowPriced false and nothing to say so")
	}
}

func TestServiceRecompute_storesAnEmptyMarketSoTheCacheStopsReadingCold(t *testing.T) {
	// The writer-side half of the cache-state contract: a league whose seven
	// feeds have stored nothing still gets an answer, or every request takes the
	// cold path forever.
	cache := NewCache(testScope)
	svc := NewService(&fakeSource{}, cache, testScope, quietLogger())

	if _, err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	got, warm := cache.Snapshot()
	if !warm {
		t.Fatalf("cache reported COLD after a recompute whose honest answer was empty")
	}
	if got.AsOf != nil || len(got.Rooms) != 0 {
		t.Errorf("stored market = asOf %v, %d rooms; want an empty answer", got.AsOf, len(got.Rooms))
	}
}

func TestServiceTrigger_coalescesABurstIntoOneRerun(t *testing.T) {
	// The seven item feeds publish within seconds of each other. A caller that
	// finds a recompute running marks it dirty and returns; the running pass
	// repeats ONCE. Six concurrent triggers must therefore cost two recomputes,
	// not six — and not one, because the last event's rows may have committed
	// after the in-flight read started.
	release := make(chan struct{})
	entered := make(chan struct{}, 1)
	var once sync.Once

	source := &fakeSource{newest: []Line{item("Vial", "Vial of Fate", 1, 343)}}
	source.beforeNewest = func() {
		once.Do(func() {
			entered <- struct{}{}
			<-release
		})
	}
	svc := NewService(source, NewCache(testScope), testScope, quietLogger())

	done := make(chan struct{})
	go func() {
		svc.Trigger(context.Background())
		close(done)
	}()

	<-entered // the first recompute is in flight and parked

	var wg sync.WaitGroup
	for i := 0; i < 6; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			svc.Trigger(context.Background())
		}()
	}
	wg.Wait()

	close(release)
	<-done

	if got := source.callCount(); got != 2 {
		t.Errorf("recomputes = %d, want 2 (the in-flight pass plus one rerun for the whole burst)", got)
	}
}

func TestServiceTrigger_recomputesOnceWhenNothingIsInFlight(t *testing.T) {
	source := &fakeSource{newest: []Line{item("Vial", "Vial of Fate", 1, 343)}}
	svc := NewService(source, NewCache(testScope), testScope, quietLogger())

	svc.Trigger(context.Background())
	svc.Trigger(context.Background())

	if got := source.callCount(); got != 2 {
		t.Errorf("recomputes = %d, want 2 — two sequential triggers are two recomputes", got)
	}
}

func TestServiceHandleEvent_recomputesRegardlessOfWhatThePayloadReports(t *testing.T) {
	// A replayed tick publishes inserted: 0 while the snapshot is fully
	// populated, so a content check would skip exactly the recompute a catch-up
	// needs.
	source := &fakeSource{newest: []Line{item("Vial", "Vial of Fate", 1, 343)}}
	svc := NewService(source, NewCache(testScope), testScope, quietLogger())

	svc.HandleEvent(context.Background(), []byte(`{"endpoint":"ninja-vial","inserted":0}`))

	if got := source.callCount(); got != 1 {
		t.Errorf("recomputes = %d, want 1", got)
	}
}

// untieredLogLine is the substring the untiered-rooms report is counted by. The
// point of that log is to surface a name nothing can parse, so the test reads
// what was written rather than how often the reporter ran.
const untieredLogLine = "room lines with no tier suffix"

func TestServiceRecompute_reportsAnUntieredRoomSetOnceUntilItChanges(t *testing.T) {
	// The boot set — Apex of Atzoatl and the ten connectors — is stable, so it is
	// reported once and then stays quiet. A guard that fires once for the process
	// instead would spend itself on that boot set and never report the name that
	// is actually new.
	var log bytes.Buffer
	source := &fakeSource{newest: []Line{
		room("Apex of Atzoatl", 10, 700),
		room("A (Tier 1)", 10, 700),
	}}
	svc := NewService(source, NewCache(testScope), testScope, slog.New(slog.NewTextHandler(&log, nil)))

	for i := 0; i < 2; i++ {
		if _, err := svc.Recompute(context.Background()); err != nil {
			t.Fatalf("Recompute %d: %v", i, err)
		}
	}
	if got := strings.Count(log.String(), untieredLogLine); got != 1 {
		t.Fatalf("reports = %d after two recomputes of the same untiered set, want 1", got)
	}

	source.newest = append(source.newest, room("Chamber of Nothing In Particular", 10, 700))
	if _, err := svc.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute after the set changed: %v", err)
	}

	if got := strings.Count(log.String(), untieredLogLine); got != 2 {
		t.Errorf("reports = %d after a NEW unparseable name appeared, want 2 — the name nothing can parse is the whole reason to log", got)
	}
	if !strings.Contains(log.String(), "Chamber of Nothing In Particular") {
		t.Errorf("the report does not name the new room; log = %s", log.String())
	}
}

func TestCacheSnapshot_readsColdBeforeAnyRecompute(t *testing.T) {
	if _, warm := NewCache(testScope).Snapshot(); warm {
		t.Errorf("a fresh cache reported warm; the handler would serve a zero Market as authoritative")
	}
}

func TestCacheSnapshot_readsWarmForAStoredEmptyMarket(t *testing.T) {
	// WARM-AND-EMPTY is not COLD. Deriving warmth from the stored value's length
	// is the defect the cache-state contract exists for.
	cache := NewCache(testScope)
	cache.Set(EmptyMarket("Allflame"))

	if _, warm := cache.Snapshot(); !warm {
		t.Errorf("cache reported COLD after storing an empty market")
	}
}

func TestCacheSnapshot_nilCacheReadsCold(t *testing.T) {
	var cache *Cache
	if _, warm := cache.Snapshot(); warm {
		t.Errorf("a nil cache reported warm")
	}
}

func TestCacheSnapshot_returnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	cache := NewCache(testScope)
	cache.Set(Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		item("Vial", "Vial of Fate", 1, 343),
	}, nil))

	first, _ := cache.Snapshot()
	first.Rooms[0].Chaos = 99999
	first.CategoriesSeen[0] = "mutated"
	for i := range first.Items {
		if first.Items[i].Price != nil {
			first.Items[i].Price.Chaos = 99999
		}
	}

	second, _ := cache.Snapshot()
	if second.Rooms[0].Chaos != 10 {
		t.Errorf("room chaos = %v after a caller wrote to an earlier snapshot, want 10", second.Rooms[0].Chaos)
	}
	if second.CategoriesSeen[0] != RoomCategory {
		t.Errorf("categoriesSeen[0] = %q after a caller wrote to an earlier snapshot", second.CategoriesSeen[0])
	}
	for _, i := range second.Items {
		if i.Price != nil && i.Price.Chaos == 99999 {
			t.Errorf("%s price was rewritten through an earlier snapshot's Price pointer", i.Name)
		}
	}
}

func TestCacheFor_panicsUnderAnotherLeague(t *testing.T) {
	// One cache serves one league for the process's lifetime. Returning the
	// receiver anyway would serve Allflame's prices under another league's scope
	// with nothing anywhere to show for it.
	defer func() {
		if recover() == nil {
			t.Errorf("For did not panic on a mismatched league")
		}
	}()
	NewCache(testScope).For(league.Historical("Mirage"))
}

func TestCacheFor_returnsTheReceiverUnderItsOwnLeague(t *testing.T) {
	cache := NewCache(testScope)
	if cache.For(testScope) != cache {
		t.Errorf("For returned a different cache under its own league")
	}
}
