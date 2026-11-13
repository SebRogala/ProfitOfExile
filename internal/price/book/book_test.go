package book

import (
	"context"
	"errors"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

var (
	t0       = time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	leagueID = "Temp-POE307"
	ballista = resolve.GemKey(leagueID, "Siege Ballista of Splintering", "20/20")
	arc      = resolve.GemKey(leagueID, "Arc", "20/20")
)

type fakeTrade struct {
	calls int
	obs   []resolve.Observation
	err   error
}

func (f *fakeTrade) ObservationsAsOf(context.Context, league.Scope, time.Time) ([]resolve.Observation, error) {
	f.calls++
	return f.obs, f.err
}

type fakeNinja struct {
	calls int
	obs   []resolve.Observation
}

func (f *fakeNinja) GemObservationsAsOf(context.Context, league.Scope, time.Time) ([]resolve.Observation, error) {
	f.calls++
	return f.obs, nil
}

func chaos(amounts ...float64) []resolve.Money {
	out := make([]resolve.Money, len(amounts))
	for i, a := range amounts {
		out[i] = resolve.Money{Amount: a, Currency: "chaos"}
	}
	return out
}

func intPtr(n int) *int { return &n }

func tradeObs(key resolve.Key, at time.Time, prices []resolve.Money) resolve.Observation {
	return resolve.Observation{Key: key, Source: resolve.SourceTrade, At: at, Prices: prices}
}

func ninjaObs(key resolve.Key, chaosPrice float64, sample int) resolve.Observation {
	return resolve.Observation{Key: key, Source: resolve.SourceNinja, At: t0.Add(-2 * time.Hour),
		Prices: chaos(chaosPrice), SampleCount: intPtr(sample)}
}

// gemPolicy mirrors lab.GemPricePolicy; the book must not import internal/lab.
var gemPolicy = resolve.Policy{ListingCount: 3, Disagreement: 3, ThinDisagreement: 2, ThinSample: 5}

type fixture struct {
	book  *Book
	trade *fakeTrade
	ninja *fakeNinja
}

// seeded builds a Temp-POE307 book over trade S3 (40c, 50c, 55c) and ninja
// Ballista 20/20 1000c sample 2 at rate 200, and seeds it at T0.
func seeded(t *testing.T, tradePrices []resolve.Money) fixture {
	t.Helper()
	tr := &fakeTrade{obs: []resolve.Observation{tradeObs(ballista, t0.Add(-time.Hour), tradePrices)}}
	nj := &fakeNinja{obs: []resolve.Observation{ninjaObs(ballista, 1000, 2), ninjaObs(arc, 100, 50)}}
	b := New(league.Historical(leagueID), gemPolicy, tr, nj, func() resolve.Rate { return resolve.Rate{Chaos: 200} })
	if err := b.Seed(context.Background(), t0); err != nil {
		t.Fatalf("Seed: %v", err)
	}
	return fixture{book: b, trade: tr, ninja: nj}
}

func s3() []resolve.Money { return chaos(40, 50, 55) }

func TestBook_seedPublishesTheResolvedTradePrice(t *testing.T) {
	f := seeded(t, s3())

	got, ok := f.book.Get(ballista)

	if !ok {
		t.Fatal("Get(Ballista) ok = false, want the seeded trade price")
	}
	if got.Source != resolve.SourceTrade || got.Chaos != 50 {
		t.Errorf("Get(Ballista) = %s %v, want trade 50", got.Source, got.Chaos)
	}
	if want := []resolve.Reason{resolve.ReasonDisagreement}; !reflect.DeepEqual(got.Reasons, want) {
		t.Errorf("Reasons = %v, want %v (50 vs thin ninja 1000)", got.Reasons, want)
	}
}

func TestBook_readsNeverCallThePorts(t *testing.T) {
	f := seeded(t, s3())

	for range 1000 {
		f.book.Get(ballista)
	}

	if f.trade.calls != 1 || f.ninja.calls != 1 {
		t.Errorf("port calls after 1000 Gets: trade %d ninja %d, want 1 and 1", f.trade.calls, f.ninja.calls)
	}
}

func TestBook_missingKeyReadsNeverCallThePorts(t *testing.T) {
	f := seeded(t, s3())
	unseeded := resolve.GemKey(leagueID, "Unseeded Gem", "20/20")

	found := false
	for range 1000 {
		if _, ok := f.book.Get(unseeded); ok {
			found = true
		}
	}

	if found {
		t.Error("Get(unseeded) ok = true, want false")
	}
	if f.trade.calls != 1 || f.ninja.calls != 1 {
		t.Errorf("port calls after 1000 missing Gets: trade %d ninja %d, want 1 and 1", f.trade.calls, f.ninja.calls)
	}
}

func TestBook_observeReResolvesThatKeyOnly(t *testing.T) {
	f := seeded(t, s3())
	arcBefore, _ := f.book.Get(arc)

	if err := f.book.Observe(tradeObs(ballista, t0.Add(time.Hour), chaos(70, 70, 70))); err != nil {
		t.Fatalf("Observe: %v", err)
	}

	if got, _ := f.book.Get(ballista); got.Chaos != 70 {
		t.Errorf("Get(Ballista).Chaos = %v, want 70", got.Chaos)
	}
	if arcAfter, _ := f.book.Get(arc); !reflect.DeepEqual(arcAfter, arcBefore) {
		t.Errorf("Get(Arc) = %+v, want the pre-Observe entry %+v unchanged", arcAfter, arcBefore)
	}
}

func TestBook_observeLeavesThePreviousSnapshotUnchanged(t *testing.T) {
	f := seeded(t, s3())
	before := f.book.Snapshot()

	if err := f.book.Observe(tradeObs(ballista, t0.Add(time.Hour), chaos(70, 70, 70))); err != nil {
		t.Fatalf("Observe: %v", err)
	}

	if got, _ := before.Get(ballista); got.Chaos != 50 {
		t.Errorf("earlier snapshot Get(Ballista).Chaos = %v, want 50", got.Chaos)
	}
}

func TestBook_setRateConvertsDivineListingsAtTheNewRate(t *testing.T) {
	sd := []resolve.Money{{Amount: 0.25, Currency: "divine"}, {Amount: 40, Currency: "chaos"}, {Amount: 55, Currency: "chaos"}}
	f := seeded(t, sd)
	if got, _ := f.book.Get(ballista); got.Chaos != 50 {
		t.Fatalf("seeded at 200: Chaos = %v, want 50", got.Chaos)
	}

	f.book.SetRate(resolve.Rate{Chaos: 250})

	if got, _ := f.book.Get(ballista); got.Chaos != 55 {
		t.Errorf("after SetRate(250): Chaos = %v, want 55 (62.5, 40, 55)", got.Chaos)
	}
	if f.trade.calls != 1 || f.ninja.calls != 1 {
		t.Errorf("port calls after SetRate: trade %d ninja %d, want 1 and 1", f.trade.calls, f.ninja.calls)
	}
}

func TestBook_observationOfAnotherLeagueIsRefused(t *testing.T) {
	f := seeded(t, s3())
	other := resolve.GemKey("Temp-B", "Siege Ballista of Splintering", "20/20")

	err := f.book.Observe(tradeObs(other, t0, s3()))

	if err == nil {
		t.Error("Observe(Temp-B key) err = nil, want a refusal")
	}
	if _, ok := f.book.Get(other); ok {
		t.Error("Get(Temp-B key) ok = true, want false")
	}
}

func TestBook_failedSeedKeepsThePreviousSnapshot(t *testing.T) {
	f := seeded(t, s3())
	f.trade.err = errors.New("db down")

	err := f.book.Seed(context.Background(), t0.Add(time.Hour))

	if err == nil {
		t.Error("second Seed err = nil, want the trade port error")
	}
	if got, ok := f.book.Get(ballista); !ok || got.Chaos != 50 {
		t.Errorf("Get(Ballista) = %v ok=%v, want 50 from the previous snapshot", got.Chaos, ok)
	}
}
