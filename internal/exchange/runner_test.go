package exchange

import (
	"context"
	"errors"
	"log/slog"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// runnerScope is the league every runner test ingests for. The feed carries all
// leagues in one payload, so otherLeague rows exist to be dropped.
var runnerScope = league.Historical("Allflame")

const otherLeague = "Hardcore Allflame"

// fixedNow is the clock the runner tests run on. It sits mid-hour on purpose: a
// bootstrap that forgets to truncate to the hour lands on a different unix hour
// and fails the bootstrap test.
var fixedNow = time.Date(2026, 8, 19, 14, 37, 5, 0, time.UTC)

// cursorHour is an arbitrary stored cursor: a whole hour, as the feed's own
// cursor always is.
var cursorHour = time.Date(2026, 8, 19, 10, 0, 0, 0, time.UTC).Unix()

var (
	errFetch   = errors.New("connection reset by peer")
	errStore   = errors.New("deadlock detected")
	errPublish = errors.New("hub refused the event")
)

// itemIDs give each market of a synthetic hour a distinct market_id. divineID is
// absent because validSpec already uses it as ItemB.
var itemIDs = []string{chaosID, scarabID, omenID, hellID, cardID}

// publishedHour builds a published hour carrying one well-formed market per
// entry of leagues, each priced at 196 ItemA for one ItemB.
func publishedHour(hour int64, leagues ...string) *HourPayload {
	payload := &HourPayload{NextChangeID: hour + secondsPerHour}
	for i, lg := range leagues {
		spec := validSpec()
		spec.league = lg
		spec.itemA = itemIDs[i%len(itemIDs)]
		payload.Markets = append(payload.Markets, spec.market())
	}
	return payload
}

// malformedHour builds a published hour whose only market is malformed: a
// market_pair of one id, which Normalize skips.
func malformedHour(hour int64) *HourPayload {
	return &HourPayload{
		NextChangeID: hour + secondsPerHour,
		Markets: []Market{{
			League:     runnerScope.ID(),
			MarketID:   chaosDivineMarket,
			MarketPair: []string{chaosID},
		}},
	}
}

// partlyMalformedHour builds a published hour carrying one well-formed market of
// the scoped league next to one malformed market, so Normalize keeps a row and
// skips a market in the same hour.
func partlyMalformedHour(hour int64) *HourPayload {
	payload := publishedHour(hour, runnerScope.ID())
	payload.Markets = append(payload.Markets, Market{
		League:     runnerScope.ID(),
		MarketID:   scarabMarket,
		MarketPair: []string{scarabID},
	})
	return payload
}

// publishedThrough answers every hour before limit with a published payload and
// limit onwards with *ErrNotPublished — a feed caught up at limit.
func publishedThrough(limit int64, leagues ...string) func(int64) (*HourPayload, error) {
	return func(hour int64) (*HourPayload, error) {
		if hour >= limit {
			return nil, &ErrNotPublished{NextChangeID: hour}
		}
		return publishedHour(hour, leagues...), nil
	}
}

// alwaysPublished answers every hour with a published payload: an unbounded
// backlog, so only MaxHoursPerTick can stop the walk.
func alwaysPublished(leagues ...string) func(int64) (*HourPayload, error) {
	return func(hour int64) (*HourPayload, error) {
		return publishedHour(hour, leagues...), nil
	}
}

// neverPublished answers every hour with *ErrNotPublished: a feed with nothing
// new, which is the steady state of a caught-up walk.
func neverPublished() func(int64) (*HourPayload, error) {
	return func(hour int64) (*HourPayload, error) {
		return nil, &ErrNotPublished{NextChangeID: hour}
	}
}

// fetchCall records one FetchHour invocation.
type fetchCall struct {
	hour  int64
	realm string
}

// fakeFetcher answers FetchHour from answer and records every call.
type fakeFetcher struct {
	mu     sync.Mutex
	calls  []fetchCall
	answer func(hour int64) (*HourPayload, error)
}

func newFetcher(answer func(int64) (*HourPayload, error)) *fakeFetcher {
	return &fakeFetcher{answer: answer}
}

// failingFetcher answers every hour with err.
func failingFetcher(err error) *fakeFetcher {
	return newFetcher(func(int64) (*HourPayload, error) { return nil, err })
}

func (f *fakeFetcher) FetchHour(_ context.Context, hour int64, realm string) (*HourPayload, error) {
	f.mu.Lock()
	f.calls = append(f.calls, fetchCall{hour: hour, realm: realm})
	f.mu.Unlock()
	return f.answer(hour)
}

func (f *fakeFetcher) recorded() []fetchCall {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]fetchCall(nil), f.calls...)
}

