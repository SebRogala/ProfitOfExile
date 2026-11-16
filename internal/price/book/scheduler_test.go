package book

import (
	"fmt"
	"math"
	"reflect"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

func schedulerDemand(name string, price, probability float64) Demand {
	k := resolve.GemKey(leagueID, name, "1")
	return Demand{Key: k, SnapshotTime: t0, Ninja: resolve.Observation{
		Key: k, Source: resolve.SourceNinja, At: t0, Prices: chaos(price)},
		PotentialContribution: price * probability, PickProbability: probability}
}

func schedulerFixture(t *testing.T, minAge time.Duration, demand []Demand, checks ...resolve.Observation) *Scheduler {
	t.Helper()
	s, err := NewScheduler(league.Historical(leagueID), resolve.Policy{ListingCount: 3, MaxRevisitAge: 6 * time.Hour}, minAge)
	if err != nil {
		t.Fatal(err)
	}
	if err := s.Reconcile(demand, checks); err != nil {
		t.Fatal(err)
	}
	return s
}

func takeName(t *testing.T, s *Scheduler, at time.Time, want string) Demand {
	t.Helper()
	d, ok := s.Take(at, resolve.Rate{Chaos: 200})
	if !ok || d.Key.Name != want {
		t.Fatalf("Take at %v = %s, %v; want %s", at, d.Key.Name, ok, want)
	}
	return d
}

func successful(t *testing.T, s *Scheduler, k resolve.Key, at time.Time) {
	t.Helper()
	if err := s.Success(tradeObs(k, at, nil)); err != nil {
		t.Fatal(err)
	}
}

func TestScheduler_NeverCheckedUsesLiteralFontContributions(t *testing.T) {
	ds := []Demand{schedulerDemand("C", 60, .1), schedulerDemand("B", 80, .3), schedulerDemand("A", 100, .6), schedulerDemand("D", 40, 0)}
	s := schedulerFixture(t, 5*time.Minute, ds)
	for _, want := range []string{"A", "B", "C", "D"} {
		d := takeName(t, s, t0, want)
		successful(t, s, d.Key, t0)
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("fresh completed population was selected")
	}
}

func TestScheduler_DueLowValuePrecedesRecentRisk(t *testing.T) {
	low, high := schedulerDemand("Low", 1, 0), schedulerDemand("High", 10000, 1)
	s := schedulerFixture(t, 5*time.Minute, []Demand{high, low}, tradeObs(low.Key, t0.Add(-6*time.Hour), nil), tradeObs(high.Key, t0.Add(-time.Hour), chaos(1)))
	takeName(t, s, t0, "Low")
}

func TestScheduler_DueUsesEarliestDeadline(t *testing.T) {
	a, b := schedulerDemand("A", 100, 1), schedulerDemand("B", 1, 0)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-7*time.Hour), nil), tradeObs(b.Key, t0.Add(-8*time.Hour), nil))
	takeName(t, s, t0, "B")
}

func TestScheduler_NeverCheckedPrecedesOverdue(t *testing.T) {
	a, b := schedulerDemand("New", 0, 0), schedulerDemand("Due", 100, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(b.Key, t0.Add(-24*time.Hour), nil))
	takeName(t, s, t0, "New")
}

func TestScheduler_DisagreementUsesMaximumSourcePrice(t *testing.T) {
	a, b := schedulerDemand("Match", 100, 1), schedulerDemand("Disagree", 100, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), chaos(100)), tradeObs(b.Key, t0.Add(-time.Hour), chaos(10)))
	takeName(t, s, t0, "Disagree")
}

func TestScheduler_TakeRecomputesNativePricesAtCurrentRate(t *testing.T) {
	for _, tc := range []struct {
		rate float64
		want string
	}{{100, "Chaos"}, {400, "Divine"}} {
		t.Run(tc.want, func(t *testing.T) {
			a, b := schedulerDemand("Divine", 0, 1), schedulerDemand("Chaos", 150, 1)
			s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), []resolve.Money{{Amount: 1, Currency: "divine"}}), tradeObs(b.Key, t0.Add(-time.Hour), chaos(150)))
			// A previous selection must not freeze scores at its rate.
			d, _ := s.Take(t0, resolve.Rate{Chaos: 100})
			if err := s.Retry(d.Key, t0); err != nil {
				t.Fatal(err)
			}
			got, ok := s.Take(t0, resolve.Rate{Chaos: tc.rate})
			if !ok || got.Key.Name != tc.want {
				t.Fatalf("rate %v selected %s, %v; want %s", tc.rate, got.Key.Name, ok, tc.want)
			}
		})
	}
}

