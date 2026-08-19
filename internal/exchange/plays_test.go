package exchange

import (
	"encoding/json"
	"math"
	"reflect"
	"sort"
	"testing"
	"time"
)

// storedAt stamps rows with the feed hour they were published in, the shape
// Repository.LoadRows hands BestPlays.
func storedAt(hour time.Time, rows ...Row) []StoredRow {
	stored := make([]StoredRow, 0, len(rows))
	for _, row := range rows {
		stored = append(stored, StoredRow{Hour: hour, Row: row})
	}
	return stored
}

// liquidChaosMarket is one hour of a market quoted in chaos that clears every
// default gate on its own: low chaos for one item at the hour's cheapest and
// high at the dearest, a thousand items traded against 110,000 chaos, stock on
// both sides.
//
// Chaos is ItemA and the item is the traded side, so the leg prices ARE low and
// high, the play's tick is 1/low, its investment is low and its ROI in chaos is
// high - low. A test about one gate overrides the single quantity that gate
// reads and inherits the rest of the liquidity, so each fixture states only what
// its test is about.
func liquidChaosMarket(item string, low, high int64) rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        item,
		volume:       [2]int64{110000, 1000},
		lowestStock:  [2]int64{2000, 200},
		highestStock: [2]int64{5000, 500},
		lowestRatio:  [2]int64{low, 1},
		highestRatio: [2]int64{high, 1},
	}
}

// liquidTriangle is the smallest window carrying a one-hop route that clears
// every default gate, with every price exact in binary: a scarab costs 10 chaos,
// the same scarab fetches a tenth of a divine, and a divine sells back for 200
// chaos — so buying in chaos and selling through divine doubles the stake.
//
// It yields exactly two plays. The one-hop route scarab -> divine -> chaos and
// the divine-quoted flip of the same scarab carry the SAME RoiPct (1), the same
// Turnover (100,000 chaos an hour) and the same Depth (8,000 scarabs), which is
// what makes the pair usable for the ranking tie-breaks. The chaos-quoted flip
// (+20% against a tick of 10%) and the mirror route (+20% likewise) are cut by
// the tick-ratio gate, and the chaos/divine market itself prints one price and
// so has no spread at all.
//
// The three specs are returned separately so a test can reprice one leg and
// leave the other two liquid.
func liquidTriangle() (chaosLeg, divineLeg, anchor rowSpec) {
	chaosLeg = rowSpec{
		itemA:        chaosID,
		itemB:        scarabID,
		volume:       [2]int64{200000, 20000},
		lowestStock:  [2]int64{2000, 300},
		highestStock: [2]int64{5000, 500},
		lowestRatio:  [2]int64{10, 1},
		highestRatio: [2]int64{12, 1},
	}
	divineLeg = rowSpec{
		itemA:        divineID,
		itemB:        scarabID,
		volume:       [2]int64{500, 8000},
		lowestStock:  [2]int64{200, 150},
		highestStock: [2]int64{400, 300},
		lowestRatio:  [2]int64{1, 20},
		highestRatio: [2]int64{1, 10},
	}
	anchor = divineChaosAnchor()
	return chaosLeg, divineLeg, anchor
}

// divineChaosAnchor is the divine/chaos market at exactly 200 chaos a divine:
// one price all hour (so it carries no play of its own) and a volume-weighted
// average of 4,000,000 / 20,000 = 200, which is the rate every divine-quoted
// play in the same window is valued at.
func divineChaosAnchor() rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        divineID,
		volume:       [2]int64{4000000, 20000},
		lowestStock:  [2]int64{50000, 3000},
		highestStock: [2]int64{100000, 5000},
		lowestRatio:  [2]int64{200, 1},
		highestRatio: [2]int64{200, 1},
	}
}

// triangleRows renders the three specs of a liquidTriangle as one hour's rows.
func triangleRows(chaosLeg, divineLeg, anchor rowSpec) []Row {
	return []Row{chaosLeg.row(), divineLeg.row(), anchor.row()}
}

// playKeys lists the ranked keys in order.
func playKeys(plays []Play) []string {
	keys := make([]string, 0, len(plays))
	for _, play := range plays {
		keys = append(keys, play.Key)
	}
	return keys
}

// playByKey returns the single ranked play carrying key.
func playByKey(t *testing.T, result Result, key string) Play {
	t.Helper()
	for _, play := range result.Plays {
		if play.Key == key {
			return play
		}
	}
	t.Fatalf("no play keyed %q (got %v)", key, playKeys(result.Plays))
	return Play{}
}

// directKey names the direct play of the market pairing quote with item, which
// rowSpec renders as the market id "quote|item".
func directKey(quote, item string) string {
	return "direct:" + quote + "|" + item
}