// hours returns the requested hour of every recorded fetch, in order.
func (f *fakeFetcher) hours() []int64 {
	calls := f.recorded()
	hours := make([]int64, len(calls))
	for i := range calls {
		hours[i] = calls[i].hour
	}
	return hours
}

func (f *fakeFetcher) callCount() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return len(f.calls)
}

// insertCall records one InsertHour invocation. The rows are copied so a later
// reuse of the runner's slice cannot rewrite what the test asserts on.
type insertCall struct {
	scope    league.Scope
	hour     time.Time
	rows     []Row
	nextHour int64
}

// fakeStore is an in-memory Store. A successful InsertHour keeps the cursor the
// way the real repository does — committed together with the hour — so a second
// RunOnce against the same fake resumes where the first one stopped.
type fakeStore struct {
	mu        sync.Mutex
	cursor    int64
	found     bool
	cursorErr error
	insertErr error
	calls     []insertCall
}

// emptyStore is a store with no cursor row, which makes the runner bootstrap.
func emptyStore() *fakeStore { return &fakeStore{} }

// storeAt is a store whose cursor row points at hour.
func storeAt(hour int64) *fakeStore { return &fakeStore{cursor: hour, found: true} }

func (s *fakeStore) Cursor(_ context.Context, _ league.Scope) (int64, bool, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.cursorErr != nil {
		return 0, false, s.cursorErr
	}
	return s.cursor, s.found, nil
}

func (s *fakeStore) InsertHour(_ context.Context, scope league.Scope, hour time.Time, rows []Row, nextHour int64) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.calls = append(s.calls, insertCall{
		scope:    scope,
		hour:     hour,
		rows:     append([]Row(nil), rows...),
		nextHour: nextHour,
	})
	if s.insertErr != nil {
		return 0, s.insertErr
	}
	s.cursor, s.found = nextHour, true
	return len(rows), nil
}

func (s *fakeStore) inserts() []insertCall {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]insertCall(nil), s.calls...)
}

func (s *fakeStore) insertCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.calls)
}

// publishCall records one Publish invocation with a copy of the payload map.
type publishCall struct {
	topic   string
	payload map[string]any
}

type fakePublisher struct {
	mu    sync.Mutex
	err   error
	calls []publishCall
}

func (p *fakePublisher) Publish(_ context.Context, topic string, payload map[string]any) error {
	copied := make(map[string]any, len(payload))
	for key, value := range payload {
		copied[key] = value
	}
	p.mu.Lock()
	p.calls = append(p.calls, publishCall{topic: topic, payload: copied})
	err := p.err
	p.mu.Unlock()
	return err
}

func (p *fakePublisher) published() []publishCall {
	p.mu.Lock()
	defer p.mu.Unlock()
	return append([]publishCall(nil), p.calls...)
}

func (p *fakePublisher) publishCount() int {
	p.mu.Lock()
	defer p.mu.Unlock()
	return len(p.calls)
}

// newRunner builds a Runner over the fakes, filling only the two fields every
// test wants pinned: the clock and a captured logger. Everything else is left to
// NewRunner's own defaults so the defaults stay under test.
func newRunner(t *testing.T, f HourFetcher, s Store, p EventPublisher, cfg RunnerConfig) (*Runner, *logCapture) {
	t.Helper()
	capture := &logCapture{}
	if cfg.Now == nil {
		cfg.Now = func() time.Time { return fixedNow }
	}
	if cfg.Logger == nil {
		cfg.Logger = slog.New(capture)
	}
	return NewRunner(f, s, p, cfg), capture
}

// assertHours compares the fetched hour sequence exactly: both which hours were
// asked for and how many times.
func assertHours(t *testing.T, got, want []int64) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("fetched hours = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("fetched hours = %v, want %v", got, want)
		}
	}
}

// payloadNumber reads an integer payload field. Both int and int64 are accepted
// because the JSON the adapter marshals cannot tell them apart; the value is
// what the event contract fixes.
func payloadNumber(t *testing.T, payload map[string]any, key string) int64 {
	t.Helper()
	value, ok := payload[key]
	if !ok {
		t.Fatalf("payload has no %q field (payload: %v)", key, payload)
	}
	switch v := value.(type) {
	case int64:
		return v
	case int:
		return int64(v)
	default:
		t.Fatalf("payload[%q] = %v (%T), want an integer", key, value, value)
		return 0
	}
}

func payloadString(t *testing.T, payload map[string]any, key string) string {
	t.Helper()
	value, ok := payload[key]
	if !ok {
		t.Fatalf("payload has no %q field (payload: %v)", key, payload)
	}
	text, ok := value.(string)
	if !ok {
		t.Fatalf("payload[%q] = %v (%T), want a string", key, value, value)
	}
	return text
}

