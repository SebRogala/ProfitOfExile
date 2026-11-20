//go:build integration

package lab

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// fixedPageSource ranks a fixed page and answers the GCP base lookup without
// SQL, so a statement count over QueryCollective counts only sparkline reads.
type fixedPageSource struct {
	*Repository
	rows []CollectiveResult
}

func (s fixedPageSource) CollectivePage(context.Context, league.Scope, CollectiveQuery) (CollectivePage, error) {
	rows := make([]CollectiveResult, len(s.rows))
	copy(rows, s.rows)
	return CollectivePage{Rows: rows, Total: len(rows)}, nil
}

func (s fixedPageSource) CollectiveBasePrices(context.Context, league.Scope, []string) (map[string]float64, error) {
	return nil, nil
}

// overlappingPageSource holds each bulk read until joiners requests wait on
// the sparkline flight, so a burst overlaps the read whatever its speed.
type overlappingPageSource struct {
	fixedPageSource
	joiners int
}

func (s overlappingPageSource) SparklineSeries(ctx context.Context, scope league.Scope, keys []CollectiveSparklineKey, hours int) (map[CollectiveSparklineKey][]SparklinePoint, error) {
	for deadline := time.Now().Add(10 * time.Second); sparklineFallbackFlights.waiting() < s.joiners && time.Now().Before(deadline); {
		time.Sleep(time.Millisecond)
	}
	return s.Repository.SparklineSeries(ctx, scope, keys, hours)
}

// seedSparkPoints inserts one gem_snapshots row per age, timed against the
// database clock the sparkline read compares with.
func seedSparkPoints(t *testing.T, pool *pgxpool.Pool, scope league.Scope, name, variant string, corrupted bool, ages ...time.Duration) {
	t.Helper()
	for i, age := range ages {
		if _, err := pool.Exec(context.Background(), `
			INSERT INTO gem_snapshots
				(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
			VALUES ($1, date_trunc('second', NOW()) - make_interval(secs => $2), $3, $4, $5, true, $6, $7, 'BLUE')`,
			scope.ID(), age.Seconds(), name, variant, corrupted, float64(100+i), 10+i); err != nil {
			t.Fatalf("seed spark point %s %s: %v", name, variant, err)
		}
	}
}

