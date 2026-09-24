package main

import (
	"context"
	"errors"
	"reflect"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
	"profitofexile/internal/server"
)

type fakeRuntimeAnalyzer struct {
	mu         sync.Mutex
	calls      []string
	errs       map[string]error
	started    chan string
	finished   chan string
	releaseFor map[string]<-chan struct{}
	startGates map[string]<-chan struct{}
	startHooks map[string]func()
}

func (f *fakeRuntimeAnalyzer) record(name string) error {
	if gate := f.startGates[name]; gate != nil {
		<-gate
	}
	if hook := f.startHooks[name]; hook != nil {
		hook()
	}
	f.mu.Lock()
	f.calls = append(f.calls, name)
	err := f.errs[name]
	f.mu.Unlock()
	if f.started != nil {
		f.started <- name
	}
	if release := f.releaseFor[name]; release != nil {
		<-release
	}
	if f.finished != nil {
		f.finished <- name
	}
	return err
}

func (f *fakeRuntimeAnalyzer) RunTransfigure(context.Context, league.Scope) error {
	return f.record("transfigure")
}

func (f *fakeRuntimeAnalyzer) RunQuality(context.Context, league.Scope) error {
	return f.record("quality")
}

func (f *fakeRuntimeAnalyzer) RecomputeLatestV2(context.Context, league.Scope) error {
	return f.record("startup-v2")
}

func (f *fakeRuntimeAnalyzer) RunV2(context.Context, league.Scope) error {
	return f.record("event-v2")
}

func (f *fakeRuntimeAnalyzer) RunFont(context.Context, league.Scope) error {
	return f.record("font")
}

func (f *fakeRuntimeAnalyzer) RunDoubleCorrupt(context.Context, league.Scope) error {
	return f.record("double-corrupt")
}

func (f *fakeRuntimeAnalyzer) RunDedication(context.Context, league.Scope) error {
	return f.record("dedication")
}

func (f *fakeRuntimeAnalyzer) snapshot() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]string(nil), f.calls...)
}

func (f *fakeRuntimeAnalyzer) waitForCall(t *testing.T, want string) {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		for _, got := range f.snapshot() {
			if got == want {
				return
			}
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("timed out waiting for analyzer call %q; calls = %v", want, f.snapshot())
}

type fakeRuntimeService struct {
	mu       sync.Mutex
	triggers int
	events   [][]byte
	eventsCh chan []byte
	started  chan struct{}
	finished chan struct{}
	release  <-chan struct{}
}

func (f *fakeRuntimeService) Trigger(context.Context) {
	f.mu.Lock()
	f.triggers++
	f.mu.Unlock()
	if f.started != nil {
		f.started <- struct{}{}
	}
	if f.release != nil {
		<-f.release
	}
	if f.finished != nil {
		f.finished <- struct{}{}
	}
}

func (f *fakeRuntimeService) HandleEvent(_ context.Context, raw []byte) {
	f.mu.Lock()
	copyRaw := append([]byte(nil), raw...)
	f.events = append(f.events, copyRaw)
	f.mu.Unlock()
	if f.eventsCh != nil {
		f.eventsCh <- copyRaw
	}
}

func (f *fakeRuntimeService) waitForEvent(t *testing.T) []byte {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		f.mu.Lock()
		if len(f.events) > 0 {
			raw := append([]byte(nil), f.events[0]...)
			f.mu.Unlock()
			return raw
		}
		f.mu.Unlock()
		time.Sleep(time.Millisecond)
	}
	f.mu.Lock()
	defer f.mu.Unlock()
	t.Fatalf("timed out waiting for service event; events = %d", len(f.events))
	return nil
}

type fakeRuntimeSubscriber struct {
	started  chan struct{}
	stopped  chan struct{}
	ctx      context.Context
	callback func(server.MercureEvent)
}

func (f *fakeRuntimeSubscriber) Run(ctx context.Context) {
	f.ctx = ctx
	close(f.started)
	<-ctx.Done()
	close(f.stopped)
}

func waitForStrings(t *testing.T, ch <-chan string, wants []string, label string) {
	t.Helper()
	wantSet := make(map[string]struct{}, len(wants))
	for _, want := range wants {
		wantSet[want] = struct{}{}
	}
	deadline := time.NewTimer(2 * time.Second)
	defer deadline.Stop()
	for len(wantSet) > 0 {
		select {
		case got := <-ch:
			if _, ok := wantSet[got]; !ok {
				t.Fatalf("unexpected %s notification %q; remaining = %v", label, got, wantSet)
			}
			delete(wantSet, got)
		case <-deadline.C:
			t.Fatalf("timed out waiting for %s; remaining = %v", label, wantSet)
		}
	}
}