// recordWithMessage returns the single captured record whose message is msg.
func recordWithMessage(t *testing.T, capture *logCapture, msg string) slog.Record {
	t.Helper()
	var found []slog.Record
	for _, rec := range capture.records() {
		if rec.Message == msg {
			found = append(found, rec)
		}
	}
	if len(found) != 1 {
		messages := make([]string, 0, len(capture.records()))
		for _, rec := range capture.records() {
			messages = append(messages, rec.Message)
		}
		t.Fatalf("got %d records with message %q, want 1 (captured: %v)", len(found), msg, messages)
	}
	return found[0]
}

// assertNoRecordWithMessage fails when any captured record carries msg.
func assertNoRecordWithMessage(t *testing.T, capture *logCapture, msg string) {
	t.Helper()
	for _, rec := range capture.records() {
		if rec.Message == msg {
			t.Fatalf("captured a %q record, want none", msg)
		}
	}
}

// assertNoWarnRecords fails when anything was logged at warn or above.
func assertNoWarnRecords(t *testing.T, capture *logCapture) {
	t.Helper()
	for _, rec := range capture.records() {
		if rec.Level >= slog.LevelWarn {
			t.Fatalf("captured a %v record %q, want nothing above info", rec.Level, rec.Message)
		}
	}
}

// waitFor polls cond until it holds, so a loop test never sleeps longer than the
// behaviour it waits for.
func waitFor(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if cond() {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("timed out after 2s waiting for %s", what)
}

func TestRunOnce_noCursorRow_bootstrapsAtTheHourOneBootstrapWindowBack(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, emptyStore(), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	// fixedNow is 2026-08-19T14:37:05Z; the default 24h bootstrap truncated to
	// the hour is 2026-08-18T14:00:00Z.
	want := time.Date(2026, 8, 18, 14, 0, 0, 0, time.UTC).Unix()
	assertHours(t, fetcher.hours(), []int64{want})
}

func TestRunOnce_configuredBootstrapWindow_movesTheFirstHourBack(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, emptyStore(), &fakePublisher{}, RunnerConfig{
		Scope:     runnerScope,
		Bootstrap: 2 * time.Hour,
	})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	want := time.Date(2026, 8, 19, 12, 0, 0, 0, time.UTC).Unix()
	assertHours(t, fetcher.hours(), []int64{want})
}

func TestRunOnce_storedCursor_fetchesTheStoredHourInsteadOfBootstrapping(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	assertHours(t, fetcher.hours(), []int64{cursorHour})
}

func TestRunOnce_cursorReadFails_returnsTheErrorWithoutFetching(t *testing.T) {
	store := emptyStore()
	store.cursorErr = errStore
	fetcher := newFetcher(alwaysPublished(runnerScope.ID()))
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if !errors.Is(err, errStore) {
		t.Fatalf("error = %v, want it to wrap %v", err, errStore)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if fetcher.callCount() != 0 {
		t.Errorf("fetched %d hours, want 0 — an unknown cursor must not spend a fetch", fetcher.callCount())
	}
}

func TestRunOnce_walksEveryPublishedHourAndStopsOnTheUnpublishedOne(t *testing.T) {
	limit := cursorHour + 3*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	if processed != 3 {
		t.Errorf("processed = %d, want 3", processed)
	}
	assertHours(t, fetcher.hours(), []int64{
		cursorHour,
		cursorHour + secondsPerHour,
		cursorHour + 2*secondsPerHour,
		limit,
	})
}

func TestRunOnce_storesEachPublishedHourWithTheFollowingHourAsCursor(t *testing.T) {
	limit := cursorHour + 3*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID(), runnerScope.ID()))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	inserts := store.inserts()
	if len(inserts) != 3 {
		t.Fatalf("InsertHour called %d times, want 3 (the unpublished hour must not be stored)", len(inserts))
	}
	for i, insert := range inserts {
		hour := cursorHour + int64(i)*secondsPerHour
		if wantHour := time.Unix(hour, 0).UTC(); !insert.hour.Equal(wantHour) {
			t.Errorf("insert %d hour = %s, want %s", i, insert.hour, wantHour)
		}
		if want := hour + secondsPerHour; insert.nextHour != want {
			t.Errorf("insert %d nextHour = %d, want %d", i, insert.nextHour, want)
		}
		if insert.scope.ID() != runnerScope.ID() {
			t.Errorf("insert %d scope = %q, want %q", i, insert.scope.ID(), runnerScope.ID())
		}
		if len(insert.rows) != 2 {
			t.Errorf("insert %d stored %d rows, want 2", i, len(insert.rows))
		}
	}
}

