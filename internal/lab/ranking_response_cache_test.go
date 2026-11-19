package lab

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

// countingLoader serves a page named after its tag and counts its calls.
type countingLoader struct {
	calls atomic.Int32
}

func (l *countingLoader) page(tag string) func(context.Context) (CollectivePage, error) {
	return func(context.Context) (CollectivePage, error) {
		l.calls.Add(1)
		return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: tag}}, Total: 1}, nil
	}
}

func pageTag(p CollectivePage) string {
	if len(p.Rows) == 0 {
		return ""
	}
	return p.Rows[0].TransfiguredName
}

func loadNormal(t *testing.T, rc *RankingAnswers, q CollectiveQuery, load func(context.Context) (CollectivePage, error)) CollectivePage {
	t.Helper()
	got, err := rc.NormalPage(context.Background(), q, load)
	if err != nil {
		t.Fatalf("NormalPage(%+v): %v", q, err)
	}
	return got
}

// eventually polls cond, failing after a second.
func eventually(t *testing.T, what string, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatalf("timed out waiting for %s", what)
		}
		time.Sleep(time.Millisecond)
	}
}

// notWithin asserts ch stays open for a short window.
func notWithin(t *testing.T, ch <-chan struct{}, what string) {
	t.Helper()
	select {
	case <-ch:
		t.Fatalf("%s happened, want it held", what)
	case <-time.After(30 * time.Millisecond):
	}
}

func TestRankingAnswers_DistinctQueriesNeverCollide(t *testing.T) {
	base := CollectiveQuery{Page: 1, PageSize: 20, Sort: CollectiveSortPrice}
	normal := []CollectiveQuery{base}
	for _, change := range []func(*CollectiveQuery){
		func(q *CollectiveQuery) { q.Sort = CollectiveSortROI },
		func(q *CollectiveQuery) { q.Sort = CollectiveSortWeightedROI },
		func(q *CollectiveQuery) { q.Variant = "20/20" },
		func(q *CollectiveQuery) { q.Color = "RED" },
		func(q *CollectiveQuery) { q.Budget = 50 },
		func(q *CollectiveQuery) { q.HideLowConfidence = true },
		func(q *CollectiveQuery) { q.Search = "arc" },
		func(q *CollectiveQuery) { q.Search = "arc|" },
		func(q *CollectiveQuery) { q.Page = 2 },
		func(q *CollectiveQuery) { q.PageSize = 21 },
		func(q *CollectiveQuery) { q.Legacy = true; q.Sort = CollectiveSortLegacyWeightedChaos },
	} {
		q := base
		change(&q)
		normal = append(normal, q)
	}
	dbase := DedicationCollectiveQuery{Variant: "21/23c", Page: 1, PageSize: 20, Sort: CollectiveSortPrice}
	dedication := []DedicationCollectiveQuery{dbase}
	for _, change := range []func(*DedicationCollectiveQuery){
		func(q *DedicationCollectiveQuery) { q.Variant = "21/20c" },
		func(q *DedicationCollectiveQuery) { q.Pool = "skill" },
		func(q *DedicationCollectiveQuery) { q.Pool = "transfigured" },
		func(q *DedicationCollectiveQuery) { q.Color = "BLUE" },
		func(q *DedicationCollectiveQuery) { q.Sort = CollectiveSortROIPct },
		func(q *DedicationCollectiveQuery) { q.Page = 3 },
		func(q *DedicationCollectiveQuery) { q.Search = "arc" },
		func(q *DedicationCollectiveQuery) { q.Legacy = true },
	} {
		q := dbase
		change(&q)
		dedication = append(dedication, q)
	}

	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	loader := &countingLoader{}
	for round := 0; round < 2; round++ {
		for i, q := range normal {
			if got := pageTag(loadNormal(t, rc, q, loader.page(fmt.Sprintf("normal %d", i)))); got != fmt.Sprintf("normal %d", i) {
				t.Errorf("round %d: normal query %+v answered %q", round, q, got)
			}
		}
		for i, q := range dedication {
			got, err := rc.DedicationPage(context.Background(), q, loader.page(fmt.Sprintf("dedication %d", i)))
			if err != nil {
				t.Fatalf("DedicationPage: %v", err)
			}
			if pageTag(got) != fmt.Sprintf("dedication %d", i) {
				t.Errorf("round %d: dedication query %+v answered %q", round, q, pageTag(got))
			}
		}
	}
	if want := int32(len(normal) + len(dedication)); loader.calls.Load() != want {
		t.Errorf("loads = %d, want %d (one per distinct query)", loader.calls.Load(), want)
	}
}

