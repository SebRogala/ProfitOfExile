package harvest

import (
	"testing"
	"time"

	"profitofexile/internal/exchange"
)

const testItemX = "Metadata/Items/Currency/TestItemX"

var newest = time.Date(2026, 10, 13, 8, 0, 0, 0, time.UTC)

// row builds a stored row with ItemA = a and ItemB = b.
func row(hour time.Time, a, b string, va, vb int64) exchange.StoredRow {
	return exchange.StoredRow{Hour: hour, Row: exchange.Row{ItemA: a, ItemB: b, VolumeA: va, VolumeB: vb}}
}

func hoursBefore(h int) time.Time {
	return newest.Add(-time.Duration(h) * time.Hour)
}

func assertPrice(t *testing.T, name string, got *float64, want float64) {
	t.Helper()
	if got == nil {
		t.Fatalf("%s = nil, want %v", name, want)
	}
	if *got != want {
		t.Fatalf("%s = %v, want %v", name, *got, want)
	}
}

func assertNil(t *testing.T, name string, got *float64) {
	t.Helper()
	if got != nil {
		t.Fatalf("%s = %v, want nil", name, *got)
	}
}

func TestComputePrices_pooledVWAPOverTheWindow(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 10, 100),
		row(hoursBefore(1), testItemX, exchange.ChaosID, 30, 240),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 8.5)
}

func TestComputePrices_readsEitherOrientation(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), exchange.ChaosID, testItemX, 100, 10),
		row(hoursBefore(1), exchange.ChaosID, testItemX, 240, 30),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 8.5)
}

func TestComputePrices_skipsZeroVolumeRows(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 10, 100),
		row(hoursBefore(1), testItemX, exchange.ChaosID, 30, 240),
		row(hoursBefore(2), testItemX, exchange.ChaosID, 0, 999),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 8.5)
}

func TestComputePrices_skipsZeroQuoteVolume(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 10, 100),
		row(hoursBefore(1), testItemX, exchange.ChaosID, 30, 240),
		row(hoursBefore(2), testItemX, exchange.ChaosID, 50, 0),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 8.5)
}

func TestComputePrices_skipsNegativeVolume(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 10, 100),
		row(hoursBefore(1), testItemX, exchange.ChaosID, 30, 240),
		row(hoursBefore(2), testItemX, exchange.ChaosID, -5, 100),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 8.5)
}

func TestComputePrices_windowIsInclusiveOfBothEnds(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(5), testItemX, exchange.ChaosID, 1, 10),
		row(hoursBefore(0), testItemX, exchange.ChaosID, 1, 20),
		row(hoursBefore(6), testItemX, exchange.ChaosID, 1, 1000),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 15)
}

func TestComputePrices_divineIsItsOwnMarket(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.DivineID, 4, 1),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "divine", got.Items[testItemX].Divine, 0.25)
	assertNil(t, "chaos", got.Items[testItemX].Chaos)
}

func TestComputePrices_lastSeenOnlyWhenUnpriced(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(10), testItemX, exchange.ChaosID, 2, 20),
		row(hoursBefore(30), testItemX, exchange.ChaosID, 1, 50),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertNil(t, "chaos", got.Items[testItemX].Chaos)
	assertPrice(t, "lastSeenChaos", got.Items[testItemX].LastSeenChaos, 10)

	priced := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 2, 20),
		row(hoursBefore(10), testItemX, exchange.ChaosID, 1, 50),
	}
	gotPriced := ComputePrices(priced, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", gotPriced.Items[testItemX].Chaos, 10)
	assertNil(t, "lastSeenChaos of a priced item", gotPriced.Items[testItemX].LastSeenChaos)
}

func TestComputePrices_lastSeenPoolsNewestQualifyingHour(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(10), testItemX, exchange.ChaosID, 3, 60),
		row(hoursBefore(10), testItemX, exchange.ChaosID, 2, 20),
		row(hoursBefore(20), testItemX, exchange.ChaosID, 1, 100),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "lastSeenChaos", got.Items[testItemX].LastSeenChaos, 16)
}

func TestComputePrices_lastSeenSkipsNewerNonqualifyingHour(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(10), testItemX, exchange.ChaosID, 3, 60),
		row(hoursBefore(10), testItemX, exchange.ChaosID, 2, 20),
		row(hoursBefore(8), testItemX, exchange.ChaosID, 0, 999),
		row(hoursBefore(20), testItemX, exchange.ChaosID, 1, 100),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "lastSeenChaos", got.Items[testItemX].LastSeenChaos, 16)
}

func TestComputePrices_unknownItemIsPresentAndUnpriced(t *testing.T) {
	got := ComputePrices(nil, newest, 6, []string{testItemX})
	ip, ok := got.Items[testItemX]
	if !ok {
		t.Fatalf("requested item %s missing from Items", testItemX)
	}
	assertNil(t, "chaos", ip.Chaos)
	assertNil(t, "divine", ip.Divine)
	assertNil(t, "lastSeenChaos", ip.LastSeenChaos)
}