func TestRunOnce_publishesOneEventPerStoredHour(t *testing.T) {
	limit := cursorHour + 2*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID(), runnerScope.ID()))
	publisher := &fakePublisher{}
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), publisher, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	events := publisher.published()
	if len(events) != 2 {
		t.Fatalf("Publish called %d times, want 2", len(events))
	}
	for i, event := range events {
		hour := cursorHour + int64(i)*secondsPerHour
		if event.topic != Topic {
			t.Errorf("event %d topic argument = %q, want %q", i, event.topic, Topic)
		}
		if got := payloadString(t, event.payload, "topic"); got != "poe/collector/currency-exchange" {
			t.Errorf("event %d topic field = %q, want %q", i, got, "poe/collector/currency-exchange")
		}
		if got := payloadString(t, event.payload, "endpoint"); got != "currency-exchange" {
			t.Errorf("event %d endpoint = %q, want %q", i, got, "currency-exchange")
		}
		if got := payloadNumber(t, event.payload, "hour"); got != hour {
			t.Errorf("event %d hour = %d, want %d", i, got, hour)
		}
		if got, want := payloadNumber(t, event.payload, "nextCursor"), hour+secondsPerHour; got != want {
			t.Errorf("event %d nextCursor = %d, want %d", i, got, want)
		}
		if got := payloadNumber(t, event.payload, "rows"); got != 2 {
			t.Errorf("event %d rows = %d, want 2", i, got)
		}
		if got, want := payloadString(t, event.payload, "timestamp"), fixedNow.Format(time.RFC3339); got != want {
			t.Errorf("event %d timestamp = %q, want %q", i, got, want)
		}
	}
}

func TestRunOnce_firstHourNotPublished_reportsNoWorkWithoutAnError(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	store := storeAt(cursorHour)
	publisher := &fakePublisher{}
	runner, _ := newRunner(t, fetcher, store, publisher, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if err != nil {
		t.Fatalf("error = %v, want nil — an unpublished hour is the normal end of a pass", err)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0", store.insertCount())
	}
	if publisher.publishCount() != 0 {
		t.Errorf("Publish called %d times, want 0", publisher.publishCount())
	}
}

func TestRunOnce_fetchFails_returnsTheErrorWithoutStoringTheHour(t *testing.T) {
	fetcher := failingFetcher(errFetch)
	store := storeAt(cursorHour)
	publisher := &fakePublisher{}
	runner, _ := newRunner(t, fetcher, store, publisher, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if !errors.Is(err, errFetch) {
		t.Fatalf("error = %v, want it to wrap %v", err, errFetch)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0", store.insertCount())
	}
	if publisher.publishCount() != 0 {
		t.Errorf("Publish called %d times, want 0", publisher.publishCount())
	}
}

func TestRunOnce_afterAFetchFailure_theNextPassRefetchesTheSameHour(t *testing.T) {
	fetcher := failingFetcher(errFetch)
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err == nil {
		t.Fatal("first RunOnce: want an error")
	}
	if _, err := runner.RunOnce(context.Background()); err == nil {
		t.Fatal("second RunOnce: want an error")
	}

	assertHours(t, fetcher.hours(), []int64{cursorHour, cursorHour})
}

func TestRunOnce_everyMarketSkipped_failsTheHourWithoutStoringIt(t *testing.T) {
	// Normalize's own "skipped malformed markets" warning goes to the default
	// logger and is asserted in normalize_test.go; redirect it so it stays out of
	// this test's output.
	captureLogs(t)

	fetcher := newFetcher(func(hour int64) (*HourPayload, error) { return malformedHour(hour), nil })
	store := storeAt(cursorHour)
	publisher := &fakePublisher{}
	runner, logs := newRunner(t, fetcher, store, publisher, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if err == nil {
		t.Fatal("error = nil, want an error — an all-skipped hour reads as feed schema drift")
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0 — the cursor must stay on the failed hour", store.insertCount())
	}
	if publisher.publishCount() != 0 {
		t.Errorf("Publish called %d times, want 0", publisher.publishCount())
	}

	record := recordWithMessage(t, logs, "currency-exchange: every market skipped, not advancing")
	if record.Level != slog.LevelError {
		t.Errorf("log level = %v, want %v", record.Level, slog.LevelError)
	}
	if got := attrInt64(t, record, "hour"); got != cursorHour {
		t.Errorf("log hour = %d, want %d", got, cursorHour)
	}
	if got := attrInt64(t, record, "skipped"); got != 1 {
		t.Errorf("log skipped = %d, want 1", got)
	}
}

func TestRunOnce_mixedLeaguePayload_storesOnlyTheScopedLeaguesRows(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID(), otherLeague, otherLeague))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	inserts := store.inserts()
	if len(inserts) != 1 {
		t.Fatalf("InsertHour called %d times, want 1", len(inserts))
	}
	rows := inserts[0].rows
	if len(rows) != 1 {
		t.Fatalf("stored %d rows, want 1 — the two %q rows must be dropped", len(rows), otherLeague)
	}
	if rows[0].League != runnerScope.ID() {
		t.Errorf("stored row league = %q, want %q", rows[0].League, runnerScope.ID())
	}
	if rows[0].MarketID != chaosDivineMarket {
		t.Errorf("stored row market = %q, want %q", rows[0].MarketID, chaosDivineMarket)
	}
}

func TestRunOnce_mixedLeaguePayload_countsTheDroppedRowsInTheHourLog(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID(), otherLeague, otherLeague))
	runner, logs := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	record := recordWithMessage(t, logs, "currency-exchange: hour stored")
	if got := attrInt64(t, record, "otherLeagueRows"); got != 2 {
		t.Errorf("log otherLeagueRows = %d, want 2", got)
	}
	if got := attrInt64(t, record, "rows"); got != 1 {
		t.Errorf("log rows = %d, want 1", got)
	}
}