func TestScheduler_TradePriceUsesMedianCheapestPolicyCount(t *testing.T) {
	a, b := schedulerDemand("Outlier", 0, 1), schedulerDemand("Steady", 0, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), chaos(1, 2, 1000, 2000, 3000)), tradeObs(b.Key, t0.Add(-time.Hour), chaos(3)))
	takeName(t, s, t0, "Steady")
}

func TestScheduler_OlderReconciliationCannotUndoImmediateSuccess(t *testing.T) {
	a, b := schedulerDemand("A", 1, 1), schedulerDemand("B", 1, 1)
	old := tradeObs(a.Key, t0.Add(-8*time.Hour), nil)
	s := schedulerFixture(t, 5*time.Minute, []Demand{a, b}, old, tradeObs(b.Key, t0.Add(-7*time.Hour), nil))
	successful(t, s, a.Key, t0)
	if err := s.Reconcile([]Demand{a, b}, []resolve.Observation{old}); err != nil {
		t.Fatal(err)
	}
	takeName(t, s, t0, "B")
	if got := s.Stats(t0); got.Overdue != 1 || got.NeverChecked != 0 {
		t.Fatalf("stats = %+v", got)
	}
}

func TestScheduler_EmptySuccessIsChecked(t *testing.T) {
	d := schedulerDemand("Empty", 100, 1)
	s := schedulerFixture(t, 5*time.Minute, []Demand{d})
	takeName(t, s, t0, "Empty")
	successful(t, s, d.Key, t0)
	if got := s.Stats(t0); got.NeverChecked != 0 {
		t.Fatalf("stats = %+v", got)
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("empty success remained never checked")
	}
}

func TestScheduler_FailureLeavesNeverChecked(t *testing.T) {
	d := schedulerDemand("Failed", 100, 1)
	s := schedulerFixture(t, 0, []Demand{d})
	takeName(t, s, t0, "Failed")
	if err := s.Retry(d.Key, t0.Add(time.Hour)); err != nil {
		t.Fatal(err)
	}
	if got := s.Stats(t0); got.NeverChecked != 1 {
		t.Fatalf("stats = %+v", got)
	}
}

func TestScheduler_RetryBoundary(t *testing.T) {
	for _, tc := range []struct {
		name   string
		offset time.Duration
		want   bool
	}{{"before", -time.Nanosecond, false}, {"at", 0, true}} {
		t.Run(tc.name, func(t *testing.T) {
			d := schedulerDemand("Retry", 1, 1)
			s := schedulerFixture(t, 0, []Demand{d})
			if err := s.Retry(d.Key, t0.Add(time.Hour)); err != nil {
				t.Fatal(err)
			}
			got, ok := s.Take(t0.Add(time.Hour+tc.offset), resolve.Rate{})
			if ok != tc.want || (ok && got.Key != d.Key) {
				t.Fatalf("Take = %v, %v; want eligibility %v", got.Key, ok, tc.want)
			}
		})
	}
}

func TestScheduler_DueBoundaryOverridesMinimumAge(t *testing.T) {
	for _, tc := range []struct {
		name    string
		offset  time.Duration
		want    bool
		overdue int
	}{{"before", -time.Nanosecond, false, 0}, {"at", 0, true, 0}, {"after", time.Nanosecond, true, 1}} {
		t.Run(tc.name, func(t *testing.T) {
			d := schedulerDemand("Due", 1, 1)
			s := schedulerFixture(t, 24*time.Hour, []Demand{d}, tradeObs(d.Key, t0, nil))
			at := t0.Add(6*time.Hour + tc.offset)
			got, ok := s.Take(at, resolve.Rate{})
			if ok != tc.want || (ok && got.Key != d.Key) {
				t.Fatalf("Take = %v, %v; want %v", got.Key, ok, tc.want)
			}
			if stats := s.Stats(at); stats.Overdue != tc.overdue {
				t.Fatalf("stats = %+v; want overdue %d", stats, tc.overdue)
			}
		})
	}
}

