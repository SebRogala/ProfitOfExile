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
// high at the dearest, a thousand items traded, stock on both sides.
//
// The chaos side's volume is set so the hour's volume-weighted price — the quote
// units traded over the item units — lands on the GEOMETRIC centre of the
// spread, sqrt(low*high). That is the anchor that sits the same distance from
// both extremes in ratio terms, so neither leg reads as junk while the spread
// stays under about 2.2x (Config.SuspectLowBand 0.67, SuspectHighBand 1.5) and a
// test that wants a flagged extreme says so by anchoring the hour elsewhere
// (chaosMarketAnchoredAt).
//
// Chaos is ItemA and the item is the traded side, so the leg prices ARE low and
// high, and the market's tick is 1/low. A test about one gate overrides the
// single quantity that gate reads and inherits the rest of the liquidity, so
// each fixture states only what its test is about.
func liquidChaosMarket(item string, low, high int64) rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        item,
		volume:       [2]int64{int64(math.Round(math.Sqrt(float64(low)*float64(high)) * 1000)), 1000},
		lowestStock:  [2]int64{2000, 200},
		highestStock: [2]int64{5000, 500},
		lowestRatio:  [2]int64{low, 1},
		highestRatio: [2]int64{high, 1},
	}
}

// chaosMarketAnchoredAt is liquidChaosMarket with the hour's mass moved: the
// same thousand items changed hands, against fair*1000 chaos, so the hour's
// volume-weighted price is exactly fair. It is how the suspect tests place an
// extreme inside or outside its band.
func chaosMarketAnchoredAt(item string, low, high int64, fair float64) rowSpec {
	spec := liquidChaosMarket(item, low, high)
	spec.volume = [2]int64{int64(math.Round(fair * 1000)), 1000}
	return spec
}

// liquidTriangle is the smallest window carrying a one-hop route that clears
// every default gate, with every price exact in binary: a scarab costs 10 chaos,
// the same scarab fetches a tenth of a divine, and a divine sells back for 200
// chaos — so buying in chaos and selling through divine nearly doubles the stake
// once each leg has paid its tick to be the order that fills.
//
// It yields exactly two plays: the one-hop route scarab -> divine -> chaos and
// the divine-quoted flip of the same scarab, both with 112,000 chaos an hour of
// turnover. The chaos-quoted flip (+20% raw, on a tick of 10%) does not survive
// the undercut at all, and the chaos/divine market itself prints one price and
// so has no spread.
//
// The divine market's volumes put its hour's volume-weighted price at 0.07
// divine a scarab, between the two extremes and inside both suspect bands, so
// the standard triangle is clean; the suspect test reprices that one hour.
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
		volume:       [2]int64{560, 8000},
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
// play in the SAME HOUR is valued at.
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

// undercutRoi is the return one round trip makes after each side has paid one
// tick to be the order that fills, spelled out the way a reader of the wire
// would recompute it: buy a tick above the hour's low, sell a tick under its
// high. The prices are passed in execution order, so a one-hop route hands it
// three.
//
// It exists so the expectation is written once, at runtime, in the same
// operation order the engine uses — a constant expression would be folded at
// arbitrary precision and could differ from the engine's float64 in the last
// bits, which is exactly what the boundary tests are reading.
func undercutRoi(buy, buyTick float64, sells ...[2]float64) float64 {
	proceeds := 1.0
	for _, sell := range sells {
		proceeds *= sell[0] * (1 - sell[1])
	}
	return proceeds/(buy*(1+buyTick)) - 1
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

func TestBestPlays_threeHours_pricesBothLegsFromTheNewestHour(t *testing.T) {
	// POE-188: the served prices are ONE hour's, the last snapshot's. The three
	// hours are staggered so that the newest hour is neither the median nor the
	// mean of the window on either side — a blend would read 90 -> 120 (median)
	// or 90 -> 123.33 (mean), and neither is a pair anybody could have traded.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 140).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 90, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 80, 110).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.Legs[0].Price != 100 {
		t.Errorf("buy price = %v, want the NEWEST hour's low 100 (the window's lows are {100, 90, 80})", play.Legs[0].Price)
	}
	if play.Legs[1].Price != 140 {
		t.Errorf("sell price = %v, want the NEWEST hour's high 140 (the window's highs are {140, 120, 110})", play.Legs[1].Price)
	}
}

func TestBestPlays_threeHours_fairIsTheNewestHoursVolumeWeightedPrice(t *testing.T) {
	// Fair is where the hour's MASS traded (quote units / item units), which is
	// what says whether a Price at the edge of the spread is opportunity or
	// quantization. It belongs to the same hour as the prices it anchors.
	rows := []StoredRow{}
	for i, chaosTraded := range []int64{110000, 90000, 100000} {
		spec := liquidChaosMarket(cardID, 100, 120)
		spec.volume = [2]int64{chaosTraded, 1000}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// The hourly averages are 110, 90 and 100 chaos; the median is 100 and the
	// mean is 100 too, so only the newest hour's 110 can pass.
	for i, leg := range play.Legs {
		wantClose(t, "leg "+leg.Action+" fair", leg.Fair, 110)
		if !leg.FairOK {
			t.Errorf("leg %d FairOK = false, want true — the hour traded 1000 items against 110000 chaos", i)
		}
	}
}

func TestBestPlays_threeHours_tickIsTheNewestHoursPriceResolution(t *testing.T) {
	// The market's quantity pairs coarsen as the window goes back (1/100, 1/50,
	// 1/20 of the price); the play reports the hour it is quoting, not the
	// middle one and not the worst.
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
		wantClose(t, "leg "+string(rune('0'+i))+" tick", leg.Tick, 1.0/100.0)
	}
	if play.Tick != play.Legs[0].Tick {
		t.Errorf("play Tick = %v, want the single market's newest tick %v", play.Tick, play.Legs[0].Tick)
	}
}

func TestBestPlays_threeHours_volumeIsTheNewestHoursTradedUnits(t *testing.T) {
	rows := []StoredRow{}
	for i, units := range []int64{10000, 100, 1000} {
		spec := liquidChaosMarket(cardID, 100, 120)
		// The chaos side moves with the item side so every hour still trades
		// around 110 chaos an item and no extreme reads as junk.
		spec.volume = [2]int64{units * 110, units}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	for i, leg := range play.Legs {
		if leg.Volume != 10000 {
			t.Errorf("leg %d volume = %v, want the newest hour's 10000 (median would be 1000)", i, leg.Volume)
		}
	}
	if play.Depth != 10000 {
		t.Errorf("Depth = %v, want the newest hour's 10000", play.Depth)
	}
}

func TestBestPlays_threeHours_stocksTheNewestHour(t *testing.T) {
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
		t.Errorf("LastHour = %v, want the window's newest hour %v", play.LastHour, feedHour)
	}
}

func TestBestPlays_playThatStoppedClearingInTheNewestHour_isNotServed(t *testing.T) {
	// Persistence is not a licence to serve a stale price. The card cleared in
	// the two older hours — enough for MinHoursSeen — and then its spread CLOSED
	// in the last snapshot: one price all hour, so the round trip loses its two
	// ticks and the sanity floor cuts it. There is no current price to show, so
	// the row is dropped rather than served at an hour-old one. The hell market
	// clears in all three and proves the window itself is alive.
	closed := liquidChaosMarket(cardID, 100, 120)
	closed.highestRatio = [2]int64{100, 1}

	rows := append(
		storedAt(feedHour, closed.row(), liquidChaosMarket(hellID, 100, 120).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 120).row(), liquidChaosMarket(hellID, 100, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 100, 120).row(), liquidChaosMarket(hellID, 100, 120).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	if want := []string{directKey(chaosID, hellID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v — the card last cleared an hour ago", playKeys(got.Plays), want)
	}
}

func TestBestPlays_hourThatFailedItsGates_isNotCountedInHoursSeen(t *testing.T) {
	// HoursSeen counts the hours the recipe CLEARED on that hour's own prices,
	// not the hours its market appeared in: the middle hour traded no cards at
	// all and so failed the liveness floor, making three sightings two hours seen.
	quiet := liquidChaosMarket(cardID, 100, 120)
	quiet.volume = [2]int64{986, 0}

	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), quiet.row()),
			storedAt(feedHour.Add(-2*time.Hour), liquidChaosMarket(cardID, 100, 120).row())...,
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 2 {
		t.Errorf("HoursSeen = %d, want 2 — the middle hour did not clear the liveness gate", play.HoursSeen)
	}
}

func TestBestPlays_singleHourWindow_capsMinHoursSeenAtTheHoursPresent(t *testing.T) {
	// An armed demand for four hours against a feed holding one. Capping is what
	// keeps a fresh league (or a just-restarted walk) from returning nothing at
	// all, and it is armed here because the shipped default of 1 is met by every
	// served play and would leave the cap unexercised.
	cfg := DefaultConfig()
	cfg.MinHoursSeen = 4

	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), cfg)

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1", play.HoursSeen)
	}
}