func TestRunOnce_hourWithOnlyOtherLeagueRows_advancesPastItWithZeroRows(t *testing.T) {
	// Hour one belongs entirely to another league; hour two is ours. If the
	// zero-row hour did not advance the cursor, hour two would never be reached.
	ours := cursorHour + secondsPerHour
	fetcher := newFetcher(func(hour int64) (*HourPayload, error) {
		switch hour {
		case cursorHour:
			return publishedHour(hour, otherLeague), nil
		case ours:
			return publishedHour(hour, runnerScope.ID()), nil
		default:
			return nil, &ErrNotPublished{NextChangeID: hour}
		}
	})
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}
	if processed != 2 {
		t.Errorf("processed = %d, want 2", processed)
	}

	inserts := store.inserts()
	if len(inserts) != 2 {
		t.Fatalf("InsertHour called %d times, want 2", len(inserts))
	}
	if len(inserts[0].rows) != 0 {
		t.Errorf("hour one stored %d rows, want 0", len(inserts[0].rows))
	}
	if inserts[0].nextHour != ours {
		t.Errorf("hour one nextHour = %d, want %d", inserts[0].nextHour, ours)
	}
	if len(inserts[1].rows) != 1 {
		t.Errorf("hour two stored %d rows, want 1", len(inserts[1].rows))
	}
}

func TestRunOnce_hourWithOnlyOtherLeagueRows_publishesZeroRows(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, otherLeague))
	publisher := &fakePublisher{}
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), publisher, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	events := publisher.published()
	if len(events) != 1 {
		t.Fatalf("Publish called %d times, want 1 — downstream recompute needs to know the hour passed", len(events))
	}
	if got := payloadNumber(t, events[0].payload, "rows"); got != 0 {
		t.Errorf("event rows = %d, want 0", got)
	}
	if got := payloadNumber(t, events[0].payload, "hour"); got != cursorHour {
		t.Errorf("event hour = %d, want %d", got, cursorHour)
	}
}

func TestRunOnce_defaultMaxHoursPerTick_stopsAfterFortyEightHours(t *testing.T) {
	fetcher := newFetcher(alwaysPublished(runnerScope.ID()))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	if processed != 48 {
		t.Errorf("processed = %d, want 48", processed)
	}
	if got := fetcher.callCount(); got != 48 {
		t.Errorf("fetched %d hours, want 48 — a 49th fetch would exceed the bound", got)
	}
	if got := store.insertCount(); got != 48 {
		t.Errorf("InsertHour called %d times, want 48", got)
	}
}

func TestRunOnce_configuredMaxHoursPerTick_boundsThePass(t *testing.T) {
	fetcher := newFetcher(alwaysPublished(runnerScope.ID()))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{
		Scope:           runnerScope,
		MaxHoursPerTick: 3,
	})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	if processed != 3 {
		t.Errorf("processed = %d, want 3", processed)
	}
	assertHours(t, fetcher.hours(), []int64{
		cursorHour,
		cursorHour + secondsPerHour,
		cursorHour + 2*secondsPerHour,
	})
	// The bounded pass leaves the cursor on the next unfetched hour, so the
	// following pass continues rather than repeating.
	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("second RunOnce: %v", err)
	}
	if got := store.inserts()[3].hour; !got.Equal(time.Unix(cursorHour+3*secondsPerHour, 0).UTC()) {
		t.Errorf("second pass first hour = %s, want %s", got, time.Unix(cursorHour+3*secondsPerHour, 0).UTC())
	}
}