func TestComputePrices_divineChaosRate(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), exchange.DivineID, exchange.ChaosID, 2, 700),
	}
	got := ComputePrices(rows, newest, 6, nil)
	if got.DivineChaosRate != 350 {
		t.Fatalf("DivineChaosRate = %v, want 350", got.DivineChaosRate)
	}

	none := ComputePrices(nil, newest, 6, nil)
	if none.DivineChaosRate != 0 {
		t.Fatalf("DivineChaosRate with no rows = %v, want 0", none.DivineChaosRate)
	}
}

func TestComputePrices_lifeforcePerDivineAndChaos(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforceWild, exchange.ChaosID, 1000, 31),
		row(hoursBefore(1), LifeforceWild, exchange.DivineID, 9905, 1),
	}
	got := ComputePrices(rows, newest, 6, nil)
	wild, ok := got.Lifeforce["Wild"]
	if !ok {
		t.Fatal("Wild missing from Lifeforce")
	}
	if wild.Chaos != 0.031 {
		t.Fatalf("Wild chaos = %v, want 0.031", wild.Chaos)
	}
	if wild.PerDivine != 9905 {
		t.Fatalf("Wild perDivine = %v, want 9905", wild.PerDivine)
	}
	if wild.Colour != "purple" {
		t.Fatalf("Wild colour = %q, want purple", wild.Colour)
	}
	if wild.ItemID != LifeforceWild {
		t.Fatalf("Wild itemId = %q, want %q", wild.ItemID, LifeforceWild)
	}
}

func TestComputePrices_lifeforceWithoutAnyPriceIsOmitted(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforceWild, exchange.DivineID, 9905, 1),
	}
	got := ComputePrices(rows, newest, 6, nil)
	if _, ok := got.Lifeforce["Vivid"]; ok {
		t.Fatal("Vivid present with no rows, want omitted")
	}
}

func TestComputePrices_lifeforceDivineOnlyIsOmitted(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforcePrimal, exchange.DivineID, 9905, 1),
	}
	got := ComputePrices(rows, newest, 6, nil)
	if _, ok := got.Lifeforce["Primal"]; ok {
		t.Fatal("Primal present with divine rows only, want omitted: no chaos quote")
	}
}

func TestComputePrices_lifeforceChaosOnlyIsOmitted(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforcePrimal, exchange.ChaosID, 1000, 31),
	}
	got := ComputePrices(rows, newest, 6, nil)
	if _, ok := got.Lifeforce["Primal"]; ok {
		t.Fatal("Primal present with chaos rows only, want omitted: no divine quote")
	}
}

func TestComputePrices_lifeforceUsesEarlierChaosQuote(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(1), LifeforceWild, exchange.DivineID, 9905, 1),
		row(hoursBefore(30), LifeforceWild, exchange.ChaosID, 1000, 30),
	}
	got := ComputePrices(rows, newest, 6, nil)
	wild, ok := got.Lifeforce["Wild"]
	if !ok {
		t.Fatal("Wild missing; the earlier chaos quote should have priced it")
	}
	if wild.Chaos != 0.03 {
		t.Fatalf("Wild chaos = %v, want 0.03 from the earlier hour", wild.Chaos)
	}
}

func TestComputePrices_lifeforcePerDivineRoundsHalfUp(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforceWild, exchange.ChaosID, 1000, 31),
		row(hoursBefore(1), LifeforceWild, exchange.DivineID, 7, 2),
	}
	got := ComputePrices(rows, newest, 6, nil)
	wild, ok := got.Lifeforce["Wild"]
	if !ok {
		t.Fatal("Wild missing; 7 lifeforce for 2 divine rounds to 4 per divine")
	}
	if wild.PerDivine != 4 {
		t.Fatalf("Wild perDivine = %v, want 4 (3.5 rounded half up)", wild.PerDivine)
	}
}

func TestComputePrices_lifeforcePerDivineBelowHalfIsOmitted(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), LifeforceWild, exchange.ChaosID, 1000, 31),
		row(hoursBefore(1), LifeforceWild, exchange.DivineID, 1, 3),
	}
	got := ComputePrices(rows, newest, 6, nil)
	if _, ok := got.Lifeforce["Wild"]; ok {
		t.Fatalf("Wild present with perDivine %v, want omitted: 0.33 rounds to zero",
			got.Lifeforce["Wild"].PerDivine)
	}
}

func TestComputePrices_windowExcludesHoursAfterNewest(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(0), testItemX, exchange.ChaosID, 1, 10),
		row(newest.Add(time.Hour), testItemX, exchange.ChaosID, 1, 1000),
	}
	got := ComputePrices(rows, newest, 6, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 10)
}

func TestComputePrices_dayWindowSpansTwentyFourHours(t *testing.T) {
	rows := []exchange.StoredRow{
		row(hoursBefore(23), testItemX, exchange.ChaosID, 1, 10),
		row(hoursBefore(24), testItemX, exchange.ChaosID, 1, 1000),
	}
	got := ComputePrices(rows, newest, 24, []string{testItemX})
	assertPrice(t, "chaos", got.Items[testItemX].Chaos, 10)
}
