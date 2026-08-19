package exchange

import (
	"context"
	"errors"
	"log/slog"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// serviceScope is the league every service test recomputes for. Historical needs
// no database and its id is what a Result must carry.
var serviceScope = league.Historical("Allflame")

var (
	errNewestHour = errors.New("newest hour: connection reset by peer")
	errLoadRows   = errors.New("load rows: statement timeout")
)

// loadCall records one LoadRows invocation: the window arithmetic is the whole
// point of the recompute's read side, so the bounds are kept verbatim.
type loadCall struct {
	scope league.Scope
	from  time.Time
	to    time.Time
}

// newestHourCall records one NewestHour invocation. Only the scope is worth
// keeping: the probe takes no other argument, and asking the wrong league for
// the anchor hour would silently window the right league's rows around another
// league's clock.
type newestHourCall struct {
	scope league.Scope
}

// fakeRows is a RowSource that answers from fixed values and records what it was
// asked for. started/release are the choreography hooks the coalescing test uses;
// they are nil in every other test, which makes NewestHour return immediately.
type fakeRows struct {
	mu sync.Mutex

	newest    time.Time
	found     bool
	newestErr error

	rows    []StoredRow
	loadErr error

	newestHours []newestHourCall
	loads       []loadCall

	// started receives one value per NewestHour call and is buffered, so the
	// fake never blocks on a test that is not reading it.
	started chan struct{}
	// release blocks NewestHour until the test closes it.
	release chan struct{}
}

func (f *fakeRows) NewestHour(_ context.Context, scope league.Scope) (time.Time, bool, error) {
	f.mu.Lock()
	f.newestHours = append(f.newestHours, newestHourCall{scope: scope})
	f.mu.Unlock()

	if f.started != nil {
		f.started <- struct{}{}
	}
	if f.release != nil {
		<-f.release
	}
	return f.newest, f.found, f.newestErr
}

func (f *fakeRows) LoadRows(_ context.Context, scope league.Scope, from, to time.Time) ([]StoredRow, error) {
	f.mu.Lock()
	f.loads = append(f.loads, loadCall{scope: scope, from: from, to: to})
	f.mu.Unlock()

	if f.loadErr != nil {
		return nil, f.loadErr
	}
	return f.rows, nil
}

func (f *fakeRows) newestHourCalls() []newestHourCall {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]newestHourCall(nil), f.newestHours...)
}

func (f *fakeRows) loadCalls() []loadCall {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]loadCall(nil), f.loads...)
}

// emptyRows is a source for a league that has never stored an hour.
func emptyRows() *fakeRows {
	return &fakeRows{}
}

// twoHourRows is a source whose newest hour is feedHour and whose rows are the
// same chaos-quoted market in feedHour and the hour before it — a window that
// clears every default gate and produces exactly one direct play.
func twoHourRows() *fakeRows {
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()),
		storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 120).row())...,
	)
	return &fakeRows{newest: feedHour, found: true, rows: rows}
}

// eightHourRows carries the same market in the eight hours ending at feedHour —
// more than the recent horizon's window and fewer than the day horizon's, so the
// two horizons cannot produce the same answer.
func eightHourRows() *fakeRows {
	var rows []StoredRow
	for i := 0; i < 8; i++ {
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), liquidChaosMarket(cardID, 100, 120).row())...)
	}
	return &fakeRows{newest: feedHour, found: true, rows: rows}
}

// notifyCounter counts the service's notify callbacks. The recompute runs on the
// caller's goroutine in most tests and on another one in the coalescing test, so
// the counter is atomic either way.
type notifyCounter struct{ n atomic.Int64 }

func (c *notifyCounter) signal() { c.n.Add(1) }

func (c *notifyCounter) count() int { return int(c.n.Load()) }

// newTestService wires a service over the fake with a captured logger, so a test
// that does not care about logging still keeps the output out of the run.
func newTestService(t *testing.T, rows RowSource, cfg Config) (*Service, *Cache, *notifyCounter) {
	t.Helper()
	cache := NewCache()
	notify := &notifyCounter{}
	logger := slog.New(&logCapture{})
	return NewService(rows, serviceScope, cfg, cache, notify.signal, logger), cache, notify
}