func TestRunOnce_storeFails_stopsTheWalkWithoutPublishing(t *testing.T) {
	fetcher := newFetcher(alwaysPublished(runnerScope.ID()))
	store := storeAt(cursorHour)
	store.insertErr = errStore
	publisher := &fakePublisher{}
	runner, _ := newRunner(t, fetcher, store, publisher, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if !errors.Is(err, errStore) {
		t.Fatalf("error = %v, want it to wrap %v", err, errStore)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if publisher.publishCount() != 0 {
		t.Errorf("Publish called %d times, want 0 — an unstored hour must not be announced", publisher.publishCount())
	}
	assertHours(t, fetcher.hours(), []int64{cursorHour})
}

func TestRunOnce_publishFails_keepsWalkingPastTheStoredHour(t *testing.T) {
	limit := cursorHour + 2*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	store := storeAt(cursorHour)
	publisher := &fakePublisher{err: errPublish}
	runner, _ := newRunner(t, fetcher, store, publisher, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())

	if err != nil {
		t.Fatalf("error = %v, want nil — the hour is already committed when the publish fails", err)
	}
	if processed != 2 {
		t.Errorf("processed = %d, want 2", processed)
	}
	inserts := store.inserts()
	if len(inserts) != 2 {
		t.Fatalf("InsertHour called %d times, want 2", len(inserts))
	}
	if inserts[1].nextHour != limit {
		t.Errorf("last nextHour = %d, want %d", inserts[1].nextHour, limit)
	}
}

func TestRunOnce_publishFails_logsAWarningNamingTheHour(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	publisher := &fakePublisher{err: errPublish}
	runner, logs := newRunner(t, fetcher, storeAt(cursorHour), publisher, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	record := recordWithMessage(t, logs, "currency-exchange: publish failed")
	if record.Level != slog.LevelWarn {
		t.Errorf("log level = %v, want %v", record.Level, slog.LevelWarn)
	}
	if got := attrInt64(t, record, "hour"); got != cursorHour {
		t.Errorf("log hour = %d, want %d", got, cursorHour)
	}
}

func TestRunOnce_nilPublisher_stillStoresEveryHour(t *testing.T) {
	limit := cursorHour + 2*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, nil, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	if processed != 2 {
		t.Errorf("processed = %d, want 2", processed)
	}
	if got := store.insertCount(); got != 2 {
		t.Errorf("InsertHour called %d times, want 2", got)
	}
}

func TestRunOnce_defaultRealm_requestsThePCRealm(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	calls := fetcher.recorded()
	if len(calls) != 1 {
		t.Fatalf("fetched %d hours, want 1", len(calls))
	}
	if calls[0].realm != RealmPC {
		t.Errorf("realm = %q, want RealmPC (%q)", calls[0].realm, RealmPC)
	}
}

func TestRunOnce_configuredRealm_isPassedToEveryFetch(t *testing.T) {
	limit := cursorHour + 2*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{
		Scope: runnerScope,
		Realm: RealmXbox,
	})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	calls := fetcher.recorded()
	if len(calls) != 3 {
		t.Fatalf("fetched %d hours, want 3", len(calls))
	}
	for i, call := range calls {
		if call.realm != RealmXbox {
			t.Errorf("fetch %d realm = %q, want %q", i, call.realm, RealmXbox)
		}
	}
}

func TestRunOnce_unscopedScope_isRejectedBeforeFetching(t *testing.T) {
	fetcher := newFetcher(alwaysPublished("Allflame"))
	store := emptyStore()
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{})

	processed, err := runner.RunOnce(context.Background())

	if !errors.Is(err, league.ErrUnscoped) {
		t.Fatalf("error = %v, want it to wrap %v", err, league.ErrUnscoped)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if fetcher.callCount() != 0 {
		t.Errorf("fetched %d hours, want 0", fetcher.callCount())
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0", store.insertCount())
	}
}

func TestRun_unscopedScope_returnsTheScopeError(t *testing.T) {
	fetcher := newFetcher(alwaysPublished("Allflame"))
	runner, _ := newRunner(t, fetcher, emptyStore(), &fakePublisher{}, RunnerConfig{Tick: time.Millisecond})

	err := runner.Run(context.Background())

	if !errors.Is(err, league.ErrUnscoped) {
		t.Fatalf("error = %v, want it to wrap %v", err, league.ErrUnscoped)
	}
	if fetcher.callCount() != 0 {
		t.Errorf("fetched %d hours, want 0", fetcher.callCount())
	}
}

func TestRun_storesTheFirstHourBeforeTheFirstTickElapses(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	store := storeAt(cursorHour)
	// A tick far longer than the test: only the immediate pass can store an hour.
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{
		Scope: runnerScope,
		Tick:  time.Hour,
	})

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- runner.Run(ctx) }()

	waitFor(t, "the immediate pass to store hour one", func() bool { return store.insertCount() == 1 })
	cancel()
	<-done

	if got := store.inserts()[0].hour; !got.Equal(time.Unix(cursorHour, 0).UTC()) {
		t.Errorf("stored hour = %s, want %s", got, time.Unix(cursorHour, 0).UTC())
	}
}

