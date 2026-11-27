//go:build integration

package lab

import (
	"context"
	"errors"
	"fmt"
	"reflect"
	"sort"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

var fontOverviewVariants = []string{"1", "1/20", "20", "20/20"}

// keep:critical — silent: restart serving must not turn a missing or legacy
// under-floor EV into a plausible number trusted by sorting and BEST.
func TestKeepCritical_FontColdPathKeepsUnderFloorEVMissing(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := fontOverviewLeague(t, pool, "font-cold-coverage")
	ctx := context.Background()
	repo := NewRepository(pool)
	now := futureTime(106)
	under := fontResultJSON(t, `{"Color":"RED","Variant":"20/20","Mode":"safe","Pool":88,"PricedGems":0,"InputCost":3.5,"EV":null,"EVRaw":null,"Profit":null,"LiquidityRisk":"LOW"}`)
	covered := fontResultJSON(t, `{"Color":"GREEN","Variant":"20/20","Mode":"safe","Pool":4,"PricedGems":1,"EV":9,"EVRaw":12,"InputCost":3.5,"Profit":8.5,"LiquidityRisk":"LOW"}`)
	under.Time, covered.Time = now, now
	if _, err := repo.SaveFontResults(ctx, scope, []FontResult{under, covered}); err != nil {
		t.Fatal(err)
	}
	for _, row := range []struct {
		color, variant string
		priced         any
		ev             float64
	}{
		{"BLUE", "1", nil, 9},
		{"BLUE", "20/20", 3, 51.5},
	} {
		if _, err := pool.Exec(ctx, `INSERT INTO font_snapshots
			(league, time, color, variant, mode, pool, priced_gems, ev, input_cost, profit, liquidity_risk)
			VALUES ($1, $2, $3, $4, 'safe', 88, $5, $6, 3.5, 8.5, 'LOW')`, scope.ID(), now, row.color, row.variant, row.priced, row.ev); err != nil {
			t.Fatal(err)
		}
	}
	var storedEV *float64
	var storedProfit float64
	if err := pool.QueryRow(ctx, `SELECT ev, profit FROM font_snapshots
		WHERE league = $1 AND time = $2 AND color = 'RED'`, scope.ID(), now).Scan(&storedEV, &storedProfit); err != nil {
		t.Fatal(err)
	}
	if storedEV != nil || storedProfit != 0 {
		t.Errorf("under-floor storage ev/profit = %v/%g, want SQL NULL/0", storedEV, storedProfit)
	}
	rows, err := repo.FontOverviewRows(ctx, scope)
	if err != nil {
		t.Fatal(err)
	}
	if len(rows) != 4 {
		t.Fatalf("cold rows = %d, want 4", len(rows))
	}
	seen := make(map[string]bool)
	for _, r := range rows {
		key := r.Color + "/" + r.Variant
		seen[key] = true
		switch key {
		case "GREEN/20/20":
			assertFontCoverage(t, r, 4, 1, 0.25, true)
			if fontNumber(r.EV) != 9 || fontNumber(r.EVRaw) != 12 || fontNumber(r.Profit) != 8.5 || r.InputCost != 3.5 {
				t.Errorf("covered round-trip EV/EVRaw/Profit/input = %v/%v/%v/%g, want 9/12/8.5/3.5", r.EV, r.EVRaw, r.Profit, r.InputCost)
			}
		case "RED/20/20", "BLUE/1":
			assertFontCoverage(t, r, 88, 0, 0, false)
		case "BLUE/20/20":
			assertFontCoverage(t, r, 88, 3, 3.0/88, false)
		default:
			t.Errorf("unexpected cold row %s", key)
		}
	}
	for _, key := range []string{"GREEN/20/20", "RED/20/20", "BLUE/1", "BLUE/20/20"} {
		if !seen[key] {
			t.Errorf("cold row %s missing", key)
		}
	}
}

// gatedCounter counts every statement and holds each one until the gate opens,
// so a burst's callers all overlap the load they would share.
type gatedCounter struct {
	statementCounter
	gateMu sync.Mutex
	gate   chan struct{}
}

func (g *gatedCounter) TraceQueryStart(ctx context.Context, conn *pgx.Conn, data pgx.TraceQueryStartData) context.Context {
	ctx = g.statementCounter.TraceQueryStart(ctx, conn, data)
	g.gateMu.Lock()
	gate := g.gate
	g.gateMu.Unlock()
	<-gate
	return ctx
}

func (g *gatedCounter) shut() chan struct{} {
	gate := make(chan struct{})
	g.gateMu.Lock()
	g.gate = gate
	g.gateMu.Unlock()
	return gate
}

func gatedRepository(t *testing.T, pool *pgxpool.Pool) (*Repository, *gatedCounter) {
	t.Helper()
	cfg, err := pgxpool.ParseConfig(pool.Config().ConnString())
	if err != nil {
		t.Fatalf("parse pool config: %v", err)
	}
	counter := &gatedCounter{gate: make(chan struct{})}
	close(counter.gate)
	cfg.ConnConfig.Tracer = counter
	gated, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatalf("open gated pool: %v", err)
	}
	t.Cleanup(gated.Close)
	if _, err := gated.Exec(context.Background(), "SELECT 1"); err != nil {
		t.Fatalf("warm gated pool: %v", err)
	}
	counter.reset()
	return NewRepository(gated), counter
}

