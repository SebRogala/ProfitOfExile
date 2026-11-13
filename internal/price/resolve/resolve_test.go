package resolve

import (
	"encoding/json"
	"os"
	"reflect"
	"testing"
	"time"
)

var t0 = time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)

var ballista = Key{League: "Temp-POE307", Kind: KindGem, Name: "Siege Ballista of Splintering", Variant: "20/20"}

var policy = Policy{ListingCount: 3, Disagreement: 3, ThinDisagreement: 2, ThinSample: 5}

var rate200 = Rate{Chaos: 200}

func chaos(amounts ...float64) []Money {
	out := make([]Money, len(amounts))
	for i, a := range amounts {
		out[i] = Money{Amount: a, Currency: "chaos"}
	}
	return out
}

func samples(n int) *int { return &n }

func tradeObs(at time.Time, prices []Money) Observation {
	return Observation{Key: ballista, Source: SourceTrade, At: at, Prices: prices}
}

func ninjaObs(at time.Time, price float64, sampleCount int) Observation {
	return Observation{Key: ballista, Source: SourceNinja, At: at, Prices: chaos(price), SampleCount: samples(sampleCount)}
}

// s3 is the hand oracle set 40c, 50c, 55c → median of cheapest 3 = 50.
func s3() []Money { return chaos(40, 50, 55) }

func mustResolve(t *testing.T, obs []Observation, asOf time.Time, rate Rate, p Policy) Resolved {
	t.Helper()
	got, ok := Resolve(obs, asOf, rate, p)
	if !ok {
		t.Fatalf("Resolve ok = false, want true")
	}
	return got
}

func assertReasons(t *testing.T, got []Reason, want ...Reason) {
	t.Helper()
	if len(got) == 0 && len(want) == 0 {
		return
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("Reasons = %v, want %v", got, want)
	}
}

func hasReason(rs []Reason, r Reason) bool {
	for _, x := range rs {
		if x == r {
			return true
		}
	}
	return false
}

type tradePriceVector struct {
	Name       string  `json:"name"`
	DivineRate float64 `json:"divineRate"`
	Listings   []struct {
		Price    float64 `json:"price"`
		Currency string  `json:"currency"`
	} `json:"listings"`
	MedianTop3 float64 `json:"medianTop3"`
}

func TestMedianCheapest_matchesTheSharedTradePriceVectors(t *testing.T) {
	data, err := os.ReadFile("../../trade/testdata/trade-price-vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var vectors []tradePriceVector
	if err := json.Unmarshal(data, &vectors); err != nil {
		t.Fatal(err)
	}
	for _, v := range vectors {
		t.Run(v.Name, func(t *testing.T) {
			prices := make([]Money, len(v.Listings))
			for i, l := range v.Listings {
				prices[i] = Money{Amount: l.Price, Currency: l.Currency}
			}
			got, _ := MedianCheapest(prices, 3, v.DivineRate)
			if got != v.MedianTop3 {
				t.Errorf("MedianCheapest = %v, want %v", got, v.MedianTop3)
			}
		})
	}
}

func TestMedianCheapest_zeroCountIsNoPrice(t *testing.T) {
	got, ok := MedianCheapest(chaos(50), 0, 200)
	if ok || got != 0 {
		t.Errorf("MedianCheapest = %v, %v; want 0, false", got, ok)
	}
}

func TestMedianCheapest_negativeCountIsNoPrice(t *testing.T) {
	got, ok := MedianCheapest(chaos(50), -1, 200)
	if ok || got != 0 {
		t.Errorf("MedianCheapest = %v, %v; want 0, false", got, ok)
	}
}

func TestChaosOf_divineWithoutARateIsNotConvertible(t *testing.T) {
	got, ok := ChaosOf(Money{Amount: 0.25, Currency: "divine"}, 0)
	if ok || got != 0 {
		t.Errorf("ChaosOf = %v, %v; want 0, false", got, ok)
	}
}

func TestChaosOf_unknownCurrencyIsNotConvertible(t *testing.T) {
	got, ok := ChaosOf(Money{Amount: 1, Currency: "exalted"}, 200)
	if ok || got != 0 {
		t.Errorf("ChaosOf = %v, %v; want 0, false", got, ok)
	}
}

func TestResolve_tradeObservationWinsOverNinja(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-time.Hour), s3()),
		ninjaObs(t0.Add(-10*time.Minute), 52, 40),
	}, t0, rate200, policy)
	if got.Source != SourceTrade || got.Chaos != 50 {
		t.Errorf("got %s %v, want trade 50", got.Source, got.Chaos)
	}
	assertReasons(t, got.Reasons)
}

// sd is 0.25 div, 40c, 55c.
func sd() []Money {
	return []Money{{Amount: 0.25, Currency: "divine"}, {Amount: 40, Currency: "chaos"}, {Amount: 55, Currency: "chaos"}}
}

func TestResolve_divineListingsConvertAtTheRatePassedAtResolveTime(t *testing.T) {
	got := mustResolve(t, []Observation{tradeObs(t0.Add(-time.Hour), sd())}, t0, Rate{Chaos: 250}, policy)
	if got.Chaos != 55 {
		t.Errorf("Chaos = %v, want 55", got.Chaos)
	}
}

func TestResolve_divineListingsConvertAtTheRatePassedAtResolveTimeAtTwoHundred(t *testing.T) {
	got := mustResolve(t, []Observation{tradeObs(t0.Add(-time.Hour), sd())}, t0, rate200, policy)
	if got.Chaos != 50 {
		t.Errorf("Chaos = %v, want 50", got.Chaos)
	}
}

