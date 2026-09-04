package exchange

import (
	"math"
	"testing"
	"time"
)

// feedHour is the hour the recorded fixture describes (unix 1787119200) and the
// base of every synthetic window the engine tests build.
var feedHour = time.Date(2026, 8, 19, 6, 0, 0, 0, time.UTC)

// rowSpec describes one synthetic normalized market. Every [2]int64 is
// {valueForItemA, valueForItemB} in market_pair order — the same convention
// marketSpec uses for raw feed markets, one layer down.
type rowSpec struct {
	itemA        string
	itemB        string
	volume       [2]int64
	lowestStock  [2]int64
	highestStock [2]int64
	lowestRatio  [2]int64
	highestRatio [2]int64
	// priceInvalid renders the row the way Normalize renders a zero-ratio
	// market: quantities intact, PriceValid false.
	priceInvalid bool
}

// row renders the spec as the engine sees it. MarketID follows the feed's "A|B"
// shape and PriceValid is true unless the spec says otherwise, so each test sets
// only the quantities its case is about.
func (s rowSpec) row() Row {
	return Row{
		League:        "Allflame",
		MarketID:      s.itemA + "|" + s.itemB,
		ItemA:         s.itemA,
		ItemB:         s.itemB,
		VolumeA:       s.volume[0],
		VolumeB:       s.volume[1],
		LowestStockA:  s.lowestStock[0],
		LowestStockB:  s.lowestStock[1],
		HighestStockA: s.highestStock[0],
		HighestStockB: s.highestStock[1],
		LowestRatioA:  s.lowestRatio[0],
		LowestRatioB:  s.lowestRatio[1],
		HighestRatioA: s.highestRatio[0],
		HighestRatioB: s.highestRatio[1],
		PriceValid:    !s.priceInvalid,
	}
}

// chaosDivineSpec is the fixture's chaos/divine market as a synthetic row: chaos
// is ItemA, divine is ItemB, one divine went for 196 chaos at the cheapest and
// 201 at the dearest, and both sides are far above any depth floor. Quantities
// are the ones recorded in testdata/hour_allflame_sample.json.
func chaosDivineSpec() rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        divineID,
		volume:       [2]int64{13001051, 65361},
		lowestStock:  [2]int64{4169809, 5444},
		highestStock: [2]int64{4564191, 8878},
		lowestRatio:  [2]int64{196, 1},
		highestRatio: [2]int64{201, 1},
	}
}

// wantClose fails when got is not want to within a relative 1e-12, the slack
// that covers reassociating the same multiplications and divisions. It is used
// for values the engine reaches through several float operations; single-Ratio
// prices are compared exactly.
func wantClose(t *testing.T, label string, got, want float64) {
	t.Helper()
	if math.IsNaN(got) || math.IsInf(got, 0) {
		t.Fatalf("%s = %v, want %v", label, got, want)
	}
	if diff := math.Abs(got - want); diff > 1e-12*math.Max(1, math.Abs(want)) {
		t.Errorf("%s = %v, want %v (off by %v)", label, got, want, diff)
	}
}

func TestPriceIn_chaosDivineRow_pricesBothDirectionsOfTheSameQuantities(t *testing.T) {
	row := chaosDivineSpec().row()

	tests := []struct {
		name     string
		item     string
		quote    string
		wantLow  float64
		wantHigh float64
	}{
		{
			name:     "one divine cost between 196 and 201 chaos",
			item:     divineID,
			quote:    chaosID,
			wantLow:  196,
			wantHigh: 201,
		},
		{
			// The reverse direction swaps which stored ratio pair is the low
			// and which is the high: the cheapest chaos comes from the pair
			// that priced divine dearest.
			name:     "one chaos cost between 1/201 and 1/196 divine",
			item:     chaosID,
			quote:    divineID,
			wantLow:  1.0 / 201.0,
			wantHigh: 1.0 / 196.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			low, high, ok := priceIn(row, tt.item, tt.quote)
			if !ok {
				t.Fatalf("ok = false, want true for %s priced in %s", tt.item, tt.quote)
			}
			if low.price != tt.wantLow {
				t.Errorf("low = %v, want %v", low.price, tt.wantLow)
			}
			if high.price != tt.wantHigh {
				t.Errorf("high = %v, want %v", high.price, tt.wantHigh)
			}
		})
	}
}

