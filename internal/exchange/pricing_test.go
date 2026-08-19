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
			if low != tt.wantLow {
				t.Errorf("low = %v, want %v", low, tt.wantLow)
			}
			if high != tt.wantHigh {
				t.Errorf("high = %v, want %v", high, tt.wantHigh)
			}
		})
	}
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
			if low != 0 || high != 0 {
				t.Errorf("prices = %v/%v, want 0/0 on a refused pair", low, high)
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
			if low != 0 || high != 0 {
				t.Errorf("prices = %v/%v, want 0/0", low, high)
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
			if low != 0 || high != 0 {
				t.Errorf("prices = %v/%v, want 0/0", low, high)
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
				t.Errorf("ok = true (low %v, high %v), want false when the high is below the low", low, high)
			}
			if low != 0 || high != 0 {
				t.Errorf("prices = %v/%v, want 0/0", low, high)
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
					row.MarketID, forward, forwardHigh, reverse, reverseHigh)
			}
			continue
		}

		priced++
		if !forwardOK || !reverseOK {
			t.Errorf("%s: priced row refused (forward ok %v, reverse ok %v)", row.MarketID, forwardOK, reverseOK)
			continue
		}
		if forward <= 0 || forwardHigh < forward {
			t.Errorf("%s: forward interval = %v..%v, want 0 < low <= high", row.MarketID, forward, forwardHigh)
		}
		if reverse <= 0 || reverseHigh < reverse {
			t.Errorf("%s: reverse interval = %v..%v, want 0 < low <= high", row.MarketID, reverse, reverseHigh)
		}
	}

	// 23 of the recorded hour's 25 markets carry usable ratios; without this
	// the loop would pass over an empty fixture.
	if priced != 23 {
		t.Errorf("priced rows = %d, want 23 (Stats.Invalid = %d of %d)", priced, stats.Invalid, stats.Rows)
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