func TestScheduler_OpportunisticMinimumAgeBoundary(t *testing.T) {
	for _, tc := range []struct {
		name   string
		offset time.Duration
		want   bool
	}{{"before", -time.Nanosecond, false}, {"at", 0, true}} {
		t.Run(tc.name, func(t *testing.T) {
			d := schedulerDemand("Recent", 1, 1)
			s := schedulerFixture(t, 5*time.Minute, []Demand{d}, tradeObs(d.Key, t0, nil))
			_, ok := s.Take(t0.Add(5*time.Minute+tc.offset), resolve.Rate{})
			if ok != tc.want {
				t.Fatalf("eligible = %v; want %v", ok, tc.want)
			}
		})
	}
}

func TestScheduler_TakeRecomputesAgePriority(t *testing.T) {
	a, b := schedulerDemand("Risk", 10000, 1), schedulerDemand("Old", 1, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), nil), tradeObs(b.Key, t0.Add(-5*time.Hour), nil))
	d := takeName(t, s, t0, "Risk")
	if err := s.Retry(d.Key, t0); err != nil {
		t.Fatal(err)
	}
	takeName(t, s, t0.Add(time.Hour), "Old")
}

func TestScheduler_EqualPriorityUsesCanonicalTies(t *testing.T) {
	for _, age := range []time.Duration{0, time.Hour, 7 * time.Hour} {
		t.Run(age.String(), func(t *testing.T) {
			a, b := schedulerDemand("A", 1, 1), schedulerDemand("B", 1, 1)
			var checks []resolve.Observation
			if age > 0 {
				checks = []resolve.Observation{tradeObs(a.Key, t0.Add(-age), nil), tradeObs(b.Key, t0.Add(-age), nil)}
			}
			s := schedulerFixture(t, 0, []Demand{b, a}, checks...)
			takeName(t, s, t0, "A")
			takeName(t, s, t0, "B")
		})
	}
}

func TestScheduler_ReplacementRemovesExcludedOverdueKeys(t *testing.T) {
	a, b := schedulerDemand("Removed", 100, 1), schedulerDemand("Retained", 1, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-10*time.Hour), nil))
	if err := s.Force(a.Key); err != nil {
		t.Fatal(err)
	}
	if err := s.Reconcile([]Demand{b}, nil); err != nil {
		t.Fatal(err)
	}
	if got := s.Stats(t0); got != (SchedulerStats{Demand: 1, NeverChecked: 1}) {
		t.Fatalf("stats = %+v", got)
	}
	takeName(t, s, t0, "Retained")
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("removed key selected")
	}
}

func TestScheduler_RepeatedReplacementDoesNotDuplicateWork(t *testing.T) {
	d := schedulerDemand("Once", 1, 1)
	s := schedulerFixture(t, 0, []Demand{d})
	takeName(t, s, t0, "Once")
	for range 3 {
		if err := s.Reconcile([]Demand{d}, nil); err != nil {
			t.Fatal(err)
		}
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("replacement duplicated an in-flight key")
	}
}

func TestScheduler_EmptyReplacementClearsCandidates(t *testing.T) {
	d := schedulerDemand("Removed", 1, 1)
	s := schedulerFixture(t, 0, []Demand{d})
	if err := s.Reconcile(nil, nil); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("empty demand selected work")
	}
	if got := s.Stats(t0); got != (SchedulerStats{}) {
		t.Fatalf("stats = %+v", got)
	}
}

func TestScheduler_ForcePrecedesOrdinaryWork(t *testing.T) {
	a, b := schedulerDemand("Ordinary", 100, 1), schedulerDemand("Forced", 1, 1)
	s := schedulerFixture(t, time.Hour, []Demand{a, b}, tradeObs(b.Key, t0, nil))
	for range 2 {
		if err := s.Force(b.Key); err != nil {
			t.Fatal(err)
		}
	}
	takeName(t, s, t0, "Forced")
	successful(t, s, b.Key, t0)
	takeName(t, s, t0, "Ordinary")
}

func TestScheduler_ForceRespectsRetry(t *testing.T) {
	d := schedulerDemand("Forced", 1, 1)
	s := schedulerFixture(t, 0, []Demand{d})
	if err := s.Retry(d.Key, t0.Add(time.Hour)); err != nil {
		t.Fatal(err)
	}
	if err := s.Force(d.Key); err != nil {
		t.Fatal(err)
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("force bypassed retry")
	}
	takeName(t, s, t0.Add(time.Hour), "Forced")
}