// A search answers its whole match set whatever the page asks for, so equal
// searches on different pages share one answer.
func TestRankingAnswers_SearchIgnoresPageInItsKey(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	loader := &countingLoader{}
	q := CollectiveQuery{Page: 1, PageSize: 20, Sort: CollectiveSortPrice, Search: "arc"}
	loadNormal(t, rc, q, loader.page("a"))
	q.Page, q.PageSize = 4, 50
	loadNormal(t, rc, q, loader.page("b"))
	if loader.calls.Load() != 1 {
		t.Errorf("loads = %d, want 1", loader.calls.Load())
	}
}

// A cached page answers only its own query: the next page of the same
// ranking still loads.
func TestRankingAnswers_SparsePageNeverMarksTheCorpusWarm(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	loader := &countingLoader{}
	loadNormal(t, rc, CollectiveQuery{Page: 1, PageSize: 20, Sort: CollectiveSortPrice}, loader.page("p1"))
	got := loadNormal(t, rc, CollectiveQuery{Page: 2, PageSize: 20, Sort: CollectiveSortPrice}, loader.page("p2"))
	if pageTag(got) != "p2" || loader.calls.Load() != 2 {
		t.Errorf("page 2 = %q after %d loads, want p2 from its own load", pageTag(got), loader.calls.Load())
	}
}

func pageQuery(n int) CollectiveQuery {
	return CollectiveQuery{Page: n, PageSize: 20, Sort: CollectiveSortPrice}
}

func TestRankingAnswers_EntryBoundEvictsLeastRecentlyUsed(t *testing.T) {
	rc := newRankingAnswers(3, rankingAnswerMaxBytes)
	loader := &countingLoader{}
	for n := 1; n <= 3; n++ {
		loadNormal(t, rc, pageQuery(n), loader.page("x"))
	}
	loadNormal(t, rc, pageQuery(1), loader.page("x")) // touch 1: 2 is now oldest
	loadNormal(t, rc, pageQuery(4), loader.page("x"))
	loader.calls.Store(0)

	loadNormal(t, rc, pageQuery(1), loader.page("x"))
	loadNormal(t, rc, pageQuery(3), loader.page("x"))
	loadNormal(t, rc, pageQuery(4), loader.page("x"))
	if n := loader.calls.Load(); n != 0 {
		t.Errorf("retained pages reloaded %d times, want 0", n)
	}
	loadNormal(t, rc, pageQuery(2), loader.page("x"))
	if n := loader.calls.Load(); n != 1 {
		t.Errorf("evicted page loaded %d times, want 1", n)
	}
}

func bigPage(tag string) func(context.Context) (CollectivePage, error) {
	return func(context.Context) (CollectivePage, error) {
		return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: tag, SellReason: strings.Repeat("x", 1000)}}}, nil
	}
}

func TestRankingAnswers_ByteBoundEvictsBeforeTheEntryBound(t *testing.T) {
	one, _ := bigPage("a")(context.Background())
	size := rankingPageBytes(one)
	if size < 1000 {
		t.Fatalf("page size estimate = %d, want it to count the 1000-byte string", size)
	}
	rc := newRankingAnswers(100, size*2+size/2)
	calls := 0
	load := func(context.Context) (CollectivePage, error) { calls++; return bigPage("a")(context.Background()) }
	for n := 1; n <= 3; n++ {
		loadNormal(t, rc, pageQuery(n), load)
	}
	calls = 0
	loadNormal(t, rc, pageQuery(2), load)
	loadNormal(t, rc, pageQuery(3), load)
	if calls != 0 {
		t.Errorf("newest two pages reloaded %d times, want 0", calls)
	}
	loadNormal(t, rc, pageQuery(1), load)
	if calls != 1 {
		t.Errorf("oldest page loaded %d times after the byte bound, want 1", calls)
	}
}