// overlap runs calls concurrently with the gate shut, opens it once every
// call waits on a shared load (or after a deadline, when they never share),
// and returns their results.
func overlap[T any](t *testing.T, counter *gatedCounter, fills *FontColdFills, calls []func() T) []T {
	t.Helper()
	gate := counter.shut()
	out := make([]T, len(calls))
	var wg sync.WaitGroup
	for i, call := range calls {
		wg.Add(1)
		go func(i int, call func() T) {
			defer wg.Done()
			out[i] = call()
		}(i, call)
	}
	deadline := time.Now().Add(3 * time.Second)
	for fills.waiting() < len(calls) && time.Now().Before(deadline) {
		time.Sleep(time.Millisecond)
	}
	close(gate)
	wg.Wait()
	if n := fills.waiting(); n != 0 {
		t.Errorf("%d callers still wait on a load after every call returned", n)
	}
	return out
}

func fontOverviewLeague(t *testing.T, pool *pgxpool.Pool, id string) league.Scope {
	t.Helper()
	registerLeague(t, pool, id)
	t.Cleanup(func() {
		for _, table := range []string{"font_snapshots", "dedication_snapshots"} {
			if _, err := pool.Exec(context.Background(), "DELETE FROM "+table+" WHERE league = $1", id); err != nil {
				t.Logf("cleanup warning: delete %s for %q: %v", table, id, err)
			}
		}
	})
	return league.Historical(id)
}