func TestRecompute_leagueWithNoStoredHour_cachesAWarmEmptyResult(t *testing.T) {
	rows := emptyRows()
	service, cache, _ := newTestService(t, rows, Config{})

	got, err := service.Recompute(context.Background())
	if err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if got.League != serviceScope.ID() {
		t.Errorf("League = %q, want %q", got.League, serviceScope.ID())
	}
	if got.Hours != 0 {
		t.Errorf("Hours = %d, want 0", got.Hours)
	}
	if got.Plays == nil {
		t.Error("Plays = nil, want an allocated empty slice — the handler renders it as []")
	}
	if len(got.Plays) != 0 {
		t.Errorf("got %d plays, want 0", len(got.Plays))
	}

	cached, warm := cache.Snapshot(DefaultHorizon)
	if !warm {
		t.Error("cache warm = false, want true — an honest empty answer is still an answer")
	}
	if !reflect.DeepEqual(cached, got) {
		t.Errorf("cached = %+v, want the returned result %+v", cached, got)
	}
}

func TestRecompute_leagueWithNoStoredHour_doesNotReadRows(t *testing.T) {
	// There is no window to read: asking for one would scan the hypertable for
	// an answer the newest-hour probe already gave.
	rows := emptyRows()
	service, _, _ := newTestService(t, rows, Config{})

	if _, err := service.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if calls := rows.loadCalls(); len(calls) != 0 {
		t.Errorf("LoadRows called %d times with %+v, want 0", len(calls), calls)
	}
}

func TestRecompute_leagueWithNoStoredHour_signalsNotifyOnce(t *testing.T) {
	rows := emptyRows()
	service, _, notify := newTestService(t, rows, Config{})

	if _, err := service.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if got := notify.count(); got != 1 {
		t.Errorf("notify called %d times, want 1", got)
	}
}

func TestRecompute_readsOneWindowSizedToTheWidestHorizon(t *testing.T) {
	// [newest − widest·1h, newest + 1h): anchored on real rows, not on the clock,
	// and half-open so the newest hour is in and the next one is out. One read
	// serves every horizon, because BestPlays keeps only the newest WindowHours
	// distinct hours of whatever it is handed.
	tests := []struct {
		name     string
		cfg      Config
		wantFrom time.Time
	}{
		{
			name:     "the served pair reaches back as far as the day horizon",
			cfg:      Config{},
			wantFrom: feedHour.Add(-24 * time.Hour),
		},
		{
			name: "configured horizons decide the span",
			cfg: Config{Horizons: []HorizonConfig{
				{Horizon: HorizonRecent, WindowHours: 2, MinHoursSeen: 1},
				{Horizon: HorizonDay, WindowHours: 5, MinHoursSeen: 1},
			}},
			wantFrom: feedHour.Add(-5 * time.Hour),
		},
		{
			name: "the widest is a maximum, not the last one listed",
			cfg: Config{Horizons: []HorizonConfig{
				{Horizon: HorizonRecent, WindowHours: 9, MinHoursSeen: 1},
				{Horizon: HorizonDay, WindowHours: 3, MinHoursSeen: 1},
			}},
			wantFrom: feedHour.Add(-9 * time.Hour),
		},
		{
			name:     "the base window is not a horizon, however wide",
			cfg:      Config{WindowHours: 48},
			wantFrom: feedHour.Add(-24 * time.Hour),
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			rows := twoHourRows()
			service, _, _ := newTestService(t, rows, tc.cfg)

			if _, err := service.Recompute(context.Background()); err != nil {
				t.Fatalf("Recompute: %v", err)
			}

			calls := rows.loadCalls()
			if len(calls) != 1 {
				t.Fatalf("LoadRows called %d times, want 1 — every horizon is ranked from the same read", len(calls))
			}
			if !calls[0].from.Equal(tc.wantFrom) {
				t.Errorf("from = %s, want %s", calls[0].from, tc.wantFrom)
			}
			if want := feedHour.Add(time.Hour); !calls[0].to.Equal(want) {
				t.Errorf("to = %s, want %s", calls[0].to, want)
			}
			if calls[0].scope.ID() != serviceScope.ID() {
				t.Errorf("scope = %q, want %q", calls[0].scope.ID(), serviceScope.ID())
			}
		})
	}
}