func TestRun_eachTickRunsAnotherPass(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{
		Scope: runnerScope,
		Tick:  time.Millisecond,
	})

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- runner.Run(ctx) }()

	// One immediate pass plus at least two ticks; every pass re-fetches the same
	// unadvanced hour.
	waitFor(t, "three passes", func() bool { return fetcher.callCount() >= 3 })
	cancel()
	<-done

	for i, hour := range fetcher.hours() {
		if hour != cursorHour {
			t.Fatalf("fetch %d requested hour %d, want %d on every pass", i, hour, cursorHour)
		}
	}
}

func TestRun_cancelledContext_returnsContextCanceled(t *testing.T) {
	fetcher := newFetcher(neverPublished())
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{
		Scope: runnerScope,
		Tick:  time.Millisecond,
	})

	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- runner.Run(ctx) }()

	waitFor(t, "the first pass", func() bool { return fetcher.callCount() >= 1 })
	cancel()

	select {
	case err := <-done:
		if !errors.Is(err, context.Canceled) {
			t.Fatalf("error = %v, want %v", err, context.Canceled)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("Run did not return within 2s of cancellation")
	}
}

func TestRun_contextAlreadyCancelled_skipsTheImmediatePass(t *testing.T) {
	fetcher := newFetcher(alwaysPublished(runnerScope.ID()))
	store := storeAt(cursorHour)
	runner, _ := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{
		Scope: runnerScope,
		Tick:  time.Hour,
	})

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := runner.Run(ctx)

	if !errors.Is(err, context.Canceled) {
		t.Fatalf("error = %v, want %v", err, context.Canceled)
	}
	if fetcher.callCount() != 0 {
		t.Errorf("fetched %d hours, want 0 — a cancelled context must not spend a fetch", fetcher.callCount())
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0", store.insertCount())
	}
}

func TestRunOnce_feedCursorHoursAhead_warnsWithTheGapInHours(t *testing.T) {
	// A 404 whose next_change_id sits three hours ahead means the hours between
	// are gone: the walk is stuck and only an operator can move it.
	fetcher := newFetcher(func(int64) (*HourPayload, error) {
		return nil, &ErrNotPublished{NextChangeID: cursorHour + 3*secondsPerHour}
	})
	store := storeAt(cursorHour)
	runner, logs := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("error = %v, want nil — a gap is reported, not raised", err)
	}
	if processed != 0 {
		t.Errorf("processed = %d, want 0", processed)
	}
	if store.insertCount() != 0 {
		t.Errorf("InsertHour called %d times, want 0 — the cursor must stay where it is", store.insertCount())
	}

	record := recordWithMessage(t, logs, "currency-exchange: feed moved past the cursor")
	if record.Level != slog.LevelWarn {
		t.Errorf("log level = %v, want %v", record.Level, slog.LevelWarn)
	}
	if got := attrInt64(t, record, "gapHours"); got != 3 {
		t.Errorf("log gapHours = %d, want 3", got)
	}
	if got := attrInt64(t, record, "hour"); got != cursorHour {
		t.Errorf("log hour = %d, want %d", got, cursorHour)
	}
}

func TestRunOnce_feedCursorOnOurHour_staysAtDebug(t *testing.T) {
	// next_change_id == our cursor is the steady state of a caught-up walk and
	// happens on every tick, so it must not reach an operator's warn stream.
	fetcher := newFetcher(neverPublished())
	runner, logs := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	record := recordWithMessage(t, logs, "currency-exchange: hour not published yet")
	if record.Level != slog.LevelDebug {
		t.Errorf("log level = %v, want %v", record.Level, slog.LevelDebug)
	}
	assertNoRecordWithMessage(t, logs, "currency-exchange: feed moved past the cursor")
}