// seedFontOverview stores a stale snapshot whose rows outrank everything, then
// the latest one: every color × variant × mode (and, for Dedication, × pool)
// at a distinct profit.
func seedFontOverview(t *testing.T, repo *Repository, scope league.Scope) {
	t.Helper()
	ctx := context.Background()
	latest := time.Now().UTC().Truncate(time.Second)
	stale := latest.Add(-time.Hour)
	modes := []string{"safe", "premium", "jackpot"}
	colors := []string{"RED", "GREEN", "BLUE"}

	var fonts []FontResult
	var dedications []DedicationResult
	n := 0
	for _, variant := range fontOverviewVariants {
		for _, mode := range modes {
			fonts = append(fonts, FontResult{Time: stale, Color: "RED", Variant: variant, Mode: mode, Pool: 1, Profit: fontFixtureFloat(1e6, FontResult{}.Profit), LiquidityRisk: "LOW"})
			for _, color := range colors {
				n++
				fonts = append(fonts, FontResult{Time: latest, Color: color, Variant: variant, Mode: mode,
					Pool: 10 + n, PricedGems: 10 + n, Winners: n % 4, PWin: 0.5, AvgWin: float64(n), EV: fontFixtureFloat(float64(2*n), FontResult{}.EV),
					InputCost: 5, Profit: fontFixtureFloat(float64((n*37)%101), FontResult{}.Profit), ThinPoolGems: n % 3, LiquidityRisk: "MEDIUM"})
			}
		}
	}
	for _, variant := range DedicationVariants {
		for _, gemType := range []string{"skill", "transfigured"} {
			for _, mode := range modes {
				dedications = append(dedications, DedicationResult{Time: stale, Variant: variant, Color: "RED", GemType: gemType, Mode: mode, Pool: 1, Profit: 1e6, LiquidityRisk: "LOW"})
				for _, color := range colors {
					n++
					dedications = append(dedications, DedicationResult{Time: latest, Variant: variant, Color: color, GemType: gemType, Mode: mode,
						Pool: n, Winners: n % 5, PWin: 0.25, AvgWinRaw: float64(n), EVRaw: float64(3 * n), InputCost: 7,
						Profit: float64((n * 53) % 211), FontsToHit: 4, ThinPoolGems: n % 2, LiquidityRisk: "HIGH",
						PoolBreakdown: []TierPoolInfo{{Tier: "TOP", Count: n}}})
				}
			}
		}
	}
	if _, err := repo.SaveFontResults(ctx, scope, fonts); err != nil {
		t.Fatalf("seed font: %v", err)
	}
	if _, err := repo.SaveDedicationResults(ctx, scope, dedications); err != nil {
		t.Fatalf("seed dedication: %v", err)
	}
}

type fontRequest struct {
	variant string
	limit   int
}

// fontRequests are a page's four variant calls, the unfiltered call, and
// limits that cut inside and across the per-variant rows.
var fontRequests = []fontRequest{
	{"1", 50}, {"1/20", 50}, {"20", 50}, {"20/20", 50}, {"", 50}, {"", 5}, {"20/20", 2},
}

// uncachedFontModes is today's answer: one query per mode, filtered and
// limited in SQL.
func uncachedFontModes(t *testing.T, repo *Repository, scope league.Scope, q fontRequest) FontAnalysis {
	t.Helper()
	var out FontAnalysis
	for _, m := range []struct {
		mode string
		dst  *[]FontResult
	}{{"safe", &out.Safe}, {"premium", &out.Premium}, {"jackpot", &out.Jackpot}} {
		rows, err := repo.LatestFontResults(context.Background(), scope, q.variant, m.mode, q.limit)
		if err != nil {
			t.Fatalf("uncached %s: %v", m.mode, err)
		}
		*m.dst = rows
	}
	return out
}

func uncachedDedicationPools(t *testing.T, repo *Repository, scope league.Scope, variant string, limit int) DedicationAnalysis {
	t.Helper()
	var out DedicationAnalysis
	var err error
	if out.Skills, err = repo.LatestDedicationResults(context.Background(), scope, variant, "skill", "", limit); err != nil {
		t.Fatalf("uncached skills: %v", err)
	}
	if out.Transfigured, err = repo.LatestDedicationResults(context.Background(), scope, variant, "transfigured", "", limit); err != nil {
		t.Fatalf("uncached transfigured: %v", err)
	}
	return out
}

// sameRows reports whether got and want hold the same rows in the same profit
// order. Rows of equal profit carry no order (the SQL has no tie-breaker), so
// they compare as a set.
func sameRows[T any](got, want []T, profit func(T) float64) bool {
	if len(got) != len(want) {
		return false
	}
	canon := func(rows []T) []T {
		out := append([]T(nil), rows...)
		sort.SliceStable(out, func(i, j int) bool {
			if profit(out[i]) != profit(out[j]) {
				return profit(out[i]) > profit(out[j])
			}
			return fmt.Sprintf("%+v", out[i]) < fmt.Sprintf("%+v", out[j])
		})
		return out
	}
	for i := range got {
		if profit(got[i]) != profit(want[i]) {
			return false
		}
	}
	return reflect.DeepEqual(canon(got), canon(want))
}