func TestBestPlays_playSeenInOnlyOneOfTwoHours_isServedWithTheCountAtDefaults(t *testing.T) {
	// POE-193's persistence-as-information rule: the one-hour recipe is SERVED,
	// beside the steady one, and what says they are different is HoursSeen. The
	// newest-hour rule already guarantees both prices are current, so dropping
	// the young one would have deleted a reading rather than a stale row.
	steady := liquidChaosMarket(cardID, 100, 120).row()
	young := liquidChaosMarket(hellID, 100, 200).row()
	rows := append(
		storedAt(feedHour, steady, young),
		storedAt(feedHour.Add(-time.Hour), steady)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	if hoursSeen := playByKey(t, got, directKey(chaosID, hellID)).HoursSeen; hoursSeen != 1 {
		t.Errorf("the one-hour play reads HoursSeen %d, want 1", hoursSeen)
	}
	if hoursSeen := playByKey(t, got, directKey(chaosID, cardID)).HoursSeen; hoursSeen != 2 {
		t.Errorf("the steady play reads HoursSeen %d, want 2", hoursSeen)
	}
}

func TestBestPlays_playSeenInOnlyOneOfTwoHours_isDroppedWhenMinHoursSeenIsArmed(t *testing.T) {
	// The same window under the knob EXCHANGE_*_MIN_HOURS_SEEN is now FOR: a
	// reader who wants ghosts gone can still have them gone, and this documents
	// what arming the level does.
	steady := liquidChaosMarket(cardID, 100, 120).row()
	ghost := liquidChaosMarket(hellID, 100, 200).row()
	rows := append(
		storedAt(feedHour, steady, ghost),
		storedAt(feedHour.Add(-time.Hour), steady)...,
	)
	cfg := DefaultConfig()
	cfg.MinHoursSeen = 2

	got := BestPlays("Allflame", rows, cfg)

	// The ghost carries by far the bigger return and is still dropped: printing
	// once in the window is the disqualifier once the level is armed.
	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
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

func TestBestPlays_directPlay_roiPctIsTheReturnAfterOneTickOfUndercutPerSide(t *testing.T) {
	// The acceptance criterion of the whole engine: whatever percentage a play
	// claims has to be arithmetic on what it shows — the two prices AND the tick
	// each side pays to be the order that fills. An order posted at exactly the
	// last realized price is behind every order already queued there.
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	tick := 1.0 / 100.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(100, tick, [2]float64{120, tick}))
	wantClose(t, "RoiPct recomputed from the emitted legs", play.RoiPct,
		undercutRoi(play.Legs[0].Price, play.Legs[0].Tick, [2]float64{play.Legs[1].Price, play.Legs[1].Tick}))
}

func TestBestPlays_directPlay_roiPctRawIsTheReturnAtTheRawExtremes(t *testing.T) {
	// RoiPctRaw is what the hour printed with nothing paid for the fill — the
	// number the pre-POE-188 engine called the edge. It sits above RoiPct by
	// exactly what the ticks cost.
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	wantClose(t, "RoiPctRaw", play.RoiPctRaw, 120.0/100.0-1)
	if !(play.RoiPct < play.RoiPctRaw) {
		t.Errorf("RoiPct = %v, want it under the raw %v — a 1%% tick a side is not free", play.RoiPct, play.RoiPctRaw)
	}
}

func TestBestPlays_oneHopPlay_roiPctUndercutsEachLegOnItsOwnTick(t *testing.T) {
	// Three legs, three different ticks: the route pays the chaos market's 10%
	// on the way in, the divine market's 10% on the way out and the anchor's
	// 0.5% to close. A single play-wide tick would misprice it.
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	wantClose(t, "RoiPct", play.RoiPct,
		undercutRoi(10, 1.0/10.0, [2]float64{1.0 / 10.0, 1.0 / 10.0}, [2]float64{200, 1.0 / 200.0}))
	wantClose(t, "RoiPct recomputed from the emitted legs", play.RoiPct,
		undercutRoi(play.Legs[0].Price, play.Legs[0].Tick,
			[2]float64{play.Legs[1].Price, play.Legs[1].Tick},
			[2]float64{play.Legs[2].Price, play.Legs[2].Tick}))
}

func TestBestPlays_edge_carriesTheSameValueAsRoiPct(t *testing.T) {
	// edge is the pre-POE-184 name kept on the wire for the desktop build that
	// still reads it; it must carry the UNDERCUT return the ranking used, not
	// the raw spread. The fixture's 1% tick keeps the two apart.
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.RoiPctRaw == play.RoiPct {
		t.Fatalf("RoiPctRaw = RoiPct = %v; the fixture has to keep the two apart", play.RoiPct)
	}
	if play.Edge != play.RoiPct {
		t.Errorf("Edge = %v, want RoiPct %v (RoiPctRaw is %v)", play.Edge, play.RoiPct, play.RoiPctRaw)
	}
}

func TestBestPlays_chaosQuotedDirectPlay_investmentIsTheUndercutEntryPriceInChaos(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// One unit costs what the BUY leg pays to be filled — the hour's low plus
	// one tick of it — and chaos is worth one chaos. The leg still shows the raw
	// 100.
	wantClose(t, "Investment", play.Investment, 100*(1+1.0/100.0))
	if play.Legs[0].Price != 100 {
		t.Errorf("buy leg price = %v, want the RAW low 100 — the undercut lives in Investment", play.Legs[0].Price)
	}
}

func TestBestPlays_chaosQuotedDirectPlay_roiIsTheChaosGainedPerExchangedUnit(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	// Buy at 101, sell at 118.80: 17.80 chaos a unit. Roi IS Investment * RoiPct
	// by construction, so only the literal difference of the two undercut prices
	// pins it — restating the identity here would assert the production formula
	// against itself.
	tick := 1.0 / 100.0
	wantClose(t, "Roi", play.Roi, 120*(1-tick)-100*(1+tick))
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

func TestBestPlays_divineQuotedDirectPlay_valuesTheUndercutEntryAtTheHoursDivineRate(t *testing.T) {
	// A play quoted in divine is still ranked in chaos, so its entry price is
	// converted through the SAME HOUR's divine/chaos rate rather than a guess.
	_, divineLeg, anchor := liquidTriangle()

	got := BestPlays("Allflame", storedAt(feedHour, divineLeg.row(), anchor.row()), DefaultConfig())

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.Legs[0].Price != 0.05 {
		t.Fatalf("buy price = %v, want a twentieth of a divine", play.Legs[0].Price)
	}
	// Entering costs 0.05 divine plus its 10% tick — 0.055 divine, 11 chaos at
	// the hour's 200 — and the flip returns 0.09 divine, 18 chaos.
	wantClose(t, "Investment", play.Investment, 0.05*1.1*200)
	wantClose(t, "Roi", play.Roi, (0.1*0.9-0.05*1.1)*200)
	wantClose(t, "Turnover", play.Turnover, 560*200)
}

func TestBestPlays_oneHopPlay_investmentAndRoiAreValuedInChaosFromTheFirstLeg(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	// The route is entered by paying 10 chaos plus the chaos market's 10% tick
	// for a scarab, and closes at 0.09 divine sold at 199.
	wantClose(t, "Investment", play.Investment, 10*1.1)
	wantClose(t, "Roi", play.Roi, 0.1*0.9*200*0.995-10*1.1)
}

func TestBestPlays_oneHopPlay_turnoverIsItsThinnestLegInChaos(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(liquidTriangle())...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	// 200,000 chaos flowed through the chaos/scarab market and 4,000,000 through
	// the chaos/divine one, but the divine/scarab leg moved only 560 divine —
	// 112,000 chaos, and a recipe is as liquid as its thinnest step.
	wantClose(t, "Turnover", play.Turnover, 560*200)
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

func TestBestPlays_buyLegUnderTheLowBand_isFlaggedSuspect(t *testing.T) {
	// The measured POE-188 failure in miniature: the hour's mass traded at 100
	// chaos and one trade printed at 60, under two thirds of fair. That low is a
	// trade nobody could repeat, and the flag is what says so — the sell side of
	// the same market, inside its band, stays clean.
	got := BestPlays("Allflame", storedAt(feedHour, chaosMarketAnchoredAt(cardID, 60, 120, 100).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if !play.Legs[0].Suspect {
		t.Errorf("buy leg Suspect = false, want true — its low %v is under 0.67 * 100", play.Legs[0].Price)
	}
	if play.Legs[1].Suspect {
		t.Errorf("sell leg Suspect = true, want false — 120 is inside 1.5 * 100")
	}
}

func TestBestPlays_sellLegOverTheHighBand_isFlaggedSuspect(t *testing.T) {
	got := BestPlays("Allflame", storedAt(feedHour, chaosMarketAnchoredAt(cardID, 90, 200, 100).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if !play.Legs[1].Suspect {
		t.Error("sell leg Suspect = false, want true — 200 is over 1.5 * 100")
	}
	if play.Legs[0].Suspect {
		t.Error("buy leg Suspect = true, want false — 90 is inside 0.67 * 100")
	}
}

func TestBestPlays_lowExactlyOnTheBand_isNotSuspect(t *testing.T) {
	// The band is a floor the low may stand on: flagged only BELOW it. The test
	// bands are halves rather than the default 0.67 so that fair * band is exact
	// in binary and the boundary case sits ON the comparison instead of a
	// rounding to one side of it.
	cfg := DefaultConfig()
	cfg.SuspectLowBand, cfg.SuspectHighBand = 0.5, 2

	tests := []struct {
		name string
		low  int64
		want bool
	}{
		{name: "exactly on the band", low: 50, want: false},
		{name: "one chaos under the band", low: 49, want: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := BestPlays("Allflame", storedAt(feedHour, chaosMarketAnchoredAt(cardID, tt.low, 200, 100).row()), cfg)

			play := playByKey(t, got, directKey(chaosID, cardID))
			if play.Legs[0].Suspect != tt.want {
				t.Errorf("buy leg Suspect = %v, want %v (low %d against 0.5 * 100)", play.Legs[0].Suspect, tt.want, tt.low)
			}
		})
	}
}

func TestBestPlays_highExactlyOnTheBand_isNotSuspect(t *testing.T) {
	cfg := DefaultConfig()
	cfg.SuspectLowBand, cfg.SuspectHighBand = 0.5, 2

	tests := []struct {
		name string
		high int64
		want bool
	}{
		{name: "exactly on the band", high: 200, want: false},
		{name: "one chaos over the band", high: 201, want: true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := BestPlays("Allflame", storedAt(feedHour, chaosMarketAnchoredAt(cardID, 50, tt.high, 100).row()), cfg)

			play := playByKey(t, got, directKey(chaosID, cardID))
			if play.Legs[1].Suspect != tt.want {
				t.Errorf("sell leg Suspect = %v, want %v (high %d against 2 * 100)", play.Legs[1].Suspect, tt.want, tt.high)
			}
		})
	}
}

func TestBestPlays_oneHopPlayWithOneSuspectLeg_isItselfSuspect(t *testing.T) {
	// One unrepeatable extreme is enough to make the whole percentage a story,
	// so the route carries the flag its middle leg raised while the two other
	// legs stay clean.
	chaosLeg, divineLeg, anchor := liquidTriangle()
	// 500 divine against 8000 scarabs is a fair of 0.0625, so the hour's dearest
	// scarab at 0.1 divine sits over 1.5 times it.
	divineLeg.volume = [2]int64{500, 8000}

	got := BestPlays("Allflame", storedAt(feedHour, triangleRows(chaosLeg, divineLeg, anchor)...), DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	if !play.Suspect {
		t.Error("play Suspect = false, want true — the sell leg is flagged")
	}
	wantLegs := []bool{false, true, false}
	for i, leg := range play.Legs {
		if leg.Suspect != wantLegs[i] {
			t.Errorf("leg %d Suspect = %v, want %v", i, leg.Suspect, wantLegs[i])
		}
	}
}

// unanchoredFlip is one hour of a chaos-quoted flip whose quote side traded
// nothing: the hour priced the card (100 chaos at the cheapest, 120 at the
// dearest) and moved 1000 of them, but no chaos volume came back, so vwapIn
// found no volume-weighted price and the leg carries vwapOK false with a vwap
// of 0.
//
// It is built as a candidate rather than as rows because BestPlays cannot reach
// this state: vwapIn fails only when a side traded nothing, and a leg with no
// quote volume has a Turnover of 0, which MinTurnoverChaos cuts before any Play
// is served. evaluate is where the missing-anchor contract is decided — what
// Leg.Fair/Leg.FairOK report, and that the suspect bands cannot fire without an
// anchor to compare against — so it is exercised one layer under the window,
// with the turnover floor set to a zero that its zero turnover cannot fail.
func unanchoredFlip() (candidate, Config) {
	o := obs{low: 100, high: 120, vwap: 0, vwapOK: false, tick: 0.01, quoteVolume: 0, volume: 1000, stock: 200}
	c := candidate{
		key:  directKey(chaosID, cardID),
		mode: ModeDirect,
		legs: []candidateLeg{
			{action: "buy", item: cardID, quote: chaosID, obs: o},
			{action: "sell", item: cardID, quote: chaosID, obs: o},
		},
	}
	// evaluate reads cfg as given — withDefaults runs in BestPlays, above it — so
	// the zero floor stands. Every other gate is the shipped one and passes: the
	// undercut return is 17.6% on a 1% tick, for 17.8 chaos of payout.
	cfg := DefaultConfig()
	cfg.MinTurnoverChaos = 0
	return c, cfg
}

func TestEvaluate_legWhoseHourHadNoVolumeWeightedPrice_reportsFairAsMissingRatherThanAsZero(t *testing.T) {
	// A fair of 0 with FairOK true would render as a free card. The pair is the
	// contract: the number is 0 because there is nothing to report, and FairOK is
	// what says so.
	c, cfg := unanchoredFlip()

	play, ok := evaluate(c, feedHour, 0, false, cfg)

	if !ok {
		t.Fatalf("evaluate rejected the candidate; the fixture no longer clears the gates (%+v)", play)
	}
	for i, leg := range play.Legs {
		if leg.FairOK {
			t.Errorf("leg %d FairOK = true, want false — the hour's quote side traded nothing", i)
		}
		if leg.Fair != 0 {
			t.Errorf("leg %d Fair = %v, want 0 — there was no volume-weighted price to report", i, leg.Fair)
		}
	}
}

func TestEvaluate_legWithoutAFairAnchor_isNeverFlaggedSuspect(t *testing.T) {
	// Suspect means "this extreme sits too far from the hour's fair value". With
	// no fair value there is no such judgement to make, and an unguarded band
	// would read the missing anchor as a fair of 0 — against which every sell
	// price is infinitely high and would be flagged.
	c, cfg := unanchoredFlip()

	play, ok := evaluate(c, feedHour, 0, false, cfg)

	if !ok {
		t.Fatalf("evaluate rejected the candidate; the fixture no longer clears the gates (%+v)", play)
	}
	for i, leg := range play.Legs {
		if leg.Suspect {
			t.Errorf("leg %d (%s at %v) Suspect = true, want false — nothing anchors it", i, leg.Action, leg.Price)
		}
	}
	if play.Suspect {
		t.Error("play Suspect = true, want false — no leg could be judged")
	}
}

func TestBestPlays_suspectPlay_ranksBehindEveryCleanOne(t *testing.T) {
	// The suspect market carries six times the return of the clean one and
	// still ranks second: a percentage built on an extreme nobody could repeat
	// is not a better play, and the reader sees it flagged rather than at the
	// top.
	rows := storedAt(feedHour,
		chaosMarketAnchoredAt(hellID, 90, 200, 100).row(),
		liquidChaosMarket(cardID, 100, 120).row(),
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{directKey(chaosID, cardID), directKey(chaosID, hellID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Fatalf("keys = %v, want the clean play first %v", playKeys(got.Plays), want)
	}
	clean, suspect := playByKey(t, got, want[0]), playByKey(t, got, want[1])
	if clean.Suspect || !suspect.Suspect {
		t.Fatalf("Suspect flags are %v and %v; the fixture no longer separates the two", clean.Suspect, suspect.Suspect)
	}
	if !(suspect.RoiPct > clean.RoiPct) {
		t.Errorf("the suspect play's RoiPct is %v against the clean %v — the fixture no longer proves the flag outranks the return",
			suspect.RoiPct, clean.RoiPct)
	}
}

func TestBestPlays_hideSuspect_dropsTheFlaggedPlayInsteadOfRankingItLast(t *testing.T) {
	rows := storedAt(feedHour,
		chaosMarketAnchoredAt(hellID, 90, 200, 100).row(),
		liquidChaosMarket(cardID, 100, 120).row(),
	)
	cfg := DefaultConfig()
	cfg.HideSuspect = true

	got := BestPlays("Allflame", rows, cfg)

	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_hideSuspect_doesNotCountAnHourWhoseLegWasFlagged(t *testing.T) {
	// Hiding happens INSIDE the per-hour evaluation, so an hour that was junk is
	// not a cleared hour either. The market's oldest hour traded its mass at 60
	// chaos and still printed a 120 high; the two newer hours are clean.
	rows := append(
		storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()),
		append(
			storedAt(feedHour.Add(-time.Hour), liquidChaosMarket(cardID, 100, 120).row()),
			storedAt(feedHour.Add(-2*time.Hour), chaosMarketAnchoredAt(cardID, 100, 120, 60).row())...,
		)...,
	)

	if got := BestPlays("Allflame", rows, DefaultConfig()); playByKey(t, got, directKey(chaosID, cardID)).HoursSeen != 3 {
		t.Fatalf("HoursSeen = %d with the flag shown, want all 3 hours counted",
			playByKey(t, got, directKey(chaosID, cardID)).HoursSeen)
	}

	cfg := DefaultConfig()
	cfg.HideSuspect = true

	got := BestPlays("Allflame", rows, cfg)

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 2 {
		t.Errorf("HoursSeen = %d, want 2 — the hidden hour must not count as a cleared one", play.HoursSeen)
	}
}

func TestBestPlays_hourWithoutADivineChaosMarket_doesNotClearItsDivineQuotedPlays(t *testing.T) {
	// The rate is read per hour, so an hour that never traded a divine cannot
	// value a divine-quoted payout in chaos at all — that hour does not count,
	// however liquid the market itself was.
	_, divineLeg, anchor := liquidTriangle()
	rows := append(
		storedAt(feedHour, divineLeg.row(), anchor.row()),
		storedAt(feedHour.Add(-time.Hour), divineLeg.row())...,
	)
	cfg := DefaultConfig()
	cfg.MinHoursSeen = 1

	got := BestPlays("Allflame", rows, cfg)

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1 — the older hour had no divine/chaos market to value the payout with", play.HoursSeen)
	}
}

// cheapDivineAnchor is divineChaosAnchor repriced: one divine went for ten
// chaos all hour, and the hour's 20,000 divine traded against 200,000 chaos put
// its volume-weighted rate on the same ten. It is the same market as the
// standard anchor's 200, so an hour carrying it is priced by its OWN rate rather
// than by the window's.
func cheapDivineAnchor() rowSpec {
	spec := divineChaosAnchor()
	spec.volume = [2]int64{200000, 20000}
	spec.lowestRatio = [2]int64{10, 1}
	spec.highestRatio = [2]int64{10, 1}
	return spec
}

func TestBestPlays_olderHourWithACheaperDivine_isValuedAtThatHoursRateNotTheNewestOnes(t *testing.T) {
	// Presence of the divine/chaos market is not the whole contract: its VALUE is
	// read per hour too. Both hours trade the same divine-quoted scarab market —
	// 560 divine of quote volume, a 0.05 -> 0.1 spread — so only the rate the hour
	// is valued at separates them. A chaos-denominated gate is what makes that
	// visible, so the turnover floor is ARMED here (it ships off since POE-191):
	// at the newest hour's 200 chaos a divine the leg carries 112,000 chaos of
	// turnover and clears it, while at the older hour's 10 the same leg carries
	// 5,600 and cannot.
	_, divineLeg, anchor := liquidTriangle()
	rows := append(
		storedAt(feedHour, divineLeg.row(), anchor.row()),
		storedAt(feedHour.Add(-time.Hour), divineLeg.row(), cheapDivineAnchor().row())...,
	)
	cfg := DefaultConfig()
	cfg.MinHoursSeen = 1
	cfg.MinTurnoverChaos = 10000

	got := BestPlays("Allflame", rows, cfg)

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.HoursSeen != 1 {
		t.Errorf("HoursSeen = %d, want 1 — the older hour's divine was worth 10 chaos, not the newest hour's 200", play.HoursSeen)
	}
}

func TestBestPlays_divineChaosRate_isTheNewestHoursRate(t *testing.T) {
	// The headline rate is the CURRENT chaos value of a divine, so it is read
	// from the last snapshot alone. The window's hourly rates disagree by design.
	rows := []StoredRow{}
	for i, chaosTraded := range []int64{10000000, 3800000, 4000000} {
		spec := divineChaosAnchor()
		spec.volume = [2]int64{chaosTraded, 20000}
		rows = append(rows, storedAt(feedHour.Add(-time.Duration(i)*time.Hour), spec.row())...)
	}

	got := BestPlays("Allflame", rows, DefaultConfig())

	// The hourly rates are 500, 190 and 200 chaos a divine: the median is 200
	// and the mean 296.7, so only the newest hour's 500 can pass.
	wantClose(t, "DivineChaosRate", got.DivineChaosRate, 500)
}

func TestBestPlays_noDivineChaosMarketInTheWindow_reportsARateOfZero(t *testing.T) {
	_, divineLeg, _ := liquidTriangle()

	got := BestPlays("Allflame", storedAt(feedHour, divineLeg.row(), liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	if got.DivineChaosRate != 0 {
		t.Errorf("DivineChaosRate = %v, want 0 — nothing in the window priced a divine", got.DivineChaosRate)
	}
}

func TestBestPlays_newestHourWithoutADivineChaosMarket_reportsARateOfZeroRatherThanAnOlderHours(t *testing.T) {
	// The rate the client displays is the CURRENT one, so it is the newest hour's
	// or nothing: an hour that never traded a divine reports 0 rather than
	// reaching back for the last hour that did. A fallback would publish an older
	// hour's 200 as today's price with nothing on the wire saying so.
	_, divineLeg, anchor := liquidTriangle()
	rows := append(
		storedAt(feedHour, divineLeg.row()),
		storedAt(feedHour.Add(-time.Hour), divineLeg.row(), anchor.row())...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	if got.DivineChaosRate != 0 {
		t.Errorf("DivineChaosRate = %v, want 0 — the newest hour had no divine/chaos market, and 200 is the OLDER hour's rate", got.DivineChaosRate)
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
	// 10,000, an entry of 101 and a payout of 17.8 against a floor of 3 — so a
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
	// turnover, a tick of 1%, seventeen units of gain per exchange.
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
	// 242%, over 100k it is 18%. The gate is inclusive at the floor.
	//
	// It ships OFF since POE-191 — the desktop offers it as a knob and recommends
	// 10,000 — so the level is armed here rather than inherited. What the test
	// pins is the arithmetic the client's knob rides on: the comparison is
	// Turnover >= the floor, in chaos, and one chaos short of it is out.
	cfg := DefaultConfig()
	cfg.MinTurnoverChaos = 10000

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

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (Turnover %v)", playKeys(got.Plays), tt.want, tt.chaosTraded)
			}
		})
	}
}

func TestBestPlays_maxTick_cutsTheSpreadsThatAreOneIntegerPriceStepWide(t *testing.T) {
	// tick = 1/max(quantity), so a market quoting ten chaos to the item can only
	// move in tenths — exactly the cap. Nine chaos to the item cannot.
	//
	// The cap ships OFF since POE-191 (DefaultConfig sets 1, which no tick can
	// exceed) and the desktop offers 10% as a recommended knob, so the level is
	// armed here. The comparison it pins is Tick <= the cap, inclusive.
	cfg := DefaultConfig()
	cfg.MaxTick = 0.10

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

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (tick 1/%d)", playKeys(got.Plays), tt.want, tt.low)
			}
		})
	}
}

func TestBestPlays_minEdgeTickRatio_cutsTheSpreadsNarrowerThanFivePriceSteps(t *testing.T) {
	// The market quotes four chaos to the card in lots of four, so one price
	// step is a quarter of the price: buying costs 5 and the 15-chaos high sells
	// for 11.25, a return of exactly five steps. Fifteen rather than a rounder
	// number because 4, 15 and a quarter are all exact in binary, so both sides
	// of the comparison are the SAME double and the case sits ON the gate
	// instead of a rounding above it.
	//
	// The ratio ships OFF since POE-191 and the desktop offers 5 as a recommended
	// knob, so it is armed here; the tick cap is armed at 0.5 as well, because a
	// market this coarse is exactly what the client's recommended 10% would drop
	// and the point here is the ratio, not the cap.
	cfg := DefaultConfig()
	cfg.MinEdgeTickRatio = 5
	cfg.MaxTick = 0.5

	tests := []struct {
		name string
		high int64
		want []string
	}{
		{
			name: "return exactly five price steps wide",
			high: 15,
			want: []string{directKey(chaosID, cardID)},
		},
		{
			name: "return one step short of five",
			high: 14,
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 4, tt.high)
			spec.volume = [2]int64{27500, 5000}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v (RoiPct %v against 5 ticks of 0.25)",
					playKeys(got.Plays), tt.want, undercutRoi(4, 0.25, [2]float64{float64(tt.high), 0.25}))
			}
		})
	}
}

