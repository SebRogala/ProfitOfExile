package exchange

import (
	"reflect"
	"testing"
)

// hourChannels fills the channels the fill simulation reads from an hour-priced
// leg's own reading, which is what gatedLeg does for every leg it builds: on a
// leg that was NOT window-priced the two readings are the same numbers, and a
// fixture that spelled them twice would drift.
func hourChannels(o obs) obs {
	o.hourLow, o.hourHigh, o.hourVwap, o.hourVwapOK = o.low, o.high, o.vwap, o.vwapOK
	return o
}

func TestDirectCandidates_chaosDivineRow_observesBothLegsQuotedInDivine(t *testing.T) {
	got := directCandidates([]Row{chaosDivineSpec().row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// Divine leads the default quote priority, so the market reads as "chaos
	// priced in divine": both legs trade the chaos side, the traded volume is
	// chaos's, and the quote volume is divine's. Both legs observe the same hour
	// of the same market and differ only in which end of the spread they execute
	// on and which side of the book they take it from — both read from action.
	//
	// Each price arrives with the integer pair the feed posted it as, oriented
	// to the LEG: chaos is the item here, so the cheapest chaos is 201 of them
	// for 1 divine — the transpose of what the row's ItemA/ItemB order stores.
	hour := func(stock int64) obs {
		return hourChannels(obs{
			low:         pricePoint{price: 1.0 / 201.0, itemQty: 201, quoteQty: 1},
			high:        pricePoint{price: 1.0 / 196.0, itemQty: 196, quoteQty: 1},
			vwap:        65361.0 / 13001051.0,
			vwapOK:      true,
			tick:        1.0 / 196.0,
			quoteVolume: 65361,
			volume:      13001051,
			stock:       stock,
		})
	}
	// The buy takes chaos off the book (4,564,191 of it), the sell hands chaos
	// over for divine (8,878 of it) — the two sides of the one market, each
	// named by the leg that executes against it.
	wantLegs := []candidateLeg{
		{action: "buy", item: chaosID, quote: divineID, obs: hour(4564191)},
		{action: "sell", item: chaosID, quote: divineID, obs: hour(8878)},
	}
	if !reflect.DeepEqual(got[0].legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", got[0].legs, wantLegs)
	}
	if got[0].mode != ModeDirect {
		t.Errorf("mode = %q, want %q", got[0].mode, ModeDirect)
	}
}

func TestDirectCandidates_chaosPreferredAsQuote_observesBothLegsQuotedInChaos(t *testing.T) {
	cfg := DefaultConfig()
	cfg.QuotePriority = []string{ChaosID, DivineID}

	got := directCandidates([]Row{chaosDivineSpec().row()}, windowView{}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	// The same market, read the other way round: divine is the traded item, so
	// the prices are the feed's own 196 and 201, the depth is divine's, and the
	// volume-weighted price is the 198.97 chaos a divine the hour actually
	// cleared at. The tick is a property of the quantity pairs, so it does not
	// depend on which side is read as the quote.
	//
	// The posted pairs transpose with the orientation: the same market that
	// reads as "201 chaos for 1 divine" under the default priority reads as
	// "1 divine for 196 chaos" here. Nothing inverted a float — the other stored
	// quantity became the item side. The stocks transpose with it, each leg
	// still naming the side it executes against.
	hour := func(stock int64) obs {
		return hourChannels(obs{
			low:         pricePoint{price: 196, itemQty: 1, quoteQty: 196},
			high:        pricePoint{price: 201, itemQty: 1, quoteQty: 201},
			vwap:        13001051.0 / 65361.0,
			vwapOK:      true,
			tick:        1.0 / 196.0,
			quoteVolume: 13001051,
			volume:      65361,
			stock:       stock,
		})
	}
	wantLegs := []candidateLeg{
		{action: "buy", item: divineID, quote: chaosID, obs: hour(8878)},
		{action: "sell", item: divineID, quote: chaosID, obs: hour(4564191)},
	}
	if !reflect.DeepEqual(got[0].legs, wantLegs) {
		t.Errorf("legs = %+v, want %+v", got[0].legs, wantLegs)
	}
}

func TestDirectCandidates_edgeIsTheSameWhicheverSideIsTheQuote(t *testing.T) {
	row := chaosDivineSpec().row()
	chaosQuoted := DefaultConfig()
	chaosQuoted.QuotePriority = []string{ChaosID, DivineID}

	inDivine := directCandidates([]Row{row}, windowView{}, DefaultConfig())
	inChaos := directCandidates([]Row{row}, windowView{}, chaosQuoted)

	if len(inDivine) != 1 || len(inChaos) != 1 {
		t.Fatalf("got %d and %d candidates, want 1 each", len(inDivine), len(inChaos))
	}
	// Inverting both prices leaves their ratio alone: orientation is a
	// presentation choice, not an arithmetic one.
	wantClose(t, "edge quoted in divine", inDivine[0].edge, 201.0/196.0-1)
	wantClose(t, "edge quoted in chaos", inChaos[0].edge, inDivine[0].edge)
}

func TestDirectCandidates_key_namesTheMarketBehindTheDirectPrefix(t *testing.T) {
	spec := chaosDivineSpec()
	spec.itemA = cardID
	spec.itemB = chaosID

	got := directCandidates([]Row{spec.row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if want := "direct:" + cardID + "|" + chaosID; got[0].key != want {
		t.Errorf("key = %q, want %q", got[0].key, want)
	}
}

func TestDirectCandidates_rowFailingAGate_producesNoCandidate(t *testing.T) {
	// The default floor is ONE traded unit per hour (POE-193): liveness asks
	// whether a trade happened, so the only volume that fails it is none at all.
	// The item side of this market is chaos, because divine leads the quote
	// priority.
	tests := []struct {
		name      string
		breakSpec func(s *rowSpec)
	}{
		{
			name:      "nothing traded on the item side",
			breakSpec: func(s *rowSpec) { s.volume[0] = 0 },
		},
		{
			// The buy leg executes against the item side, so an empty one
			// leaves it nothing to take and the flip dies with it.
			name:      "no stock on the item side, which the buy leg executes against",
			breakSpec: func(s *rowSpec) { s.highestStock[0] = 0 },
		},
		{
			// And the sell leg executes against the quote side. Gating the two
			// legs separately is what keeps a FLIP demanding both sides after
			// the gate started following the action (2026-08-23): neither leg
			// asks for the other's side, and the pair of them asks for each.
			name:      "no stock on the quote side, which the sell leg executes against",
			breakSpec: func(s *rowSpec) { s.highestStock[1] = 0 },
		},
		{
			name:      "row carries no usable price",
			breakSpec: func(s *rowSpec) { s.priceInvalid = true },
		},
		{
			name: "highest ratio prices the item below the lowest",
			breakSpec: func(s *rowSpec) {
				s.lowestRatio = [2]int64{201, 1}
				s.highestRatio = [2]int64{196, 1}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			spec := chaosDivineSpec()
			tt.breakSpec(&spec)

			if got := directCandidates([]Row{spec.row()}, windowView{}, DefaultConfig()); len(got) != 0 {
				t.Errorf("got %d candidates, want none: %+v", len(got), got)
			}
		})
	}
}

func TestDirectCandidates_aSingleTradedUnit_keepsTheCandidateAtDefaults(t *testing.T) {
	// The shipped floor's inclusive boundary. One unit changing hands is the
	// weakest true statement about a leg, and POE-193 made it the whole default
	// demand: a market that traded once this hour is a market, and how many units
	// are worth the reader's time is the reader's call.
	spec := chaosDivineSpec()
	spec.volume[0] = 1

	got := directCandidates([]Row{spec.row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1: one traded unit clears the default floor", len(got))
	}
	if got[0].legs[0].obs.volume != 1 {
		t.Errorf("leg volume = %v, want 1", got[0].legs[0].obs.volume)
	}
}

func TestDirectCandidates_itemVolumeExactlyAtAnArmedFloor_keepsTheCandidate(t *testing.T) {
	// The knob's inclusive boundary. Ten units an hour is what the engine
	// enforced until POE-193 and what EXCHANGE_MIN_VOLUME_PER_HOUR is now FOR, so
	// the level is armed here rather than assumed.
	spec := chaosDivineSpec()
	spec.volume[0] = 10
	cfg := DefaultConfig()
	cfg.MinVolumePerHour = 10

	got := directCandidates([]Row{spec.row()}, windowView{}, cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1: the floor is inclusive", len(got))
	}
	if got[0].legs[0].obs.volume != 10 {
		t.Errorf("leg volume = %v, want 10", got[0].legs[0].obs.volume)
	}
}

func TestDirectCandidates_itemVolumeOneUnitUnderAnArmedFloor_producesNoCandidate(t *testing.T) {
	// The other side of the armed boundary: the same nine units the default now
	// serves are dropped once a reader types the old level in.
	spec := chaosDivineSpec()
	spec.volume[0] = 9
	cfg := DefaultConfig()
	cfg.MinVolumePerHour = 10

	if got := directCandidates([]Row{spec.row()}, windowView{}, cfg); len(got) != 0 {
		t.Errorf("got %d candidates, want none at an armed floor of 10: %+v", len(got), got)
	}
}

func TestDirectCandidates_untradedQuoteSide_stillProducesACandidate(t *testing.T) {
	// Only the traded item side is gated; the quote side's volume is not
	// consulted.
	spec := chaosDivineSpec()
	spec.volume[1] = 0

	got := directCandidates([]Row{spec.row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if got[0].legs[0].item != chaosID {
		t.Errorf("traded item = %s, want %s", got[0].legs[0].item, chaosID)
	}
}

func TestDirectCandidates_servedFlip_neverMarksALegDepleted(t *testing.T) {
	// A flip's two legs execute against opposite sides of one market, so each
	// leg's "other side" is the side the other leg already had to find stock on.
	// A candidate that got this far therefore cannot carry the mark, and the
	// property is asserted rather than assumed because the flag is set in the
	// shared gatedLeg and would otherwise be one wrong comparison away from
	// appearing on every flip.
	got := directCandidates([]Row{chaosDivineSpec().row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	for i, leg := range got[0].legs {
		if leg.obs.depletedSide {
			t.Errorf("leg %d (%s) depletedSide = true, want false — a served flip has stock on both sides", i, leg.action)
		}
	}
}

func TestDirectCandidates_untradedQuoteSide_marksTheHourAsCarryingNoFairPrice(t *testing.T) {
	// No divine changed hands, so the hour has no volume-weighted price at all.
	// That is a MISSING reading, not a price of zero: vwapOK is what keeps the
	// aggregator from averaging the 0 into the leg's fair anchor.
	spec := chaosDivineSpec()
	spec.volume[1] = 0

	got := directCandidates([]Row{spec.row()}, windowView{}, DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if got[0].legs[0].obs.vwapOK {
		t.Errorf("vwapOK = true, want false when the quote side traded nothing")
	}
	if got[0].legs[0].obs.vwap != 0 {
		t.Errorf("vwap = %v, want 0 beside vwapOK false", got[0].legs[0].obs.vwap)
	}
}

func TestDirectCandidates_thinScoredHourOverASpreadBearingWindow_pricesFromTheWindow(t *testing.T) {
	// The scored hour traded ONE card at a single 552 print, so it prints no
	// spread of its own. The five hours behind it each printed 486/1148, and the
	// leg is priced from those realized extremes while its HOUR channels keep
	// the scored hour's own reading for the fill simulation (C3).
	rows := apocalypseWindowFeed(cardID, 6, 1, 0)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	o := got[0].legs[0].obs
	if o.low.price != 486 || o.high.price != 1148 {
		t.Errorf("priced interval = %v/%v, want the window's 486/1148", o.low.price, o.high.price)
	}
	if o.low.itemQty != 1 || o.low.quoteQty != 486 {
		t.Errorf("low pair = %d/%d, want the pair the window hour posted, 486 chaos for 1 card", o.low.quoteQty, o.low.itemQty)
	}
	if o.hourLow.price != 552 || o.hourHigh.price != 552 {
		t.Errorf("hour channels = %v/%v, want the scored hour's own 552/552", o.hourLow.price, o.hourHigh.price)
	}
	if o.hourVwap != 552 || !o.hourVwapOK {
		t.Errorf("hourVwap = %v (ok %v), want the scored hour's 552", o.hourVwap, o.hourVwapOK)
	}
	// The anchor a window-priced leg is judged against is the window's POOLED
	// volume-weighted price: 552 chaos over one card plus five hours of 750,000
	// over a thousand.
	wantClose(t, "vwap", o.vwap, (552.0+5*750000.0)/(1.0+5*1000.0))
	if !o.vwapOK {
		t.Errorf("vwapOK = false, want the window's anchor")
	}
	if !o.windowPriced || o.windowHours != 6 || o.windowVolume != 5001 {
		t.Errorf("window marks = %v/%d/%v, want true/6/5001", o.windowPriced, o.windowHours, o.windowVolume)
	}
	// tick and the traded volume stay the scored hour's: they say what the market
	// is doing NOW, which is the half of the row the window does not touch (C5).
	if o.tick != 1.0/552.0 {
		t.Errorf("tick = %v, want the scored hour's %v", o.tick, 1.0/552.0)
	}
	if o.volume != 1 {
		t.Errorf("volume = %v, want the scored hour's single card", o.volume)
	}
}

func TestDirectCandidates_scoredHourAtTheThinThreshold_pricesFromItsOwnHour(t *testing.T) {
	// Two cards is Config.ThinHourVolume exactly, and the threshold is a floor
	// the hour has to fall UNDER: the same window that priced the leg above is
	// present and is not read.
	rows := apocalypseWindowFeed(cardID, 6, 2, 0)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	o := got[0].legs[0].obs
	if o.windowPriced {
		t.Errorf("windowPriced = true on a hour that traded %v units against a threshold of %v, want false", o.volume, DefaultConfig().ThinHourVolume)
	}
	if o.low != o.hourLow || o.high != o.hourHigh {
		t.Errorf("priced interval = %v/%v, want the hour channels' %v/%v", o.low, o.high, o.hourLow, o.hourHigh)
	}
	if o.low.price != 552 {
		t.Errorf("low.price = %v, want the scored hour's own 552", o.low.price)
	}
	if o.windowHours != 0 || o.windowVolume != 0 {
		t.Errorf("window marks = %d/%v, want zero on an hour-priced leg", o.windowHours, o.windowVolume)
	}
}

func TestDirectCandidates_thinHourWithNoOtherPricedHourInTheWindow_pricesFromItsOwnHour(t *testing.T) {
	// A thin hour standing alone: one card in the whole window, under
	// Config.MinWindowVolume. There is nothing to price from, so the leg serves
	// what its own hour printed and carries no mark — the window path is not a
	// licence to invent a spread.
	rows := apocalypseWindowFeed(cardID, 1, 1, 0)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	o := got[0].legs[0].obs
	if o.windowPriced {
		t.Errorf("windowPriced = true over a one-card window, want false")
	}
	if o.low.price != 552 || o.high.price != 552 {
		t.Errorf("priced interval = %v/%v, want the scored hour's own 552/552", o.low.price, o.high.price)
	}
}

func TestDirectCandidates_thinHourVolumeAtTheLivenessFloor_neverWindowPrices(t *testing.T) {
	// The misconfiguration the field docs warn about: at
	// ThinHourVolume <= MinVolumePerHour the window path is INERT, because every
	// hour thin enough to trigger it was already dropped by the liveness gate.
	// The two knobs answer different questions and are deliberately not coupled,
	// so this is inspectable rather than silently corrected.
	cfg := DefaultConfig()
	cfg.ThinHourVolume = cfg.MinVolumePerHour
	rows := apocalypseWindowFeed(cardID, 6, 1, 0)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), cfg)

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want 1", len(got))
	}
	if o := got[0].legs[0].obs; o.windowPriced {
		t.Errorf("windowPriced = true at ThinHourVolume == MinVolumePerHour (%v), want an inert window path", cfg.ThinHourVolume)
	}
}

// windowOnlyRows renders `hours` hours of one chaos-quoted market that printed
// the 2026-09-04 book's spread (spreadHour) in the hours named by `backs`, and
// published NO ROW AT ALL in every other hour of the span.
//
// It is the shape POE-252's liveness change is about: a market the feed simply
// did not carry this hour, whose window behind it is full of realized prints.
func windowOnlyRows(item string, backs ...int) []StoredRow {
	rows := make([]StoredRow, 0, len(backs))
	for _, back := range backs {
		rows = append(rows, storedBack(back, spreadHour(item)))
	}
	return rows
}

func TestDirectCandidates_publishedButUntradedScoredHour_isCarriedByItsWindow(t *testing.T) {
	// The scored hour published a row and nobody traded in it, which is what the
	// feed sends for a quiet hour: the last ratios intact, both volumes zero.
	// Before POE-252 the leg was dropped for that zero and the newest-hour rule
	// deleted the recipe; now the five hours behind it carry the hour's liveness
	// and print its price.
	untraded := spreadHour(cardID)
	untraded.volume = [2]int64{0, 0}
	rows := append([]StoredRow{storedBack(0, untraded)}, windowOnlyRows(cardID, 1, 2, 3, 4, 5)...)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want the flip the window carries", len(got))
	}
	o := got[0].legs[0].obs
	if !o.rescued {
		t.Errorf("rescued = false on an hour that traded nothing, want the window to have carried it")
	}
	if !o.windowPriced || o.windowHours != 5 {
		t.Errorf("window marks = %v/%d, want true/5 — the untraded scored hour contributes no print of its own", o.windowPriced, o.windowHours)
	}
	if o.low.price != 486 || o.high.price != 1148 {
		t.Errorf("priced interval = %v/%v, want the window's realized 486/1148", o.low.price, o.high.price)
	}
	// Depth, turnover and the book reading all come from the newest contributing
	// window row rather than from the silent hour: one row family, one hour, one
	// story, and a served row whose depth is zero blanks the reader's scale.
	if o.volume != 1000 || o.quoteVolume != 750000 {
		t.Errorf("volume/quoteVolume = %v/%v, want the newest contributing hour's 1000/750000", o.volume, o.quoteVolume)
	}
	if o.stock != 6 {
		t.Errorf("stock = %d, want the newest contributing hour's item-side book", o.stock)
	}
}

func TestDirectCandidates_rescuedHourWhoseNewestContributingRowHasNoExecutingStock_producesNoCandidate(t *testing.T) {
	// The stock DEMAND is unchanged by the liveness relaxation — ADR-017's second
	// amendment stands and the side the leg executes against must be non-empty.
	// What moved is only which row that demand is read on: the newest CONTRIBUTING
	// window row, so the book reading is as old as the price beside it and no
	// older.
	//
	// Only that one row is emptied here; the four hours behind it keep their
	// stock, so an implementation reading the demand off any other contributor —
	// or off the oldest — would serve this market and fail.
	untraded := spreadHour(cardID)
	untraded.volume = [2]int64{0, 0}
	newestContributor := spreadHour(cardID)
	newestContributor.highestStock = [2]int64{600, 0}
	rows := append(
		[]StoredRow{storedBack(0, untraded), storedBack(1, newestContributor)},
		windowOnlyRows(cardID, 2, 3, 4, 5)...,
	)

	got := directCandidates([]Row{rows[0].Row}, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 0 {
		t.Errorf("got %d candidates against an empty ask side, want none — the buy has nothing to take", len(got))
	}
}

func TestDirectCandidates_marketWithNoRowInTheScoredHour_isEnumeratedFromTheWindow(t *testing.T) {
	// The acceptance line POE-252 was filed on — "served in every hour that has
	// at least one trade in the window" — reaches hours the feed published no row
	// for at all, which is nine of the twenty-six measured hours. The market is
	// enumerated out of the span index rather than out of the scored hour's rows,
	// and everything it reads comes from the newest row inside its window.
	//
	// The newest contributing hour is given quantities of its own so the
	// assertions below discriminate: an implementation reading depth off the
	// oldest contributor, or off a zero Row, returns different numbers.
	newest := pairedHour(chaosID, cardID, [2]int64{500, 1}, [2]int64{1148, 1}, [2]int64{7000, 10})
	newest.highestStock = [2]int64{4321, 77}
	rows := append([]StoredRow{storedBack(1, newest)}, windowOnlyRows(cardID, 2, 3, 4, 5)...)

	got := directCandidates(nil, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates for a market with no row this hour, want the one its window carries", len(got))
	}
	if want := directKey(chaosID, cardID); got[0].key != want {
		t.Errorf("key = %q, want %q — the id comes from the span index, not from an absent row", got[0].key, want)
	}
	o := got[0].legs[0].obs
	if !o.rescued || !o.windowPriced {
		t.Errorf("rescued/windowPriced = %v/%v, want both true on a market with no row of its own", o.rescued, o.windowPriced)
	}
	if o.volume != 10 || o.quoteVolume != 7000 || o.stock != 77 {
		t.Errorf("volume/quoteVolume/stock = %v/%v/%d, want the newest contributing row's 10/7000/77", o.volume, o.quoteVolume, o.stock)
	}
	if o.tick != 1.0/500.0 {
		t.Errorf("tick = %v, want the newest contributing row's %v", o.tick, 1.0/500.0)
	}
	if o.low.price != 486 || o.high.price != 1148 {
		t.Errorf("priced interval = %v/%v, want the window's realized 486/1148", o.low.price, o.high.price)
	}
}

func TestDirectCandidates_marketWithNoPricedRowAnywhereInTheWindow_producesNoCandidate(t *testing.T) {
	// The guard that says the window path is not a resurrection machine. This
	// market traded normally seven hours ago and has published nothing since, so
	// there is no realized print anywhere inside the clock span to serve — and an
	// enumeration that walked the span index without asking the window for a
	// price would bring it straight back at a seven-hour-old ratio.
	rows := windowOnlyRows(cardID, 6, 7)

	got := directCandidates(nil, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 0 {
		t.Errorf("got %d candidates from a silent window, want none whatever the market did seven hours ago", len(got))
	}
}

func TestDirectCandidates_divineQuotedMarketWithNoRowInTheScoredHour_keepsItsOrientation(t *testing.T) {
	// Which side of a market is the ITEM is decided by orient off a row, and on a
	// window-only market there is no scored row to decide it from. Falling back to
	// a zero Row would leave both sides empty, orient would answer with the zero
	// ids, and the leg would price the pair the wrong way round while still
	// returning a plausible-looking number — which is why this is pinned rather
	// than inspected.
	//
	// The scarab is quoted in divine here: one divine buys between 10 and 20
	// scarabs, so a scarab is worth 0.05 to 0.1 divine.
	_, divineLeg, _ := liquidTriangle()
	rows := []StoredRow{storedBack(1, divineLeg), storedBack(2, divineLeg)}

	got := directCandidates(nil, viewAt(feedHour, rows), DefaultConfig())

	if len(got) != 1 {
		t.Fatalf("got %d candidates, want the divine-quoted flip its window carries", len(got))
	}
	leg := got[0].legs[0]
	if leg.item != scarabID || leg.quote != divineID {
		t.Errorf("leg trades %q in %q, want the scarab quoted in divine", leg.item, leg.quote)
	}
	if leg.obs.low.price != 1.0/20.0 || leg.obs.high.price != 1.0/10.0 {
		t.Errorf("priced interval = %v/%v, want 0.05/0.1 divine per scarab", leg.obs.low.price, leg.obs.high.price)
	}
}

func TestDirectCandidates_twoWindowOnlyMarketsInOneHour_keepTheirOwnKeys(t *testing.T) {
	// The candidate key is built from the market id the span index is being
	// walked under, never from the scored row. Read off a zero Row both keys
	// collapse to the bare prefix "direct:", and BestPlays' per-hour seen map
	// then treats the two markets as one recipe and drops the second without an
	// error of any kind — a silent data loss, which is why the two keys are
	// asserted rather than a count of two.
	const secondCardID = "Metadata/Items/DivinationCards/DivinationCardSecondWindowOnly"
	rows := append(windowOnlyRows(cardID, 1, 2), windowOnlyRows(secondCardID, 1, 2)...)

	got := directCandidates(nil, viewAt(feedHour, rows), DefaultConfig())

	// Ascending id order, which is what makes the enumeration independent of the
	// order storage returned rows in.
	want := []string{directKey(chaosID, secondCardID), directKey(chaosID, cardID)}
	if keys := candidateKeys(got); !reflect.DeepEqual(keys, want) {
		t.Errorf("keys = %v, want %v", keys, want)
	}
}

func TestDirectCandidates_windowPathDisarmed_enumeratesNoMarketWithoutAScoredRow(t *testing.T) {
	// ThinHourVolume: 0 is the one field that turns POE-252 off, and it must turn
	// off the ENUMERATION as well as the pricing: nothing is thin, so nothing is
	// window-priced and nothing is rescued. A reader can prove the whole feature
	// inert from one number, which is what the C3 calibration guard leans on.
	cfg := DefaultConfig()
	cfg.ThinHourVolume = 0
	rows := windowOnlyRows(cardID, 1, 2, 3, 4, 5)

	got := directCandidates(nil, viewAt(feedHour, rows), cfg)

	if len(got) != 0 {
		t.Errorf("got %d candidates with the window path disarmed, want none", len(got))
	}
}

func TestDirectCandidates_twoRowsForOneScoredMarketHour_letTheTradedCopyWinTheKey(t *testing.T) {
	// One hour holds one row per market — the repository's
	// PRIMARY KEY (league, time, market_id) forbids a second — but BestPlays is
	// total over whatever rows it is handed, and pricing.go and crossquote.go
	// both defend a duplicated hour explicitly, so this one does too.
	//
	// BestPlays keeps the FIRST candidate under a key. A copy that traded nothing
	// is RESCUED by the window its live twin contributes to, so walking the hour
	// in ONE pass emits the rescued copy first and serves a window-priced row
	// where an hour-live one existed. The two sub-passes make the traded copy win
	// the key exactly as it did before POE-252.
	untraded := spreadHour(cardID)
	untraded.volume = [2]int64{0, 0}
	// The traded copy prints an interval of its own, distinct from the 486/1148
	// the rest of the window realized, so every assertion below names which copy
	// answered rather than reading a number both could have produced.
	traded := pairedHour(chaosID, cardID, [2]int64{500, 1}, [2]int64{900, 1}, [2]int64{4500, 9})
	rows := append(
		[]StoredRow{storedBack(0, untraded), storedBack(0, traded)},
		windowOnlyRows(cardID, 1, 2, 3)...,
	)

	got := directCandidates([]Row{rows[0].Row, rows[1].Row}, viewAt(feedHour, rows), DefaultConfig())

	// Both copies still produce a candidate; which one comes FIRST is the whole
	// question, because that is the one the per-hour seen map keeps.
	if len(got) != 2 {
		t.Fatalf("got %d candidates from two copies of one market-hour, want 2", len(got))
	}
	o := got[0].legs[0].obs
	if o.rescued || o.windowPriced {
		t.Errorf("winning candidate rescued/windowPriced = %v/%v, want false/false — the traded copy priced its own hour", o.rescued, o.windowPriced)
	}
	if o.low.price != 500 || o.high.price != 900 {
		t.Errorf("winning interval = %v/%v, want the traded copy's own 500/900", o.low.price, o.high.price)
	}
	if o.volume != 9 {
		t.Errorf("winning volume = %v, want the traded copy's 9 cards", o.volume)
	}
}
