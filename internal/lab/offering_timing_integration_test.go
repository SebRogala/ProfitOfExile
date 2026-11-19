//go:build integration

package lab

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// The writer-side twin (POE-158). The tick used to store offering timings only
// when the result was non-empty, which leaves the cache COLD for a league with
// no fragment rows — and a cold cache is exactly what sends every subsequent
// MarketOverview request back to the ten queries the tick just ran.
//
// The observable is what the cache holds afterwards, not what the call returns:
// an assertion on the returned count passes with the bug present, because the
// count is zero either way.
func TestRefreshOfferingTimings_StoresAnEmptyAnswerRatherThanLeavingTheCacheCold(t *testing.T) {
	pool := labIntegrationPool(t)
	// A league that owns no fragment_snapshots rows, so every offering's
	// current-price query returns NULL and the computed answer is empty.
	scope := league.Historical("POE158-NoFragments")
	cache := NewCache(scope)

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	n, err := RefreshOfferingTimings(ctx, NewRepository(pool), cache, scope)
	if err != nil {
		t.Fatalf("RefreshOfferingTimings: %v", err)
	}
	if n != 0 {
		t.Fatalf("computed %d offerings for a league with no fragment rows, want 0; the fixture is not exercising the empty answer", n)
	}

	stored := cache.For(scope).OfferingTiming()
	if stored == nil {
		t.Fatal("the tick stored nothing for an empty answer; the cache stays cold and every request recomputes it")
	}
	if string(stored) != "[]" {
		t.Errorf("cached offering timing = %s, want [] — `null` decodes to nil and reads back as never-stored", stored)
	}
}

func seedOfferingLeague(t *testing.T, prefix string) (*Repository, *statementCounter, league.Scope) {
	t.Helper()
	pool := labIntegrationPool(t)
	scope := retentionTestLeague(t, pool, prefix, "fragment_snapshots")
	now := time.Now().UTC()
	for _, id := range []string{"offer-gift", "offer-dedication"} {
		for h := 1; h <= 30; h++ {
			if _, err := pool.Exec(context.Background(),
				`INSERT INTO fragment_snapshots (league, time, fragment_id, chaos) VALUES ($1, $2, $3, $4)`,
				scope.ID(), now.Add(-time.Duration(h)*time.Hour), id, float64(10+h%7)); err != nil {
				t.Fatalf("seed %s: %v", id, err)
			}
		}
	}
	counted, counter := countingPool(t, pool.Config().ConnString())
	return NewRepository(counted), counter, scope
}

// burstServe runs n overlapping ServeOfferingTimings calls and returns each
// answer marshalled, with its stale flag.
func burstServe(t *testing.T, n int, serve func() ([]OfferingTiming, bool)) ([]string, []bool) {
	t.Helper()
	bodies, stale := make([]string, n), make([]bool, n)
	start := make(chan struct{})
	var wg sync.WaitGroup
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			<-start
			v, s := serve()
			b, _ := json.Marshal(v)
			bodies[i], stale[i] = string(b), s
		}(i)
	}
	close(start)
	wg.Wait()
	return bodies, stale
}

// POE-410: one request and a 300-request cold burst both cost the ten
// statements of one fill; a warm read costs none; every answer equals the
// unshared computation.
func TestKeepHappyOfferingTimingSharedFill(t *testing.T) {
	repo, counter, scope := seedOfferingLeague(t, "POE410-SharedFill")
	ctx := context.Background()
	direct, _ := json.Marshal(repo.OfferingTimings(ctx, scope))
	counter.reset()

	one := NewCache(scope)
	v, stale := ServeOfferingTimings(ctx, repo, one, scope)
	if got := counter.reset(); got != 10 {
		t.Fatalf("one cold read sent %d statements, want 10", got)
	}
	if b, _ := json.Marshal(v); string(b) != string(direct) || stale {
		t.Fatalf("one cold read = %s (stale %v), want %s fresh", b, stale, direct)
	}

	cache := NewCache(scope)
	bodies, stales := burstServe(t, 300, func() ([]OfferingTiming, bool) { return ServeOfferingTimings(ctx, repo, cache, scope) })
	if got := counter.reset(); got != 10 {
		t.Errorf("300 overlapping cold reads sent %d statements, want 10", got)
	}
	for i := range bodies {
		if bodies[i] != string(direct) || stales[i] {
			t.Fatalf("read %d = %s (stale %v), want %s fresh", i, bodies[i], stales[i], direct)
		}
	}

	ServeOfferingTimings(ctx, repo, cache, scope)
	if got := counter.reset(); got != 0 {
		t.Errorf("warm read sent %d statements, want 0", got)
	}

	pool := labIntegrationPool(t)
	empty := retentionTestLeague(t, pool, "POE410-SharedFillEmpty", "fragment_snapshots")
	emptyCache := NewCache(empty)
	burstServe(t, 300, func() ([]OfferingTiming, bool) { return ServeOfferingTimings(ctx, repo, emptyCache, empty) })
	if got := counter.reset(); got != 2 {
		t.Errorf("300 overlapping cold reads without current prices sent %d statements, want 2", got)
	}
}