func TestPriceIn_carriesTheIntegerPairEachExtremeWasPostedAs(t *testing.T) {
	// The in-game exchange only posts whole quantities, so the pair is what a
	// player can actually enter and the float is not: one divine for 196 chaos
	// is an order, 0.005102 divine per chaos is not. Reading the market the
	// other way round transposes the pair rather than inverting a float, and it
	// takes the pair from the OTHER stored ratio map — the cheapest chaos comes
	// out of the pair that priced divine dearest.
	row := chaosDivineSpec().row()

	tests := []struct {
		name     string
		item     string
		quote    string
		wantLow  pricePoint
		wantHigh pricePoint
	}{
		{
			name:     "buy 1 divine for 196 chaos, sell it for 201",
			item:     divineID,
			quote:    chaosID,
			wantLow:  pricePoint{price: 196, itemQty: 1, quoteQty: 196},
			wantHigh: pricePoint{price: 201, itemQty: 1, quoteQty: 201},
		},
		{
			name:     "buy 201 chaos for 1 divine, sell 196 for 1",
			item:     chaosID,
			quote:    divineID,
			wantLow:  pricePoint{price: 1.0 / 201.0, itemQty: 201, quoteQty: 1},
			wantHigh: pricePoint{price: 1.0 / 196.0, itemQty: 196, quoteQty: 1},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			low, high, ok := priceIn(row, tt.item, tt.quote)
			if !ok {
				t.Fatalf("ok = false, want true for %s priced in %s", tt.item, tt.quote)
			}
			if low != tt.wantLow {
				t.Errorf("low = %+v, want %+v", low, tt.wantLow)
			}
			if high != tt.wantHigh {
				t.Errorf("high = %+v, want %+v", high, tt.wantHigh)
			}
		})
	}
}

func TestPriceIn_everyPricedFixtureRow_pairDividesExactlyIntoItsPrice(t *testing.T) {
	// The wire contract the desktop renders from: Ratio(quoteQty, itemQty) is
	// the leg's Price, to the bit. It holds because pointOf builds both from one
	// division — a pair copied off the row beside a price fetched from anywhere
	// else would drift on the first market whose quantities are not a power of
	// two, and the fixture hour carries 23 of them in both directions.
	rows, _ := Normalize(loadFixtureHour(t))

	checked := 0
	for _, row := range rows {
		if !row.PriceValid {
			continue
		}
		for _, direction := range []struct {
			item  string
			quote string
		}{
			{item: row.ItemB, quote: row.ItemA},
			{item: row.ItemA, quote: row.ItemB},
		} {
			low, high, ok := priceIn(row, direction.item, direction.quote)
			if !ok {
				t.Errorf("%s: priced row refused for %s in %s", row.MarketID, direction.item, direction.quote)
				continue
			}
			for _, point := range []pricePoint{low, high} {
				checked++
				want, ok := Ratio(point.quoteQty, point.itemQty)
				if !ok {
					t.Errorf("%s: pair %d/%d has no price", row.MarketID, point.quoteQty, point.itemQty)
					continue
				}
				if point.price != want {
					t.Errorf("%s: price %v != Ratio(%d, %d) = %v",
						row.MarketID, point.price, point.quoteQty, point.itemQty, want)
				}
			}
		}
	}

	// 23 priced rows, two directions, two extremes each.
	if checked != 92 {
		t.Errorf("checked %d price points, want 92", checked)
	}
}