func TestResolve_observationsAfterAsOfAreIgnored(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(time.Minute), s3()),
		ninjaObs(t0.Add(-time.Hour), 52, 40),
	}, t0, rate200, policy)
	if got.Source != SourceNinja || got.Chaos != 52 {
		t.Errorf("got %s %v, want ninja 52", got.Source, got.Chaos)
	}
}

func TestResolve_newestTradeObservationWins(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-3*time.Hour), chaos(40, 40, 40)),
		tradeObs(t0.Add(-time.Hour), s3()),
	}, t0, rate200, policy)
	if got.Chaos != 50 {
		t.Errorf("Chaos = %v, want 50", got.Chaos)
	}
}

func TestResolve_neverCheckedItemIsPricedFromNinjaWithItsFlag(t *testing.T) {
	got := mustResolve(t, []Observation{ninjaObs(t0.Add(-time.Hour), 1000, 40)}, t0, rate200, policy)
	if got.Source != SourceNinja || got.Chaos != 1000 {
		t.Errorf("got %s %v, want ninja 1000", got.Source, got.Chaos)
	}
	assertReasons(t, got.Reasons, ReasonNeverChecked)
}

func TestResolve_neverCheckedThinItemCarriesTheThinFlag(t *testing.T) {
	got := mustResolve(t, []Observation{ninjaObs(t0.Add(-time.Hour), 1000, 2)}, t0, rate200, policy)
	assertReasons(t, got.Reasons, ReasonNeverChecked, ReasonThinSample)
}

func TestResolve_thinNinjaDisagreeingBeyondTwoTimesIsFlaggedOnTheTradePrice(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-time.Hour), s3()),
		ninjaObs(t0.Add(-time.Hour), 125, 2),
	}, t0, rate200, policy)
	if got.Source != SourceTrade || got.Chaos != 50 {
		t.Errorf("got %s %v, want trade 50", got.Source, got.Chaos)
	}
	assertReasons(t, got.Reasons, ReasonDisagreement)
}

func TestResolve_thickNinjaAtTwoAndAHalfTimesIsNotADisagreement(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-time.Hour), s3()),
		ninjaObs(t0.Add(-time.Hour), 125, 40),
	}, t0, rate200, policy)
	if got.Source != SourceTrade {
		t.Errorf("Source = %s, want trade", got.Source)
	}
	assertReasons(t, got.Reasons)
}

func TestResolve_thinNinjaAtExactlyTwoTimesIsNotADisagreement(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-time.Hour), s3()),
		ninjaObs(t0.Add(-time.Hour), 100, 2),
	}, t0, rate200, policy)
	if got.Source != SourceTrade {
		t.Errorf("Source = %s, want trade", got.Source)
	}
	assertReasons(t, got.Reasons)
}

func TestResolve_tradeOlderThanMaxRevisitAgeStandsWithAStaleFlag(t *testing.T) {
	p := policy
	p.MaxRevisitAge = 24 * time.Hour
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-25*time.Hour), s3()),
		ninjaObs(t0.Add(-5*time.Minute), 1000, 40),
	}, t0, rate200, p)
	if got.Source != SourceTrade || got.Chaos != 50 {
		t.Errorf("got %s %v, want trade 50", got.Source, got.Chaos)
	}
	if !hasReason(got.Reasons, ReasonStaleEvidence) {
		t.Errorf("Reasons = %v, want stale-evidence among them", got.Reasons)
	}
}

func TestResolve_tradeAtExactlyMaxRevisitAgeIsNotStale(t *testing.T) {
	p := policy
	p.MaxRevisitAge = 24 * time.Hour
	got := mustResolve(t, []Observation{tradeObs(t0.Add(-24*time.Hour), s3())}, t0, rate200, p)
	if got.Source != SourceTrade {
		t.Errorf("Source = %s, want trade", got.Source)
	}
	if hasReason(got.Reasons, ReasonStaleEvidence) {
		t.Errorf("Reasons = %v, want no stale-evidence", got.Reasons)
	}
}

func TestResolve_confidenceNeverScalesThePrice(t *testing.T) {
	p := policy
	p.MaxRevisitAge = 24 * time.Hour
	got := mustResolve(t, []Observation{tradeObs(t0.Add(-400*time.Hour), s3())}, t0, rate200, p)
	if !hasReason(got.Reasons, ReasonStaleEvidence) {
		t.Fatalf("Reasons = %v, want stale-evidence (precondition)", got.Reasons)
	}
	if got.Chaos != 50 {
		t.Errorf("Chaos = %v, want 50", got.Chaos)
	}
}

func TestResolve_tradeWithOnlyUnconvertiblePricesFallsBackToNinjaWithTheNoRateFlag(t *testing.T) {
	got := mustResolve(t, []Observation{
		tradeObs(t0.Add(-time.Hour), []Money{{Amount: 0.25, Currency: "divine"}}),
		ninjaObs(t0.Add(-time.Hour), 52, 40),
	}, t0, Rate{}, policy)
	if got.Source != SourceNinja || got.Chaos != 52 {
		t.Errorf("got %s %v, want ninja 52", got.Source, got.Chaos)
	}
	assertReasons(t, got.Reasons, ReasonNoDivineRate)
}

func TestResolve_noObservationIsNoPrice(t *testing.T) {
	if got, ok := Resolve(nil, t0, rate200, policy); ok {
		t.Errorf("Resolve(nil) = %+v, true; want ok false", got)
	}
}

func TestResolve_ageIsMeasuredFromAsOf(t *testing.T) {
	got := mustResolve(t, []Observation{tradeObs(t0.Add(-90*time.Minute), s3())}, t0, rate200, policy)
	if got.Age != 90*time.Minute {
		t.Errorf("Age = %v, want 90m", got.Age)
	}
}