func TestRecompute_ranksTheSameRowsOncePerHorizon(t *testing.T) {
	// Eight stored hours: the recent horizon reads the newest six of them and
	// the day horizon all eight, from ONE read. Two horizons that answered with
	// the same window would make the toggle a decoration.
	service, cache, _ := newTestService(t, eightHourRows(), Config{})

	if _, err := service.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	tests := []struct {
		horizon   Horizon
		wantHours int
	}{
		{horizon: HorizonRecent, wantHours: 6},
		{horizon: HorizonDay, wantHours: 8},
	}
	for _, tc := range tests {
		t.Run(string(tc.horizon), func(t *testing.T) {
			cached, warm := cache.Snapshot(tc.horizon)
			if !warm {
				t.Fatalf("%s cache warm = false, want true", tc.horizon)
			}
			if cached.Hours != tc.wantHours {
				t.Errorf("Hours = %d, want %d", cached.Hours, tc.wantHours)
			}
			if cached.Horizon != string(tc.horizon) {
				t.Errorf("Horizon = %q, want %q — a client must not mistake one window's body for the other's",
					cached.Horizon, tc.horizon)
			}
			if len(cached.Plays) != 1 {
				t.Fatalf("got %d plays, want 1 (%v)", len(cached.Plays), playKeys(cached.Plays))
			}
			if got := cached.Plays[0].HoursSeen; got != tc.wantHours {
				t.Errorf("HoursSeen = %d, want %d", got, tc.wantHours)
			}
		})
	}
}

func TestRecompute_returnsTheFirstConfiguredHorizonsResult(t *testing.T) {
	// The endpoint answers a request that names no horizon from the first one,
	// so the value Recompute hands back — the one cmd/server logs and publishes
	// counts from — has to be that same answer rather than the last computed.
	tests := []struct {
		name        string
		cfg         Config
		wantHorizon Horizon
		wantHours   int
	}{
		{
			name:        "the served pair leads with recent",
			cfg:         Config{},
			wantHorizon: HorizonRecent,
			wantHours:   6,
		},
		{
			name: "a configuration that leads with day returns day",
			cfg: Config{Horizons: []HorizonConfig{
				{Horizon: HorizonDay, WindowHours: 24, MinHoursSeen: 18},
				{Horizon: HorizonRecent, WindowHours: 6, MinHoursSeen: 4},
			}},
			wantHorizon: HorizonDay,
			wantHours:   8,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			service, _, _ := newTestService(t, eightHourRows(), tc.cfg)

			got, err := service.Recompute(context.Background())
			if err != nil {
				t.Fatalf("Recompute: %v", err)
			}

			if got.Horizon != string(tc.wantHorizon) {
				t.Errorf("Horizon = %q, want %q", got.Horizon, tc.wantHorizon)
			}
			if got.Hours != tc.wantHours {
				t.Errorf("Hours = %d, want the %s window's %d", got.Hours, tc.wantHorizon, tc.wantHours)
			}
		})
	}
}

func TestHorizonConfig_overlaysOnlyTheWindowAndTheHoursSeen(t *testing.T) {
	// A horizon is a span and a persistence demand, nothing else: the gates, the
	// cut and the quote priority are the engine's and must survive the overlay,
	// or the two horizons would silently rank by different rules.
	base := DefaultConfig()
	base.MinEdge = 0.5
	base.MinTurnoverChaos = 250
	base.QuotePriority = []string{ChaosID}

	got := horizonConfig(base, HorizonConfig{Horizon: HorizonDay, WindowHours: 24, MinHoursSeen: 18})

	want := base
	want.WindowHours, want.MinHoursSeen = 24, 18
	if !reflect.DeepEqual(got, want) {
		t.Errorf("horizonConfig() = %+v, want %+v", got, want)
	}
}