func waitForStringSubset(t *testing.T, ch <-chan string, wants []string, label string) {
	t.Helper()
	wantSet := make(map[string]struct{}, len(wants))
	for _, want := range wants {
		wantSet[want] = struct{}{}
	}
	deadline := time.NewTimer(2 * time.Second)
	defer deadline.Stop()
	for len(wantSet) > 0 {
		select {
		case got := <-ch:
			delete(wantSet, got)
		case <-deadline.C:
			t.Fatalf("timed out waiting for %s; remaining = %v", label, wantSet)
		}
	}
}

func waitForStruct(t *testing.T, ch <-chan struct{}, label string) {
	t.Helper()
	select {
	case <-ch:
	case <-time.After(2 * time.Second):
		t.Fatalf("timed out waiting for %s", label)
	}
}

type fakeRuntimeTimer struct {
	mu        sync.Mutex
	callback  func()
	stopCalls int
	fired     bool
}

func (f *fakeRuntimeTimer) Stop() bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.stopCalls++
	return !f.fired
}

func (f *fakeRuntimeTimer) fire() {
	f.mu.Lock()
	f.fired = true
	callback := f.callback
	f.mu.Unlock()
	callback()
}

func (f *fakeRuntimeTimer) stops() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.stopCalls
}

type fakeRuntimeLock struct {
	mu       sync.Mutex
	releases int
}

func (f *fakeRuntimeLock) Release() {
	f.mu.Lock()
	f.releases++
	f.mu.Unlock()
}

func runtimeScope() league.Scope {
	return league.Historical("Mirage")
}

func stampedEndpoint(endpoint string) string {
	return `{"league":"Mirage","leagueRevision":0,"endpoint":"` + endpoint + `"}`
}

func TestRecomputeRuntime_startStartsSubscriberWithTopics(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.mercureURL = "http://mercure.test/.well-known/mercure"
	runtime.subscriberKey = "subscriber-key"
	subscriber := &fakeRuntimeSubscriber{
		started: make(chan struct{}),
		stopped: make(chan struct{}),
	}
	var gotURL, gotKey string
	var gotTopics []string
	runtime.subscriberFactory = func(url string, topics []string, key string, callback func(server.MercureEvent)) runtimeSubscriber {
		gotURL = url
		gotTopics = append([]string(nil), topics...)
		gotKey = key
		subscriber.callback = callback
		return subscriber
	}
	defer runtime.Close()

	startDone := make(chan struct{})
	go func() {
		runtime.Start()
		close(startDone)
	}()
	select {
	case <-startDone:
	case <-time.After(2 * time.Second):
		t.Fatal("Start waited for subscriber.Run")
	}
	waitForStruct(t, subscriber.started, "subscriber.Run start")

	wantTopics := []string{
		"poe/collector/gems",
		"poe/collector/currency",
		"poe/collector/fragments",
		"poe/collector/trade-tick",
		"poe/admin/recompute",
		"poe/collector/currency-exchange",
		"poe/collector/items/incursion-temple",
		"poe/collector/items/vial",
		"poe/collector/items/unique-armour",
		"poe/collector/items/unique-accessory",
		"poe/collector/items/unique-weapon",
		"poe/collector/items/unique-jewel",
		"poe/collector/items/unique-flask",
	}
	if gotURL != runtime.mercureURL || gotKey != runtime.subscriberKey {
		t.Fatalf("subscriber wiring = (%q, %q), want (%q, %q)", gotURL, gotKey, runtime.mercureURL, runtime.subscriberKey)
	}
	if !reflect.DeepEqual(gotTopics, wantTopics) {
		t.Fatalf("subscriber topics = %v, want %v", gotTopics, wantTopics)
	}
	if subscriber.callback == nil {
		t.Fatal("subscriber callback is nil")
	}

	runtime.Close()
	waitForStruct(t, subscriber.stopped, "subscriber cancellation")
}