func TestBestPlays_moreHoursThanTheWindow_aggregatesOnlyTheNewestOnes(t *testing.T) {
	var rows []StoredRow
	for i := 0; i < 8; i++ {
		rows = append(rows, storedAt(feedHour.Add(time.Duration(i)*time.Hour), liquidChaosMarket(cardID, 100, 120).row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	if got.League != "Allflame" {
		t.Errorf("League = %q, want %q", got.League, "Allflame")
	}
	if got.Hours != 6 {
		t.Errorf("Hours = %d, want 6 (the window), not 8 (the rows)", got.Hours)
	}
	// The window is half-open: the two oldest hours are left out and To is the
	// newest hour plus one.
	if want := feedHour.Add(2 * time.Hour); !got.From.Equal(want) {
		t.Errorf("From = %v, want %v", got.From, want)
	}
	if want := feedHour.Add(8 * time.Hour); !got.To.Equal(want) {
		t.Errorf("To = %v, want %v", got.To, want)
	}
	if play := playByKey(t, got, directKey(chaosID, cardID)); play.HoursSeen != 6 {
		t.Errorf("HoursSeen = %d, want 6", play.HoursSeen)
	}
}

func TestBestPlays_threeHours_pricesEachLegAtTheMedianOfItsOwnSideOfTheSpread(t *testing.T) {
	// The three hours are crafted so that the median low and the median high
	// come from DIFFERENT hours: no hour offered 100 -> 130, which is what the
	// play is built to claim. A newest-hour snapshot would show 100 -> 140 and
	// an average 100 -> 130 only by coincidence of these numbers, so the lows
	// are staggered too.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 140).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 90, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 110, 130).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.Legs[0].Price != 100 {
		t.Errorf("buy price = %v, want the median hourly LOW 100 of {100, 90, 110}", play.Legs[0].Price)
	}
	if play.Legs[1].Price != 130 {
		t.Errorf("sell price = %v, want the median hourly HIGH 130 of {140, 120, 130}", play.Legs[1].Price)
	}
}

func TestBestPlays_threeHours_outlierHourDoesNotMoveTheMedianPrices(t *testing.T) {
	// One hour printed a tenfold spread. A mean would carry it into every number
	// the play claims (buy 80, sell 180, +125%); the median is unmoved, which is
	// the whole reason the aggregation is not an average.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 40, 300).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.Legs[0].Price != 100 || play.Legs[1].Price != 120 {
		t.Errorf("legs = %v -> %v, want the unmoved medians 100 -> 120",
			play.Legs[0].Price, play.Legs[1].Price)
	}
	wantClose(t, "RoiPct", play.RoiPct, 120.0/100.0-1)
}

func TestBestPlays_directPlay_roiPctIsReproducibleFromTheEmittedLegPrices(t *testing.T) {
	// The acceptance criterion of the whole engine: whatever percentage a play
	// claims has to be arithmetic on the two prices it shows, not a separately
	// aggregated statistic that happens to travel with them.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 140).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 90, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 110, 130).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	wantClose(t, "RoiPct", play.RoiPct, 130.0/100.0-1)
	wantClose(t, "RoiPct recomputed from the legs", play.RoiPct, play.Legs[1].Price/play.Legs[0].Price-1)
}

func TestBestPlays_oneHopPlay_roiPctIsReproducibleFromTheThreeEmittedLegPrices(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	// Ten chaos buys a scarab, the scarab sells for a tenth of a divine, and the
	// divine sells back for 200 chaos.
	wantClose(t, "RoiPct", play.RoiPct, (1.0/10.0)*200.0/10.0-1)
	wantClose(t, "RoiPct recomputed from the legs", play.RoiPct,
		play.Legs[1].Price*play.Legs[2].Price/play.Legs[0].Price-1)
}

func TestBestPlays_oneHopPlayOverThreeHours_roiPctIsReproducibleFromTheMedianLegPrices(t *testing.T) {
	// Same acceptance criterion as the direct case, over the shape that has to
	// hold it across three markets AND three hours. The hours are staggered so
	// the median buy price comes from the NEWEST hour and the median sell price
	// from the middle one: no hour offered the 100 chaos -> 1 divine pair the
	// play claims, so a percentage lifted from any single hour — the newest
	// hour's own 1.5 included — cannot reproduce the legs shown.
	hours := []struct {
		buyChaos   int64    // the hour's cheapest chaos price of one scarab
		sellDivine [2]int64 // the hour's dearest divine price of one scarab
	}{
		{buyChaos: 100, sellDivine: [2]int64{125, 100}},
		{buyChaos: 80, sellDivine: [2]int64{100, 100}},
		{buyChaos: 120, sellDivine: [2]int64{80, 100}},
	}

	var rows []StoredRow
	for i, hour := range hours {
		chaosLeg, divineLeg, anchor := liquidTriangle()
		chaosLeg.lowestRatio, chaosLeg.highestRatio = [2]int64{hour.buyChaos, 1}, [2]int64{130, 1}
		chaosLeg.volume = [2]int64{2000000, 20000}
		divineLeg.lowestRatio, divineLeg.highestRatio = [2]int64{50, 100}, hour.sellDivine
		divineLeg.volume = [2]int64{5000, 5000}

		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), triangleRows(chaosLeg, divineLeg, anchor)...)...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	if play.Legs[0].Price != 100 {
		t.Fatalf("buy price = %v, want the median hourly LOW 100 of {100, 80, 120}", play.Legs[0].Price)
	}
	if play.Legs[1].Price != 1 {
		t.Fatalf("sell price = %v, want the median hourly HIGH 1 divine of {1.25, 1, 0.8}", play.Legs[1].Price)
	}
	// One divine closes the route at 200 chaos, so the medians double the stake.
	wantClose(t, "RoiPct", play.RoiPct, 1)
	wantClose(t, "RoiPct recomputed from the legs", play.RoiPct,
		play.Legs[1].Price*play.Legs[2].Price/play.Legs[0].Price-1)
}

func TestBestPlays_edge_carriesTheSameValueAsRoiPct(t *testing.T) {
	// edge is the pre-POE-184 name kept on the wire for the desktop build that
	// still reads it; it must carry the window's RoiPct and not the other
	// percentage on the play. The three staggered hours separate the two: the
	// medians make RoiPct 130/100 - 1 while the newest hour alone printed
	// 140/100 - 1, so an edge fed from the newest hour cannot pass.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 140).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 90, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 110, 130).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.RoiPctNewestHour == play.RoiPct {
		t.Fatalf("RoiPctNewestHour = RoiPct = %v; the fixture has to keep the two apart", play.RoiPct)
	}
	if play.Edge != play.RoiPct {
		t.Errorf("Edge = %v, want RoiPct %v (RoiPctNewestHour is %v)", play.Edge, play.RoiPct, play.RoiPctNewestHour)
	}
}

func TestBestPlays_newestHourQuieterThanTheWindow_reportsItsEdgeBelowRoiPct(t *testing.T) {
	// RoiPctNewestHour is the NOW reading, not a bound: the newest hour traded a
	// tenth of the window's typical spread, so it sits BELOW the median-based
	// RoiPct rather than above it.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 110).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 140).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 100, 140).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	wantClose(t, "RoiPctNewestHour", play.RoiPctNewestHour, 110.0/100.0-1)
	wantClose(t, "RoiPct", play.RoiPct, 140.0/100.0-1)
	if !(play.RoiPctNewestHour < play.RoiPct) {
		t.Errorf("RoiPctNewestHour = %v, want it below the window's RoiPct %v",
			play.RoiPctNewestHour, play.RoiPct)
	}
}