// POE-410 C1: a failed fill is never cached. Without a prior value its partial
// answer is served stale, shared only within its burst, and the next burst
// loads again; a failed refresh keeps the prior complete value, served stale;
// a successful fill — an empty one included — publishes and clears stale.
func TestKeepCriticalOfferingFailureNotCached(t *testing.T) {
	scope := league.Historical("POE410-FillFailure")
	c := NewCache(scope).For(scope)
	ctx := context.Background()
	partial := []OfferingTiming{{Name: "Gift to the Goddess", FragmentID: "offer-gift", CurrentPrice: 7}}

	var loads atomic.Int32
	release := make(chan struct{})
	failing := func(context.Context) ([]OfferingTiming, error) {
		loads.Add(1)
		<-release
		return partial, errors.New("hourly query failed")
	}
	done := make(chan struct{})
	var bodies []string
	var stales []bool
	go func() {
		bodies, stales = burstServe(t, 300, func() ([]OfferingTiming, bool) { return serveOfferingTimings(ctx, failing, c) })
		close(done)
	}()
	for c.offeringFill.waiting() < 300 {
		time.Sleep(time.Millisecond)
	}
	close(release)
	<-done
	if n := loads.Load(); n != 1 {
		t.Errorf("a 300-read failed burst loaded %d times, want 1", n)
	}
	want, _ := json.Marshal(partial)
	for i := range bodies {
		if bodies[i] != string(want) || !stales[i] {
			t.Fatalf("read %d = %s (stale %v), want the partial %s stale", i, bodies[i], stales[i], want)
		}
	}
	if data, _ := c.OfferingTimingState(); data != nil {
		t.Fatalf("a failed fill cached %s", data)
	}

	var okLoads atomic.Int32
	complete := []OfferingTiming{{Name: "Gift to the Goddess", FragmentID: "offer-gift", CurrentPrice: 9}}
	succeeding := func(context.Context) ([]OfferingTiming, error) { okLoads.Add(1); return complete, nil }
	if _, stale := serveOfferingTimings(ctx, succeeding, c); stale || okLoads.Load() != 1 {
		t.Fatalf("the read after a failed burst: stale %v, loads %d; want a fresh load", stale, okLoads.Load())
	}
	wantComplete, _ := json.Marshal(complete)
	if data, stale := c.OfferingTimingState(); string(data) != string(wantComplete) || stale {
		t.Fatalf("after a successful fill the cache holds %s (stale %v), want %s fresh", data, stale, wantComplete)
	}

	immediate := func(context.Context) ([]OfferingTiming, error) { return partial, errors.New("sparkline query failed") }
	if _, err := c.offeringFill.do(ctx, immediate, publishOfferingTimings(c), nil); err == nil {
		t.Fatal("a failed refresh reported no error")
	}
	v, stale := serveOfferingTimings(ctx, immediate, c)
	if b, _ := json.Marshal(v); string(b) != string(wantComplete) || !stale {
		t.Fatalf("after a failed refresh: %s (stale %v), want the prior %s stale", b, stale, wantComplete)
	}

	none := func(context.Context) ([]OfferingTiming, error) { return []OfferingTiming{}, nil }
	if _, err := c.offeringFill.do(ctx, none, publishOfferingTimings(c), nil); err != nil {
		t.Fatalf("successful empty refresh: %v", err)
	}
	if data, stale := c.OfferingTimingState(); string(data) != "[]" || stale {
		t.Errorf("after a successful empty refresh the cache holds %s (stale %v), want [] fresh", data, stale)
	}
}