func fontProfit(r FontResult) float64             { return fontNumber(r.Profit) }
func dedicationProfit(r DedicationResult) float64 { return r.Profit }

func sameFontModes(got, want FontAnalysis) bool {
	return sameRows(got.Safe, want.Safe, fontProfit) && sameRows(got.Premium, want.Premium, fontProfit) &&
		sameRows(got.Jackpot, want.Jackpot, fontProfit)
}

func sameDedicationPools(got, want DedicationAnalysis) bool {
	return sameRows(got.Skills, want.Skills, dedicationProfit) && sameRows(got.Transfigured, want.Transfigured, dedicationProfit)
}

type fontOutcome struct {
	req fontRequest
	got FontAnalysis
	err error
}

type dedicationOutcome struct {
	variant string
	got     DedicationAnalysis
	err     error
}

// TestKeepHappyFontOverviewGroupedFill: 300 cold Font pages (four variant
// calls each) overlapping one another run one grouped Font statement in total,
// 300 cold Dedication pages (both variants) one grouped Dedication statement,
// and every answer equals today's per-mode query answer.
func TestKeepHappyFontOverviewGroupedFill(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := fontOverviewLeague(t, pool, "POE410-FontGroupedFill")
	plain := NewRepository(pool)
	seedFontOverview(t, plain, scope)

	wantFont := make(map[fontRequest]FontAnalysis)
	for _, q := range fontRequests {
		wantFont[q] = uncachedFontModes(t, plain, scope, q)
	}
	if got := len(wantFont[fontRequest{"20/20", 2}].Safe); got != 2 {
		t.Fatalf("fixture: limited 20/20 safe has %d rows, want 2", got)
	}
	wantDedication := map[string]DedicationAnalysis{}
	for _, v := range DedicationVariants {
		wantDedication[v] = uncachedDedicationPools(t, plain, scope, v, 50)
	}

	repo, counter := gatedRepository(t, pool)
	cache := NewCache(scope)
	fills := cache.For(scope).FontColdFills()

	var fontCalls []func() fontOutcome
	for page := 0; page < 300; page++ {
		for _, v := range fontOverviewVariants {
			q := fontRequest{v, 50}
			fontCalls = append(fontCalls, func() fontOutcome {
				got, err := FontModes(context.Background(), repo, cache, scope, q.variant, q.limit)
				return fontOutcome{q, got, err}
			})
		}
	}
	for _, q := range fontRequests {
		fontCalls = append(fontCalls, func() fontOutcome {
			got, err := FontModes(context.Background(), repo, cache, scope, q.variant, q.limit)
			return fontOutcome{q, got, err}
		})
	}
	for i, o := range overlap(t, counter, fills, fontCalls) {
		if o.err != nil {
			t.Fatalf("font call %d: %v", i, o.err)
		}
		if !sameFontModes(o.got, wantFont[o.req]) {
			t.Fatalf("font call %d (%+v) = %+v, want today's %+v", i, o.req, o.got, wantFont[o.req])
		}
	}
	if got := counter.reset(); got != 1 {
		t.Errorf("overlapping cold Font burst ran %d statements, want 1 grouped statement", got)
	}

	var dedicationCalls []func() dedicationOutcome
	for page := 0; page < 300; page++ {
		for _, v := range DedicationVariants {
			dedicationCalls = append(dedicationCalls, func() dedicationOutcome {
				got, err := DedicationPools(context.Background(), repo, cache, scope, v, 50)
				return dedicationOutcome{v, got, err}
			})
		}
	}
	for i, o := range overlap(t, counter, fills, dedicationCalls) {
		if o.err != nil {
			t.Fatalf("dedication call %d: %v", i, o.err)
		}
		if !sameDedicationPools(o.got, wantDedication[o.variant]) {
			t.Fatalf("dedication call %d (%s) = %+v, want today's %+v", i, o.variant, o.got, wantDedication[o.variant])
		}
	}
	if got := counter.reset(); got != 1 {
		t.Errorf("overlapping cold Dedication burst ran %d statements, want 1 grouped statement", got)
	}
}