func TestBestPlays_threeHours_volumeIsTheMedianOfTheHourlyTradedUnits(t *testing.T) {
	rows := []StoredRow{}
	for i, units := range []int64{1000, 100, 10000} {
		spec := liquidChaosMarket(cardID, 100, 120)
		spec.volume = [2]int64{110000, units}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	for i, leg := range play.Legs {
		if leg.Volume != 1000 {
			t.Errorf("leg %d volume = %v, want the median hourly 1000 of {1000, 100, 10000}", i, leg.Volume)
		}
	}
	if play.Depth != 1000 {
		t.Errorf("Depth = %v, want the median 1000", play.Depth)
	}
}

func TestBestPlays_threeHours_fairIsTheMedianHourlyVolumeWeightedPrice(t *testing.T) {
	// Fair is where the hour's MASS traded (quote units / item units), which is
	// what says whether a Price at the edge of the spread is opportunity or
	// quantization.
	rows := []StoredRow{}
	for i, chaosTraded := range []int64{110000, 105000, 5000} {
		spec := liquidChaosMarket(cardID, 100, 120)
		spec.volume = [2]int64{chaosTraded, 1000}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// The hourly averages are 110, 105 and 5 chaos; the median is 105 and the
	// mean would be 73.3.
	for i, leg := range play.Legs {
		wantClose(t, "leg "+leg.Action+" fair", leg.Fair, 105)
		if i > 0 && play.Legs[i].Fair != play.Legs[0].Fair {
			t.Errorf("leg %d fair = %v, want both legs of a flip to share the market's anchor %v",
				i, play.Legs[i].Fair, play.Legs[0].Fair)
		}
	}
}

func TestBestPlays_hourThatTradedNoQuoteUnits_isLeftOutOfTheFairMedian(t *testing.T) {
	// An hour whose quote side reported nothing has NO volume-weighted price;
	// folding its 0 into the median would drag the anchor toward zero and make a
	// live market look free.
	rows := []StoredRow{}
	for i, chaosTraded := range []int64{0, 110000, 100000} {
		spec := liquidChaosMarket(cardID, 100, 120)
		spec.volume = [2]int64{chaosTraded, 1000}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// The two hours that DID clear averaged 110 and 100, so the anchor is 105.
	// Counting the empty hour would have made it 100.
	wantClose(t, "Fair", play.Legs[0].Fair, 105)
}

func TestBestPlays_threeHours_tickIsTheMedianHourlyPriceResolution(t *testing.T) {
	// The market's quantity pairs coarsen hour by hour (1/100, 1/50, 1/20 of the
	// price); the play reports the middle one, not the newest and not the worst.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 110).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 50, 110).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 20, 110).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	for i, leg := range play.Legs {
		wantClose(t, "leg "+string(rune('0'+i))+" tick", leg.Tick, 1.0/50.0)
	}
	if play.Tick != play.Legs[0].Tick {
		t.Errorf("play Tick = %v, want the single market's median tick %v", play.Tick, play.Legs[0].Tick)
	}
}

func TestBestPlays_playSeenInThreeHours_stocksTheNewestHour(t *testing.T) {
	// Stock is liveness only — "is this side on the book right now" — so it is
	// the newest hour's reading and not an aggregate of the window.
	rows := []StoredRow{}
	for i, stock := range []int64{111, 999, 777} {
		spec := liquidChaosMarket(cardID, 100, 120)
		spec.highestStock = [2]int64{5000, stock}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	for i, leg := range play.Legs {
		if leg.Stock != 111 {
			t.Errorf("leg %d stock = %d, want the newest hour's 111 (median would be 777)", i, leg.Stock)
		}
	}
}

func TestBestPlays_playSeenInThreeHours_reportsTheNewestHourAsLastHour(t *testing.T) {
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 100, 120).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if !play.LastHour.Equal(feedHour) {
		t.Errorf("LastHour = %v, want the newest hour %v", play.LastHour, feedHour)
	}
	if play.HoursSeen != 3 {
		t.Errorf("HoursSeen = %d, want 3", play.HoursSeen)
	}
}

func TestBestPlays_chaosQuotedDirectPlay_investmentIsTheEntryPriceInChaos(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// One unit costs what the BUY leg pays, and chaos is worth one chaos.
	if play.Investment != 100 {
		t.Errorf("Investment = %v, want the buy leg's 100 chaos", play.Investment)
	}
}

func TestBestPlays_chaosQuotedDirectPlay_roiIsTheChaosGainedPerExchangedUnit(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// Buy at 100, sell at 120: twenty chaos a unit. Roi IS Investment * RoiPct
	// by construction, so only the literal pins it — restating the identity
	// here would assert the production formula against itself.
	wantClose(t, "Roi", play.Roi, 20)
}

func TestBestPlays_chaosQuotedDirectPlay_turnoverIsTheChaosThatFlowedThroughTheMarket(t *testing.T) {
	spec := liquidChaosMarket(cardID, 100, 120)
	spec.volume = [2]int64{123000, 1000}

	got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// The QUOTE side's traded units, valued in chaos — not the 1000 items that
	// changed hands, which is Depth.
	if play.Turnover != 123000 {
		t.Errorf("Turnover = %v, want the hour's 123000 chaos of quote volume", play.Turnover)
	}
	if play.Depth != 1000 {
		t.Errorf("Depth = %v, want the 1000 items traded", play.Depth)
	}
}

func TestBestPlays_divineQuotedDirectPlay_valuesTheEntryAtTheWindowsDivineRate(t *testing.T) {
	// A play quoted in divine is still ranked in chaos, so its entry price is
	// converted through the window's own divine/chaos rate rather than a guess.
	_, divineLeg, anchor := liquidTriangle()

	got := BestPlays("Allflame", storedAt(feedHour, divineLeg.row(), anchor.row()), DefaultConfig())

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.Legs[0].Price != 0.05 {
		t.Fatalf("buy price = %v, want a twentieth of a divine", play.Legs[0].Price)
	}
	// 0.05 divine at 200 chaos a divine is a 10 chaos entry, and the flip
	// doubles it.
	wantClose(t, "Investment", play.Investment, 10)
	wantClose(t, "Roi", play.Roi, 10)
	wantClose(t, "Turnover", play.Turnover, 500*200)
}

func TestBestPlays_oneHopPlay_investmentAndRoiAreValuedInChaosFromTheFirstLeg(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	// The route is entered by paying 10 chaos for a scarab; one turn doubles it.
	wantClose(t, "Investment", play.Investment, 10)
	wantClose(t, "Roi", play.Roi, 10)
}

func TestBestPlays_oneHopPlay_turnoverIsItsThinnestLegInChaos(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	// 200,000 chaos flowed through the chaos/scarab market and 4,000,000 through
	// the chaos/divine one, but the divine/scarab leg moved only 500 divine —
	// 100,000 chaos, and a recipe is as liquid as its thinnest step.
	wantClose(t, "Turnover", play.Turnover, 500*200)
}

func TestBestPlays_oneHopPlay_tickIsTheCoarsestOfItsThreeLegs(t *testing.T) {
	// The scarab is repriced around 100 chaos, where the chaos market quotes it
	// to the unit (1/100) and the divine market in lots of 28 (1/28), so the
	// coarsest step belongs to the MIDDLE leg — neither the leg the route is
	// entered on nor the market that closes it.
	chaosLeg, divineLeg, anchor := liquidTriangle()
	chaosLeg.lowestRatio, chaosLeg.highestRatio = [2]int64{100, 1}, [2]int64{110, 1}
	chaosLeg.volume = [2]int64{2000000, 20000}
	divineLeg.lowestRatio, divineLeg.highestRatio = [2]int64{17, 28}, [2]int64{23, 28}
	divineLeg.volume = [2]int64{5000, 7000}

	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(chaosLeg, divineLeg, anchor)...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	wantClose(t, "Tick", play.Tick, 1.0/28.0)
	wantClose(t, "leg 0 tick", play.Legs[0].Tick, 1.0/100.0)
	wantClose(t, "leg 2 tick", play.Legs[2].Tick, 1.0/200.0)
}

func TestBestPlays_oneHopPlay_depthIsItsThinnestLeg(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	wantVolumes := []float64{20000, 8000, 20000}
	for i, leg := range play.Legs {
		if leg.Volume != wantVolumes[i] {
			t.Errorf("leg %d volume = %v, want %v", i, leg.Volume, wantVolumes[i])
		}
	}
	// The scarab sold against divine is the bottleneck: the recipe cannot move
	// more units per hour than its thinnest step.
	if play.Depth != 8000 {
		t.Errorf("Depth = %v, want 8000", play.Depth)
	}
}

func TestBestPlays_divineChaosMarketInTheWindow_ratesADivineAtItsMedianHourlyAverage(t *testing.T) {
	// The rate is measured from the same table as everything else and moves with
	// the league, so it is the median of the market's hourly volume-weighted
	// prices — one loud hour must not reprice every divine-quoted play.
	rows := []StoredRow{}
	for i, chaosTraded := range []int64{10000000, 3800000, 4000000} {
		spec := divineChaosAnchor()
		spec.volume = [2]int64{chaosTraded, 20000}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	// The hourly rates are 500, 190 and 200 chaos a divine: the median is 200
	// and the mean would be 296.7.
	wantClose(t, "DivineChaosRate", got.DivineChaosRate, 200)
}

func TestBestPlays_noDivineChaosMarketInTheWindow_reportsARateOfZero(t *testing.T) {
	_, divineLeg, _ := liquidTriangle()

	got := BestPlays("Allflame", storedAt(feedHour, divineLeg.row(), liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	if got.DivineChaosRate != 0 {
		t.Errorf("DivineChaosRate = %v, want 0 — nothing in the window priced a divine", got.DivineChaosRate)
	}
}

func TestBestPlays_noDivineChaosMarketInTheWindow_ranksOnlyTheChaosQuotedPlays(t *testing.T) {
	// Without a rate a divine-quoted payout cannot be compared to a chaos one at
	// all, so it is dropped rather than valued at a guess. The chaos-quoted play
	// in the same window is untouched.
	//
	// The omen market is priced so that ONLY the missing rate can drop it: at a
	// substituted rate of one chaos per divine it would still clear every gate
	// the ranking applies — 12,000 quote units of turnover against a floor of
	// 10,000, an entry of 100 and a payout of 20 against a floor of 3 — so a
	// play that comes back here is a rate that was guessed, not a gate that bit.
	inDivine := rowSpec{
		itemA:        divineID,
		itemB:        omenID,
		volume:       [2]int64{12000, 100},
		lowestStock:  [2]int64{300, 40},
		highestStock: [2]int64{600, 90},
		lowestRatio:  [2]int64{100, 1},
		highestRatio: [2]int64{120, 1},
	}

	got := BestPlays("Allflame", storedAt(feedHour, inDivine.row(), liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_marketQuotedInNeitherChaosNorDivine_isDropped(t *testing.T) {
	// A card priced in scarabs pays out in scarabs. Every gate below the ranking
	// is denominated in chaos, so such a market has no comparable number at all
	// and is dropped even though its spread and its depth are fine.
	// Neither side is a listed currency, so orient leaves ItemA as the traded
	// item and ItemB as its quote: the scarab is priced in cards, 100 of them at
	// the hour's cheapest and 120 at the dearest. Every chaos gate would pass on
	// those numbers if the payout could be valued at all — 110,000 quote units of
	// turnover, a tick of 1%, twenty units of gain per exchange.
	inScarabs := rowSpec{
		itemA:        scarabID,
		itemB:        cardID,
		volume:       [2]int64{1000, 110000},
		lowestStock:  [2]int64{200, 2000},
		highestStock: [2]int64{500, 5000},
		lowestRatio:  [2]int64{1, 120},
		highestRatio: [2]int64{1, 100},
	}

	got := BestPlays("Allflame", storedAt(feedHour, inScarabs.row(), liquidChaosMarket(hellID, 100, 120).row()), DefaultConfig())

	if want := []string{directKey(chaosID, hellID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want only the chaos-quoted market %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_minTurnoverChaos_cutsTheMarketsTooSmallForTheSpreadToBeReal(t *testing.T) {
	// The measured floor: under 100 chaos an hour the median robust edge is
	// 242%, over 100k it is 18%. The gate is inclusive at 10,000.
	tests := []struct {
		name        string
		chaosTraded int64
		want        []string
	}{
		{
			name:        "exactly at the floor",
			chaosTraded: 10000,
			want:        []string{directKey(chaosID, cardID)},
		},
		{
			name:        "one chaos an hour below the floor",
			chaosTraded: 9999,
			want:        []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 100, 120)
			spec.volume = [2]int64{tt.chaosTraded, 100}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (Turnover %v)", playKeys(got.Plays), tt.want, tt.chaosTraded)
			}
		})
	}
}

func TestBestPlays_maxTick_cutsTheSpreadsThatAreOneIntegerPriceStepWide(t *testing.T) {
	// tick = 1/max(quantity), so a market quoting ten chaos to the item can only
	// move in tenths — exactly the cap. Nine chaos to the item cannot.
	tests := []struct {
		name string
		low  int64
		want []string
	}{
		{
			name: "resolution exactly at the cap",
			low:  10,
			want: []string{directKey(chaosID, cardID)},
		},
		{
			name: "resolution one integer step coarser than the cap",
			low:  9,
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, tt.low, 20)
			spec.volume = [2]int64{11000, 1000}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (tick 1/%d)", playKeys(got.Plays), tt.want, tt.low)
			}
		})
	}
}

func TestBestPlays_minEdgeTickRatio_cutsTheSpreadsNarrowerThanFivePriceSteps(t *testing.T) {
	// On a market quoting to the unit at 128 chaos, one price step IS one chaos,
	// so a five-chaos spread is exactly five steps wide and a four-chaos spread
	// is not — even though both clear MinEdge and MinROIChaos. 128 rather than a
	// rounder number because 5/128 and 133/128 - 1 are the SAME double: at 100 the
	// two sides of the comparison differ in their last bits and the case would sit
	// a rounding above the gate instead of on it.
	tests := []struct {
		name string
		high int64
		want []string
	}{
		{
			name: "spread exactly five price steps wide",
			high: 133,
			want: []string{directKey(chaosID, cardID)},
		},
		{
			name: "spread one step short of five",
			high: 132,
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 128, tt.high)
			spec.volume = [2]int64{130000, 1000}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (RoiPct %v against 5 ticks of %v)",
					playKeys(got.Plays), tt.want, float64(tt.high)/128.0-1, 5.0/128.0)
			}
		})
	}
}

func TestBestPlays_minROIChaos_cutsThePlaysWhosePayoutIsARoundingError(t *testing.T) {
	// A percentage says nothing about what a flip pays. Both markets below quote
	// an item at 6.25 chaos with a fine tick and a real spread; only the payout
	// per exchanged unit separates them, and the floor is three chaos.
	tests := []struct {
		name string
		high [2]int64
		want []string
	}{
		{
			name: "3.125 chaos an exchange clears the floor",
			high: [2]int64{75, 8}, // 9.375 chaos: +50% on 6.25
			want: []string{directKey(chaosID, cardID)},
		},
		{
			name: "2.5 chaos an exchange does not",
			high: [2]int64{35, 4}, // 8.75 chaos: +40% on 6.25
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 0, 0)
			spec.lowestRatio = [2]int64{25, 4} // 6.25 chaos, tick 1/25
			spec.highestRatio = tt.high
			spec.volume = [2]int64{37500, 5000}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v", playKeys(got.Plays), tt.want)
			}
		})
	}
}