func TestRunOnce_someMarketsSkipped_warnsSeparatelyFromTheStoredHourLine(t *testing.T) {
	// Normalize's own skip warning goes to the default logger; redirect it so it
	// stays out of this test's output.
	captureLogs(t)

	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(func(hour int64) (*HourPayload, error) {
		if hour >= limit {
			return nil, &ErrNotPublished{NextChangeID: hour}
		}
		return partlyMalformedHour(hour), nil
	})
	store := storeAt(cursorHour)
	runner, logs := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(context.Background())
	if err != nil {
		t.Fatalf("error = %v, want nil — one bad market is not schema drift", err)
	}
	if processed != 1 {
		t.Errorf("processed = %d, want 1 — the hour is stored despite the skip", processed)
	}
	if got := len(store.inserts()[0].rows); got != 1 {
		t.Errorf("stored %d rows, want 1 — the malformed market is dropped", got)
	}

	record := recordWithMessage(t, logs, "currency-exchange: some markets skipped as malformed")
	if record.Level != slog.LevelWarn {
		t.Errorf("log level = %v, want %v", record.Level, slog.LevelWarn)
	}
	if got := attrInt64(t, record, "skipped"); got != 1 {
		t.Errorf("log skipped = %d, want 1", got)
	}
	if got := attrInt64(t, record, "hour"); got != cursorHour {
		t.Errorf("log hour = %d, want %d", got, cursorHour)
	}
	if got := attrInt64(t, record, "markets"); got != 2 {
		t.Errorf("log markets = %d, want 2 — the skip count needs its denominator", got)
	}
}

func TestRunOnce_noMarketSkipped_logsNoSkipWarning(t *testing.T) {
	limit := cursorHour + secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	runner, logs := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	if _, err := runner.RunOnce(context.Background()); err != nil {
		t.Fatalf("RunOnce: %v", err)
	}

	assertNoRecordWithMessage(t, logs, "currency-exchange: some markets skipped as malformed")
}

func TestRunOnce_perHourDelay_pacesTheHoursOfOnePass(t *testing.T) {
	const delay = 30 * time.Millisecond
	limit := cursorHour + 3*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{
		Scope:        runnerScope,
		PerHourDelay: delay,
	})

	start := time.Now()
	processed, err := runner.RunOnce(context.Background())
	elapsed := time.Since(start)

	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}
	if processed != 3 {
		t.Fatalf("processed = %d, want 3", processed)
	}
	// Three stored hours means at least two gaps between consecutive fetches;
	// the delay is never paid before the first hour.
	if want := 2 * delay; elapsed < want {
		t.Errorf("pass took %s, want at least %s — the hours were not paced", elapsed, want)
	}
}

func TestRunOnce_zeroPerHourDelay_walksWithoutPacing(t *testing.T) {
	limit := cursorHour + 3*secondsPerHour
	fetcher := newFetcher(publishedThrough(limit, runnerScope.ID()))
	runner, _ := newRunner(t, fetcher, storeAt(cursorHour), &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	start := time.Now()
	processed, err := runner.RunOnce(context.Background())
	elapsed := time.Since(start)

	if err != nil {
		t.Fatalf("RunOnce: %v", err)
	}
	if processed != 3 {
		t.Fatalf("processed = %d, want 3", processed)
	}
	// The zero value is "no pacing": NewRunner must not substitute a default the
	// way it does for Tick and Bootstrap. 10ms is far below the 250ms the
	// collector wiring passes, and far above three in-memory fake round trips.
	if elapsed > 10*time.Millisecond {
		t.Errorf("unpaced pass took %s, want well under 10ms", elapsed)
	}
}

func TestRunOnce_contextCancelledMidWalk_stopsQuietlyKeepingTheStoredHours(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Hour one succeeds; the fetch of hour two is cancelled under the runner the
	// way a shutdown cancels it.
	fetcher := newFetcher(func(hour int64) (*HourPayload, error) {
		if hour == cursorHour {
			return publishedHour(hour, runnerScope.ID()), nil
		}
		cancel()
		return nil, ctx.Err()
	})
	store := storeAt(cursorHour)
	runner, logs := newRunner(t, fetcher, store, &fakePublisher{}, RunnerConfig{Scope: runnerScope})

	processed, err := runner.RunOnce(ctx)

	if err != nil {
		t.Fatalf("error = %v, want nil — shutdown is not a feed failure", err)
	}
	if processed != 1 {
		t.Errorf("processed = %d, want 1 — the hour stored before the cancellation still counts", processed)
	}
	if store.insertCount() != 1 {
		t.Errorf("InsertHour called %d times, want 1", store.insertCount())
	}
	assertNoWarnRecords(t, logs)
}