func TestBestPlays_minEdgeTickRatio_judgesTheUndercutReturnNotTheRawSpread(t *testing.T) {
	// The gate reads the return an order that gets TAKEN can expect. This omen
	// prints half a divine at the cheapest and three quarters at the dearest on
	// a 10% tick: the raw spread is 50%, exactly the five steps the gate asks
	// for, while the undercut return is 22.7% and does not come close. Which
	// price system the gate reads is the engine's decision and stays the engine's
	// after POE-191, so the level is armed here and the pin is unchanged.
	inDivine := rowSpec{
		itemA:        divineID,
		itemB:        omenID,
		volume:       [2]int64{500, 800},
		lowestStock:  [2]int64{300, 40},
		highestStock: [2]int64{600, 90},
		lowestRatio:  [2]int64{5, 10},
		highestRatio: [2]int64{75, 100},
	}
	rows := storedAt(feedHour, inDivine.row(), divineChaosAnchor().row())

	cfg := DefaultConfig()
	cfg.MinEdgeTickRatio = 5
	if got := BestPlays("Allflame", rows, cfg); len(got.Plays) != 0 {
		t.Fatalf("keys = %v, want none — the undercut return is under five ticks", playKeys(got.Plays))
	}

	// With the ratio lowered to two steps the same hour clears, which is what
	// makes the raw reading visible: it is at the gate the undercut failed.
	cfg.MinEdgeTickRatio = 2

	got := BestPlays("Allflame", rows, cfg)

	play := playByKey(t, got, directKey(divineID, omenID))
	wantClose(t, "RoiPctRaw", play.RoiPctRaw, 5*play.Tick)
	if !(play.RoiPct < 5*play.Tick) {
		t.Errorf("RoiPct = %v, want it under the five ticks of %v the raw spread cleared", play.RoiPct, 5*play.Tick)
	}
}

