//go:build integration

package lab

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// countingRepository counts the statements its pool sends; a warm-up keeps
// connect-time statements out of the count.
func countingRepository(t *testing.T, pool *pgxpool.Pool) (*Repository, *statementCounter) {
	t.Helper()
	counted, counter := countingPool(t, pool.Config().ConnString())
	if _, err := counted.Exec(context.Background(), "SELECT 1"); err != nil {
		t.Fatalf("warm counting pool: %v", err)
	}
	counter.reset()
	return NewRepository(counted), counter
}

// baseCountingSource counts the GCP base lookups, so a statement count can be
// split into ranking and base-price statements.
type baseCountingSource struct {
	*Repository
	lookups atomic.Int32
}

func (s *baseCountingSource) CollectiveBasePrices(ctx context.Context, scope league.Scope, names []string) (map[string]float64, error) {
	s.lookups.Add(1)
	return s.Repository.CollectiveBasePrices(ctx, scope, names)
}

// rankingLeague registers a league holding both rankings' corpora and removes
// them all on cleanup.
func rankingLeague(t *testing.T, pool *pgxpool.Pool, id string) league.Scope {
	t.Helper()
	scope := collectiveLeague(t, pool, id)
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), "DELETE FROM dedication_snapshots WHERE league = $1", id); err != nil {
			t.Logf("cleanup warning: delete dedication_snapshots for %q: %v", id, err)
		}
	})
	return scope
}

// seedRankingCorpora seeds 30 Normal 20/20 rows on distinct bases (each with a
// priced 20/0 base row, the GCP lookup's input) and 30 Dedication gems.
func seedRankingCorpora(t *testing.T, pool *pgxpool.Pool, scope league.Scope, at time.Time) {
	t.Helper()
	var rows []collectiveTransfigureRow
	for i := 0; i < 30; i++ {
		base := fmt.Sprintf("Base %02d", i)
		rows = append(rows,
			collectiveTransfigureRow{name: fmt.Sprintf("Gem %02d", i), base: base, roi: float64(1000 - i)},
			collectiveTransfigureRow{name: fmt.Sprintf("Gem %02d", i), base: base, variant: "20", basePrice: float64(5 + i), roi: 1},
		)
	}
	seedCollective(t, pool, scope, at, rows, fullSellability(rows), nil)
	seedDedication(t, pool, scope, at, pricedDedicationGems(30), blueSkillCost)
}

// burst runs n calls of f released together and returns their results.
func burst[T any](n int, f func() T) []T {
	out := make([]T, n)
	start := make(chan struct{})
	var wg sync.WaitGroup
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			<-start
			out[i] = f()
		}(i)
	}
	close(start)
	wg.Wait()
	return out
}

type collectiveOutcome struct {
	got CollectiveQueryResult
	err error
}

type pageOutcome struct {
	got CollectivePage
	err error
}

var sharedLoadQuery = CollectiveQuery{Page: 1, PageSize: 20, Sort: CollectiveSortPrice, Variant: "20/20", SparkVariant: "20/20"}

var sharedLoadDedicationQuery = DedicationCollectiveQuery{Variant: "21/23c", Page: 1, PageSize: 20, Sort: CollectiveSortPrice}

// collectiveBurst runs 300 equal Normal requests and checks every body.
func collectiveBurst(t *testing.T, src CollectiveSource, cache *Cache, scope league.Scope, want CollectiveQueryResult) {
	t.Helper()
	for i, o := range burst(300, func() collectiveOutcome {
		got, err := QueryCollective(context.Background(), src, cache, scope, sharedLoadQuery)
		return collectiveOutcome{got, err}
	}) {
		if o.err != nil {
			t.Fatalf("request %d: %v", i, o.err)
		}
		if !reflect.DeepEqual(o.got, want) {
			t.Fatalf("request %d body = %+v, want the uncached body %+v", i, o.got, want)
		}
	}
}

func dedicationBurst(t *testing.T, repo *Repository, cache *Cache, scope league.Scope, want CollectivePage) {
	t.Helper()
	for i, o := range burst(300, func() pageOutcome {
		got, err := cache.For(scope).Rankings().DedicationPage(context.Background(), sharedLoadDedicationQuery,
			func(ctx context.Context) (CollectivePage, error) {
				return repo.DedicationCollectivePage(ctx, scope, sharedLoadDedicationQuery)
			})
		return pageOutcome{got, err}
	}) {
		if o.err != nil {
			t.Fatalf("dedication request %d: %v", i, o.err)
		}
		if !reflect.DeepEqual(o.got, want) {
			t.Fatalf("dedication request %d page = %+v, want the uncached page %+v", i, o.got, want)
		}
	}
}