// An answer larger than the whole byte bound is served but never retained;
// equal requests still share its one in-flight load.
func TestRankingAnswers_OversizedAnswerIsSharedButNotRetained(t *testing.T) {
	one, _ := bigPage("a")(context.Background())
	rc := newRankingAnswers(100, rankingPageBytes(one)/2)
	release := make(chan struct{})
	var calls atomic.Int32
	load := func(context.Context) (CollectivePage, error) {
		calls.Add(1)
		<-release
		return bigPage("a")(context.Background())
	}
	q := pageQuery(1)
	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if got := loadNormal(t, rc, q, load); pageTag(got) != "a" {
				t.Errorf("waiter got %q, want a", pageTag(got))
			}
		}()
	}
	eventually(t, "ten waiters", func() bool { return rc.waiting(normalKeyPrefix) == 10 })
	close(release)
	wg.Wait()
	if calls.Load() != 1 {
		t.Errorf("ten equal requests ran %d loads, want 1", calls.Load())
	}
	loadNormal(t, rc, q, load)
	if calls.Load() != 2 {
		t.Errorf("oversized answer served from memory, want a fresh load (loads = %d)", calls.Load())
	}
}

// One waiter giving up neither cancels the shared load nor the others' answer.
func TestRankingAnswers_WaiterCancellationIsIndependent(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	release := make(chan struct{})
	var loadErr error
	load := func(ctx context.Context) (CollectivePage, error) {
		<-release
		loadErr = ctx.Err()
		return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: "a"}}}, nil
	}
	q := pageQuery(1)
	cancelled, cancel := context.WithCancel(context.Background())
	firstErr := make(chan error, 1)
	go func() {
		_, err := rc.NormalPage(cancelled, q, load)
		firstErr <- err
	}()
	eventually(t, "the first waiter", func() bool { return rc.waiting(normalKeyPrefix) == 1 })
	second := make(chan CollectivePage, 1)
	go func() { second <- loadNormal(t, rc, q, load) }()
	eventually(t, "the second waiter", func() bool { return rc.waiting(normalKeyPrefix) == 2 })

	cancel()
	if err := <-firstErr; !errors.Is(err, context.Canceled) {
		t.Errorf("cancelled waiter err = %v, want context.Canceled", err)
	}
	close(release)
	if got := <-second; pageTag(got) != "a" {
		t.Errorf("remaining waiter got %q, want a", pageTag(got))
	}
	if loadErr != nil {
		t.Errorf("shared load saw ctx err %v, want it to outlive the cancelled waiter", loadErr)
	}
}

func TestRankingAnswers_PublishInvalidatesOnlyTheAnswersReadingThatInput(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	normal, dedication, bases := &countingLoader{}, &countingLoader{}, atomic.Int32{}
	loadBases := func(context.Context, []string) (map[string]float64, error) {
		bases.Add(1)
		return map[string]float64{"Arc": 1}, nil
	}
	dq := DedicationCollectiveQuery{Variant: "21/23c", Page: 1, PageSize: 20, Sort: CollectiveSortPrice}
	readAll := func() {
		loadNormal(t, rc, pageQuery(1), normal.page("n"))
		if _, err := rc.DedicationPage(context.Background(), dq, dedication.page("d")); err != nil {
			t.Fatal(err)
		}
		if _, err := rc.BasePrices(context.Background(), []string{"Arc"}, loadBases); err != nil {
			t.Fatal(err)
		}
	}
	readAll()
	for _, tt := range []struct {
		input                      RankingInput
		normal, dedication, bases int32
	}{
		{RankingInputTransfigure, 1, 0, 1},
		{RankingInputGemSignals, 1, 0, 0},
		{RankingInputGemFeatures, 1, 0, 0},
		{RankingInputGemSnapshots, 0, 1, 0},
		{RankingInputDedication, 0, 1, 0},
	} {
		normal.calls.Store(0)
		dedication.calls.Store(0)
		bases.Store(0)
		rc.Publish(tt.input)
		readAll()
		readAll()
		if normal.calls.Load() != tt.normal || dedication.calls.Load() != tt.dedication || bases.Load() != tt.bases {
			t.Errorf("after publishing %d: loads normal %d dedication %d bases %d, want %d %d %d", tt.input,
				normal.calls.Load(), dedication.calls.Load(), bases.Load(), tt.normal, tt.dedication, tt.bases)
		}
	}
}