func TestBestPlays_minVolumePerHour_stillDropsTheLegNobodyTraded(t *testing.T) {
	// The unit-volume floor survived POE-184 as a LIVENESS gate: it is no longer
	// what judges liquidity (Turnover is), but a leg the hour did not trade at
	// all is not executable at any depth. The market is a thousand-chaos item,
	// so ten units still clear the chaos turnover floor.
	tests := []struct {
		name  string
		units int64
		want  []string
	}{
		{
			name:  "exactly at the floor",
			units: 10,
			want:  []string{directKey(chaosID, cardID)},
		},
		{
			name:  "one unit below the floor",
			units: 9,
			want:  []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 1200, 1500)
			spec.volume = [2]int64{13000, tt.units}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (%d units traded)", playKeys(got.Plays), tt.want, tt.units)
			}
		})
	}
}

func TestBestPlays_singleHourWindow_capsMinHoursSeenAtTheHoursPresent(t *testing.T) {
	// The default asks for two hours; only one exists. Capping is what keeps a
	// fresh league (or a just-restarted walk) from returning nothing at all.
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1", play.HoursSeen)
	}
}

func TestBestPlays_playSeenInOnlyOneOfTwoHours_isDroppedAsAGhost(t *testing.T) {
	steady := liquidChaosMarket(cardID, 100, 120).row()
	ghost := liquidChaosMarket(hellID, 100, 200).row()
	rows := append(
		storedAt(feedHour, steady, ghost),
		storedAt(feedHour.Add(-time.Hour), steady)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	// The ghost carries by far the bigger ROI (100% against 20%) and is still
	// dropped: printing once in the window is the disqualifier.
	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_minEdge_cutsThePlaysBelowTheFloor(t *testing.T) {
	rows := storedAt(feedHour,
		liquidChaosMarket(hellID, 100, 200).row(), // +100%
		liquidChaosMarket(cardID, 100, 150).row(), // +50%
	)

	tests := []struct {
		name    string
		minEdge float64
		want    []string
	}{
		{
			name:    "floor below both returns keeps both",
			minEdge: 0.25,
			want:    []string{directKey(chaosID, hellID), directKey(chaosID, cardID)},
		},
		{
			name:    "floor exactly on the smaller return keeps it",
			minEdge: 0.5,
			want:    []string{directKey(chaosID, hellID), directKey(chaosID, cardID)},
		},
		{
			name:    "floor a hair above the smaller return drops it",
			minEdge: 0.5000001,
			want:    []string{directKey(chaosID, hellID)},
		},
		{
			name:    "floor above both returns keeps nothing",
			minEdge: 1.5,
			want:    []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cfg := DefaultConfig()
			cfg.MinEdge = tt.minEdge

			got := BestPlays("Allflame", rows, cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v", playKeys(got.Plays), tt.want)
			}
		})
	}
}

func TestBestPlays_negativeMinEdge_surfacesTheReturnsTheDefaultFloorHides(t *testing.T) {
	// An explicitly negative MinEdge is a choice rather than an unset field, and
	// this is what it buys: the returns the two-percent floor hides. What it can
	// no longer do is surface a LOSING route — MinEdgeTickRatio compares the
	// return against a positive multiple of the tick, so a negative one cannot
	// clear it whatever MinEdge says (see the report on Config.MinEdge's doc).
	spec := liquidChaosMarket(cardID, 1000, 1010)
	spec.volume = [2]int64{100000, 100}
	rows := storedAt(feedHour, spec.row())

	if got := BestPlays("Allflame", rows, DefaultConfig()); len(got.Plays) != 0 {
		t.Fatalf("the default floor kept %v, want a 1%% return to sit below it", playKeys(got.Plays))
	}

	cfg := DefaultConfig()
	cfg.MinEdge = -1

	got := BestPlays("Allflame", rows, cfg)

	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_maxPlays_truncatesToTheHighestRanked(t *testing.T) {
	rows := storedAt(feedHour,
		liquidChaosMarket(hellID, 100, 200).row(),   // +100%
		liquidChaosMarket(cardID, 100, 160).row(),   // +60%
		liquidChaosMarket(omenID, 100, 140).row(),   // +40%
		liquidChaosMarket(scarabID, 100, 130).row(), // +30%
	)
	cfg := DefaultConfig()
	cfg.MaxPlays = 2

	got := BestPlays("Allflame", rows, cfg)

	want := []string{directKey(chaosID, hellID), directKey(chaosID, cardID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the two highest ranked %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_ranksByRoiPctThenTurnoverThenKey(t *testing.T) {
	// Three of the four markets are crafted to the same +20%, and two of those to
	// the same turnover, so each tie-break decides exactly one position. The
	// second-placed market is also the THINNEST in units traded, which is what
	// proves the tie-break moved from depth to chaos turnover.
	expensive := liquidChaosMarket(cardID, 1000, 1200)
	expensive.volume = [2]int64{330000, 300}

	rows := storedAt(feedHour,
		liquidChaosMarket(scarabID, 100, 120).row(), // +20%, 110k chaos an hour
		liquidChaosMarket(omenID, 100, 120).row(),   // +20%, 110k chaos an hour
		expensive.row(), // +20%, 330k chaos an hour on 300 units
		liquidChaosMarket(hellID, 100, 150).row(), // +50%
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{
		directKey(chaosID, hellID),   // biggest return
		directKey(chaosID, cardID),   // same return as the rest, most chaos through it
		directKey(chaosID, omenID),   // tied on return and turnover, smaller key
		directKey(chaosID, scarabID), // tied on return and turnover, bigger key
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
	if card, omen := playByKey(t, got, want[1]), playByKey(t, got, want[2]); card.Depth >= omen.Depth {
		t.Errorf("the runner-up's Depth is %v against %v — the fixture no longer separates turnover from depth",
			card.Depth, omen.Depth)
	}
}

func TestBestPlays_directPlayTiedWithAOneHop_isRankedFirst(t *testing.T) {
	// The route and the divine-quoted flip carry an ROI of exactly 1 and a
	// turnover of exactly 100,000 chaos, so only the mode can separate them. The
	// flip's key sorts BEHIND the route's, which is how this proves the mode
	// tie-break outranks the key tie-break.
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	want := []string{
		directKey(divineID, scarabID),
		oneHopKey(scarabID, chaosID, divineID),
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the direct flip ahead of the route %v", playKeys(got.Plays), want)
	}
	for _, play := range got.Plays {
		wantClose(t, play.Key+" RoiPct", play.RoiPct, 1)
		wantClose(t, play.Key+" Turnover", play.Turnover, 100000)
	}
}

func TestModeRank_unknownMode_sortsBehindBothShapes(t *testing.T) {
	// Nothing in the engine emits a third mode today; the rank exists so that
	// adding one without touching the comparator parks it at the back of a tie
	// rather than ahead of the two shapes that were ranked deliberately.
	if got := modeRank(Mode("2-hop")); got <= modeRank(ModeOneHop) {
		t.Errorf("modeRank(2-hop) = %d, want more than the one-hop rank %d", got, modeRank(ModeOneHop))
	}
}

func TestBestPlays_marketRepeatedWithinOneHour_countsAsOneHour(t *testing.T) {
	// Two rows for the same market in one hour is a storage accident, not a
	// second sighting: counting it twice would let a duplicate satisfy the
	// ghost filter on its own.
	row := liquidChaosMarket(cardID, 100, 120).row()

	got := BestPlays("Allflame", storedAt(feedHour, row, row), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1", play.HoursSeen)
	}
	if play.Legs[0].Volume != 1000 {
		t.Errorf("leg volume = %v, want the hour's traded volume 1000", play.Legs[0].Volume)
	}
}

func TestBestPlays_noRows_returnsAnEmptyWindow(t *testing.T) {
	got := BestPlays("Allflame", nil, DefaultConfig())

	if got.League != "Allflame" {
		t.Errorf("League = %q, want %q", got.League, "Allflame")
	}
	if got.Hours != 0 {
		t.Errorf("Hours = %d, want 0", got.Hours)
	}
	if !got.From.IsZero() || !got.To.IsZero() {
		t.Errorf("window = %v..%v, want the zero times", got.From, got.To)
	}
	if got.Plays == nil {
		t.Fatal("Plays = nil, want an allocated empty slice so it marshals as []")
	}
	if len(got.Plays) != 0 {
		t.Errorf("Plays = %v, want none", playKeys(got.Plays))
	}
}

// astrolabeInChaos is the one measured market of the POE-184 26-hour Allflame
// sample that SHOULD survive the gates: 82 to 100 chaos an omen, ~70k chaos an
// hour. The old engine ranked it at +1846% off a 1-hop through divine that was
// tick noise; both the gate test and the reading test below read it from here so
// neither can drift from the recorded hour on its own.
func astrolabeInChaos() rowSpec {
	return rowSpec{
		itemA: chaosID, itemB: omenID,
		volume:       [2]int64{70000, 750},
		lowestStock:  [2]int64{4000, 100},
		highestStock: [2]int64{9000, 300},
		lowestRatio:  [2]int64{82, 1},
		highestRatio: [2]int64{100, 1},
	}
}

func TestBestPlays_measuredNoiseMarkets_areCutByTheGates(t *testing.T) {
	// The three markets that motivated POE-184, rebuilt from the 26-hour
	// Allflame measurement. Each row is one hour of a real market; what has to
	// hold is the OUTCOME, because more than one gate bites some of them — the
	// levels themselves are pinned one at a time by the boundary tests above.
	tests := []struct {
		name string
		spec rowSpec
		want []string
	}{
		{
			// VWAP 0.219c against a 0.0286 / 1.00 extreme pair: a whole chaos of
			// apparent spread on 109 chaos an hour, at a tick of 100%.
			name: "Divine Vessel, 109 chaos an hour",
			spec: rowSpec{
				itemA: chaosID, itemB: cardID,
				volume:       [2]int64{109, 500},
				lowestStock:  [2]int64{300, 2000},
				highestStock: [2]int64{600, 4000},
				lowestRatio:  [2]int64{1, 35},
				highestRatio: [2]int64{1, 1},
			},
			want: []string{},
		},
		{
			// 1:2 and 1:1 in the same hour is a 100% "edge" that is one integer
			// step, on a market that IS liquid enough to pass the turnover gate.
			name: "Delirium Scarab, a 100% tick",
			spec: rowSpec{
				itemA: chaosID, itemB: scarabID,
				volume:       [2]int64{20000, 30000},
				lowestStock:  [2]int64{5000, 9000},
				highestStock: [2]int64{9000, 15000},
				lowestRatio:  [2]int64{1, 2},
				highestRatio: [2]int64{1, 1},
			},
			want: []string{},
		},
		{
			// The one that should survive.
			name: "Astrolabe quoted in chaos",
			spec: astrolabeInChaos(),
			want: []string{directKey(chaosID, omenID)},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := BestPlays("Allflame", storedAt(feedHour, tt.spec.row()), DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v", playKeys(got.Plays), tt.want)
			}
		})
	}
}

func TestBestPlays_astrolabeQuotedInChaos_ranksAtItsMeasuredReturn(t *testing.T) {
	// Its real reading, against the +1846% the old engine claimed for it.
	got := BestPlays("Allflame", storedAt(feedHour, astrolabeInChaos().row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, omenID))
	wantClose(t, "RoiPct", play.RoiPct, 100.0/82.0-1)
	wantClose(t, "Roi", play.Roi, 18)
	wantClose(t, "Investment", play.Investment, 82)
	wantClose(t, "Turnover", play.Turnover, 70000)
}

func TestBestPlays_recordedHour_ranksFinitePlaysOfBothShapes(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	got := BestPlays("Allflame", storedAt(feedHour, rows...), DefaultConfig())

	// The recorded hour is frozen input: 23 priced markets out of 25 have to
	// yield plays of both shapes, and every number in them has to be finite and
	// positive. The exact counts are a characterization, not a spec — measured
	// on this fixture under DefaultConfig: 18 direct and 8 one-hop — so they are
	// read as "some of each" rather than pinned, which would make an unrelated
	// tuning change look like a regression.
	direct, oneHop := 0, 0
	for _, play := range got.Plays {
		switch play.Mode {
		case ModeDirect:
			direct++
		case ModeOneHop:
			oneHop++
		default:
			t.Errorf("%s: mode = %q, want %q or %q", play.Key, play.Mode, ModeDirect, ModeOneHop)
		}

		if math.IsNaN(play.RoiPct) || math.IsInf(play.RoiPct, 0) {
			t.Errorf("%s: RoiPct = %v, want a finite number", play.Key, play.RoiPct)
		}
		if play.Depth <= 0 {
			t.Errorf("%s: Depth = %v, want the thinnest leg's volume", play.Key, play.Depth)
		}
		if len(play.Legs) < 2 {
			t.Errorf("%s: %d legs, want at least the buy and the sell", play.Key, len(play.Legs))
		}
		for i, leg := range play.Legs {
			if !(leg.Price > 0) || math.IsInf(leg.Price, 0) {
				t.Errorf("%s: leg %d price = %v, want a positive finite price", play.Key, i, leg.Price)
			}
		}
	}
	if direct <= 0 {
		t.Errorf("direct plays = %d, want the recorded hour to yield same-market flips", direct)
	}
	if oneHop <= 0 {
		t.Errorf("one-hop plays = %d, want the recorded hour to yield cross-quote routes", oneHop)
	}
}

func TestBestPlays_zeroValueConfig_scoresTheHourLikeDefaultConfig(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))
	stored := storedAt(feedHour, rows...)

	got := BestPlays("Allflame", stored, Config{})

	if want := BestPlays("Allflame", stored, DefaultConfig()); !reflect.DeepEqual(got, want) {
		t.Errorf("zero-value config produced %d plays (%v...), want the DefaultConfig result of %d",
			len(got.Plays), playKeys(got.Plays), len(want.Plays))
	}
}

func TestDefaultConfig_isTheDocumentedTuning(t *testing.T) {
	want := Config{
		WindowHours:      6,
		MinVolumePerHour: 10,
		MinEdge:          0.02,
		MinTurnoverChaos: 10000,
		MaxTick:          0.10,
		MinEdgeTickRatio: 5,
		MinROIChaos:      3,
		MinHoursSeen:     2,
		MaxPlays:         100,
		QuotePriority:    []string{DivineID, ChaosID},
		Horizons: []HorizonConfig{
			{Horizon: HorizonRecent, WindowHours: 6, MinHoursSeen: 4},
			{Horizon: HorizonDay, WindowHours: 24, MinHoursSeen: 18},
		},
	}

	if got := DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("DefaultConfig() = %+v, want %+v", got, want)
	}
}

func TestConfigWithDefaults_zeroValue_fillsEveryFieldFromDefaultConfig(t *testing.T) {
	if got, want := (Config{}).withDefaults(), DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("Config{}.withDefaults() = %+v, want %+v", got, want)
	}
}

func TestConfigWithDefaults_fillsTheUnsetFieldsIndependently(t *testing.T) {
	tests := []struct {
		name string
		cfg  Config
		want Config
	}{
		{
			name: "only MaxPlays set",
			cfg:  Config{MaxPlays: 3},
			want: withField(func(c *Config) { c.MaxPlays = 3 }),
		},
		{
			name: "only WindowHours set",
			cfg:  Config{WindowHours: 24},
			want: withField(func(c *Config) { c.WindowHours = 24 }),
		},
		{
			name: "only MinVolumePerHour set",
			cfg:  Config{MinVolumePerHour: 0.5},
			want: withField(func(c *Config) { c.MinVolumePerHour = 0.5 }),
		},
		{
			name: "only MinEdge set",
			cfg:  Config{MinEdge: 0.5},
			want: withField(func(c *Config) { c.MinEdge = 0.5 }),
		},
		{
			name: "only MinTurnoverChaos set",
			cfg:  Config{MinTurnoverChaos: 250},
			want: withField(func(c *Config) { c.MinTurnoverChaos = 250 }),
		},
		{
			name: "only MaxTick set",
			cfg:  Config{MaxTick: 0.5},
			want: withField(func(c *Config) { c.MaxTick = 0.5 }),
		},
		{
			name: "only MinEdgeTickRatio set",
			cfg:  Config{MinEdgeTickRatio: 2},
			want: withField(func(c *Config) { c.MinEdgeTickRatio = 2 }),
		},
		{
			name: "only MinROIChaos set",
			cfg:  Config{MinROIChaos: 25},
			want: withField(func(c *Config) { c.MinROIChaos = 25 }),
		},
		{
			name: "only MinHoursSeen set",
			cfg:  Config{MinHoursSeen: 4},
			want: withField(func(c *Config) { c.MinHoursSeen = 4 }),
		},
		{
			name: "only QuotePriority set",
			cfg:  Config{QuotePriority: []string{ChaosID}},
			want: withField(func(c *Config) { c.QuotePriority = []string{ChaosID} }),
		},
		{
			name: "only Horizons set",
			cfg:  Config{Horizons: []HorizonConfig{{Horizon: HorizonDay, WindowHours: 12, MinHoursSeen: 9}}},
			want: withField(func(c *Config) {
				c.Horizons = []HorizonConfig{{Horizon: HorizonDay, WindowHours: 12, MinHoursSeen: 9}}
			}),
		},
		{
			name: "a negative MinEdge is a choice, not an unset field",
			cfg:  Config{MinEdge: -0.5},
			want: withField(func(c *Config) { c.MinEdge = -0.5 }),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.cfg.withDefaults(); !reflect.DeepEqual(got, tt.want) {
				t.Errorf("withDefaults() = %+v, want %+v", got, tt.want)
			}
		})
	}
}

func TestConfigWithDefaults_nonPositiveCount_fallsBackToTheDefault(t *testing.T) {
	// Negative counts, floors and caps have no meaning, so they read as unset —
	// the one exception being MinEdge, which the table above pins. It is also
	// why no gate can be switched off by passing 0 or -1; the way to run without
	// one is a value that cannot bind.
	cfg := Config{
		WindowHours:      -1,
		MinVolumePerHour: -1,
		MinTurnoverChaos: -1,
		MaxTick:          -1,
		MinEdgeTickRatio: -1,
		MinROIChaos:      -1,
		MinHoursSeen:     -1,
		MaxPlays:         -1,
	}

	if got, want := cfg.withDefaults(), DefaultConfig(); !reflect.DeepEqual(got, want) {
		t.Errorf("withDefaults() = %+v, want %+v", got, want)
	}
}

// withField returns DefaultConfig with one field overridden.
func withField(set func(*Config)) Config {
	cfg := DefaultConfig()
	set(&cfg)
	return cfg
}

func TestResult_marshalsWithTheFieldNamesTheHandlerPublishes(t *testing.T) {
	result := Result{
		League:          "Allflame",
		Horizon:         string(HorizonRecent),
		From:            feedHour,
		To:              feedHour.Add(time.Hour),
		Hours:           1,
		DivineChaosRate: 198.97,
		Plays: []Play{{
			Key:  directKey(chaosID, cardID),
			Mode: ModeDirect,
			Legs: []Leg{
				{Action: "buy", Item: cardID, Quote: chaosID, Price: 100, Fair: 110, Tick: 0.01, Volume: 1000, Stock: 500},
				{Action: "sell", Item: cardID, Quote: chaosID, Price: 120, Fair: 110, Tick: 0.01, Volume: 1000, Stock: 500},
			},
			RoiPct:           0.2,
			Edge:             0.2,
			RoiPctNewestHour: 0.35,
			Roi:              20,
			Investment:       100,
			Turnover:         110000,
			Tick:             0.01,
			Depth:            1000,
			HoursSeen:        1,
			LastHour:         feedHour,
		}},
	}

	data, err := json.Marshal(result)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var envelope struct {
		Plays []map[string]json.RawMessage `json:"plays"`
	}
	if err := json.Unmarshal(data, &envelope); err != nil {
		t.Fatalf("decode plays: %v", err)
	}
	if len(envelope.Plays) != 1 {
		t.Fatalf("got %d plays, want 1", len(envelope.Plays))
	}
	wantKeys(t, "result", data, "league", "horizon", "from", "to", "hours", "divineChaosRate", "plays")
	wantKeys(t, "play", mustMarshal(t, envelope.Plays[0]),
		"key", "mode", "legs", "roiPct", "edge", "roiPctNewestHour", "roi", "investment",
		"turnover", "tick", "depth", "hoursSeen", "lastHour")

	var legs []map[string]json.RawMessage
	if err := json.Unmarshal(envelope.Plays[0]["legs"], &legs); err != nil {
		t.Fatalf("decode legs: %v", err)
	}
	wantKeys(t, "leg", mustMarshal(t, legs[0]), "action", "item", "quote", "price", "fair", "tick", "volume", "stock")

	if got := string(envelope.Plays[0]["mode"]); got != `"direct"` {
		t.Errorf("mode = %s, want %q", got, "direct")
	}

	var round Result
	if err := json.Unmarshal(data, &round); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !reflect.DeepEqual(round, result) {
		t.Errorf("round trip = %+v, want %+v", round, result)
	}
}

// wantKeys fails unless the JSON object carries exactly the named keys.
func wantKeys(t *testing.T, label string, data []byte, want ...string) {
	t.Helper()
	var object map[string]json.RawMessage
	if err := json.Unmarshal(data, &object); err != nil {
		t.Fatalf("decode %s: %v", label, err)
	}
	got := make([]string, 0, len(object))
	for key := range object {
		got = append(got, key)
	}
	sort.Strings(got)
	sort.Strings(want)
	if !reflect.DeepEqual(got, want) {
		t.Errorf("%s JSON keys = %v, want %v", label, got, want)
	}
}

// mustMarshal re-encodes a decoded object so its keys can be inspected.
func mustMarshal(t *testing.T, object map[string]json.RawMessage) []byte {
	t.Helper()
	data, err := json.Marshal(object)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return data
}