func uncachedCollective(t *testing.T, repo *Repository, scope league.Scope) CollectiveQueryResult {
	t.Helper()
	want, err := QueryCollective(context.Background(), repo, nil, scope, sharedLoadQuery)
	if err != nil {
		t.Fatalf("uncached QueryCollective: %v", err)
	}
	if len(want.Results) == 0 || len(want.BasePriceIndex) == 0 {
		t.Fatalf("uncached body has %d rows and %d base prices, want both non-empty", len(want.Results), len(want.BasePriceIndex))
	}
	return want
}

// TestKeepHappyRankingAnswerSharedLoad: 300 equal cold requests per analysis
// generation share one ranking statement (and one GCP base lookup); warm
// requests run none; every body equals the uncached one.
func TestKeepHappyRankingAnswerSharedLoad(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := rankingLeague(t, pool, "POE410-SharedLoad")
	seedRankingCorpora(t, pool, scope, time.Now())
	plain := NewRepository(pool)
	want := uncachedCollective(t, plain, scope)
	wantDedication := dedicationPage(t, plain, scope, sharedLoadDedicationQuery)

	counted, counter := countingRepository(t, pool)
	repo := &baseCountingSource{Repository: counted}
	cache := NewCache(scope)
	// Warm sparklines keep the decoration off SQL; uncached and cached bodies
	// carry the same (empty) series.
	cache.For(scope).SetSparklinesByName(nil, nil, time.Now())

	for generation := 0; generation < 2; generation++ {
		// Sparklines are warm, so a Normal burst's statements are its ranking
		// statements plus its base lookups.
		normalCounts := func() (ranking, base int) {
			b := int(repo.lookups.Swap(0))
			return counter.reset() - b, b
		}
		collectiveBurst(t, repo, cache, scope, want)
		if ranking, base := normalCounts(); ranking != 1 || base != 1 {
			t.Errorf("generation %d cold Normal burst ran %d ranking and %d base-price statements, want 1 and 1", generation, ranking, base)
		}
		collectiveBurst(t, repo, cache, scope, want)
		if ranking, base := normalCounts(); ranking != 0 || base != 0 {
			t.Errorf("generation %d warm Normal burst ran %d ranking and %d base-price statements, want none", generation, ranking, base)
		}

		dedicationBurst(t, counted, cache, scope, wantDedication)
		if got := counter.reset(); got != 1 {
			t.Errorf("generation %d cold Dedication burst ran %d ranking statements, want 1", generation, got)
		}
		dedicationBurst(t, counted, cache, scope, wantDedication)
		if got := counter.reset(); got != 0 {
			t.Errorf("generation %d warm Dedication burst ran %d ranking statements, want none", generation, got)
		}

		cache.For(scope).Rankings().Publish(RankingInputTransfigure | RankingInputGemSnapshots)
	}
}

// revisionCase writes one ranking input through its analyzer publication point
// and names the mode whose answer the write must change.
type revisionCase struct {
	name       string
	dedication bool
	write      func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time)
}