func TestRecomputeRuntime_startReturnsWhileStartupWorkBlocked(t *testing.T) {
	release := make(chan struct{})
	released := false
	defer func() {
		if !released {
			close(release)
		}
	}()
	analyzer := &fakeRuntimeAnalyzer{
		started:  make(chan string, 6),
		finished: make(chan string, 6),
		releaseFor: map[string]<-chan struct{}{
			"startup-v2":     release,
			"font":           release,
			"double-corrupt": release,
			"dedication":     release,
			"transfigure":    release,
			"quality":        release,
		},
	}
	service := &fakeRuntimeService{
		started:  make(chan struct{}, 2),
		finished: make(chan struct{}, 2),
		release:  release,
	}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer
	runtime.exchange = service
	runtime.temple = service

	startDone := make(chan struct{})
	go func() {
		runtime.Start()
		close(startDone)
	}()
	select {
	case <-startDone:
	case <-time.After(2 * time.Second):
		t.Fatal("Start waited for blocked startup work")
	}
	waitForStringSubset(t, analyzer.started, []string{"startup-v2", "transfigure", "quality"}, "startup analyzer work")
	waitForStruct(t, service.started, "first startup service work")
	waitForStruct(t, service.started, "second startup service work")

	close(release)
	released = true
	waitForStrings(t, analyzer.finished, []string{"startup-v2", "font", "double-corrupt", "dedication", "transfigure", "quality"}, "startup analyzer release")
	for range 2 {
		waitForStruct(t, service.finished, "startup service release")
	}
}

func TestRecomputeRuntime_rejectsStaleCollectorEvent(t *testing.T) {
	analyzer := &fakeRuntimeAnalyzer{}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer
	createdTimers := 0
	runtime.timerFactory = func(time.Duration, func()) runtimeTimer {
		createdTimers++
		return &fakeRuntimeTimer{}
	}

	runtime.dispatch(server.MercureEvent{
		Topic: "poe/collector/gems",
		Data:  `{"league":"Mirage","leagueRevision":1,"endpoint":"ninja_gems"}`,
	})

	if got := analyzer.snapshot(); len(got) != 0 {
		t.Fatalf("stale collector event ran analyzer calls %v", got)
	}
	if createdTimers != 0 {
		t.Fatalf("stale collector event created %d delayed timers, want 0", createdTimers)
	}
}

func TestRecomputeRuntime_rejectsStaleEnabledTradeTick(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	tradeWork := make(chan []byte, 1)
	runtime.tradeTick = func(_ context.Context, raw []byte) {
		tradeWork <- raw
	}

	runtime.dispatch(server.MercureEvent{
		Topic: "poe/collector/trade-tick",
		Data:  `{"league":"Mirage","leagueRevision":1}`,
	})

	if got := runtime.eventGuard.Rejected(); got != 1 {
		t.Fatalf("stale trade rejection count = %d, want 1", got)
	}
	select {
	case raw := <-tradeWork:
		t.Fatalf("stale trade event reached feature work: %s", raw)
	case <-time.After(100 * time.Millisecond):
	}
}

func TestRecomputeRuntime_rejectsStaleExchangeEvent(t *testing.T) {
	service := &fakeRuntimeService{eventsCh: make(chan []byte, 1)}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.exchange = service

	runtime.dispatch(server.MercureEvent{
		Topic: exchange.Topic,
		Data:  `{"league":"Mirage","leagueRevision":1}`,
	})

	if got := runtime.eventGuard.Rejected(); got != 1 {
		t.Fatalf("stale exchange rejection count = %d, want 1", got)
	}
	select {
	case raw := <-service.eventsCh:
		t.Fatalf("stale exchange event reached feature work: %s", raw)
	case <-time.After(100 * time.Millisecond):
	}
}

func TestRecomputeRuntime_adminTopicBypassesCollectorStampGuard(t *testing.T) {
	analyzer := &fakeRuntimeAnalyzer{}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer

	runtime.dispatch(server.MercureEvent{Topic: "poe/admin/recompute"})
	analyzer.waitForCall(t, "dedication")

	want := []string{"transfigure", "quality", "startup-v2", "font", "double-corrupt", "dedication"}
	if got := analyzer.snapshot(); len(got) != len(want) {
		t.Fatalf("admin calls = %v, want %v", got, want)
	}
	for i := range want {
		if analyzer.snapshot()[i] != want[i] {
			t.Fatalf("admin call %d = %q, want %q", i, analyzer.snapshot()[i], want[i])
		}
	}
}

func TestRecomputeRuntime_disabledTradeWarnsOnce(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	warnings := 0
	runtime.warnTradeDisabled = func() { warnings++ }

	for range 2 {
		runtime.dispatch(server.MercureEvent{Topic: "poe/collector/trade-tick", Data: "{}"})
	}

	if warnings != 1 {
		t.Fatalf("disabled-trade warnings = %d, want 1", warnings)
	}
}

