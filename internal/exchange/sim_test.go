package exchange

import (
	"testing"
	"time"
)

// flipHour is ONE hour of a chaos-quoted direct flip as recordSim files it: a
// flip's two legs are the same market row, so one low, one high, one tick and
// one volume-weighted price describe the hour; the conversion is the identity
// (there is no third leg to come back through); and one chaos is worth one
// chaos, so the hour is valuable.
//
// The simulateEntry tests below are written at this layer rather than through
// BestPlays because what the estimator PAYS branches on the hours that followed
// an entry — a chase that fires, a sell that fills two hours later, an exit hour
// with no volume-weighted price to meet the market against — and a window that
// reached each of those branches would also have to keep its recipe clearing
// every gate in the hour the play is served from. The wiring from rows to
// entries is pinned separately, in the BestPlays tests further down.
func flipHour(offset int, low, high, vwap, tick float64) simObs {
	return simObs{
		hour:       feedHour.Add(time.Duration(offset) * time.Hour),
		entryLow:   low,
		entryTick:  tick,
		sellHigh:   high,
		sellTick:   tick,
		sellVwap:   vwap,
		sellVwapOK: true,
		convVwap:   1,
		convVwapOK: true,
		entryRate:  1,
		valuable:   true,
	}
}

// chasedBuy is what a posted buy pays to be the order that fills: one tick above
// the hour's low. It is the entry hour's price when the market came down to meet
// it, and the LATER hour's when the chase had to reprice.
func chasedBuy(low, tick float64) float64 {
	return low * (1 + tick)
}

// postedSell is the mirror on the sell side: one tick under the entry hour's
// high, the price the resting order fills at whenever some later hour reaches
// it.
func postedSell(high, tick float64) float64 {
	return high * (1 - tick)
}

// fireSale is what an unsold unit exits at in the lookahead's last data hour:
// halfway between that hour's volume-weighted price and its undercut high.
func fireSale(vwap, high, tick float64) float64 {
	return (vwap + high*(1-tick)) / 2
}

// simEntry is what ONE simulated entry realized, spelled in the estimator's
// operation order: the fraction the exit made over the chased outlay (through
// the conversion the proceeds came back on, 1 on a flip), and the chaos that
// fraction is worth on one unit at the ENTRY hour's own rate.
//
// It exists so every expectation below is computed at runtime in the same order
// the engine reaches it — a constant folded at arbitrary precision could differ
// from the engine's float64 in the last bits.
func simEntry(fill, exit, conv, rate float64) (pct, chaos float64) {
	pct = exit*conv/fill - 1
	return pct, fill * rate * pct
}

// vwapOf is the volume-weighted price a rowSpec's hour traded at: the quote
// units that changed hands over the item units, the same division vwapIn makes.
// Every spec in these tests puts the quote on side A and the traded item on side
// B, which is the orientation liquidChaosMarket and liquidTriangle build.
func vwapOf(spec rowSpec) float64 {
	return float64(spec.volume[0]) / float64(spec.volume[1])
}

// hourlyFeed stamps one hour per argument, OLDEST first, so the LAST one is the
// newest hour — the hour a served play is priced from, and the one hour that can
// never be a simulated entry.
func hourlyFeed(hours ...[]Row) []StoredRow {
	stored := make([]StoredRow, 0, len(hours))
	for i, rows := range hours {
		hour := feedHour.Add(-time.Duration(len(hours)-1-i) * time.Hour)
		stored = append(stored, storedAt(hour, rows...)...)
	}
	return stored
}

// steadyFeed is n hours of the same market, the newest of them feedHour.
func steadyFeed(n int, spec rowSpec) []StoredRow {
	hours := make([][]Row, 0, n)
	for i := 0; i < n; i++ {
		hours = append(hours, []Row{spec.row()})
	}
	return hourlyFeed(hours...)
}