func TestKeepCriticalRankingAnswerRevision(t *testing.T) {
	ctx := context.Background()
	for _, tc := range []revisionCase{
		{name: "SaveTransfigureResults", write: func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time) {
			if _, err := a.saveTransfigureResults(ctx, scope, []TransfigureResult{{
				Time: at, BaseName: "Base 00", TransfiguredName: "Gem 00", Variant: "20/20", BasePrice: 10,
				TransfiguredPrice: 9000, ROI: 8990, ROIPct: 89900, BaseListings: 20, TransfiguredListings: 20,
				GemColor: "BLUE", Confidence: "OK",
			}, {
				Time: at, BaseName: "Base 00", TransfiguredName: "Gem 00", Variant: "20", BasePrice: 12,
				TransfiguredPrice: 13, ROI: 1, ROIPct: 8, BaseListings: 20, TransfiguredListings: 20,
				GemColor: "BLUE", Confidence: "OK",
			}}); err != nil {
				t.Fatalf("save transfigure: %v", err)
			}
		}},
		{name: "SaveGemSignals", write: func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time) {
			if _, err := a.saveGemSignals(ctx, scope, []GemSignal{{Time: at, Name: "Gem 00", Variant: "20/20", Signal: "DUMPING", Sellability: 40}}); err != nil {
				t.Fatalf("save signals: %v", err)
			}
		}},
		{name: "SaveGemFeatures", write: func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time) {
			if _, err := a.saveGemFeatures(ctx, scope, []GemFeature{{Time: at, Name: "Gem 00", Variant: "20/20", CV: 0.75, HistPosition: 33, MarketDepth: 2}}); err != nil {
				t.Fatalf("save features: %v", err)
			}
		}},
		{name: "SaveDedicationResults", dedication: true, write: func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time) {
			if _, err := a.saveDedicationResults(ctx, scope, []DedicationResult{{
				Time: at, Variant: "21/23c", Color: "BLUE", GemType: "skill", Mode: "safe", Pool: 1, Winners: 1, InputCost: 400,
			}}); err != nil {
				t.Fatalf("save dedication: %v", err)
			}
		}},
		{name: "GemSnapshotEventWithoutRunDedication", dedication: true, write: func(t *testing.T, a *Analyzer, scope league.Scope, at time.Time) {
			// The collector commits outside the barrier; the accepted event is
			// what advances the generation. No Dedication analysis follows.
			seedDedication(t, a.repo.pool, scope, at, []dedicationGem{{name: "Ded 00", chaos: 7777}}, nil)
			a.cache.For(scope).Rankings().Publish(RankingInputGemSnapshots)
		}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			pool := labIntegrationPool(t)
			scope := rankingLeague(t, pool, "POE410-Rev-"+tc.name)
			at := time.Now().Add(-time.Hour)
			seedRankingCorpora(t, pool, scope, at)
			plain := NewRepository(pool)
			counted, counter := countingRepository(t, pool)
			repo := &baseCountingSource{Repository: counted}
			cache := NewCache(scope)
			cache.For(scope).SetSparklinesByName(nil, nil, time.Now())
			analyzer := NewAnalyzer(NewRepository(pool), nil, cache, nil)

			// Ranking statements: everything the counted pool sent except the
			// base lookups (sparklines are warm).
			rankingStatements := func() int { return counter.reset() - int(repo.lookups.Swap(0)) }
			read := func() any {
				got, err := QueryCollective(ctx, repo, cache, scope, sharedLoadQuery)
				if err != nil {
					t.Fatalf("QueryCollective: %v", err)
				}
				return got.Results
			}
			uncached := func() any { return uncachedCollective(t, plain, scope).Results }
			if tc.dedication {
				read = func() any {
					got, err := cache.For(scope).Rankings().DedicationPage(ctx, sharedLoadDedicationQuery, func(ctx context.Context) (CollectivePage, error) {
						return counted.DedicationCollectivePage(ctx, scope, sharedLoadDedicationQuery)
					})
					if err != nil {
						t.Fatalf("DedicationPage: %v", err)
					}
					return got
				}
				uncached = func() any { return dedicationPage(t, plain, scope, sharedLoadDedicationQuery) }
			}

			before := read()
			read()
			if got := rankingStatements(); got != 1 {
				t.Fatalf("two reads before the write ran %d ranking statements, want 1", got)
			}

			tc.write(t, analyzer, scope, at.Add(time.Minute))

			after := read()
			if got := rankingStatements(); got != 1 {
				t.Errorf("first read after the write ran %d ranking statements, want 1 (a fresh load)", got)
			}
			if reflect.DeepEqual(after, before) {
				t.Fatalf("answer unchanged by the write: the fixture must change it")
			}
			if want := uncached(); !reflect.DeepEqual(after, want) {
				t.Errorf("answer after the write = %+v, want the uncached %+v", after, want)
			}
		})
	}
}

// baseFailingSource fails the GCP base lookup while fail is set. Each lookup
// first waits until the whole burst waits on it, so one burst is one loader.
type baseFailingSource struct {
	*Repository
	cache   *Cache
	scope   league.Scope
	fail    atomic.Bool
	lookups atomic.Int32
}

func (s *baseFailingSource) CollectiveBasePrices(ctx context.Context, scope league.Scope, names []string) (map[string]float64, error) {
	s.lookups.Add(1)
	deadline := time.Now().Add(5 * time.Second)
	for s.cache.For(s.scope).Rankings().waiting(basePriceKeyPrefix) < 300 && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	if s.fail.Load() {
		return nil, errors.New("injected base price failure")
	}
	return s.Repository.CollectiveBasePrices(ctx, scope, names)
}

func TestKeepCriticalRankingBasePriceFailureRetry(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := rankingLeague(t, pool, "POE410-BaseRetry")
	seedRankingCorpora(t, pool, scope, time.Now())
	repo := NewRepository(pool)
	want := uncachedCollective(t, repo, scope)
	partial := want
	partial.BasePriceIndex = map[CollectiveBaseKey]float64{}

	cache := NewCache(scope)
	cache.For(scope).SetSparklinesByName(nil, nil, time.Now())
	src := &baseFailingSource{Repository: repo, cache: cache, scope: scope}

	src.fail.Store(true)
	collectiveBurst(t, src, cache, scope, partial)
	if n := src.lookups.Swap(0); n != 1 {
		t.Errorf("failing burst ran %d base lookups, want 1", n)
	}

	src.fail.Store(false)
	collectiveBurst(t, src, cache, scope, want)
	if n := src.lookups.Swap(0); n != 1 {
		t.Errorf("retry burst ran %d base lookups, want 1 (the failure must not be retained)", n)
	}
}