func TestRecompute_probesTheNewestHourForTheServiceScope(t *testing.T) {
	// The probe picks the anchor the whole window hangs off. Asking another
	// league for it would window THIS league's rows around a foreign clock, and
	// the LoadRows scope assertion would not notice.
	rows := twoHourRows()
	service, _, _ := newTestService(t, rows, Config{})

	if _, err := service.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	calls := rows.newestHourCalls()
	if len(calls) != 1 {
		t.Fatalf("NewestHour called %d times, want 1", len(calls))
	}
	if calls[0].scope.ID() != serviceScope.ID() {
		t.Errorf("scope = %q, want %q", calls[0].scope.ID(), serviceScope.ID())
	}
}

func TestRecompute_storedHours_ranksThemIntoTheResult(t *testing.T) {
	service, _, _ := newTestService(t, twoHourRows(), Config{})

	got, err := service.Recompute(context.Background())
	if err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if got.Hours != 2 {
		t.Errorf("Hours = %d, want 2 — both stored hours carried the market", got.Hours)
	}
	if got.League != serviceScope.ID() {
		t.Errorf("League = %q, want %q", got.League, serviceScope.ID())
	}
	if len(got.Plays) != 1 {
		t.Fatalf("got %d plays, want 1 (%v)", len(got.Plays), playKeys(got.Plays))
	}
	if want := directKey(chaosID, cardID); got.Plays[0].Key != want {
		t.Errorf("play key = %q, want %q", got.Plays[0].Key, want)
	}
	if want := feedHour.Add(time.Hour); !got.To.Equal(want) {
		t.Errorf("To = %s, want %s", got.To, want)
	}
}

func TestRecompute_storedHours_cachesExactlyTheReturnedResult(t *testing.T) {
	service, cache, _ := newTestService(t, twoHourRows(), Config{})

	got, err := service.Recompute(context.Background())
	if err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	cached, warm := cache.Snapshot(DefaultHorizon)
	if !warm {
		t.Fatal("cache warm = false, want true")
	}
	if !reflect.DeepEqual(cached, got) {
		t.Errorf("cached = %+v, want the returned result %+v", cached, got)
	}
}

func TestRecompute_storedHours_signalsNotifyOnce(t *testing.T) {
	service, _, notify := newTestService(t, twoHourRows(), Config{})

	if _, err := service.Recompute(context.Background()); err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	if got := notify.count(); got != 1 {
		t.Errorf("notify called %d times, want 1", got)
	}
}

func TestRecompute_storageFails_returnsTheErrorAndLeavesTheCacheCold(t *testing.T) {
	// A failed read must not overwrite a good answer with an empty one, and must
	// not tell clients that anything changed.
	tests := []struct {
		name string
		rows *fakeRows
		want error
	}{
		{
			name: "newest hour probe fails",
			rows: &fakeRows{newestErr: errNewestHour},
			want: errNewestHour,
		},
		{
			name: "window read fails",
			rows: &fakeRows{newest: feedHour, found: true, loadErr: errLoadRows},
			want: errLoadRows,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			service, cache, notify := newTestService(t, tc.rows, Config{})

			got, err := service.Recompute(context.Background())

			if !errors.Is(err, tc.want) {
				t.Fatalf("error = %v, want %v", err, tc.want)
			}
			if got.League != "" || got.Hours != 0 || got.Plays != nil {
				t.Errorf("result = %+v, want the zero Result", got)
			}
			if _, warm := cache.Snapshot(DefaultHorizon); warm {
				t.Error("cache warm = true, want false — a failed read must not warm the cache")
			}
			if n := notify.count(); n != 0 {
				t.Errorf("notify called %d times, want 0", n)
			}
		})
	}
}