// seedSparkRun bulk-inserts n points for one series, one second apart and
// ending a minute ago.
func seedSparkRun(t *testing.T, pool *pgxpool.Pool, scope league.Scope, name, variant string, n int) {
	t.Helper()
	if _, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots
			(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		SELECT $1, date_trunc('second', NOW()) - interval '1 minute' - make_interval(secs => i), $2, $3, false, true, i, i % 50, 'BLUE'
		FROM generate_series(1, $4::int) AS i`, scope.ID(), name, variant, n); err != nil {
		t.Fatalf("seed spark run %s %s: %v", name, variant, err)
	}
}

// referenceSparklines is the per-variant fallback the bulk read replaces: one
// SparklineData read per variant (or one for an explicit SparkVariant), keyed
// by each page row's own variant.
func referenceSparklines(t *testing.T, repo *Repository, scope league.Scope, sparkVariant string, rows []CollectiveResult) map[CollectiveSparklineKey][]SparklinePoint {
	t.Helper()
	ctx := context.Background()
	out := make(map[CollectiveSparklineKey][]SparklinePoint)
	if sparkVariant != "" {
		names := make([]string, 0, len(rows))
		for _, r := range rows {
			names = append(names, r.TransfiguredName)
		}
		points, err := repo.SparklineData(ctx, scope, names, sparkVariant, SparklineWindowHours)
		if err != nil {
			t.Fatalf("reference SparklineData: %v", err)
		}
		for _, r := range rows {
			if series, ok := points[r.TransfiguredName]; ok {
				out[CollectiveSparklineKey{Name: r.TransfiguredName, Variant: r.Variant}] = series
			}
		}
		return out
	}
	byVariant := map[string][]string{}
	for _, r := range rows {
		byVariant[r.Variant] = append(byVariant[r.Variant], r.TransfiguredName)
	}
	for variant, names := range byVariant {
		points, err := repo.SparklineData(ctx, scope, names, variant, SparklineWindowHours)
		if err != nil {
			t.Fatalf("reference SparklineData %s: %v", variant, err)
		}
		for name, series := range points {
			out[CollectiveSparklineKey{Name: name, Variant: variant}] = series
		}
	}
	return out
}

// sparkFallbackPage seeds a mixed four-variant 50-row page: one gem in two
// variants, one ranked gem with no series, a newer-than-window point and a
// corrupted row per gem that the read must drop, and an unrequested gem.
func sparkFallbackPage(t *testing.T, pool *pgxpool.Pool, scope league.Scope) []CollectiveResult {
	t.Helper()
	variants := []string{"1", "1/20", "20", "20/20"}
	rows := make([]CollectiveResult, 0, 50)
	for i := 0; i < 48; i++ {
		name, variant := fmt.Sprintf("Spark Gem %02d", i), variants[i%4]
		rows = append(rows, CollectiveResult{TransfiguredName: name, BaseName: "Spark Base", Variant: variant})
		seedSparkPoints(t, pool, scope, name, variant, false, 11*time.Hour, 6*time.Hour, time.Hour, 13*time.Hour)
		seedSparkPoints(t, pool, scope, name, variant, true, 2*time.Hour)
	}
	// Spark Gem 00 is "1"; its "20" market is a different series.
	rows = append(rows, CollectiveResult{TransfiguredName: "Spark Gem 00", BaseName: "Spark Base", Variant: "20"})
	seedSparkPoints(t, pool, scope, "Spark Gem 00", "20", false, 3*time.Hour, 30*time.Minute)
	rows = append(rows, CollectiveResult{TransfiguredName: "Spark Gem Missing", BaseName: "Spark Base", Variant: "20/20"})
	seedSparkPoints(t, pool, scope, "Spark Unrequested", "20/20", false, time.Hour)
	return rows
}

func sparkQuery(sparkVariant string) CollectiveQuery {
	return CollectiveQuery{Page: 1, PageSize: CollectiveMaxPageSize, Sort: CollectiveSortPrice, SparkVariant: sparkVariant}
}

func querySparklines(t *testing.T, src CollectiveSource, cache *Cache, scope league.Scope, q CollectiveQuery) map[CollectiveSparklineKey][]SparklinePoint {
	t.Helper()
	got, err := QueryCollective(context.Background(), src, cache, scope, q)
	if err != nil {
		t.Fatalf("QueryCollective: %v", err)
	}
	return got.Sparklines
}

// TestKeepHappySparklineFallbackBulk: a cold page's sparkline fallback is one
// bulk statement whatever its variant mix, 300 overlapping equal misses share
// it, a warm cache runs none, and every body equals the per-variant read.
func TestKeepHappySparklineFallbackBulk(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-SparkBulk")
	rows := sparkFallbackPage(t, pool, scope)
	plain := NewRepository(pool)
	counted, counter := countingRepository(t, pool)
	src := fixedPageSource{Repository: counted, rows: rows}

	for name, sparkVariant := range map[string]string{"mixed page": "", "explicit SparkVariant": "20"} {
		want := referenceSparklines(t, plain, scope, sparkVariant, rows)
		if len(want) == 0 {
			t.Fatalf("SparkVariant %q reference has no series, want a decorated page", sparkVariant)
		}
		counter.reset()
		got := querySparklines(t, src, NewCache(scope), scope, sparkQuery(sparkVariant))
		statements := counter.reset()
		t.Run(name+" runs one statement", func(t *testing.T) {
			if statements != 1 {
				t.Errorf("SparkVariant %q cold page ran %d sparkline statements, want 1", sparkVariant, statements)
			}
		})
		t.Run(name+" equals the per-variant read", func(t *testing.T) {
			if !reflect.DeepEqual(got, want) {
				t.Errorf("SparkVariant %q sparklines = %v, want the per-variant read %v", sparkVariant, got, want)
			}
		})
	}

	want := referenceSparklines(t, plain, scope, "", rows)
	cold := NewCache(scope)
	counter.reset()
	outcomes := burst(300, func() collectiveOutcome {
		got, err := QueryCollective(context.Background(), overlappingPageSource{src, 300}, cold, scope, sparkQuery(""))
		return collectiveOutcome{got, err}
	})
	statements := counter.reset()
	t.Run("overlapping burst shares one statement", func(t *testing.T) {
		if statements != 1 {
			t.Errorf("300 overlapping cold requests ran %d sparkline statements, want 1", statements)
		}
	})
	t.Run("overlapping burst bodies equal the per-variant read", func(t *testing.T) {
		for i, o := range outcomes {
			if o.err != nil {
				t.Fatalf("burst request %d: %v", i, o.err)
			}
			if !reflect.DeepEqual(o.got.Sparklines, want) {
				t.Fatalf("burst request %d sparklines = %v, want the per-variant read", i, o.got.Sparklines)
			}
		}
	})
	t.Run("fallback leaves the corpus cold", func(t *testing.T) {
		if cold.For(scope).HasSparklines() {
			t.Error("a page-limited fallback marked the sparkline corpus warm")
		}
	})
	t.Run("warm cache runs no statement", func(t *testing.T) {
		warm := NewCache(scope)
		warm.For(scope).SetSparklinesByName(nil, nil, time.Now())
		counter.reset()
		querySparklines(t, src, warm, scope, sparkQuery(""))
		if n := counter.reset(); n != 0 {
			t.Errorf("warm page ran %d sparkline statements, want 0", n)
		}
	})
}

// A failed fallback read is not retained: the next cold request reads again
// and serves the series.
func TestKeepCriticalSparklineFallbackFailureRetry(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-SparkRetry")
	rows := sparkFallbackPage(t, pool, scope)
	want := referenceSparklines(t, NewRepository(pool), scope, "", rows)

	broken, err := pgxpool.New(context.Background(), pool.Config().ConnString())
	if err != nil {
		t.Fatalf("open pool: %v", err)
	}
	broken.Close()
	cache := NewCache(scope)
	if got := querySparklines(t, fixedPageSource{Repository: NewRepository(broken), rows: rows}, cache, scope, sparkQuery("")); len(got) != 0 {
		t.Fatalf("failed read served %d series, want none", len(got))
	}
	if got := querySparklines(t, fixedPageSource{Repository: NewRepository(pool), rows: rows}, cache, scope, sparkQuery("")); !reflect.DeepEqual(got, want) {
		t.Errorf("retry sparklines = %v, want the per-variant read", got)
	}
}

// One variant holding more than 10,000 window rows keeps that variant's
// global first-10,000 selection by (name, time); the cut falls inside the
// second series and both series share timestamps, so selection and ordering
// across names are both exercised. Other variants keep their own budget.
func TestKeepCriticalSparklineFallbackRowSelection(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-SparkCap")
	seedSparkRun(t, pool, scope, "Cap Gem A", "20", 6000)
	seedSparkRun(t, pool, scope, "Cap Gem B", "20", 6000)
	seedSparkRun(t, pool, scope, "Cap Gem C", "1", 300)
	rows := []CollectiveResult{
		{TransfiguredName: "Cap Gem B", Variant: "20"},
		{TransfiguredName: "Cap Gem A", Variant: "20"},
		{TransfiguredName: "Cap Gem C", Variant: "1"},
	}
	want := referenceSparklines(t, NewRepository(pool), scope, "", rows)
	if a, b := len(want[CollectiveSparklineKey{"Cap Gem A", "20"}]), len(want[CollectiveSparklineKey{"Cap Gem B", "20"}]); a != 6000 || b != 4000 {
		t.Fatalf("reference selection = %d + %d points, want 6000 + 4000", a, b)
	}
	counted, counter := countingRepository(t, pool)
	got := querySparklines(t, fixedPageSource{Repository: counted, rows: rows}, NewCache(scope), scope, sparkQuery(""))
	statements := counter.reset()
	t.Run("one statement", func(t *testing.T) {
		if statements != 1 {
			t.Errorf("cold page ran %d sparkline statements, want 1", statements)
		}
	})
	t.Run("per-variant selection", func(t *testing.T) {
		if !reflect.DeepEqual(got, want) {
			t.Errorf("bulk selection differs from the per-variant read: got %d/%d/%d points",
				len(got[CollectiveSparklineKey{"Cap Gem A", "20"}]), len(got[CollectiveSparklineKey{"Cap Gem B", "20"}]), len(got[CollectiveSparklineKey{"Cap Gem C", "1"}]))
		}
	})
}

// TestKeepCriticalSparklineFallbackWindow: the window is evaluated by each
// cold load, so a later request after the first completes reads again and
// drops a point that has aged past 12 hours meanwhile.
func TestKeepCriticalSparklineFallbackWindow(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-SparkWindow")
	const edge = 3 * time.Second
	seedSparkPoints(t, pool, scope, "Window Gem", "20/20", false, time.Duration(SparklineWindowHours)*time.Hour-edge, time.Hour)
	seeded := time.Now()
	rows := []CollectiveResult{{TransfiguredName: "Window Gem", Variant: "20/20"}}
	counted, counter := countingRepository(t, pool)
	src := fixedPageSource{Repository: counted, rows: rows}
	cache := NewCache(scope)
	key := CollectiveSparklineKey{Name: "Window Gem", Variant: "20/20"}

	if got := querySparklines(t, src, cache, scope, sparkQuery(""))[key]; len(got) != 2 {
		t.Fatalf("first request = %d points, want 2 (edge point still inside the window)", len(got))
	}
	time.Sleep(time.Until(seeded.Add(edge + 1500*time.Millisecond)))
	counter.reset()
	got := querySparklines(t, src, cache, scope, sparkQuery(""))[key]
	statements := counter.reset()
	t.Run("later request reads again", func(t *testing.T) {
		if statements != 1 {
			t.Errorf("later cold request ran %d sparkline statements, want 1", statements)
		}
	})
	t.Run("aged point is dropped", func(t *testing.T) {
		if len(got) != 1 || got[0].Price != 101 {
			t.Errorf("later request = %+v, want only the 1h point (price 101)", got)
		}
	})
}