func TestScheduler_InvalidInputPreservesPreviousDemand(t *testing.T) {
	valid := schedulerDemand("Valid", 1, 1)
	cases := map[string]func(*Demand){
		"foreign":              func(d *Demand) { d.Key.League = "Temp-Foreign" },
		"alias":                func(d *Demand) { d.Key.Variant = "1/0" },
		"currency":             func(d *Demand) { d.Key.Kind = resolve.KindCurrency },
		"empty name":           func(d *Demand) { d.Key.Name = "" },
		"invalid variant":      func(d *Demand) { d.Key.Variant = "garbage" },
		"nan exposure":         func(d *Demand) { d.PotentialContribution = math.NaN() },
		"infinite exposure":    func(d *Demand) { d.PotentialContribution = math.Inf(1) },
		"negative probability": func(d *Demand) { d.PickProbability = -1 },
		"large probability":    func(d *Demand) { d.PickProbability = 1.1 },
		"missing stamp":        func(d *Demand) { d.SnapshotTime = time.Time{} },
		"wrong observation":    func(d *Demand) { d.Ninja.Source = resolve.SourceTrade },
	}
	for name, change := range cases {
		t.Run(name, func(t *testing.T) {
			s := schedulerFixture(t, 0, []Demand{valid})
			bad := schedulerDemand("Bad", 1, 1)
			change(&bad)
			if err := s.Reconcile([]Demand{bad}, nil); err == nil {
				t.Fatal("invalid demand accepted")
			}
			takeName(t, s, t0, "Valid")
		})
	}
}

func TestScheduler_ForeignMutationsAreRefused(t *testing.T) {
	d := schedulerDemand("Local", 1, 1)
	foreign := resolve.GemKey("Temp-Foreign", "Other", "1")
	for _, op := range []string{"reconcile", "success", "retry", "force"} {
		t.Run(op, func(t *testing.T) {
			s := schedulerFixture(t, 0, []Demand{d})
			var err error
			switch op {
			case "reconcile":
				err = s.Reconcile(nil, []resolve.Observation{tradeObs(foreign, t0, nil)})
			case "success":
				err = s.Success(tradeObs(foreign, t0, nil))
			case "retry":
				err = s.Retry(foreign, t0)
			case "force":
				err = s.Force(foreign)
			}
			if err == nil {
				t.Fatal("foreign operation accepted")
			}
			takeName(t, s, t0, "Local")
		})
	}
}

func TestScheduler_TradeOnlyCheckDoesNotCreateDemand(t *testing.T) {
	d := schedulerDemand("History", 100, 1)
	s := schedulerFixture(t, 0, nil, tradeObs(d.Key, t0.Add(-24*time.Hour), chaos(100)))
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("historical check created demand")
	}
}

func TestScheduler_InvalidConstructorIsRefused(t *testing.T) {
	for _, tc := range []struct {
		name   string
		scope  league.Scope
		age    time.Duration
		count  int
		minAge time.Duration
	}{
		{"unscoped", league.Scope{}, time.Hour, 3, 0}, {"zero age", league.Historical(leagueID), 0, 3, 0},
		{"zero listings", league.Historical(leagueID), time.Hour, 0, 0}, {"negative minimum", league.Historical(leagueID), time.Hour, 3, -1},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := NewScheduler(tc.scope, resolve.Policy{ListingCount: tc.count, MaxRevisitAge: tc.age}, tc.minAge); err == nil {
				t.Fatal("invalid constructor accepted")
			}
		})
	}
}

func TestScheduler_FinitePopulationCompletesCoverage(t *testing.T) {
	var ds []Demand
	for i := 0; i < 8; i++ {
		ds = append(ds, schedulerDemand(fmt.Sprintf("Gem-%d", i), float64(i+1), 1))
	}
	s := schedulerFixture(t, 5*time.Minute, ds)
	visited := map[resolve.Key]bool{}
	for i := 0; i < 8; i++ {
		at := t0.Add(time.Duration(i) * 45 * time.Second)
		d, ok := s.Take(at, resolve.Rate{})
		if !ok || visited[d.Key] {
			t.Fatalf("coverage step %d repeated/missed %v", i, d.Key)
		}
		visited[d.Key] = true
		successful(t, s, d.Key, at)
	}
	if got := s.Stats(t0.Add(6 * time.Minute)); got.NeverChecked != 0 || got.Overdue != 0 {
		t.Fatalf("coverage stats = %+v", got)
	}
}