func TestRecompute_unscopedScope_isRejectedBeforeReadingStorage(t *testing.T) {
	rows := emptyRows()
	cache := NewCache()
	notify := &notifyCounter{}
	service := NewService(rows, league.Scope{}, Config{}, cache, notify.signal, slog.New(&logCapture{}))

	_, err := service.Recompute(context.Background())

	if !errors.Is(err, league.ErrUnscoped) {
		t.Fatalf("error = %v, want it to wrap %v", err, league.ErrUnscoped)
	}
	if got := rows.newestHourCalls(); len(got) != 0 {
		t.Errorf("NewestHour called %d times, want 0", len(got))
	}
	if _, warm := cache.Snapshot(DefaultHorizon); warm {
		t.Error("cache warm = true, want false")
	}
	if n := notify.count(); n != 0 {
		t.Errorf("notify called %d times, want 0", n)
	}
}

func TestRecompute_nilNotifyAndNilLogger_stillWarmsTheCache(t *testing.T) {
	// A server started without a Mercure hub passes no notify, and a caller that
	// wants the default logger passes none: neither may cost a recompute.
	cache := NewCache()
	service := NewService(twoHourRows(), serviceScope, Config{}, cache, nil, nil)

	got, err := service.Recompute(context.Background())
	if err != nil {
		t.Fatalf("Recompute: %v", err)
	}

	cached, warm := cache.Snapshot(DefaultHorizon)
	if !warm {
		t.Fatal("cache warm = false, want true")
	}
	if !reflect.DeepEqual(cached, got) {
		t.Errorf("cached = %+v, want the returned result %+v", cached, got)
	}
}

func TestTrigger_recomputesOnce(t *testing.T) {
	rows := twoHourRows()
	service, cache, _ := newTestService(t, rows, Config{})

	service.Trigger(context.Background())

	if got := rows.newestHourCalls(); len(got) != 1 {
		t.Errorf("NewestHour called %d times, want 1", len(got))
	}
	if _, warm := cache.Snapshot(DefaultHorizon); !warm {
		t.Error("cache warm = false, want true")
	}
}

func TestTrigger_twoTriggersDuringOneRun_collapseIntoASingleRerun(t *testing.T) {
	// The subscriber fires one Trigger per stored hour, so a catch-up pass would
	// otherwise mean N full window reads for one answer. What must survive the
	// collapse is the LAST event: the in-flight read may have started before its
	// rows were committed, so exactly one rerun follows the burst — not none
	// (lost update) and not one per trigger (N window reads).
	rows := twoHourRows()
	rows.started = make(chan struct{}, 8)
	rows.release = make(chan struct{})
	service, _, _ := newTestService(t, rows, Config{})

	done := make(chan struct{})
	go func() {
		defer close(done)
		service.Trigger(context.Background())
	}()

	<-rows.started // the first recompute is inside NewestHour and holds `running`

	service.Trigger(context.Background()) // returns immediately, marks dirty
	service.Trigger(context.Background()) // returns immediately, dirty already set

	close(rows.release)
	<-done

	if got := rows.newestHourCalls(); len(got) != 2 {
		t.Errorf("NewestHour called %d times, want 2 — the running pass plus one dirty rerun", len(got))
	}
}

func TestTrigger_recomputeFails_logsAWarningInsteadOfPanicking(t *testing.T) {
	// Trigger is fire-and-forget from the subscriber and the startup goroutine:
	// the error has nowhere to be returned to, so it must be visible in the log.
	capture := captureLogs(t)
	cache := NewCache()
	service := NewService(&fakeRows{newestErr: errNewestHour}, serviceScope, Config{}, cache, nil, nil)

	service.Trigger(context.Background())

	rec := recordWithMessage(t, capture, "currency-exchange: recompute failed")
	if rec.Level != slog.LevelWarn {
		t.Errorf("level = %v, want %v", rec.Level, slog.LevelWarn)
	}
	var errText string
	rec.Attrs(func(a slog.Attr) bool {
		if a.Key == "error" {
			errText = a.Value.String()
			return false
		}
		return true
	})
	if !strings.Contains(errText, errNewestHour.Error()) {
		t.Errorf("error attribute = %q, want it to carry %q", errText, errNewestHour.Error())
	}
	if !strings.HasPrefix(errText, "exchange service: recompute: ") {
		t.Errorf("error attribute = %q, want it to name the failing operation", errText)
	}
	if _, warm := cache.Snapshot(DefaultHorizon); warm {
		t.Error("cache warm = true, want false")
	}
}