func TestSimulateEntry_laterLowUnderThePostedBuy_fillsAtThePostedPrice(t *testing.T) {
	// The buy rests at the entry hour's low plus one tick and the next data hour
	// came down to meet it, so the trip is measured against THAT price. The next
	// hour's own low is 96 — filling there would be a better entry than a resting
	// order could have got.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 124, 126, 125, 1.0/16.0),
	}
	fill := chasedBuy(100, 1.0/16.0)
	if obs[1].entryLow > fill {
		t.Fatalf("the next hour's low %v is above the posted buy %v — the fixture chases instead of filling", obs[1].entryLow, fill)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(fill, postedSell(128, 1.0/16.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_laterLowExactlyAtThePostedBuy_fillsWithoutChasing(t *testing.T) {
	// The chase fires only when the market moved AWAY: a later low that lands
	// exactly on the posted price is a fill, not a reprice. Both sides of the
	// comparison are the same double here — a quarter of 100 is exact in binary —
	// so the case sits ON the boundary, and the next hour's coarser eighth of a
	// tick is what a chase would add to the outlay.
	obs := []simObs{
		flipHour(0, 100, 256, 160, 1.0/4.0),
		flipHour(1, 125, 130, 128, 1.0/8.0),
		flipHour(2, 190, 200, 195, 1.0/4.0),
	}
	fill := chasedBuy(100, 1.0/4.0)
	if obs[1].entryLow != fill {
		t.Fatalf("the next hour's low %v is not the posted buy %v — the fixture no longer sits on the boundary", obs[1].entryLow, fill)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(fill, postedSell(256, 1.0/4.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_laterLowAboveThePostedBuy_repricesTheChaseToThatHoursUndercutLow(t *testing.T) {
	// The market ran away from the resting buy, so the order chases it: the fill
	// is the NEXT hour's low plus that hour's OWN tick, not the entry hour's. The
	// two hours are given different ticks (a sixteenth and an eighth) so the
	// assertion separates the two readings — an eighth of 112 is 14 chaos of
	// difference in what the trip cost.
	obs := []simObs{
		flipHour(0, 100, 192, 140, 1.0/16.0),
		flipHour(1, 112, 120, 116, 1.0/8.0),
		flipHour(2, 200, 208, 204, 1.0/16.0),
	}
	if posted := chasedBuy(100, 1.0/16.0); obs[1].entryLow <= posted {
		t.Fatalf("the next hour's low %v is at or under the posted buy %v — the fixture fills instead of chasing", obs[1].entryLow, posted)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(chasedBuy(112, 1.0/8.0), postedSell(192, 1.0/16.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_laterHighOverThePostedSell_fillsAtThePostedPriceNotThatHoursHigh(t *testing.T) {
	// A resting sell is taken at the price it was posted at. The hour that filled
	// it printed 256 — double the posted 120 — and none of that is the seller's:
	// the order was in the book at 120 and that is what it got.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 200, 256, 220, 1.0/16.0),
	}
	posted := postedSell(128, 1.0/16.0)
	if obs[2].sellHigh <= posted {
		t.Fatalf("the filling hour's high %v is at or under the posted sell %v — the fixture no longer separates the two", obs[2].sellHigh, posted)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), posted, 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_laterHighExactlyAtThePostedSell_fillsTheRestingOrder(t *testing.T) {
	// A high that reached the posted price took the order: the comparison is at
	// or above, not strictly above. A sixteenth of 128 is exact in binary, so the
	// filling hour's 120 IS the posted 120 rather than a rounding beside it — and
	// that hour's own fire-sale price of 106.25 is what the entry would have
	// realized instead had the boundary been read the other way.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 96, 120, 100, 1.0/16.0),
	}
	posted := postedSell(128, 1.0/16.0)
	if obs[2].sellHigh != posted {
		t.Fatalf("the filling hour's high %v is not the posted sell %v — the fixture no longer sits on the boundary", obs[2].sellHigh, posted)
	}
	if unsold := fireSale(obs[2].sellVwap, obs[2].sellHigh, obs[2].sellTick); unsold == posted {
		t.Fatalf("the fire-sale price is also %v — the fixture cannot tell a fill from an exit", unsold)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), posted, 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_hourThatFilledTheBuy_isNotAlsoScannedForTheSell(t *testing.T) {
	// One hour cannot both hand the unit over and take it back: the sell scan
	// starts STRICTLY after the hour the buy filled in. That hour printed a high
	// of 160 against a posted sell of 120 and is skipped anyway, so the trip ends
	// unsold and fire-sales at the last lookahead hour instead.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 160, 100, 1.0/16.0),
		flipHour(2, 40, 48, 44, 1.0/16.0),
	}
	posted := postedSell(128, 1.0/16.0)
	if obs[1].sellHigh < posted {
		t.Fatalf("the buy-filling hour's high %v is under the posted sell %v — the fixture no longer proves the scan skipped it", obs[1].sellHigh, posted)
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), fireSale(44, 48, 1.0/16.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_sellThatNeverFills_exitsAtTheLastLookaheadHoursMidPrice(t *testing.T) {
	// Nothing in the lookahead reached the posted 480, so the unit is dumped at
	// the LAST hour of it — halfway between that hour's volume-weighted price and
	// its own undercut high, on that hour's own tick. The three later hours all
	// price differently, so reading the exit from any other one of them is a
	// different number.
	obs := []simObs{
		flipHour(0, 100, 512, 200, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 80, 88, 84, 1.0/16.0),
		flipHour(3, 64, 80, 72, 1.0/8.0),
	}
	posted := postedSell(512, 1.0/16.0)
	for i, later := range obs[1:] {
		if later.sellHigh >= posted {
			t.Fatalf("hour %d printed a high of %v against the posted sell %v — the fixture sells instead of exiting", i+1, later.sellHigh, posted)
		}
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected the entry; the fixture no longer holds a later data hour")
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), fireSale(72, 80, 1.0/8.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_hourExactlyAtTheLookaheadDeadline_stillFillsTheEntry(t *testing.T) {
	// The lookahead is h+1..h+SimLookaheadHours and the last of those hours is
	// IN: three hours after the entry is the deadline, not past it. The feed
	// skipped the two hours between, which is what puts the boundary on the
	// comparison instead of beside it.
	obs := []simObs{
		flipHour(0, 100, 512, 200, 1.0/16.0),
		flipHour(3, 64, 80, 72, 1.0/8.0),
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatalf("simulateEntry rejected an entry whose next data hour is exactly %d hours later", DefaultConfig().SimLookaheadHours)
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), fireSale(72, 80, 1.0/8.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_nextDataHourPastTheLookahead_isNotAnEntry(t *testing.T) {
	// One hour past the deadline is a different trade. A feed gap shortens the
	// trip to nothing rather than reaching further forward for an hour to sell
	// into, so the hour is not simulable at all and averages into nothing.
	obs := []simObs{
		flipHour(0, 100, 512, 200, 1.0/16.0),
		flipHour(4, 64, 80, 72, 1.0/8.0),
	}

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if ok {
		t.Errorf("simulateEntry returned %v / %v chaos, want no entry — the next data hour is %d hours later, past the %d-hour lookahead",
			pct, chaos, 4, DefaultConfig().SimLookaheadHours)
	}
}

func TestSimulateEntry_unsoldExitHourWithoutAVolumeWeightedPrice_isNotAnEntry(t *testing.T) {
	// The fire-sale meets the market halfway between fair and the undercut high,
	// and an hour that reported no traded mass has no fair to meet. The entry is
	// dropped rather than exited at half of one price, because a fabricated exit
	// would move the headline mean.
	obs := []simObs{
		flipHour(0, 100, 512, 200, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
	}
	obs[1].sellVwap, obs[1].sellVwapOK = 0, false

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if ok {
		t.Errorf("simulateEntry returned %v / %v chaos, want no entry — the exit hour has no volume-weighted price", pct, chaos)
	}
}

func TestSimulateEntry_soldTripEndingOnAnHourWithoutAVolumeWeightedPrice_stillCounts(t *testing.T) {
	// The missing-fair skip is deliberately asymmetric: it guards the fire-sale
	// price and nothing else. A trip whose sell FILLED needs no fair value — the
	// exit is the price the order was posted at — so the same missing reading
	// leaves this entry in the mean. Losing that asymmetry would drop the winners
	// along with the losers.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 200, 256, 220, 1.0/16.0),
	}
	obs[2].sellVwap, obs[2].sellVwapOK = 0, false

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if !ok {
		t.Fatal("simulateEntry rejected an entry whose sell filled; the missing fair value guards the fire-sale alone")
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, 1.0/16.0), postedSell(128, 1.0/16.0), 1, 1)
	wantClose(t, "realized fraction", pct, wantPct)
	wantClose(t, "realized chaos", chaos, wantChaos)
}

func TestSimulateEntry_oneHopWhoseConversionMarketDidNotPriceTheFillHour_isNotAnEntry(t *testing.T) {
	// A triangle's proceeds come back through a third market, and the hour the
	// sell filled in is the hour that conversion is read from. An hour that
	// market did not price cannot close the route, so the entry is dropped rather
	// than valued at the leg's identity — which is what a flip converts through
	// and would quietly turn the route into one.
	obs := []simObs{
		flipHour(0, 100, 128, 110, 1.0/16.0),
		flipHour(1, 96, 100, 98, 1.0/16.0),
		flipHour(2, 200, 256, 220, 1.0/16.0),
	}
	for i := range obs {
		obs[i].convVwap, obs[i].convVwapOK = 200, true
	}
	obs[2].convVwap, obs[2].convVwapOK = 0, false

	pct, chaos, ok := simulateEntry(obs, 0, DefaultConfig())

	if ok {
		t.Errorf("simulateEntry returned %v / %v chaos, want no entry — the conversion market did not price the hour the sell filled in", pct, chaos)
	}
}

func TestBestPlays_twoHourWindow_simulatesTheOneEntryItsTruncatedLookaheadAllows(t *testing.T) {
	// The shortest window that simulates anything at all. The lookahead is three
	// hours and only one of them exists, which is enough: the older hour is an
	// entry, the newer one is where it exits. Nothing sells here — the scan
	// starts after the hour the buy filled in, and there is no hour after it — so
	// the entry fire-sales at the newest hour and the play's expectation is that
	// single trip.
	spec := liquidChaosMarket(cardID, 100, 120)
	tick := 1.0 / 100.0

	got := BestPlays("Allflame", hourlyFeed([]Row{spec.row()}, []Row{spec.row()}), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 1 {
		t.Fatalf("SimEntries = %d, want 1 — two data hours offer exactly one entry", play.SimEntries)
	}
	wantPct, wantChaos := simEntry(chasedBuy(100, tick), fireSale(vwapOf(spec), 120, tick), 1, 1)
	wantClose(t, "ExpectedRoiPct", play.ExpectedRoiPct, wantPct)
	wantClose(t, "ExpectedRoi", play.ExpectedRoi, wantChaos)
}

func TestBestPlays_newestHour_isNeverASimulatedEntry(t *testing.T) {
	// An entry needs a later hour to fill in, and the last snapshot has none. The
	// recipe traded in all three hours — HoursSeen says so — and only two of them
	// could have carried an order.
	spec := liquidChaosMarket(cardID, 100, 120)

	got := BestPlays("Allflame", steadyFeed(3, spec), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 3 {
		t.Fatalf("HoursSeen = %d, want 3 — the fixture no longer clears in every hour", play.HoursSeen)
	}
	if play.SimEntries != 2 {
		t.Errorf("SimEntries = %d, want 2 — the newest of the three hours has nothing later to fill in", play.SimEntries)
	}
}

func TestBestPlays_hourWhoseLegFailedItsGate_isNotASimulatedEntry(t *testing.T) {
	// The simulation records a candidate before the quality gates, not before the
	// LIVENESS ones: an hour in which no card changed hands observed nothing
	// about this recipe and could not have filled an order either. The same
	// window with that one hour alive is the control, so the fixture differs in
	// one number.
	spec := liquidChaosMarket(cardID, 100, 120)
	quiet := spec
	quiet.volume = [2]int64{986, 0}
	hours := [][]Row{{spec.row()}, {quiet.row()}, {spec.row()}, {spec.row()}}

	got := BestPlays("Allflame", hourlyFeed(hours...), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 2 {
		t.Errorf("SimEntries = %d, want 2 — the quiet hour is neither an entry nor a fill hour", play.SimEntries)
	}

	hours[1] = []Row{spec.row()}
	control := BestPlays("Allflame", hourlyFeed(hours...), DefaultConfig())
	if entries := playByKey(t, control, directKey(chaosID, cardID)).SimEntries; entries != 3 {
		t.Errorf("SimEntries = %d with the hour alive, want 3 — the fixture no longer isolates the liveness gate", entries)
	}
}

func TestBestPlays_hourWithoutADivineChaosMarket_isNotASimulatedEntryForItsDivineQuotedPlays(t *testing.T) {
	// An entry realizes CHAOS, and an hour that never traded a divine cannot say
	// what a divine-quoted outlay was worth. Such an hour is a data hour — the
	// scarab market priced and traded, so it can still fill a later order — and
	// it is not an entry: valuing it at a rate of zero would average a free trade
	// into the mean. The same window with the rate present is the control.
	_, divineLeg, anchor := liquidTriangle()
	hours := [][]Row{
		{divineLeg.row()},
		{divineLeg.row(), anchor.row()},
		{divineLeg.row(), anchor.row()},
	}

	got := BestPlays("Allflame", hourlyFeed(hours...), DefaultConfig())

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.SimEntries != 1 {
		t.Errorf("SimEntries = %d, want 1 — the oldest hour had no divine/chaos market to value an entry with", play.SimEntries)
	}

	hours[0] = []Row{divineLeg.row(), anchor.row()}
	control := BestPlays("Allflame", hourlyFeed(hours...), DefaultConfig())
	if entries := playByKey(t, control, directKey(divineID, scarabID)).SimEntries; entries != 2 {
		t.Errorf("SimEntries = %d with the rate present, want 2 — the fixture no longer isolates the missing rate", entries)
	}
}

func TestBestPlays_divineQuotedPlay_valuesEachSimulatedEntryAtItsOwnHoursDivineRate(t *testing.T) {
	// The per-hour discipline the whole package is built on, applied to the
	// simulation: an entry's outlay was made in ITS hour, at that hour's exchange
	// rate. The oldest hour's divine went for 10 chaos and the two newer ones for
	// 200, on identical scarab prices, so the mean can only come out right if
	// each entry was valued where it was made — repricing both at the newest
	// hour's 200 more than doubles ExpectedRoi.
	//
	// ExpectedRoiPct is the same two trips without the rates, which is what makes
	// the pair readable: the fractions are shared, the chaos is not.
	_, divineLeg, anchor := liquidTriangle()
	rows := hourlyFeed(
		[]Row{divineLeg.row(), cheapDivineAnchor().row()},
		[]Row{divineLeg.row(), anchor.row()},
		[]Row{divineLeg.row(), anchor.row()},
	)
	tick, fill := 0.1, chasedBuy(0.05, 0.1)

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, directKey(divineID, scarabID))
	if play.SimEntries != 2 {
		t.Fatalf("SimEntries = %d, want 2 — the fixture no longer offers exactly the two entries the mean is written for", play.SimEntries)
	}
	// The oldest hour's sell fills two hours later at its posted price; the middle
	// hour has only the newest hour left and fire-sales into it.
	soldPct, soldChaos := simEntry(fill, postedSell(0.1, tick), 1, 10)
	exitPct, exitChaos := simEntry(fill, fireSale(vwapOf(divineLeg), 0.1, tick), 1, 200)
	wantClose(t, "ExpectedRoi", play.ExpectedRoi, (soldChaos+exitChaos)/2)
	wantClose(t, "ExpectedRoiPct", play.ExpectedRoiPct, (soldPct+exitPct)/2)
}

func TestBestPlays_playWhoseSimulationLosesMoney_isServedWithANegativeExpectedRoi(t *testing.T) {
	// The card fell from 100 chaos to 20 across three hours, so both entries
	// bought high and dumped low: the expectation is a LOSS, while the hour the
	// play is priced from still prints a positive round trip. Nothing is hidden —
	// ADR-015's serve-and-flag rule — so the row is served with the negative
	// number on it rather than dropped, and a reader can see what the displayed
	// percentage is worth.
	entry := liquidChaosMarket(cardID, 100, 128)
	middle := liquidChaosMarket(cardID, 60, 72)
	newest := liquidChaosMarket(cardID, 20, 24)

	got := BestPlays("Allflame", hourlyFeed([]Row{entry.row()}, []Row{middle.row()}, []Row{newest.row()}), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if !(play.Roi > 0) {
		t.Fatalf("Roi = %v, want the newest hour's round trip to still read positive — the fixture no longer separates the two numbers", play.Roi)
	}
	exit := fireSale(vwapOf(newest), 24, 1.0/20.0)
	_, entryChaos := simEntry(chasedBuy(100, 1.0/100.0), exit, 1, 1)
	_, middleChaos := simEntry(chasedBuy(60, 1.0/60.0), exit, 1, 1)
	wantClose(t, "ExpectedRoi", play.ExpectedRoi, (entryChaos+middleChaos)/2)
	if !(play.ExpectedRoi < 0) {
		t.Errorf("ExpectedRoi = %v, want a loss — both entries exited under what they paid", play.ExpectedRoi)
	}
}

func TestBestPlays_fewerSimulatedEntriesThanTheGuard_flagsLowCoverage(t *testing.T) {
	// Two entries against a guard of three: the mean is a story rather than an
	// expectation, and the play says so instead of ranking on it. The guard is
	// armed at three because the shipped twelve would be capped at this feed's
	// four possible entry hours and both this test and its boundary partner would
	// read the same.
	spec := liquidChaosMarket(cardID, 100, 120)
	quiet := spec
	quiet.volume = [2]int64{986, 0}
	cfg := DefaultConfig()
	cfg.MinSimEntries = 3

	got := BestPlays("Allflame", hourlyFeed(
		[]Row{spec.row()}, []Row{quiet.row()}, []Row{quiet.row()}, []Row{spec.row()}, []Row{spec.row()},
	), cfg)

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 2 {
		t.Fatalf("SimEntries = %d, want 2 — the fixture no longer sits under the guard of %d", play.SimEntries, cfg.MinSimEntries)
	}
	if !play.LowCoverage {
		t.Error("LowCoverage = false, want true — two entries is under the guard of three")
	}
}

func TestBestPlays_simulatedEntriesExactlyAtTheGuard_isNotLowCoverage(t *testing.T) {
	// The guard is a floor the count may stand on: flagged only BELOW it. One
	// hour fewer of trading than its partner test leaves three entries against a
	// guard of three, and the play is covered.
	spec := liquidChaosMarket(cardID, 100, 120)
	quiet := spec
	quiet.volume = [2]int64{986, 0}
	cfg := DefaultConfig()
	cfg.MinSimEntries = 3

	got := BestPlays("Allflame", hourlyFeed(
		[]Row{spec.row()}, []Row{quiet.row()}, []Row{spec.row()}, []Row{spec.row()}, []Row{spec.row()},
	), cfg)

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 3 {
		t.Fatalf("SimEntries = %d, want 3 — the fixture no longer sits exactly on the guard of %d", play.SimEntries, cfg.MinSimEntries)
	}
	if play.LowCoverage {
		t.Error("LowCoverage = true, want false — three entries meets the guard of three")
	}
}

func TestBestPlays_feedShorterThanTheGuard_capsItAtTheHoursThatCouldHaveBeenEntries(t *testing.T) {
	// A four-hour feed offers three entry hours and cannot offer a twelfth, so
	// the shipped guard is capped at what the feed could produce — the same
	// capping MinHoursSeen carries. Without it a young league would flag every
	// play it serves for failing to measure hours that do not exist.
	got := BestPlays("Allflame", steadyFeed(4, liquidChaosMarket(cardID, 100, 120)), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 3 {
		t.Fatalf("SimEntries = %d, want 3 — four data hours offer three entries", play.SimEntries)
	}
	if play.LowCoverage {
		t.Errorf("LowCoverage = true, want false — three entries is every entry the feed could hold, against a guard of %d", DefaultConfig().MinSimEntries)
	}
}

func TestBestPlays_singleHourFeed_flagsLowCoverageOnASimulationThatMeasuredNothing(t *testing.T) {
	// The one case the cap cannot rescue: a feed of a single hour offers no later
	// hour to fill in, so nothing was measured at all. Capping the guard to zero
	// would let "we measured nothing" pass as covered and rank an unmeasured
	// recipe above a measured one, so zero entries is flagged on its own.
	got := BestPlays("Allflame", storedAt(feedHour, liquidChaosMarket(cardID, 100, 120).row()), DefaultConfig())

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.SimEntries != 0 {
		t.Fatalf("SimEntries = %d, want 0 — one hour cannot hold an entry", play.SimEntries)
	}
	if !play.LowCoverage {
		t.Error("LowCoverage = false, want true — nothing was simulated at all")
	}
	if play.ExpectedRoi != 0 || play.ExpectedRoiPct != 0 {
		t.Errorf("ExpectedRoi/ExpectedRoiPct = %v/%v, want 0 — a mean over no entries is not a number",
			play.ExpectedRoi, play.ExpectedRoiPct)
	}
}

func TestBestPlays_oneHopPlay_valuesTheSimulatedExitThroughTheConversionMarketsVwap(t *testing.T) {
	// A triangle's proceeds are in the SECOND quote and have to come back: the
	// simulated exit is multiplied by the conversion market's volume-weighted
	// price in the hour the trip ended. The anchor here trades at 220 chaos a
	// divine while printing 200, so the rate the conversion uses is visible in
	// the answer — reading the leg's price instead, or converting through the
	// identity a flip uses, are both different numbers.
	chaosLeg, divineLeg, _ := liquidTriangle()
	anchor := divineChaosAnchor()
	anchor.volume = [2]int64{4400000, 20000}
	rows := hourlyFeed(triangleRows(chaosLeg, divineLeg, anchor), triangleRows(chaosLeg, divineLeg, anchor))

	got := BestPlays("Allflame", rows, DefaultConfig())

	play := playByKey(t, got, oneHopKey(scarabID, chaosID, divineID))
	if play.SimEntries != 1 {
		t.Fatalf("SimEntries = %d, want 1 — two data hours offer exactly one entry", play.SimEntries)
	}
	// Buy the scarab in chaos at 10 plus the chaos market's 10% tick, dump it
	// unsold into the newest hour's divine market, and convert that divine at the
	// hour's 220 rather than at the 200 it printed.
	fill := chasedBuy(10, 0.1)
	exit := fireSale(vwapOf(divineLeg), 0.1, 0.1)
	wantPct, wantChaos := simEntry(fill, exit, vwapOf(anchor), 1)
	wantClose(t, "ExpectedRoiPct", play.ExpectedRoiPct, wantPct)
	wantClose(t, "ExpectedRoi", play.ExpectedRoi, wantChaos)
}

func TestBestPlays_rankingWindowShorterThanTheSimWindow_stillSimulatesTheWholeSimWindow(t *testing.T) {
	// The two windows are independent, which is what makes ExpectedRoi a property
	// of the FEED rather than of the horizon a reader picked. Twenty-four stored
	// hours: the recent horizon ranks six of them and still averages its
	// expectation over all twenty-three entries the day holds, so both horizons
	// serve the same number for the same recipe.
	rows := steadyFeed(24, liquidChaosMarket(cardID, 100, 120))
	base := DefaultConfig()
	horizons := DefaultHorizons()

	recent := BestPlays("Allflame", rows, horizonConfig(base, horizons[0]))
	day := BestPlays("Allflame", rows, horizonConfig(base, horizons[1]))

	if recent.Hours != 6 || day.Hours != 24 {
		t.Fatalf("ranked hours = %d and %d, want 6 and 24 — the fixture no longer separates the two horizons", recent.Hours, day.Hours)
	}
	recentPlay := playByKey(t, recent, directKey(chaosID, cardID))
	dayPlay := playByKey(t, day, directKey(chaosID, cardID))
	if recentPlay.SimEntries != 23 {
		t.Errorf("SimEntries = %d on the six-hour ranking, want 23 — the simulation reads its own 24-hour window", recentPlay.SimEntries)
	}
	if recentPlay.SimEntries != dayPlay.SimEntries {
		t.Errorf("SimEntries = %d on the recent horizon and %d on the day one, want the same sample",
			recentPlay.SimEntries, dayPlay.SimEntries)
	}
	if recentPlay.ExpectedRoi != dayPlay.ExpectedRoi {
		t.Errorf("ExpectedRoi = %v on the recent horizon and %v on the day one, want one expectation per recipe",
			recentPlay.ExpectedRoi, dayPlay.ExpectedRoi)
	}
	if recentPlay.ExpectedRoiPct != dayPlay.ExpectedRoiPct {
		t.Errorf("ExpectedRoiPct = %v on the recent horizon and %v on the day one, want one expectation per recipe",
			recentPlay.ExpectedRoiPct, dayPlay.ExpectedRoiPct)
	}
}

func TestBestPlays_simWindowShorterThanTheRankingWindow_simulatesOnlyTheSimWindow(t *testing.T) {
	// The independence runs the other way too. When the RANKING window is the
	// wider of the two, the hours past the sim window are read for persistence
	// and nothing else: twenty-four hours ranked, six simulated, five entries.
	// Feeding the whole ranked span to the simulation would widen the sim window
	// with the horizon, which is exactly what a shared expectation depends on not
	// happening.
	cfg := DefaultConfig()
	cfg.WindowHours, cfg.SimWindowHours = 24, 6

	got := BestPlays("Allflame", steadyFeed(24, liquidChaosMarket(cardID, 100, 120)), cfg)

	play := playByKey(t, got, directKey(chaosID, cardID))
	if play.HoursSeen != 24 {
		t.Fatalf("HoursSeen = %d, want 24 — the fixture no longer ranks the wider window", play.HoursSeen)
	}
	if play.SimEntries != 5 {
		t.Errorf("SimEntries = %d, want 5 — six simulated hours offer five entries, whatever the ranking reads", play.SimEntries)
	}
}