func TestPriceIn_everyPricedFixtureRow_recordedPairsAreReduced(t *testing.T) {
	// The wire promises a REDUCED pair, and nothing in this package divides one
	// down — the claim rests entirely on the feed publishing them that way, the
	// same assumption tickOf has made since POE-184 (1/max(x, y) is the true
	// price step only on a reduced pair). This test pins the RECORDED fixture
	// hour only: it proves the pairs the direction tests read are reduced, and
	// it cannot observe live feed drift — a frozen file never changes. The live
	// guard now exists and is not this test: isReduced, called on both pairs of
	// every kept row in Normalize, counting Stats.NonReduced and warning
	// (POE-197).
	//
	// Measured beyond the fixture on 2026-08-22: 0 of 91,520 stored priced
	// market-hours carried a common factor on either ratio pair.
	rows, _ := Normalize(loadFixtureHour(t))

	for _, row := range rows {
		if !row.PriceValid {
			continue
		}
		low, high, ok := priceIn(row, row.ItemB, row.ItemA)
		if !ok {
			t.Errorf("%s: priced row refused", row.MarketID)
			continue
		}
		for _, point := range []pricePoint{low, high} {
			if g := gcd(point.itemQty, point.quoteQty); g != 1 {
				t.Errorf("%s: pair %d/%d shares a factor of %d — the recorded fixture pair is not reduced; the direction tests above read these pairs as-is",
					row.MarketID, point.quoteQty, point.itemQty, g)
			}
		}
	}
}

// gcd is the test's own reducer, deliberately not a production helper: the
// engine does not reduce pairs, and a shared helper would read as if it did.
// It stays independent of isReduced (normalize.go) on purpose — a fixture test
// that guards the ingest check must not be written in terms of the code it
// guards.
func gcd(a, b int64) int64 {
	for b != 0 {
		a, b = b, a%b
	}
	return a
}

func TestPriceIn_pairTheRowDoesNotCarry_returnsNotOk(t *testing.T) {
	row := chaosDivineSpec().row()

	tests := []struct {
		name  string
		item  string
		quote string
	}{
		{name: "item is not on the row", item: cardID, quote: chaosID},
		{name: "quote is not on the row", item: divineID, quote: cardID},
		{name: "neither side is on the row", item: cardID, quote: scarabID},
		{name: "both sides name the same id", item: chaosID, quote: chaosID},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			low, high, ok := priceIn(row, tt.item, tt.quote)
			if ok {
				t.Errorf("ok = true, want false: the row prices %s against %s, not %s against %s",
					row.ItemB, row.ItemA, tt.item, tt.quote)
			}
			if low != (pricePoint{}) || high != (pricePoint{}) {
				t.Errorf("points = %+v/%+v, want zero points on a refused pair", low, high)
			}
		})
	}
}

func TestPriceIn_unpricedRow_returnsNotOk(t *testing.T) {
	// The quantities are perfectly usable; only the flag says the row is not
	// priceable, and that alone must stop the engine.
	spec := chaosDivineSpec()
	spec.priceInvalid = true
	row := spec.row()

	tests := []struct {
		name  string
		item  string
		quote string
	}{
		{name: "divine in chaos", item: divineID, quote: chaosID},
		{name: "chaos in divine", item: chaosID, quote: divineID},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			low, high, ok := priceIn(row, tt.item, tt.quote)
			if ok {
				t.Errorf("ok = true, want false on a PriceValid = false row")
			}
			if low != (pricePoint{}) || high != (pricePoint{}) {
				t.Errorf("points = %+v/%+v, want zero points", low, high)
			}
		})
	}
}

func TestPriceIn_zeroRatioQuantity_returnsNotOkInsteadOfDividing(t *testing.T) {
	// PriceValid stays true on purpose: the quantity guard has to hold on its
	// own, not lean on the flag Normalize would also have cleared.
	tests := []struct {
		name  string
		spec  func(s *rowSpec)
		item  string
		quote string
	}{
		{
			name:  "lowest ratio quote quantity is zero",
			spec:  func(s *rowSpec) { s.lowestRatio[0] = 0 },
			item:  divineID,
			quote: chaosID,
		},
		{
			name:  "lowest ratio item quantity is zero",
			spec:  func(s *rowSpec) { s.lowestRatio[1] = 0 },
			item:  divineID,
			quote: chaosID,
		},
		{
			name:  "highest ratio quote quantity is zero",
			spec:  func(s *rowSpec) { s.highestRatio[0] = 0 },
			item:  divineID,
			quote: chaosID,
		},
		{
			name:  "highest ratio item quantity is zero",
			spec:  func(s *rowSpec) { s.highestRatio[1] = 0 },
			item:  divineID,
			quote: chaosID,
		},
		{
			name:  "zero quantity in the reverse direction too",
			spec:  func(s *rowSpec) { s.highestRatio[0] = 0 },
			item:  chaosID,
			quote: divineID,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			tt.spec(&spec)

			low, high, ok := priceIn(spec.row(), tt.item, tt.quote)
			if ok {
				t.Errorf("ok = true, want false: a zero quantity has no price")
			}
			if low != (pricePoint{}) || high != (pricePoint{}) {
				t.Errorf("points = %+v/%+v, want zero points", low, high)
			}
		})
	}
}