func TestBestPlays_minROIChaos_cutsThePlaysWhosePayoutIsARoundingError(t *testing.T) {
	// A percentage says nothing about what a flip pays. Both markets below quote
	// an item at 8 chaos in lots of sixteen — a 6.25% tick, an entry of 8.50 and
	// a return well over five steps — so only the chaos per exchanged unit
	// separates them.
	//
	// The floor ships OFF since POE-191 and the desktop offers 3 chaos as a
	// recommended knob, so it is armed here. What the test pins is that the gate reads
	// Roi (the chaos an exchange pays) and not RoiPct: the two markets differ by
	// a quarter of a chaos on the same percentage-shaped fixture.
	cfg := DefaultConfig()
	cfg.MinROIChaos = 3

	tests := []struct {
		name string
		high [2]int64
		want []string
	}{
		{
			name: "3.22 chaos an exchange clears the floor",
			high: [2]int64{25, 2}, // 12.50 chaos, 11.71875 after the undercut
			want: []string{directKey(chaosID, cardID)},
		},
		{
			name: "2.98 chaos an exchange does not",
			high: [2]int64{49, 4}, // 12.25 chaos, 11.484375 after the undercut
			want: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := liquidChaosMarket(cardID, 8, 12)
			spec.lowestRatio = [2]int64{16, 2} // 8 chaos, tick 1/16
			spec.highestRatio = tt.high
			spec.volume = [2]int64{30000, 3000}

			got := BestPlays("Allflame", storedAt(feedHour, spec.row()), cfg)
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				t.Errorf("keys = %v, want %v", playKeys(got.Plays), tt.want)
			}
		})
	}
}