func TestHandleEvent_malformedPayload_stillRecomputes(t *testing.T) {
	// The event's contents are deliberately unread: a replayed hour publishes
	// rows: 0 while being fully populated, so a content check would skip exactly
	// the recompute a crash recovery needs.
	rows := twoHourRows()
	service, cache, _ := newTestService(t, rows, Config{})

	service.HandleEvent(context.Background(), []byte("}{ not json at all"))

	if got := rows.newestHourCalls(); len(got) != 1 {
		t.Errorf("NewestHour called %d times, want 1", len(got))
	}
	cached, warm := cache.Snapshot(DefaultHorizon)
	if !warm {
		t.Fatal("cache warm = false, want true")
	}
	if len(cached.Plays) != 1 {
		t.Errorf("cached %d plays, want 1", len(cached.Plays))
	}
}

func TestCache_beforeAnyRecompute_readsAsCold(t *testing.T) {
	cache := NewCache()

	got, warm := cache.Snapshot(DefaultHorizon)

	if warm {
		t.Error("warm = true, want false — nothing has been computed yet")
	}
	if got.League != "" || got.Hours != 0 || got.Plays != nil {
		t.Errorf("result = %+v, want the zero Result", got)
	}
}

func TestCache_nilReceiver_readsAsCold(t *testing.T) {
	// A server started without the currency-exchange pillar registers the route
	// with a nil cache; reading it must answer "no answer yet", not panic.
	var cache *Cache

	got, warm := cache.Snapshot(DefaultHorizon)

	if warm {
		t.Error("warm = true, want false")
	}
	if got.Plays != nil {
		t.Errorf("Plays = %v, want nil", got.Plays)
	}
	cache.Set(DefaultHorizon, Result{League: "Allflame"}) // must not panic either
}

func TestCache_emptyResult_readsAsWarm(t *testing.T) {
	// WARM-AND-EMPTY is not COLD: a league whose honest answer is "no plays"
	// must not leave the cache reading as "not computed yet" forever.
	cache := NewCache()

	cache.Set(DefaultHorizon, Result{League: "Allflame", Plays: []Play{}})

	got, warm := cache.Snapshot(DefaultHorizon)
	if !warm {
		t.Error("warm = false, want true")
	}
	if got.Plays == nil {
		t.Error("Plays = nil, want an allocated empty slice")
	}
	if len(got.Plays) != 0 {
		t.Errorf("got %d plays, want 0", len(got.Plays))
	}
}

func TestCache_oneHorizonSet_leavesTheOtherCold(t *testing.T) {
	// Each horizon warms on its own, so a client asking for the day ranking
	// before the first recompute finished must not be served the recent one.
	cache := NewCache()
	cache.Set(HorizonRecent, Result{League: "Allflame", Horizon: string(HorizonRecent), Hours: 6, Plays: []Play{{Key: "direct:a"}}})

	recent, recentWarm := cache.Snapshot(HorizonRecent)
	day, dayWarm := cache.Snapshot(HorizonDay)

	if !recentWarm {
		t.Error("recent warm = false, want true")
	}
	if recent.Hours != 6 || len(recent.Plays) != 1 {
		t.Errorf("recent = %+v, want the stored six-hour result", recent)
	}
	if dayWarm {
		t.Error("day warm = true, want false — nothing has been computed for it")
	}
	if day.Horizon != "" || day.Plays != nil {
		t.Errorf("day = %+v, want the zero Result", day)
	}
}

func TestCache_secondSet_replacesTheStoredResultWholesale(t *testing.T) {
	// Results are whole-corpus answers, so the newest recompute is the truth —
	// a merge would keep plays the newest window no longer supports.
	cache := NewCache()
	first := Result{League: "Allflame", Hours: 6, Plays: []Play{{Key: "direct:a"}, {Key: "direct:b"}}}
	second := Result{League: "Allflame", Hours: 1, Plays: []Play{{Key: "direct:c"}}}

	cache.Set(DefaultHorizon, first)
	cache.Set(DefaultHorizon, second)

	got, _ := cache.Snapshot(DefaultHorizon)
	if !reflect.DeepEqual(got, second) {
		t.Errorf("snapshot = %+v, want %+v", got, second)
	}
}