// failingOverview fails every grouped read while fail is set.
type failingOverview struct {
	*Repository
	mu   sync.Mutex
	fail bool
}

var errOverviewDown = errors.New("overview read failed")

func (f *failingOverview) failing() bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.fail
}

func (f *failingOverview) FontOverviewRows(ctx context.Context, scope league.Scope) ([]FontResult, error) {
	if f.failing() {
		return nil, errOverviewDown
	}
	return f.Repository.FontOverviewRows(ctx, scope)
}

func (f *failingOverview) DedicationOverviewRows(ctx context.Context, scope league.Scope) ([]DedicationResult, error) {
	if f.failing() {
		return nil, errOverviewDown
	}
	return f.Repository.DedicationOverviewRows(ctx, scope)
}

// publishingOverview publishes an analyzer answer while its grouped read is
// in flight, as a tick landing mid-load would.
type publishingOverview struct {
	*Repository
	publish func()
}

func (p *publishingOverview) FontOverviewRows(ctx context.Context, scope league.Scope) ([]FontResult, error) {
	rows, err := p.Repository.FontOverviewRows(ctx, scope)
	p.publish()
	return rows, err
}

func (p *publishingOverview) DedicationOverviewRows(ctx context.Context, scope league.Scope) ([]DedicationResult, error) {
	rows, err := p.Repository.DedicationOverviewRows(ctx, scope)
	p.publish()
	return rows, err
}