func TestPriceIn_highestPriceBelowTheLowest_returnsNotOk(t *testing.T) {
	// A row whose "highest" ratio pair prices the item below its "lowest" pair
	// is incoherent: an edge built on it would be negative and its inverse
	// would look like free money. Both directions must refuse it.
	spec := chaosDivineSpec()
	spec.lowestRatio = [2]int64{201, 1}
	spec.highestRatio = [2]int64{196, 1}
	row := spec.row()

	tests := []struct {
		name  string
		item  string
		quote string
	}{
		{name: "divine in chaos", item: divineID, quote: chaosID},
		{name: "chaos in divine", item: chaosID, quote: divineID},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			low, high, ok := priceIn(row, tt.item, tt.quote)
			if ok {
				t.Errorf("ok = true (low %v, high %v), want false when the high is below the low", low.price, high.price)
			}
			if low != (pricePoint{}) || high != (pricePoint{}) {
				t.Errorf("points = %+v/%+v, want zero points", low, high)
			}
		})
	}
}

func TestPriceIn_everyPricedFixtureRow_yieldsAUsableIntervalBothWays(t *testing.T) {
	rows, stats := Normalize(loadFixtureHour(t))

	priced := 0
	for _, row := range rows {
		forward, forwardHigh, forwardOK := priceIn(row, row.ItemB, row.ItemA)
		reverse, reverseHigh, reverseOK := priceIn(row, row.ItemA, row.ItemB)

		if !row.PriceValid {
			if forwardOK || reverseOK {
				t.Errorf("%s: unpriced row was priced (%v/%v, %v/%v)",
					row.MarketID, forward.price, forwardHigh.price, reverse.price, reverseHigh.price)
			}
			continue
		}

		priced++
		if !forwardOK || !reverseOK {
			t.Errorf("%s: priced row refused (forward ok %v, reverse ok %v)", row.MarketID, forwardOK, reverseOK)
			continue
		}
		if forward.price <= 0 || forwardHigh.price < forward.price {
			t.Errorf("%s: forward interval = %v..%v, want 0 < low <= high", row.MarketID, forward.price, forwardHigh.price)
		}
		if reverse.price <= 0 || reverseHigh.price < reverse.price {
			t.Errorf("%s: reverse interval = %v..%v, want 0 < low <= high", row.MarketID, reverse.price, reverseHigh.price)
		}
	}

	// 23 of the recorded hour's 25 markets carry usable ratios; without this
	// the loop would pass over an empty fixture.
	if priced != 23 {
		t.Errorf("priced rows = %d, want 23 (Stats.Invalid = %d of %d)", priced, stats.Invalid, stats.Rows)
	}
}

func TestVwapIn_pricesBothDirectionsFromTheSameTradedVolumes(t *testing.T) {
	// The hour traded 13,001,051 chaos against 65,361 divine, so the price its
	// mass cleared at is one of those two quotients — which one depends on the
	// direction asked for, and they are not each other.
	row := chaosDivineSpec().row()

	tests := []struct {
		name  string
		item  string
		quote string
		want  float64
	}{
		{
			name:  "a divine cleared at the chaos traded per divine traded",
			item:  divineID,
			quote: chaosID,
			want:  13001051.0 / 65361.0,
		},
		{
			name:  "a chaos cleared at the divine traded per chaos traded",
			item:  chaosID,
			quote: divineID,
			want:  65361.0 / 13001051.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := vwapIn(row, tt.item, tt.quote)
			if !ok {
				t.Fatalf("ok = false, want true for %s priced in %s", tt.item, tt.quote)
			}
			if got != tt.want {
				t.Errorf("vwap = %v, want %v (quote units traded per item unit traded)", got, tt.want)
			}
		})
	}
}