func TestCache_snapshotPlaysAreACopy_soAReaderCannotCorruptTheCache(t *testing.T) {
	cache := NewCache()
	cache.Set(DefaultHorizon, Result{League: "Allflame", Plays: []Play{{Key: "direct:a"}, {Key: "direct:b"}}})

	// Rewriting through the snapshot is what a handler filtering or sorting in
	// place would do. Assigning through the index (rather than appending, which
	// reallocates a full slice and would pass either way) is what reaches the
	// backing array the cache would be sharing.
	snapshot, _ := cache.Snapshot(DefaultHorizon)
	snapshot.Plays[0].Key = "rewritten by the reader"

	got, _ := cache.Snapshot(DefaultHorizon)
	if got.Plays[0].Key != "direct:a" {
		t.Errorf("cached play key = %q, want %q — the snapshot aliased the cached slice",
			got.Plays[0].Key, "direct:a")
	}
}

func TestLastUpdated(t *testing.T) {
	// The newest feed hour is To − 1h, because To is the half-open window's
	// exclusive upper bound. A result covering no hours has no such hour at all.
	tests := []struct {
		name   string
		result Result
		want   time.Time
		wantOK bool
	}{
		{
			name:   "result covering hours reports the newest one",
			result: Result{Hours: 3, To: feedHour.Add(time.Hour)},
			want:   feedHour,
			wantOK: true,
		},
		{
			name:   "result covering no hours reports none",
			result: Result{Hours: 0},
			wantOK: false,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got, ok := LastUpdated(tc.result)

			if ok != tc.wantOK {
				t.Fatalf("ok = %t, want %t", ok, tc.wantOK)
			}
			if ok && !got.Equal(tc.want) {
				t.Errorf("lastUpdated = %s, want %s", got, tc.want)
			}
		})
	}
}

func TestUpdatePayload_resultCoveringHours_carriesTheNewestFeedHour(t *testing.T) {
	now := time.Date(2026, 8, 19, 14, 37, 5, 0, time.UTC)
	result := Result{
		League: "Allflame",
		Hours:  6,
		To:     feedHour.Add(time.Hour),
		Plays:  []Play{{Key: "direct:a"}, {Key: "direct:b"}},
	}

	payload := UpdatePayload(result, now)

	want := map[string]any{
		"topic":       UpdatedTopic,
		"league":      "Allflame",
		"lastUpdated": feedHour.Format(time.RFC3339),
		"hours":       6,
		"plays":       2,
		"timestamp":   "2026-08-19T14:37:05Z",
	}
	if !reflect.DeepEqual(payload, want) {
		t.Errorf("payload = %+v, want %+v", payload, want)
	}
}

func TestUpdatePayload_resultCoveringNoHours_rendersLastUpdatedAsNull(t *testing.T) {
	// "The feed has no hour yet" must not render as the zero time, which a
	// client would read as a real — and very stale — timestamp.
	now := time.Date(2026, 8, 19, 14, 37, 5, 0, time.UTC)

	payload := UpdatePayload(Result{League: "Allflame", Plays: []Play{}}, now)

	value, ok := payload["lastUpdated"]
	if !ok {
		t.Fatalf("payload carries no lastUpdated key (%v)", payload)
	}
	if value != nil {
		t.Errorf("lastUpdated = %v, want nil", value)
	}
	if payload["hours"] != 0 {
		t.Errorf("hours = %v, want 0", payload["hours"])
	}
	if payload["plays"] != 0 {
		t.Errorf("plays = %v, want 0", payload["plays"])
	}
}