// A fill in progress when an event or write arrives cannot leave its
// (pre-event) answer serving afterwards, and the write waits for the fill.
func TestRankingAnswers_FillBeforePublishIsNotServedAfterIt(t *testing.T) {
	for _, tt := range []struct {
		name  string
		write func(rc *RankingAnswers, wrote chan<- struct{})
	}{
		{"event", func(rc *RankingAnswers, wrote chan<- struct{}) {
			rc.Publish(RankingInputGemSnapshots)
			close(wrote)
		}},
		{"commit", func(rc *RankingAnswers, wrote chan<- struct{}) {
			rc.Commit(RankingInputGemSnapshots, func() error { close(wrote); return nil })
		}},
	} {
		t.Run(tt.name, func(t *testing.T) {
			rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
			dq := DedicationCollectiveQuery{Variant: "21/23c", Page: 1, PageSize: 20, Sort: CollectiveSortPrice}
			release := make(chan struct{})
			done := make(chan CollectivePage, 1)
			go func() {
				got, _ := rc.DedicationPage(context.Background(), dq, func(context.Context) (CollectivePage, error) {
					<-release
					return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: "old"}}}, nil
				})
				done <- got
			}()
			eventually(t, "the fill", func() bool { return rc.waiting(dedicationKeyPrefix) == 1 })
			wrote := make(chan struct{})
			go tt.write(rc, wrote)
			notWithin(t, wrote, "the write during the fill")
			close(release)
			<-wrote
			if got := <-done; pageTag(got) != "old" {
				t.Errorf("fill answered %q, want old", pageTag(got))
			}
			got, err := rc.DedicationPage(context.Background(), dq, func(context.Context) (CollectivePage, error) {
				return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: "new"}}}, nil
			})
			if err != nil || pageTag(got) != "new" {
				t.Errorf("read after the write = %q (%v), want new", pageTag(got), err)
			}
		})
	}
}

// A request arriving while a write holds the barrier joins a flight of the
// old generation; once the write lands, new requests start their own and
// only that new answer is retained.
func TestRankingAnswers_OldGenerationFlightServesOnlyItsOwnWaiters(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	q := pageQuery(1)
	writing, finish := make(chan struct{}), make(chan struct{})
	committed := make(chan struct{})
	go func() {
		rc.Commit(RankingInputTransfigure, func() error { close(writing); <-finish; return nil })
		close(committed)
	}()
	<-writing
	oldAnswer := make(chan CollectivePage, 1)
	go func() {
		oldAnswer <- loadNormal(t, rc, q, func(context.Context) (CollectivePage, error) {
			return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: "old flight"}}}, nil
		})
	}()
	eventually(t, "the old-generation waiter", func() bool { return rc.waiting(normalKeyPrefix) == 1 })
	close(finish)
	<-committed

	newer := loadNormal(t, rc, q, func(context.Context) (CollectivePage, error) {
		return CollectivePage{Rows: []CollectiveResult{{TransfiguredName: "new flight"}}}, nil
	})
	if pageTag(newer) != "new flight" {
		t.Errorf("post-write request answered %q, want its own new flight", pageTag(newer))
	}
	if got := <-oldAnswer; pageTag(got) != "old flight" {
		t.Errorf("old waiter answered %q, want its own old flight", pageTag(got))
	}
	again := loadNormal(t, rc, q, func(context.Context) (CollectivePage, error) {
		t.Error("retained answer reloaded")
		return CollectivePage{}, nil
	})
	if pageTag(again) != "new flight" {
		t.Errorf("retained answer = %q, want new flight", pageTag(again))
	}
}

// A failed load is returned to its waiters and never retained.
func TestRankingAnswers_FailedLoadIsNotRetained(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	calls := 0
	load := func(context.Context, []string) (map[string]float64, error) {
		calls++
		if calls == 1 {
			return nil, errors.New("boom")
		}
		return map[string]float64{"Arc": 3}, nil
	}
	if _, err := rc.BasePrices(context.Background(), []string{"Arc"}, load); err == nil {
		t.Fatal("first lookup err = nil, want the load's error")
	}
	got, err := rc.BasePrices(context.Background(), []string{"Arc"}, load)
	if err != nil || got["Arc"] != 3 || calls != 2 {
		t.Errorf("retry = %v (%v) after %d loads, want Arc 3 from a second load", got, err, calls)
	}
}

// Rows handed out are the caller's: decorating them leaves the retained
// answer as loaded.
func TestRankingAnswers_ReturnedRowsAreACopy(t *testing.T) {
	rc := newRankingAnswers(rankingAnswerMaxEntries, rankingAnswerMaxBytes)
	loader := &countingLoader{}
	first := loadNormal(t, rc, pageQuery(1), loader.page("a"))
	first.Rows[0].TradeConfidenceNote = "decorated"
	if got := loadNormal(t, rc, pageQuery(1), loader.page("a")); got.Rows[0].TradeConfidenceNote != "" {
		t.Errorf("retained row carries %q, want the undecorated answer", got.Rows[0].TradeConfidenceNote)
	}
}