// TestKeepCriticalFontColdFillNeverWarms: a cold load serves only the requests
// overlapping it. It never warms or overwrites the analyzer's corpus, a failure
// leaves the next request cold, and an analyzer answer published while the load
// ran wins over the load.
func TestKeepCriticalFontColdFillNeverWarms(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := fontOverviewLeague(t, pool, "POE410-FontColdNeverWarms")
	plain := NewRepository(pool)
	seedFontOverview(t, plain, scope)
	repo, counter := gatedRepository(t, pool)
	ctx := context.Background()
	q := fontRequest{"20/20", 50}
	want := uncachedFontModes(t, plain, scope, q)
	wantDedication := uncachedDedicationPools(t, plain, scope, DefaultDedicationVariant, 50)

	t.Run("warm empty corpora run no SQL", func(t *testing.T) {
		cache := NewCache(scope)
		cache.For(scope).SetFont(FontAnalysis{})
		cache.For(scope).SetDedication(DedicationCorpus{})
		font, err := FontModes(ctx, repo, cache, scope, q.variant, q.limit)
		if err != nil || len(font.Safe)+len(font.Premium)+len(font.Jackpot) != 0 {
			t.Fatalf("warm empty Font = %+v, %v; want empty modes", font, err)
		}
		ded, err := DedicationPools(ctx, repo, cache, scope, DefaultDedicationVariant, 50)
		if err != nil || len(ded.Skills)+len(ded.Transfigured) != 0 {
			t.Fatalf("warm empty Dedication = %+v, %v; want empty pools", ded, err)
		}
		if got := counter.reset(); got != 0 {
			t.Errorf("warm empty corpora ran %d statements, want none", got)
		}
	})

	t.Run("completed loads retain nothing", func(t *testing.T) {
		cache := NewCache(scope)
		for round := 0; round < 2; round++ {
			got, err := FontModes(ctx, repo, cache, scope, q.variant, q.limit)
			if err != nil || !sameFontModes(got, want) {
				t.Fatalf("round %d Font = %+v, %v; want today's %+v", round, got, err, want)
			}
			ded, err := DedicationPools(ctx, repo, cache, scope, DefaultDedicationVariant, 50)
			if err != nil || !sameDedicationPools(ded, wantDedication) {
				t.Fatalf("round %d Dedication = %+v, %v; want today's %+v", round, ded, err, wantDedication)
			}
			if got := counter.reset(); got != 2 {
				t.Errorf("round %d sequential cold reads ran %d statements, want 2 (one grouped load each, none retained)", round, got)
			}
			if _, warm := cache.For(scope).Font(); warm {
				t.Fatalf("round %d: a cold Font load marked the corpus warm", round)
			}
			if cache.For(scope).HasDedication() {
				t.Fatalf("round %d: a cold Dedication load marked the corpus warm", round)
			}
		}
	})

	t.Run("a failed load stays cold", func(t *testing.T) {
		cache := NewCache(scope)
		src := &failingOverview{Repository: repo, fail: true}
		if _, err := FontModes(ctx, src, cache, scope, q.variant, q.limit); !errors.Is(err, errOverviewDown) {
			t.Fatalf("failed Font load err = %v, want %v", err, errOverviewDown)
		}
		if _, err := DedicationPools(ctx, src, cache, scope, DefaultDedicationVariant, 50); !errors.Is(err, errOverviewDown) {
			t.Fatalf("failed Dedication load err = %v, want %v", err, errOverviewDown)
		}
		if _, warm := cache.For(scope).Font(); warm || cache.For(scope).HasDedication() {
			t.Fatal("a failed load marked a corpus warm")
		}
		src.mu.Lock()
		src.fail = false
		src.mu.Unlock()
		counter.reset()
		got, err := FontModes(ctx, src, cache, scope, q.variant, q.limit)
		if err != nil || !sameFontModes(got, want) {
			t.Fatalf("Font after a failure = %+v, %v; want today's %+v", got, err, want)
		}
		if n := counter.reset(); n != 1 {
			t.Errorf("Font after a failure ran %d statements, want a fresh grouped load", n)
		}
	})

	t.Run("an analyzer answer published mid-load wins", func(t *testing.T) {
		cache := NewCache(scope)
		published := FontAnalysis{Safe: []FontResult{{Variant: "20/20", Color: "BLUE", Mode: "safe", Profit: 42}}}
		publishedDedication := DedicationCorpus{Analysis: DedicationAnalysis{
			Skills: []DedicationResult{{Variant: DefaultDedicationVariant, Color: "GREEN", GemType: "skill", Mode: "safe", Profit: 7}},
		}}
		src := &publishingOverview{Repository: repo, publish: func() {
			cache.For(scope).SetFont(published)
			cache.For(scope).SetDedication(publishedDedication)
		}}
		got, err := FontModes(ctx, src, cache, scope, q.variant, q.limit)
		if err != nil {
			t.Fatalf("FontModes: %v", err)
		}
		if !reflect.DeepEqual(got.Safe, published.Safe) || len(got.Premium)+len(got.Jackpot) != 0 {
			t.Fatalf("Font served %+v after the analyzer published %+v; want the analyzer's answer", got, published)
		}
		if stored, _ := cache.For(scope).Font(); !reflect.DeepEqual(stored, published) {
			t.Fatalf("analyzer Font corpus = %+v, want it untouched as %+v", stored, published)
		}

		cache = NewCache(scope)
		ded, err := DedicationPools(ctx, src, cache, scope, DefaultDedicationVariant, 50)
		if err != nil {
			t.Fatalf("DedicationPools: %v", err)
		}
		if !reflect.DeepEqual(ded.Skills, publishedDedication.Analysis.Skills) || len(ded.Transfigured) != 0 {
			t.Fatalf("Dedication served %+v after the analyzer published; want the analyzer's answer", ded)
		}
		if stored, _ := cache.For(scope).Dedication(); !reflect.DeepEqual(stored.Skills, publishedDedication.Analysis.Skills) {
			t.Fatalf("analyzer Dedication corpus = %+v, want it untouched", stored)
		}
	})

}