func TestScheduler_OverloadRetainsDeferredOverdueDiagnostics(t *testing.T) {
	var ds []Demand
	var checks []resolve.Observation
	for i := 0; i < 5; i++ {
		d := schedulerDemand(fmt.Sprintf("Gem-%d", i), float64(i+1), 1)
		ds = append(ds, d)
		checks = append(checks, tradeObs(d.Key, t0.Add(-6*time.Hour), nil))
	}
	s := schedulerFixture(t, 0, ds, checks...)
	for i := 0; i < 2; i++ {
		d, _ := s.Take(t0, resolve.Rate{})
		successful(t, s, d.Key, t0)
	}
	if err := s.Retry(ds[0].Key, t0.Add(24*time.Hour)); err != nil {
		t.Fatal(err)
	}
	got := s.Stats(t0.Add(time.Minute))
	if got.Demand != 5 || got.Overdue != 3 || got.OldestAge != 6*time.Hour+time.Minute {
		t.Fatalf("overload stats = %+v", got)
	}
}

func TestScheduler_OwnsInputAndReturnedPrices(t *testing.T) {
	a, b := schedulerDemand("A", 100, 1), schedulerDemand("B", 150, 1)
	obs := tradeObs(a.Key, t0.Add(-time.Hour), chaos(100))
	s := schedulerFixture(t, 0, []Demand{a, b}, obs, tradeObs(b.Key, t0.Add(-time.Hour), chaos(150)))
	a.Ninja.Prices[0].Amount = 10000
	obs.Prices[0].Amount = 10000
	d := takeName(t, s, t0, "B")
	d.Ninja.Prices[0].Amount = 1
	if err := s.Retry(d.Key, t0); err != nil {
		t.Fatal(err)
	}
	takeName(t, s, t0, "B")
}

func TestScheduler_ConcurrentReconciliationPreservesNewestSuccess(t *testing.T) {
	d := schedulerDemand("Concurrent", 1, 1)
	s := schedulerFixture(t, time.Hour, []Demand{d})
	var wg sync.WaitGroup
	for i := 0; i < 20; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			if err := s.Reconcile([]Demand{d}, []resolve.Observation{tradeObs(d.Key, t0.Add(-time.Duration(i)*time.Hour), nil)}); err != nil {
				t.Error(err)
			}
			if err := s.Success(tradeObs(d.Key, t0, nil)); err != nil {
				t.Error(err)
			}
			s.Stats(t0)
			s.Take(t0, resolve.Rate{})
		}(i)
	}
	wg.Wait()
	if got := s.Stats(t0); !reflect.DeepEqual(got, SchedulerStats{Demand: 1}) {
		t.Fatalf("concurrent stats = %+v", got)
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("older reconciliation undid fresh success")
	}
}

func TestScheduler_OpportunisticRiskIncludesCheckAge(t *testing.T) {
	a, b := schedulerDemand("Young", 100, 1), schedulerDemand("Old", 80, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), nil), tradeObs(b.Key, t0.Add(-5*time.Hour), nil))
	takeName(t, s, t0, "Old")
}

func TestScheduler_DueTiesUseFontExposure(t *testing.T) {
	a, b := schedulerDemand("A", 1, 1), schedulerDemand("B", 100, 1)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-7*time.Hour), nil), tradeObs(b.Key, t0.Add(-7*time.Hour), nil))
	takeName(t, s, t0, "B")
}

func TestScheduler_EqualRiskUsesOlderCheck(t *testing.T) {
	a, b := schedulerDemand("A", 0, 0), schedulerDemand("B", 0, 0)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), nil), tradeObs(b.Key, t0.Add(-2*time.Hour), nil))
	takeName(t, s, t0, "B")
}

func TestScheduler_InvalidSourceAmountsDoNotPoisonRanking(t *testing.T) {
	a, b := schedulerDemand("A", 0, 0), schedulerDemand("B", 1, 1)
	a.Ninja.Prices = chaos(math.NaN(), math.Inf(1), -100)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), a.Ninja.Prices), tradeObs(b.Key, t0.Add(-time.Hour), nil))
	takeName(t, s, t0, "B")
}