func TestBestPlays_minVolumePerHour_stillDropsTheLegNobodyTraded(t *testing.T) {
	// The unit-volume floor survived POE-184 as a LIVENESS gate and POE-193 as
	// the weakest true statement about a leg: it is not what judges liquidity
	// (Turnover is, client-side now) and it is not a size bar either, but a leg
	// the hour did not trade at all is not executable at any depth, whatever the
	// reader's bankroll.
	tests := []struct {
		name  string
		units int64
		want  []string
	}{
		{
			name:  "a single traded unit",
			units: 1,
			want:  []string{directKey(chaosID, cardID)},
		},
		{
			name:  "nothing traded",
			units: 0,
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

func TestBestPlays_expensiveMarketUnderTheOldFloor_isServedAtDefaults(t *testing.T) {
	// The market that set the rule (2026-08-22): a chaos/divination-card market
	// turning over thousands of chaos an hour while its ITEM side moves only a
	// handful of units, because a card is expensive enough that real money moves
	// in few of them. Seven units an hour is under the old floor of 10 and alive
	// on both sides, and the visibility rule says a live market is served.
	//
	// Both readings are asserted, because "served" and "served as a thin market"
	// are the claim together: the play appears, and its chaos turnover — the
	// liquidity number, which unit volume never was — is in the thousands.
	spec := liquidChaosMarket(cardID, 1200, 1500)
	spec.volume = [2]int64{9500, 7}

	got := BestPlays("Allflame", storedAt(feedHour, spec.row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.Depth != 7 {
		t.Errorf("Depth = %v, want the 7 units the hour traded", play.Depth)
	}
	if play.Turnover != 9500 {
		t.Errorf("Turnover = %v chaos/hour, want 9500 — the flow the old unit floor was hiding", play.Turnover)
	}

	// The same hour under the level the knob is now FOR, so the fixture is proved
	// to sit under it rather than merely asserted to.
	armed := DefaultConfig()
	armed.MinVolumePerHour = 10
	if keys := playKeys(BestPlays("Allflame", storedAt(feedHour, spec.row()), armed).Plays); len(keys) != 0 {
		t.Errorf("keys = %v at an armed floor of 10, want none — the fixture no longer sits under the old floor", keys)
	}
}

func TestBestPlays_minEdge_cutsThePlaysBelowTheFloor(t *testing.T) {
	rows := storedAt(feedHour,
		liquidChaosMarket(hellID, 100, 200).row(),
		liquidChaosMarket(cardID, 100, 150).row(),
	)
	// The floor is judged on the UNDERCUT return, so the boundary is spelled the
	// way the engine reaches it rather than as the raw 50% the card's extremes
	// suggest.
	tick := 1.0 / 100.0
	cardRoiPct := undercutRoi(100, tick, [2]float64{150, tick})

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
			minEdge: cardRoiPct,
			want:    []string{directKey(chaosID, hellID), directKey(chaosID, cardID)},
		},
		{
			name:    "floor one representable step above the smaller return drops it",
			minEdge: math.Nextafter(cardRoiPct, 1),
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

// barelyProfitableFlip is one hour of a market whose UNDERCUT round trip gains
// about 0.05% — positive, and under DefaultConfig's 0.1% sanity floor. The market
// quotes 10,000 chaos to the card at the hour's cheapest and 10,007 at its
// dearest, so its tick is 1/10,000 and the two ticks the round trip pays eat
// almost all of the seven-chaos spread.
func barelyProfitableFlip() rowSpec {
	return liquidChaosMarket(cardID, 10000, 10007)
}

func TestBestPlays_negativeMinEdge_surfacesTheReturnsTheSanityFloorHides(t *testing.T) {
	// An explicitly negative MinEdge is a choice rather than an unset field, and
	// this is what it buys after POE-191: the gains that are real and smaller
	// than the floor. The floor is all that stands between this play and the
	// list — every other gate is off by default — so the two runs below differ
	// in exactly one number.
	rows := storedAt(feedHour, barelyProfitableFlip().row())

	if got := BestPlays("Allflame", rows, DefaultConfig()); len(got.Plays) != 0 {
		play := got.Plays[0]
		t.Fatalf("the sanity floor kept a play returning %v, want the fixture's return under DefaultConfig().MinEdge of %v",
			play.RoiPct, DefaultConfig().MinEdge)
	}

	cfg := DefaultConfig()
	cfg.MinEdge = -1

	got := BestPlays("Allflame", rows, cfg)

	if want := []string{directKey(chaosID, cardID)}; !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Fatalf("keys = %v, want %v", playKeys(got.Plays), want)
	}
	// The fixture has to be a GAIN the floor hid, not a loss the floor caught —
	// otherwise the test above would pass for the wrong reason.
	if play := got.Plays[0]; !(play.RoiPct > 0 && play.RoiPct < DefaultConfig().MinEdge) {
		t.Errorf("RoiPct = %v, want a positive return under the 0.1%% floor", play.RoiPct)
	}
}

func TestBestPlays_undercutReturnBelowZero_isNotServedUnderTheDefaults(t *testing.T) {
	// The floor POE-191 kept when it handed the quality gates to the client: the
	// server serves everything sane, and a round trip that ends with less than it
	// started is not sane under any bankroll.
	//
	// Both markets below print the same +10% RAW spread and differ only in price
	// resolution. Ten chaos to the card can only move in tenths, so the two ticks
	// the round trip pays turn that spread into a 10% LOSS; a thousand chaos to
	// the card moves in thousandths and the same spread survives as a +9.8% gain.
	// The pair is what keeps the failing case honest — the loss is cut for being
	// a loss, not for being an unpriceable or dead market.
	tests := []struct {
		name string
		low  int64
		high int64
		want []string
	}{
		{
			name: "a tenth-of-a-chaos tick turns the spread into a loss",
			low:  10,
			high: 11,
			want: []string{},
		},
		{
			name: "the same spread on a thousandth-of-a-chaos tick still gains",
			low:  1000,
			high: 1100,
			want: []string{directKey(chaosID, cardID)},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rows := storedAt(feedHour, liquidChaosMarket(cardID, tt.low, tt.high).row())

			got := BestPlays("Allflame", rows, DefaultConfig())
			if !reflect.DeepEqual(playKeys(got.Plays), tt.want) {
				tick := 1 / float64(tt.low)
				t.Errorf("keys = %v, want %v (undercut return %v)",
					playKeys(got.Plays), tt.want, undercutRoi(float64(tt.low), tick, [2]float64{float64(tt.high), tick}))
			}
		})
	}
}

func TestBestPlays_undercutReturnBelowZero_isNotServedEvenWithANegativeMinEdge(t *testing.T) {
	// MinEdge is the floor a deploy can lower (EXCHANGE_MIN_EDGE); the bar under
	// it is structural. withDefaults clamps MinEdgeTickRatio and MinROIChaos to
	// at least 0, and Roi carries RoiPct's sign because Investment is positive,
	// so Roi >= MinROIChaos rejects a losing route however low MinEdge goes.
	// Losing the clamp — or the gate — is what this test is here to catch.
	//
	// The caller below asks for the loosest gates it can express: -1 on all
	// three. The fixture is a two-chaos card at a half-chaos tick, so the losing
	// round trip returns -16.7% on a payout of -0.50 chaos — inside every one of
	// those -1s, and served the moment the clamp stops turning them into zeroes.
	rows := storedAt(feedHour, liquidChaosMarket(cardID, 2, 5).row())
	cfg := DefaultConfig()
	cfg.MinEdge = -1
	cfg.MinEdgeTickRatio, cfg.MinROIChaos = -1, -1

	if got := BestPlays("Allflame", rows, cfg); len(got.Plays) != 0 {
		t.Errorf("keys = %v, want none — the round trip returns %v",
			playKeys(got.Plays), got.Plays[0].RoiPct)
	}
}

func TestBestPlays_maxPlays_truncatesToTheHighestRanked(t *testing.T) {
	rows := storedAt(feedHour,
		liquidChaosMarket(hellID, 100, 200).row(),
		liquidChaosMarket(cardID, 100, 160).row(),
		liquidChaosMarket(omenID, 100, 140).row(),
		liquidChaosMarket(scarabID, 100, 130).row(),
	)
	cfg := DefaultConfig()
	cfg.MaxPlays = 2

	got := BestPlays("Allflame", rows, cfg)

	want := []string{directKey(chaosID, hellID), directKey(chaosID, cardID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the two highest ranked %v", playKeys(got.Plays), want)
	}
}

func TestBestPlays_ranksByExpectedRoiThenTurnoverThenKey(t *testing.T) {
	// The ranking sorts on what the orders would have REALIZED, not on what the
	// served hour printed, and the hell market is here to keep the two apart: its
	// newest hour reads +96%, the biggest displayed return of the four, and it
	// ranks LAST because an entry made an hour earlier at 300 chaos exits into
	// that hour at a loss. Sorting on RoiPct instead would put it first.
	//
	// The other three print the same hour and the same prices an hour before it,
	// so their entries realize identically; the card's market moved twice the
	// volume on both sides, which raises its Turnover without touching the
	// volume-weighted price the exit is read from. That leaves each tie-break
	// deciding exactly one position.
	thickInChaos := liquidChaosMarket(cardID, 100, 130)
	thickInChaos.volume = [2]int64{thickInChaos.volume[0] * 2, thickInChaos.volume[1] * 2}

	rows := append(
		storedAt(feedHour,
			liquidChaosMarket(scarabID, 100, 130).row(),
			liquidChaosMarket(omenID, 100, 130).row(),
			thickInChaos.row(),
			liquidChaosMarket(hellID, 100, 200).row(),
		),
		storedAt(feedHour.Add(-time.Hour),
			liquidChaosMarket(scarabID, 100, 120).row(),
			liquidChaosMarket(omenID, 100, 120).row(),
			liquidChaosMarket(cardID, 100, 120).row(),
			liquidChaosMarket(hellID, 300, 360).row(),
		)...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{
		directKey(chaosID, cardID),   // tied on the simulated payout, most chaos through it
		directKey(chaosID, omenID),   // tied on payout and turnover, smaller key
		directKey(chaosID, scarabID), // tied on payout and turnover, bigger key
		directKey(chaosID, hellID),   // the biggest printed return, a simulated loss
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
	card, omen, hell := playByKey(t, got, want[0]), playByKey(t, got, want[1]), playByKey(t, got, want[3])
	if card.ExpectedRoi != omen.ExpectedRoi || !(card.Turnover > omen.Turnover) {
		t.Errorf("the runner-up pair reads %v/%v chaos of expectation on %v/%v of turnover — the fixture no longer isolates the turnover tie-break",
			card.ExpectedRoi, omen.ExpectedRoi, card.Turnover, omen.Turnover)
	}
	if !(hell.RoiPct > card.RoiPct) || !(hell.ExpectedRoi < card.ExpectedRoi) {
		t.Errorf("the last-placed play reads %v printed / %v simulated against the winner's %v / %v — the fixture no longer separates the two numbers",
			hell.RoiPct, hell.ExpectedRoi, card.RoiPct, card.ExpectedRoi)
	}
}

func TestBestPlays_playsWithNothingSimulated_rankByTurnoverRatherThanDepth(t *testing.T) {
	// One hour of feed: no entry can be simulated in it, so every play carries the
	// same zero expectation and the ranking falls through to the tie-break under
	// it. What that tie-break reads is the chaos that FLOWED through the market,
	// not the units that changed hands — the runner-up here is the THINNEST of
	// the four in units traded, so a comparator reading Depth would rank it last.
	thickInChaos := liquidChaosMarket(cardID, 100, 120)
	thickInChaos.volume = [2]int64{115000, 800}

	rows := storedAt(feedHour,
		liquidChaosMarket(scarabID, 100, 120).row(), // 109,545 chaos an hour on 1000 units
		liquidChaosMarket(omenID, 100, 120).row(),   // the same
		thickInChaos.row(),                          // 115,000 chaos an hour on 800 units
		liquidChaosMarket(hellID, 100, 150).row(),   // 122,474 chaos an hour on 1000 units
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{
		directKey(chaosID, hellID),   // most chaos through it
		directKey(chaosID, cardID),   // second most, on the fewest units
		directKey(chaosID, omenID),   // tied on turnover, smaller key
		directKey(chaosID, scarabID), // tied on turnover, bigger key
	}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want %v", playKeys(got.Plays), want)
	}
	for _, play := range got.Plays {
		if play.SimEntries != 0 || play.ExpectedRoi != 0 {
			t.Fatalf("%s reads %d simulated entries at %v chaos — the fixture no longer falls through to the turnover tie-break",
				play.Key, play.SimEntries, play.ExpectedRoi)
		}
	}
	if card, omen := playByKey(t, got, want[1]), playByKey(t, got, want[2]); card.Depth >= omen.Depth {
		t.Errorf("the runner-up's Depth is %v against %v — the fixture no longer separates turnover from depth",
			card.Depth, omen.Depth)
	}
}

func TestBestPlays_biggerSimulatedFractionOnASmallerStake_ranksBehindTheBiggerChaosPayout(t *testing.T) {
	// ExpectedRoi and ExpectedRoiPct are two readings of the same entries and the
	// ranking sorts on the CHAOS one, the same way MinROIChaos judges a payout
	// rather than a percentage: a card bought for 4.25 chaos that comes back 55%
	// up made two chaos, and one bought for 401 that came back 14% up made
	// fifty-seven. Sorting on the fraction would put the small one first, which is
	// how a reader ends up chasing rounding errors.
	//
	// The cheap market is quoted in sixteenths so its price can be small without
	// its tick being coarse — 4 to 8 chaos on a 1/16 step, which clears the same
	// gates the expensive one does.
	cheap := rowSpec{
		itemA:        chaosID,
		itemB:        cardID,
		volume:       [2]int64{5657, 1000}, // a volume-weighted 5.657, between the two extremes
		lowestStock:  [2]int64{2000, 200},
		highestStock: [2]int64{5000, 500},
		lowestRatio:  [2]int64{16, 4}, // 4 chaos a card, tick 1/16
		highestRatio: [2]int64{32, 4}, // 8 chaos a card
	}
	dear := liquidChaosMarket(hellID, 400, 480)
	rows := hourlyFeed(
		[]Row{cheap.row(), dear.row()},
		[]Row{cheap.row(), dear.row()},
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	want := []string{directKey(chaosID, hellID), directKey(chaosID, cardID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Errorf("keys = %v, want the bigger payout first %v", playKeys(got.Plays), want)
	}
	small, big := playByKey(t, got, want[1]), playByKey(t, got, want[0])
	if !(small.ExpectedRoiPct > big.ExpectedRoiPct) || !(big.ExpectedRoi > small.ExpectedRoi) {
		t.Errorf("the pair reads %v/%v as fractions and %v/%v as chaos — the fixture no longer crosses the two readings",
			small.ExpectedRoiPct, big.ExpectedRoiPct, small.ExpectedRoi, big.ExpectedRoi)
	}
}

func TestBestPlays_lowCoveragePlay_ranksBehindEveryCoveredOneAndAheadOfEverySuspect(t *testing.T) {
	// The three bands, in the order the ranking reads them, each holding the play
	// that would win on the number below it: the suspect market carries the
	// biggest simulated payout of the three and still ranks last, and the
	// low-coverage market outpays the covered one and still ranks second. "We
	// could not measure this" and "we measured this and it is good" are different
	// claims, and only the second earns a place at the top.
	//
	// The hell market traded nothing in the three oldest hours, which is what
	// leaves it a single simulated entry against the guard while the other two
	// are measured over four. The guard is armed at two because the shipped
	// twelve would be capped at this feed's four possible entries and flag every
	// row.
	card := liquidChaosMarket(cardID, 100, 120).row()
	suspectOmen := chaosMarketAnchoredAt(omenID, 90, 400, 100).row()
	hell := liquidChaosMarket(hellID, 100, 150)
	quietHell := hell
	quietHell.volume = [2]int64{1100, 0}
	rows := hourlyFeed(
		[]Row{card, quietHell.row(), suspectOmen},
		[]Row{card, quietHell.row(), suspectOmen},
		[]Row{card, quietHell.row(), suspectOmen},
		[]Row{card, hell.row(), suspectOmen},
		[]Row{card, hell.row(), suspectOmen},
	)
	cfg := DefaultConfig()
	cfg.MinSimEntries = 2

	got := BestPlays("Allflame", rows, cfg)

	want := []string{directKey(chaosID, cardID), directKey(chaosID, hellID), directKey(chaosID, omenID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Fatalf("keys = %v, want covered, then low-coverage, then suspect %v", playKeys(got.Plays), want)
	}
	covered, thin, suspect := playByKey(t, got, want[0]), playByKey(t, got, want[1]), playByKey(t, got, want[2])
	if covered.LowCoverage || covered.Suspect {
		t.Fatalf("the leader reads lowCoverage %v / suspect %v, want a clean covered play", covered.LowCoverage, covered.Suspect)
	}
	if !thin.LowCoverage || thin.Suspect || !(thin.ExpectedRoi > covered.ExpectedRoi) {
		t.Fatalf("the runner-up reads lowCoverage %v / suspect %v on %v chaos against the leader's %v — the fixture no longer proves the flag outranks the payout",
			thin.LowCoverage, thin.Suspect, thin.ExpectedRoi, covered.ExpectedRoi)
	}
	if !suspect.Suspect || !(suspect.ExpectedRoi > thin.ExpectedRoi) {
		t.Fatalf("the last play reads suspect %v on %v chaos against the low-coverage %v — the fixture no longer proves the suspect band is read first",
			suspect.Suspect, suspect.ExpectedRoi, thin.ExpectedRoi)
	}
}

// tiedShapes is the smallest window in which a direct flip and a one-hop route
// come out with EXACTLY the same undercut return, the same turnover and the same
// entry, so the ranking has nothing left to separate them but the shape.
//
// Every price and every tick is a dyadic fraction, so the undercut arithmetic is
// exact and both shapes reach the SAME double: the entry costs 16 chaos at a
// sixteenth of a tick, 17 once undercut, and both sell sides come to 29.765625
// chaos — the flip selling its card at 31.75 chaos, the route selling its scarab
// for 0.25 divine (0.234375 undercut) and closing that divine at 128 chaos (127
// undercut).
//
// The three markets the route walks carry no play of their own: the chaos/scarab
// spread is 16 to 17 and the other two print one price all hour, so all three
// lose money once a tick is paid per side.
func tiedShapes() (flip, buyLeg, sellLeg, anchor rowSpec) {
	flip = rowSpec{
		itemA:        chaosID,
		itemB:        hellID,
		volume:       [2]int64{110000, 5000},
		lowestStock:  [2]int64{4000, 900},
		highestStock: [2]int64{9000, 1500},
		lowestRatio:  [2]int64{16, 1},  // 16 chaos, tick 1/16
		highestRatio: [2]int64{127, 4}, // 31.75 chaos
	}
	buyLeg = rowSpec{
		itemA:        chaosID,
		itemB:        scarabID,
		volume:       [2]int64{110000, 6875},
		lowestStock:  [2]int64{4000, 900},
		highestStock: [2]int64{9000, 1500},
		lowestRatio:  [2]int64{16, 1}, // 16 chaos, tick 1/16
		highestRatio: [2]int64{17, 1},
	}
	sellLeg = rowSpec{
		itemA:        divineID,
		itemB:        scarabID,
		volume:       [2]int64{1000, 4000},
		lowestStock:  [2]int64{200, 900},
		highestStock: [2]int64{400, 1500},
		lowestRatio:  [2]int64{4, 16}, // a quarter of a divine, tick 1/16
		highestRatio: [2]int64{4, 16},
	}
	anchor = rowSpec{
		itemA:        chaosID,
		itemB:        divineID,
		volume:       [2]int64{12800000, 100000},
		lowestStock:  [2]int64{50000, 3000},
		highestStock: [2]int64{100000, 5000},
		lowestRatio:  [2]int64{128, 1}, // 128 chaos a divine, tick 1/128
		highestRatio: [2]int64{128, 1},
	}
	return flip, buyLeg, sellLeg, anchor
}

func TestBestPlays_directPlayTiedWithAOneHop_isRankedFirst(t *testing.T) {
	// Nothing but the shape separates the two, and the direct flip wins because
	// it carries one execution risk instead of three. The route's key sorts
	// AHEAD of the flip's ("1-hop:" before "direct:"), which is how this proves
	// the mode tie-break outranks the key tie-break.
	flip, buyLeg, sellLeg, anchor := tiedShapes()

	got := BestPlays("Allflame", storedAt(feedHour, flip.row(), buyLeg.row(), sellLeg.row(), anchor.row()), DefaultConfig())

	want := []string{directKey(chaosID, hellID), oneHopKey(scarabID, chaosID, divineID)}
	if !reflect.DeepEqual(playKeys(got.Plays), want) {
		t.Fatalf("keys = %v, want the flip ahead of the route %v", playKeys(got.Plays), want)
	}
	direct, route := got.Plays[0], got.Plays[1]
	if direct.RoiPct != route.RoiPct || direct.Turnover != route.Turnover {
		t.Errorf("the two shapes read %v/%v chaos and %v/%v chaos — the fixture no longer ties them",
			direct.RoiPct, route.RoiPct, direct.Turnover, route.Turnover)
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
	// hold is the OUTCOME — the levels themselves are pinned one at a time by the
	// boundary tests above.
	//
	// The two noise markets are still cut with the quality gates off, and the
	// reason is worth stating: both print a 100% tick, so the sell leg's undercut
	// price is Price*(1-1) = 0 and the round trip returns -100%. Junk that coarse
	// fails the sanity floor on its own arithmetic, which is why relaxing the
	// defaults did not let it back in.
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
	// Its real reading, against the +1846% the old engine claimed for it — and
	// against the +21.95% its raw extremes still suggest before the two ticks
	// are paid.
	got := BestPlays("Allflame", storedAt(feedHour, astrolabeInChaos().row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, omenID))
	tick := 1.0 / 82.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(82, tick, [2]float64{100, tick}))
	wantClose(t, "RoiPctRaw", play.RoiPctRaw, 100.0/82.0-1)
	wantClose(t, "Investment", play.Investment, 82*(1+tick))
	wantClose(t, "Roi", play.Roi, 100*(1-tick)-82*(1+tick))
	wantClose(t, "Turnover", play.Turnover, 70000)
}

// mawrBlaiddAt is the measured Mawr Blaidd / Chaos card market in one recorded
// hour of the 2026-08-19 Mirage feed, the market that started POE-188: its 17:00
// hour printed a low of 81 chaos against a volume-weighted 245.58, and by 19:00
// the same market read 249 to 300 around a 257.65 average. Aggregating the two
// is what made the served play say "buy at 80.50".
func mawrBlaiddAt(low, high, chaosTraded, cardsTraded int64) rowSpec {
	return rowSpec{
		itemA: chaosID, itemB: cardID,
		volume:       [2]int64{chaosTraded, cardsTraded},
		lowestStock:  [2]int64{9000, 40},
		highestStock: [2]int64{15000, 120},
		lowestRatio:  [2]int64{low, 1},
		highestRatio: [2]int64{high, 1},
	}
}

func TestBestPlays_mawrBlaiddNewestHour_readsThatHoursOwnExtremesAndAverage(t *testing.T) {
	// The recorded 19:00 hour, served beside the junk 17:00 hour that used to
	// drag the aggregate down: 249 and 300 are the prices, 257.65 the anchor,
	// and neither extreme is far enough from it to be flagged.
	rows := append(
		storedAt(feedHour, mawrBlaiddAt(249, 300, 10306, 40).row()),
		storedAt(feedHour.Add(-2*time.Hour), mawrBlaiddAt(81, 350, 14735, 60).row())...,
	)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.Legs[0].Price != 249 || play.Legs[1].Price != 300 {
		t.Errorf("legs = %v -> %v, want the 19:00 hour's 249 -> 300 (the 17:00 hour printed 81)",
			play.Legs[0].Price, play.Legs[1].Price)
	}
	wantClose(t, "Fair", play.Legs[0].Fair, 10306.0/40.0)
	if play.Suspect {
		t.Error("Suspect = true, want false — 249 and 300 both sit inside the bands around 257.65")
	}
	tick := 1.0 / 249.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(249, tick, [2]float64{300, tick}))
}

func TestBestPlays_mawrBlaiddJunkHour_flagsTheLowAsSuspect(t *testing.T) {
	// The hour that started it all, read on its own: 81 chaos against a
	// volume-weighted 245.58 is a third of fair, so the buy leg is flagged while
	// the 350 high — inside 1.5 times fair — is not.
	got := BestPlays("Allflame", storedAt(feedHour, mawrBlaiddAt(81, 350, 14735, 60).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if !play.Legs[0].Suspect {
		t.Errorf("buy leg Suspect = false, want true — 81 against a fair of %v", play.Legs[0].Fair)
	}
	if play.Legs[1].Suspect {
		t.Errorf("sell leg Suspect = true, want false — 350 against a fair of %v", play.Legs[1].Fair)
	}
	if !play.Suspect {
		t.Error("play Suspect = false, want true — one flagged leg is enough")
	}
}

func TestBestPlays_essenceOfInsanityJunkHour_flagsTheHighAsSuspect(t *testing.T) {
	// The mirror of the Mawr Blaidd case on the sell side, measured the same
	// hour: essences traded at 13.79 chaos on average and one went for 73, five
	// times fair. The legs still show what the feed printed — 10 and 73 — with
	// the flag beside them rather than a substituted price.
	essence := rowSpec{
		itemA: chaosID, itemB: omenID,
		volume:       [2]int64{13790, 1000},
		lowestStock:  [2]int64{9000, 400},
		highestStock: [2]int64{15000, 900},
		lowestRatio:  [2]int64{10, 1},
		highestRatio: [2]int64{73, 1},
	}

	got := BestPlays("Allflame", storedAt(feedHour, essence.row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, omenID))
	if play.Legs[0].Price != 10 || play.Legs[1].Price != 73 {
		t.Errorf("legs = %v -> %v, want the observed 10 -> 73", play.Legs[0].Price, play.Legs[1].Price)
	}
	if !play.Legs[1].Suspect {
		t.Errorf("sell leg Suspect = false, want true — 73 against a fair of %v", play.Legs[1].Fair)
	}
	if play.Legs[0].Suspect {
		t.Errorf("buy leg Suspect = true, want false — 10 against a fair of %v", play.Legs[0].Fair)
	}
}

func TestBestPlays_recordedHour_ranksFinitePlaysUnderTheDefaultGates(t *testing.T) {
	rows, _ := Normalize(loadFixtureHour(t))

	got := BestPlays("Allflame", storedAt(feedHour, rows...), DefaultConfig())

	// The recorded hour is frozen input: its 23 priced markets have to yield both
	// shapes, and every number in them has to be finite and positive. How MANY of
	// each is a characterization, not a spec, so the counts are read as "some"
	// rather than pinned, which would make an unrelated tuning change look like a
	// regression. That routes appear at all IS pinned, and is what POE-191
	// changed: the hour's best cross-quote route returns 3.3 ticks on a payout of
	// 2.94 chaos, which the old server-side levels of 5 and 3 cut. Those levels
	// now live client-side, and the test below arms them on the same rows.
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
		if play.RoiPctRaw < play.RoiPct {
			t.Errorf("%s: RoiPctRaw = %v, want it at or above the undercut return %v", play.Key, play.RoiPctRaw, play.RoiPct)
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
		t.Errorf("one-hop plays = %d, want the recorded hour's cross-quote routes, which the relaxed defaults no longer cut", oneHop)
	}
	for _, play := range got.Plays {
		if play.Mode == ModeOneHop && len(play.Legs) != 3 {
			t.Errorf("%s: %d legs, want the three of a route", play.Key, len(play.Legs))
		}
	}
}

func TestBestPlays_recordedHourUnderTheOldServerLevels_yieldsNoOneHopRoutes(t *testing.T) {
	// What the levels this engine used to enforce actually CUT, measured on a
	// recorded hour. Every one of them is armed explicitly below, including the
	// liveness floor: until POE-193 that one was inherited from DefaultConfig at
	// 10, and inheriting a level that has since moved would have made this test
	// quietly measure a different set than its name claims.
	//
	// This is no longer a migration invariant about the desktop. POE-191 moved the
	// levels to the desktop's knobs and shipped them armed, so this test and
	// gateDefaults were one claim in two languages; POE-193 turned those defaults
	// off — the levels are the documented RECOMMENDED tightening now, and the
	// desktop's out-of-the-box table is everything the server serves. The test
	// stays because the levels stay: it is the evidence for what a reader who
	// types them gets, and the reason "most 1-hop routes fail Edge vs step" is a
	// measurement rather than a claim.
	//
	// The route is the case that decides it: its coarsest leg steps 9.1%, inside
	// the 10% tick cap, and its undercut return is 29.6% — three ticks rather
	// than the five the ratio asks for, on a payout of 2.94 chaos rather than 3.
	// Either level alone cuts it, so both have to be armed for this to mean what
	// it says, and the flips have to survive or the levels would be cutting the
	// whole hour rather than the route.
	rows, _ := Normalize(loadFixtureHour(t))
	cfg := DefaultConfig()
	cfg.MinTurnoverChaos, cfg.MaxTick = 10000, 0.10
	cfg.MinEdgeTickRatio, cfg.MinROIChaos = 5, 3
	// The fifth old level: the server enforced 2% before POE-191 made it the
	// client's minRoiPct knob. Armed so the result is the whole level set's, not
	// coincidental on this fixture.
	cfg.MinEdge = 0.02
	// The liveness level the engine carried until POE-193. It can only tighten
	// what the four quality levels already cut, so the no-routes assertion below
	// is unaffected by it — what it buys is that this test names the old set
	// completely instead of depending on a default that has moved.
	cfg.MinVolumePerHour = 10

	got := BestPlays("Allflame", storedAt(feedHour, rows...), cfg)

	direct, oneHop := 0, 0
	for _, play := range got.Plays {
		switch play.Mode {
		case ModeDirect:
			direct++
		case ModeOneHop:
			oneHop++
		}
	}
	if oneHop != 0 {
		t.Errorf("one-hop plays = %d, want none: the hour's best route returns 3.3 ticks against the 5 the ratio asks for, on a payout of 2.94 chaos against the floor of 3", oneHop)
	}
	if direct <= 0 {
		t.Errorf("direct plays = %d, want the flips that cleared the old levels to still clear them", direct)
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
	// The four quality levels are 0 / 1 / 0 / 0 on purpose since POE-191: off,
	// with 10,000 / 0.10 / 5 / 3 offered as the desktop's knobs — armed by
	// default until POE-193, recommended rather than applied since. Changing one
	// of them back here changes what every user is shown and cannot be undone
	// from the app, which is why the whole tuning is pinned rather than described.
	//
	// The two floors that remained server-side are pinned for the same reason and
	// at the bottom of their range since POE-193 (ADR-017): MinVolumePerHour 1
	// asks only that a trade happened, and MinHoursSeen 1 — on the base config
	// and on both horizons — cannot drop a served play at all, because a served
	// play cleared the window's newest hour. Raising either here re-arms a floor
	// for every user, which is what the ADR says a default may not do; the old
	// 10 / 4-of-6 / 18-of-24 are what the env knobs are for. MaxPlays 2000 is a
	// payload guard sized above the sane set, not a cut into it.
	//
	// The three sim knobs are pinned for a different reason: 24 / 3 / 12 are the
	// parameters ExpectedRoi was calibrated at (POE-193), they take no
	// environment override, and moving one invalidates the measurement rather
	// than adjusting a preference.
	want := Config{
		WindowHours:       6,
		MinVolumePerHour:  1,
		MinEdge:           0.001,
		MinTurnoverChaos:  0,
		MaxTick:           1,
		MinEdgeTickRatio:  0,
		MinROIChaos:       0,
		SuspectLowBand:    0.67,
		SuspectHighBand:   1.5,
		HideSuspect:       false,
		MinHoursSeen:      1,
		SimWindowHours:    24,
		SimLookaheadHours: 3,
		MinSimEntries:     12,
		MaxPlays:          2000,
		QuotePriority:     []string{DivineID, ChaosID},
		Horizons: []HorizonConfig{
			{Horizon: HorizonRecent, WindowHours: 6, MinHoursSeen: 1},
			{Horizon: HorizonDay, WindowHours: 24, MinHoursSeen: 1},
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
			name: "only SuspectLowBand set",
			cfg:  Config{SuspectLowBand: 0.9},
			want: withField(func(c *Config) { c.SuspectLowBand = 0.9 }),
		},
		{
			name: "only SuspectHighBand set",
			cfg:  Config{SuspectHighBand: 1.1},
			want: withField(func(c *Config) { c.SuspectHighBand = 1.1 }),
		},
		{
			name: "only HideSuspect set",
			cfg:  Config{HideSuspect: true},
			want: withField(func(c *Config) { c.HideSuspect = true }),
		},
		{
			name: "only MinHoursSeen set",
			cfg:  Config{MinHoursSeen: 4},
			want: withField(func(c *Config) { c.MinHoursSeen = 4 }),
		},
		{
			name: "only SimWindowHours set",
			cfg:  Config{SimWindowHours: 48},
			want: withField(func(c *Config) { c.SimWindowHours = 48 }),
		},
		{
			name: "only SimLookaheadHours set",
			cfg:  Config{SimLookaheadHours: 6},
			want: withField(func(c *Config) { c.SimLookaheadHours = 6 }),
		},
		{
			name: "only MinSimEntries set",
			cfg:  Config{MinSimEntries: 20},
			want: withField(func(c *Config) { c.MinSimEntries = 20 }),
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
	// Negative counts, floors, caps and bands have no meaning, so they read as
	// unset — the one exception being MinEdge, which the table above pins.
	//
	// For the three levels DefaultConfig now leaves at 0 this is a CLAMP rather
	// than a restoration, and it is load-bearing: a negative MinEdgeTickRatio or
	// MinROIChaos would make evaluate's last two gates admit a losing round trip,
	// which is the bar under MinEdge (see
	// TestBestPlays_undercutReturnBelowZero_isNotServedEvenWithANegativeMinEdge).
	cfg := Config{
		WindowHours:       -1,
		MinVolumePerHour:  -1,
		MinTurnoverChaos:  -1,
		MaxTick:           -1,
		MinEdgeTickRatio:  -1,
		MinROIChaos:       -1,
		SuspectLowBand:    -1,
		SuspectHighBand:   -1,
		MinHoursSeen:      -1,
		SimWindowHours:    -1,
		SimLookaheadHours: -1,
		MinSimEntries:     -1,
		MaxPlays:          -1,
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
				{Action: "buy", Item: cardID, Quote: chaosID, Price: 100, Fair: 110, FairOK: true, Tick: 0.01, Volume: 1000, Stock: 500},
				{Action: "sell", Item: cardID, Quote: chaosID, Price: 200, Fair: 110, FairOK: true, Tick: 0.01, Volume: 1000, Stock: 500, Suspect: true},
			},
			RoiPct:     0.96,
			Edge:       0.96,
			RoiPctRaw:  1,
			Roi:        97,
			Investment: 101,
			Turnover:   110000,
			Tick:       0.01,
			Depth:      1000,
			Suspect:    true,
			HoursSeen:  1,
			// The simulation's four, and the shape they take on a play whose
			// optimistic hour reads +96%: a measured LOSS over three entries, too
			// few to lean on. A round trip that drops any of them would drop the
			// only numbers the ranking sorts on.
			ExpectedRoi:    -12.5,
			ExpectedRoiPct: -0.1238,
			SimEntries:     3,
			LowCoverage:    true,
			LastHour:       feedHour,
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
		"key", "mode", "legs", "roiPct", "edge", "roiPctRaw", "roi", "investment",
		"turnover", "tick", "depth", "suspect", "hoursSeen", "expectedRoi",
		"expectedRoiPct", "simEntries", "lowCoverage", "lastHour")

	var legs []map[string]json.RawMessage
	if err := json.Unmarshal(envelope.Plays[0]["legs"], &legs); err != nil {
		t.Fatalf("decode legs: %v", err)
	}
	wantKeys(t, "leg", mustMarshal(t, legs[0]),
		"action", "item", "quote", "price", "fair", "fairOk", "tick", "volume", "stock", "suspect")

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