func TestVwapIn_itemEqualsQuote_returnsNotOk(t *testing.T) {
	// Both readings would come from the same column, so the quotient is a
	// tautological 1 rather than a price. Without the guard the caller would
	// take that 1 for a fair anchor.
	got, ok := vwapIn(chaosDivineSpec().row(), chaosID, chaosID)

	if ok {
		t.Errorf("ok = true (vwap %v), want false: an item has no price in itself", got)
	}
	if got != 0 {
		t.Errorf("vwap = %v, want 0 on a refused pair", got)
	}
}

func TestVwapIn_quoteSideTradedNothing_returnsNotOk(t *testing.T) {
	// The divisor is the item side, so an empty QUOTE side still yields a
	// finite 0 — a price of zero, which reads as a free item. The hour has no
	// volume-weighted price at all and has to say so.
	spec := chaosDivineSpec()
	spec.volume[0] = 0
	row := spec.row()

	got, ok := vwapIn(row, divineID, chaosID)

	if ok {
		t.Errorf("ok = true (vwap %v), want false: no chaos changed hands this hour", got)
	}
	if got != 0 {
		t.Errorf("vwap = %v, want 0", got)
	}
}

func TestVwapIn_itemSideTradedNothing_returnsNotOk(t *testing.T) {
	// The item side is the divisor: without the guard this is a division by
	// zero and the leg's fair anchor becomes +Inf.
	spec := chaosDivineSpec()
	spec.volume[1] = 0
	row := spec.row()

	got, ok := vwapIn(row, divineID, chaosID)

	if ok {
		t.Errorf("ok = true (vwap %v), want false: no divine changed hands this hour", got)
	}
	if got != 0 {
		t.Errorf("vwap = %v, want 0", got)
	}
}