func TestScheduler_ExtremePricesKeepZeroProbabilityAtZeroRisk(t *testing.T) {
	a, b := schedulerDemand("A", 0, 0), schedulerDemand("B", 1, 1)
	a.Ninja.Prices = chaos(math.MaxFloat64)
	s := schedulerFixture(t, 0, []Demand{a, b}, tradeObs(a.Key, t0.Add(-time.Hour), chaos(1)), tradeObs(b.Key, t0.Add(-time.Hour), nil))
	takeName(t, s, t0, "B")
}

func TestScheduler_DuplicateDemandIsRejectedAtomically(t *testing.T) {
	d := schedulerDemand("Kept", 1, 1)
	s := schedulerFixture(t, 0, []Demand{d})
	bad := schedulerDemand("Duplicate", 1, 1)
	if err := s.Reconcile([]Demand{bad, bad}, nil); err == nil {
		t.Fatal("duplicate demand accepted")
	}
	takeName(t, s, t0, "Kept")
}

func TestScheduler_InvalidCheckDoesNotReplaceDemand(t *testing.T) {
	for _, tc := range []struct {
		name   string
		change func(*resolve.Observation)
	}{
		{"missing time", func(o *resolve.Observation) { o.At = time.Time{} }},
		{"wrong source", func(o *resolve.Observation) { o.Source = resolve.SourceNinja }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			d := schedulerDemand("Kept", 1, 1)
			s := schedulerFixture(t, 0, []Demand{d})
			o := tradeObs(d.Key, t0, nil)
			tc.change(&o)
			if err := s.Reconcile(nil, []resolve.Observation{o}); err == nil {
				t.Fatal("invalid check accepted")
			}
			takeName(t, s, t0, "Kept")
		})
	}
}

func TestScheduler_AbsentForceKeyIsRefused(t *testing.T) {
	d := schedulerDemand("Absent", 1, 1)
	s := schedulerFixture(t, 0, nil)
	if err := s.Force(d.Key); err == nil {
		t.Fatal("absent force accepted")
	}
	if _, ok := s.Take(t0, resolve.Rate{}); ok {
		t.Fatal("absent force created demand")
	}
}

func TestScheduler_FailedAttemptRetainsSuccessfulDeadline(t *testing.T) {
	d := schedulerDemand("Failed", 1, 1)
	s := schedulerFixture(t, 0, []Demand{d}, tradeObs(d.Key, t0.Add(-7*time.Hour), nil))
	takeName(t, s, t0, "Failed")
	if err := s.Retry(d.Key, t0.Add(24*time.Hour)); err != nil {
		t.Fatal(err)
	}
	if got := s.Stats(t0.Add(time.Hour)); got.Overdue != 1 || got.OldestAge != 8*time.Hour {
		t.Fatalf("deferred stats = %+v", got)
	}
	if _, ok := s.Take(t0.Add(time.Hour), resolve.Rate{}); ok {
		t.Fatal("retry ignored")
	}
}

func TestScheduler_ReplacementUsesCurrentSnapshotExposure(t *testing.T) {
	a, b := schedulerDemand("A", 100, 1), schedulerDemand("B", 1, 1)
	s := schedulerFixture(t, 0, []Demand{a, b})
	a.PotentialContribution = 0
	if err := s.Reconcile([]Demand{a, b}, nil); err != nil {
		t.Fatal(err)
	}
	takeName(t, s, t0, "B")
}

func TestScheduler_VariantChecksRemainSeparate(t *testing.T) {
	var ds []Demand
	for _, v := range []string{"1", "1/20", "20", "20/20"} {
		d := schedulerDemand("Siege Ballista", 1, 1)
		d.Key = resolve.GemKey(leagueID, d.Key.Name, v)
		d.Ninja.Key = d.Key
		ds = append(ds, d)
	}
	s := schedulerFixture(t, time.Hour, ds, tradeObs(ds[0].Key, t0, nil))
	for _, want := range ds[1:] {
		got, ok := s.Take(t0, resolve.Rate{})
		if !ok || got.Key != want.Key {
			t.Fatalf("Take = %v, %v; want %v", got.Key, ok, want.Key)
		}
	}
	if got := s.Stats(t0); got.NeverChecked != 3 {
		t.Fatalf("variant stats = %+v", got)
	}
}