func TestRecomputeRuntime_routesExchangeTopic(t *testing.T) {
	service := &fakeRuntimeService{}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.exchange = service
	raw := `{"league":"Mirage","leagueRevision":0}`

	runtime.dispatch(server.MercureEvent{Topic: exchange.Topic, Data: raw})
	if got := string(service.waitForEvent(t)); got != raw {
		t.Fatalf("exchange event = %s, want %s", got, raw)
	}
}

func TestRecomputeRuntime_routesTempleEndpoint(t *testing.T) {
	service := &fakeRuntimeService{}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.temple = service
	raw := stampedEndpoint("ninja-vial")

	runtime.dispatch(server.MercureEvent{Topic: "poe/collector/items/vial", Data: raw})
	if got := string(service.waitForEvent(t)); got != raw {
		t.Fatalf("temple event = %s, want %s", got, raw)
	}
}

func TestRecomputeRuntime_eventV2ErrorStopsDependentStages(t *testing.T) {
	analyzer := &fakeRuntimeAnalyzer{errs: map[string]error{"event-v2": errors.New("v2 failed")}}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer

	runtime.runGemV2Chain(context.Background())

	if got, want := analyzer.snapshot(), []string{"event-v2"}; len(got) != len(want) || got[0] != want[0] {
		t.Fatalf("event pipeline calls = %v, want %v", got, want)
	}
}

func TestRecomputeRuntime_eventV2SuccessPreservesAnalysisOrder(t *testing.T) {
	analyzer := &fakeRuntimeAnalyzer{}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer

	runtime.runGemV2Chain(context.Background())

	want := []string{"event-v2", "font", "double-corrupt", "dedication"}
	if got := analyzer.snapshot(); !reflect.DeepEqual(got, want) {
		t.Fatalf("successful event pipeline calls = %v, want %v", got, want)
	}
}

func TestRecomputeRuntime_transfigureStartsWhileQualityBlocked(t *testing.T) {
	qualityStarted := make(chan struct{})
	release := make(chan struct{})
	released := false
	defer func() {
		if !released {
			close(release)
		}
	}()
	analyzer := &fakeRuntimeAnalyzer{
		finished: make(chan string, 8),
		releaseFor: map[string]<-chan struct{}{
			"quality": release,
		},
		startHooks: map[string]func(){
			"quality": func() { close(qualityStarted) },
		},
		startGates: map[string]<-chan struct{}{
			"transfigure": qualityStarted,
		},
	}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer
	runtime.timerFactory = func(time.Duration, func()) runtimeTimer {
		return &fakeRuntimeTimer{}
	}

	runtime.dispatch(server.MercureEvent{Topic: "poe/collector/gems", Data: stampedEndpoint("ninja_gems")})
	analyzer.waitForCall(t, "quality")
	analyzer.waitForCall(t, "transfigure")

	close(release)
	released = true
	waitForStrings(t, analyzer.finished,
		[]string{"transfigure", "quality", "event-v2", "font", "double-corrupt", "dedication"},
		"quality release")
}

func TestRecomputeRuntime_qualityStartsWhileTransfigureBlocked(t *testing.T) {
	transfigureStarted := make(chan struct{})
	release := make(chan struct{})
	released := false
	defer func() {
		if !released {
			close(release)
		}
	}()
	analyzer := &fakeRuntimeAnalyzer{
		finished: make(chan string, 8),
		releaseFor: map[string]<-chan struct{}{
			"transfigure": release,
		},
		startHooks: map[string]func(){
			"transfigure": func() { close(transfigureStarted) },
		},
		startGates: map[string]<-chan struct{}{
			"quality": transfigureStarted,
		},
	}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer
	runtime.timerFactory = func(time.Duration, func()) runtimeTimer {
		return &fakeRuntimeTimer{}
	}

	runtime.dispatch(server.MercureEvent{Topic: "poe/collector/gems", Data: stampedEndpoint("ninja_gems")})
	analyzer.waitForCall(t, "transfigure")
	analyzer.waitForCall(t, "quality")

	close(release)
	released = true
	waitForStrings(t, analyzer.finished,
		[]string{"transfigure", "quality", "event-v2", "font", "double-corrupt", "dedication"},
		"transfigure release")
}