func TestTickOf_bothPairsNonUnit_reportsTheCoarserStep(t *testing.T) {
	// A step is one unit of the pair's LARGER quantity, so the coarser of the
	// row's two pairs is the one with the smaller maximum — and it can be
	// either pair, on either side of it.
	tests := []struct {
		name         string
		lowestRatio  [2]int64
		highestRatio [2]int64
		want         float64
	}{
		{
			name:         "the lowest pair is the coarser one",
			lowestRatio:  [2]int64{17, 28},
			highestRatio: [2]int64{50, 3},
			want:         1.0 / 28.0,
		},
		{
			name:         "the highest pair is the coarser one",
			lowestRatio:  [2]int64{100, 7},
			highestRatio: [2]int64{9, 40},
			want:         1.0 / 40.0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			spec.lowestRatio, spec.highestRatio = tt.lowestRatio, tt.highestRatio

			if got := tickOf(spec.row()); got != tt.want {
				t.Errorf("tickOf = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestTickOf_pairWithNoPositiveQuantity_reportsNoResolution(t *testing.T) {
	// Such a row resolves no price at all. Returning 0 keeps the caller away
	// from the 1/0 the formula would otherwise produce.
	tests := []struct {
		name         string
		lowestRatio  [2]int64
		highestRatio [2]int64
	}{
		{
			name:         "the lowest pair carries no quantity",
			lowestRatio:  [2]int64{0, 0},
			highestRatio: [2]int64{201, 1},
		},
		{
			name:         "the highest pair carries no quantity",
			lowestRatio:  [2]int64{196, 1},
			highestRatio: [2]int64{0, 0},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			spec.lowestRatio, spec.highestRatio = tt.lowestRatio, tt.highestRatio

			if got := tickOf(spec.row()); got != 0 {
				t.Errorf("tickOf = %v, want 0", got)
			}
		})
	}
}

func TestOrient_defaultPriority_picksTheCurrencySideAsQuote(t *testing.T) {
	priority := DefaultConfig().QuotePriority

	tests := []struct {
		name      string
		itemA     string
		itemB     string
		wantItem  string
		wantQuote string
	}{
		{
			name:      "divine outranks chaos when chaos is ItemA",
			itemA:     chaosID,
			itemB:     divineID,
			wantItem:  chaosID,
			wantQuote: divineID,
		},
		{
			name:      "divine outranks chaos when divine is ItemA",
			itemA:     divineID,
			itemB:     chaosID,
			wantItem:  chaosID,
			wantQuote: divineID,
		},
		{
			name:      "chaos outranks a non-currency ItemA",
			itemA:     cardID,
			itemB:     chaosID,
			wantItem:  cardID,
			wantQuote: chaosID,
		},
		{
			name:      "chaos outranks a non-currency ItemB",
			itemA:     chaosID,
			itemB:     cardID,
			wantItem:  cardID,
			wantQuote: chaosID,
		},
		{
			name:      "neither side listed leaves ItemB as the quote",
			itemA:     cardID,
			itemB:     scarabID,
			wantItem:  cardID,
			wantQuote: scarabID,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			item, quote := orient(rowSpec{itemA: tt.itemA, itemB: tt.itemB}.row(), priority)
			if item != tt.wantItem || quote != tt.wantQuote {
				t.Errorf("orient = (item %s, quote %s), want (item %s, quote %s)",
					item, quote, tt.wantItem, tt.wantQuote)
			}
		})
	}
}

func TestOrient_chaosPreferredOverDivine_flipsTheChaosDivineMarket(t *testing.T) {
	item, quote := orient(chaosDivineSpec().row(), []string{ChaosID, DivineID})

	if item != divineID || quote != chaosID {
		t.Errorf("orient = (item %s, quote %s), want divine priced in chaos when chaos leads the priority",
			item, quote)
	}
}

func TestVolumeOf_returnsTheTradedUnitsOfTheNamedSide(t *testing.T) {
	row := chaosDivineSpec().row()

	tests := []struct {
		name string
		item string
		want int64
	}{
		{name: "ItemA side", item: chaosID, want: 13001051},
		{name: "ItemB side", item: divineID, want: 65361},
		{name: "an item the row does not carry", item: cardID, want: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := volumeOf(row, tt.item); got != tt.want {
				t.Errorf("volumeOf(%s) = %d, want %d", tt.item, got, tt.want)
			}
		})
	}
}

func TestQuoteVolumeOf_readsTheQuoteSideNotTheItemSide(t *testing.T) {
	// It is the liquidity reading of a leg — the units of the CURRENCY that
	// flowed — so on a market whose two sides traded wildly different unit
	// counts it must never come back with the item side's number.
	row := chaosDivineSpec().row()

	tests := []struct {
		name  string
		quote string
		want  int64
	}{
		{name: "quoted in the ItemA side", quote: chaosID, want: 13001051},
		{name: "quoted in the ItemB side", quote: divineID, want: 65361},
		{name: "quoted in a currency the row does not carry", quote: cardID, want: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := quoteVolumeOf(row, tt.quote); got != tt.want {
				t.Errorf("quoteVolumeOf(%s) = %d, want %d", tt.quote, got, tt.want)
			}
		})
	}
}

func TestStockOf_returnsTheHighestStockOfTheNamedSide(t *testing.T) {
	// The lowest stock differs from the highest on both sides, so reading the
	// wrong pair of columns cannot pass.
	row := chaosDivineSpec().row()

	tests := []struct {
		name string
		item string
		want int64
	}{
		{name: "ItemA side", item: chaosID, want: 4564191},
		{name: "ItemB side", item: divineID, want: 8878},
		{name: "an item the row does not carry", item: cardID, want: 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := stockOf(row, tt.item); got != tt.want {
				t.Errorf("stockOf(%s) = %d, want %d", tt.item, got, tt.want)
			}
		})
	}
}