func TestUpdatePayload_localTimes_areRenderedInUTC(t *testing.T) {
	// The topic is consumed by clients in every time zone; the feed hour is a UTC
	// identity and the event timestamp must be comparable across servers.
	zone := time.FixedZone("UTC+7", 7*60*60)
	now := time.Date(2026, 8, 19, 21, 37, 5, 0, zone)
	result := Result{League: "Allflame", Hours: 1, To: feedHour.Add(time.Hour).In(zone)}

	payload := UpdatePayload(result, now)

	if want := feedHour.Format(time.RFC3339); payload["lastUpdated"] != want {
		t.Errorf("lastUpdated = %v, want %q", payload["lastUpdated"], want)
	}
	if want := "2026-08-19T14:37:05Z"; payload["timestamp"] != want {
		t.Errorf("timestamp = %v, want %q", payload["timestamp"], want)
	}
}

func TestDebouncer_burstOfSignals_callsFnOnceAfterTheQuietPeriod(t *testing.T) {
	var calls atomic.Int64
	debouncer := NewDebouncer(50*time.Millisecond, func() { calls.Add(1) })

	for i := 0; i < 5; i++ {
		debouncer.Signal()
		time.Sleep(2 * time.Millisecond)
	}

	waitFor(t, "the debounced call", func() bool { return calls.Load() >= 1 })
	// Two further quiet periods prove the burst produced one call and not five
	// arriving one after another.
	time.Sleep(100 * time.Millisecond)
	if got := calls.Load(); got != 1 {
		t.Errorf("fn called %d times, want 1 — the burst must collapse into one call", got)
	}
}

func TestDebouncer_secondBurstAfterTheQuietPeriod_callsFnAgain(t *testing.T) {
	// The collapse is per burst, not once per process: a later stored hour must
	// still reach the clients.
	var calls atomic.Int64
	debouncer := NewDebouncer(50*time.Millisecond, func() { calls.Add(1) })

	debouncer.Signal()
	waitFor(t, "the first debounced call", func() bool { return calls.Load() == 1 })

	debouncer.Signal()
	waitFor(t, "the second debounced call", func() bool { return calls.Load() == 2 })

	time.Sleep(100 * time.Millisecond)
	if got := calls.Load(); got != 2 {
		t.Errorf("fn called %d times, want exactly 2", got)
	}
}

func TestDebouncer_stopBeforeTheQuietPeriodElapses_dropsThePendingCall(t *testing.T) {
	// cmd/server defers Stop: a publish still pending when the process is going
	// away announces "the served answer changed" from a server that is no longer
	// serving it. Clients refetch on their next poll instead.
	var calls atomic.Int64
	debouncer := NewDebouncer(50*time.Millisecond, func() { calls.Add(1) })

	debouncer.Signal()
	debouncer.Stop()

	time.Sleep(120 * time.Millisecond) // two quiet periods
	if got := calls.Load(); got != 0 {
		t.Errorf("fn called %d times, want 0 — Stop drops the pending call", got)
	}
}

func TestDebouncer_signalAfterStop_startsAFreshQuietPeriod(t *testing.T) {
	// Stop cancels what is pending; it does not retire the debouncer. A Stop
	// that left it dead would silence every later update on a server that kept
	// running.
	var calls atomic.Int64
	debouncer := NewDebouncer(20*time.Millisecond, func() { calls.Add(1) })

	debouncer.Signal()
	debouncer.Stop()
	debouncer.Signal()

	waitFor(t, "the debounced call after Stop", func() bool { return calls.Load() == 1 })
}

func TestDebouncer_stopOnANilDebouncer_isANoOp(t *testing.T) {
	// A server wired without the pillar never builds one, and cmd/server defers
	// Stop unconditionally.
	var debouncer *Debouncer

	debouncer.Stop()
}

func TestDebouncer_unusableDebouncer_signalsAreNoOps(t *testing.T) {
	// cmd/server passes Debouncer.Signal as the service's notify without a nil
	// branch, so a server wired without a hub must survive both shapes.
	var nilDebouncer *Debouncer
	nilDebouncer.Signal()

	NewDebouncer(time.Millisecond, nil).Signal()

	// Nothing to observe but the absence of a panic: reaching this line after a
	// timer would have fired is the assertion.
	time.Sleep(20 * time.Millisecond)
}