func TestRecomputeRuntime_startupV2ErrorContinuesDependentStages(t *testing.T) {
	analyzer := &fakeRuntimeAnalyzer{errs: map[string]error{"startup-v2": errors.New("v2 failed")}}
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runtime.analyzer = analyzer

	runtime.startupV2AndFont()

	want := []string{"startup-v2", "font", "double-corrupt", "dedication"}
	if got := analyzer.snapshot(); len(got) != len(want) {
		t.Fatalf("startup pipeline calls = %v, want %v", got, want)
	}
	for i := range want {
		if analyzer.snapshot()[i] != want[i] {
			t.Fatalf("startup call %d = %q, want %q", i, analyzer.snapshot()[i], want[i])
		}
	}
}

func TestRecomputeRuntime_replacesPendingGemTimer(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	var timers []*fakeRuntimeTimer
	var delays []time.Duration
	runtime.timerFactory = func(delay time.Duration, callback func()) runtimeTimer {
		delays = append(delays, delay)
		timer := &fakeRuntimeTimer{callback: callback}
		timers = append(timers, timer)
		return timer
	}

	runtime.dispatch(server.MercureEvent{Topic: "poe/collector/gems", Data: stampedEndpoint("ninja_gems")})
	runtime.dispatch(server.MercureEvent{Topic: "poe/collector/gems", Data: stampedEndpoint("ninja_gems")})

	if len(timers) != 2 {
		t.Fatalf("created timers = %d, want 2", len(timers))
	}
	for i, got := range delays {
		if got != 15*time.Minute {
			t.Fatalf("timer %d delay = %s, want %s", i, got, 15*time.Minute)
		}
	}
	if got := timers[0].stops(); got != 1 {
		t.Fatalf("replaced timer Stop calls = %d, want 1", got)
	}
}

func TestRecomputeRuntime_closeStopsPendingTimer(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	var timer *fakeRuntimeTimer
	runtime.timerFactory = func(_ time.Duration, callback func()) runtimeTimer {
		timer = &fakeRuntimeTimer{callback: callback}
		return timer
	}
	runtime.scheduleDelayedRecompute()

	runtime.Close()

	if got := timer.stops(); got != 1 {
		t.Fatalf("pending timer Stop calls = %d, want 1", got)
	}
}

func TestRecomputeRuntime_closePreservesStartedDelayedRun(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	lock := &fakeRuntimeLock{}
	runs := 0
	entered := make(chan struct{})
	release := make(chan struct{})
	runtime.prepareDelayed = func(context.Context, league.Scope) (runtimeLock, string, error) {
		return lock, "", nil
	}
	runtime.runV2 = func(context.Context, league.Scope) error {
		close(entered)
		<-release
		runs++
		return nil
	}
	var timer *fakeRuntimeTimer
	runtime.timerFactory = func(_ time.Duration, callback func()) runtimeTimer {
		timer = &fakeRuntimeTimer{callback: callback}
		return timer
	}
	runtime.scheduleDelayedRecompute()

	done := make(chan struct{})
	go func() {
		timer.fire()
		close(done)
	}()
	<-entered
	runtime.Close()
	close(release)
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("started delayed callback did not finish after release")
	}

	if runs != 1 {
		t.Fatalf("started delayed runs = %d, want 1", runs)
	}
	lock.mu.Lock()
	defer lock.mu.Unlock()
	if lock.releases != 1 {
		t.Fatalf("started delayed lock releases = %d, want 1", lock.releases)
	}
}

func TestRecomputeRuntime_delayedSkipDoesNotRunV2(t *testing.T) {
	runtime := newRecomputeRuntime(context.Background(), runtimeScope())
	runs := 0
	runtime.prepareDelayed = func(context.Context, league.Scope) (runtimeLock, string, error) {
		return nil, "league data lock held by another recompute", nil
	}
	runtime.runV2 = func(context.Context, league.Scope) error {
		runs++
		return nil
	}

	runtime.runDelayedRecompute()

	if runs != 0 {
		t.Fatalf("skipped delayed runs = %d, want 0", runs)
	}
}

func TestAdaptDelayedLockResult_preservesNilSkip(t *testing.T) {
	lock, skip, err := adaptDelayedLockResult(nil, "league changed", nil)
	if lock != nil {
		t.Fatal("nil process lock became a non-nil runtime lock")
	}
	if skip != "league changed" || err != nil {
		t.Fatalf("adapted skip = (%v, %q, %v), want (nil, %q, nil)", lock, skip, err, "league changed")
	}
}
