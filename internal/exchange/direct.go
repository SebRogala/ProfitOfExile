package exchange

// obs is what ONE hour observed about one leg's market.
//
// It is the unit a Play is built from: evaluate prices one hour's legs straight
// off these, so every number a Play shows traces back to ONE of them — the hour
// Play.LastHour names. low and high are the hour's cheapest and
// dearest realized price of the leg's item in its quote, each carrying the
// integer quantity pair the feed posted it as (priceIn); a buy leg executes on
// the low and a sell leg on the high, so which of the two a Play shows is read
// from the leg's action and never from the market row again. vwap is the
// price its traded mass actually cleared at (vwapIn), and vwapOK says whether
// the hour had one at all — an hour whose quote side reported no volume carries
// vwap 0, which is a missing reading rather than a price of zero, and evaluate
// must report as missing (Leg.FairOK) rather than as a price; tick is the coarsest
// step the market's quantity pairs can express (tickOf); quoteVolume and volume
// are the two sides' traded units.
//
// stock is the book side this leg EXECUTES AGAINST — the item side for a buy,
// the quote side for a sell (gatedLeg) — and depletedSide says the OTHER side
// carried none that hour. Both are liveness only: lowest/highest stock are the
// hour's min and max of total book size and say nothing about the extreme
// (corr <= 0.13 against the edge), so nothing scores on either.
//
// TWO READINGS OF THE SPREAD live here since POE-252, and which one a consumer
// takes is the whole of ADR-016's calibration lock. low/high/vwap are what the
// leg is PRICED and JUDGED at, and on a thin hour they are the trailing
// window's (windowPriceIn) rather than this hour's. hourLow/hourHigh/hourVwap/
// hourVwapOK are ALWAYS ONE row's own priceIn and vwapIn result and never a
// window's, whatever the leg was priced at — the SCORED row on an hour-live leg,
// and the source (newest contributing window) row on a rescued one, which is the
// only row such a leg has. They are what the fill simulation reads — every one
// of them, and nothing else (recordSim). windowPriced says which reading the
// first three carry, with windowHours and windowVolume as the span behind it.
//
// tick, quoteVolume, volume and stock are read from the row the leg's NON-PRICE
// readings come from — the scored hour's on an hour-live leg, which is every leg
// that existed before POE-252, and the newest contributing window row on a
// window-RESCUED one. On an hour-live leg they say what the market is doing NOW,
// which is the half of the row the window deliberately does not touch
// (Play.WindowPriced, C5); on a rescued leg there is no such hour, so they are
// as old as the price beside them and no older, bounded by
// Config.WindowPriceHours.
//
// rescued says the hour cleared ONLY because the window carried its liveness:
// this market either published no row this hour or published one that traded
// under Config.MinVolumePerHour (windowRescued). Such an hour has no reading of
// its own, so BestPlays records no simulation entry for it and does not count it
// toward Play.HoursSeen — the two counters that would otherwise move on every
// pre-POE-252 market the relaxation reaches. hourLow/hourHigh/hourVwap/
// hourVwapOK on a rescued leg are the SOURCE row's own single-hour reading
// rather than this hour's, which is honest only because nothing reads them on
// such a leg.
type obs struct {
	low          pricePoint
	high         pricePoint
	vwap         float64
	vwapOK       bool
	hourLow      pricePoint
	hourHigh     pricePoint
	hourVwap     float64
	hourVwapOK   bool
	windowPriced bool
	windowHours  int
	windowVolume float64
	rescued      bool
	tick         float64
	quoteVolume  float64
	volume       float64
	stock        int64
	depletedSide bool
}

// candidateLeg is one leg of a play as one hour observed it: the recipe
// (action, item, quote — the parts that are the same in every hour) plus that
// hour's measurements.
type candidateLeg struct {
	action string
	item   string
	quote  string
	obs    obs
}

// candidate is one play as it was observed in ONE hour: the recipe plus that
// hour's edge and per-leg observations. BestPlays aggregates candidates sharing
// a key across the hours of its window into a Play.
//
// hour is not a field: the aggregator always knows which hour's rows it passed
// to the unit that produced the candidate.
type candidate struct {
	key  string
	mode Mode
	legs []candidateLeg
	// edge is retained for the per-hour candidate tests; Play.RoiPctRaw supersedes it.
	edge float64
}

// legRow is one market's SCORED-HOUR reading, and it is a triple rather than a
// Row because since POE-252 the third field can be false.
//
// marketID is carried beside the row instead of being read off it: on a
// window-only market there is no scored row to read it from, and a zero Row
// would collapse "direct:" + r.MarketID to the bare prefix "direct:", under
// which BestPlays' per-hour seen map treats every such market as one recipe and
// silently drops all but the first. present says the feed published this market
// this hour at all, which is a different question from whether the row traded.
type legRow struct {
	marketID string
	row      Row
	present  bool
}

// windowRescued reports that an hour clears ONLY because the trailing window
// carries its liveness: the market published no row this hour, or published one
// that traded under Config.MinVolumePerHour.
//
// It is the one distinction POE-252's liveness change turns on, and it is NOT
// the same question as Play.WindowPriced. An hour that traded on its own row is
// HOUR-LIVE whether or not it was thin enough to take a window price: it records
// a fill-simulation entry from its own hour channels and counts toward
// Play.HoursSeen, exactly as it did before POE-252. A rescued hour does neither,
// because it has no reading of its own to record or to count — which is what
// keeps every pre-POE-252 ExpectedRoi and HoursSeen still while the relaxation
// reaches every market in the feed.
//
// An absent row counts as volume 0, so a market that simply stopped publishing
// is rescued rather than gone.
func windowRescued(present bool, r Row, item string, cfg Config) bool {
	return !(present && float64(volumeOf(r, item)) >= cfg.MinVolumePerHour)
}

// windowRescued reports whether ANY leg of the candidate cleared only on the
// window. One rescued leg is enough: an hour in which some step of the recipe
// had no price of its own is not an hour the recipe was priced in.
func (c candidate) windowRescued() bool {
	for _, leg := range c.legs {
		if leg.obs.rescued {
			return true
		}
	}
	return false
}

// directCandidates finds every same-market flip the scored hour can serve.
//
// A direct play buys the item at the cheapest realized price and sells it at the
// dearest, on the same market:
//
//	edge = high/low - 1
//
// Both extremes are realized trades rather than two live sides of a book, so the
// edge is the optimistic reading (see priceIn): it reaches the Play as
// RoiPctRaw, beside the same round trip after one tick of undercut per leg
// (Play.RoiPct — the ranking is ExpectedRoi's). It is also orientation-
// independent: pricing the market the other way round inverts both prices and
// leaves the ratio unchanged.
//
// A market contributes only when its leg prices and BOTH legs pass their own
// gate — alive on the scored hour or on the window behind it, and stock on the
// side each leg executes against.
//
// THE LIST OF MARKETS is no longer the scored hour's rows alone (POE-252). The
// hour's own rows are walked first, exactly as before, and then every OTHER
// market the span carries is tried window-only, in ascending id order. A market
// the feed simply did not publish this hour is still a market the reader can act
// on when the six hours behind it printed a spread, and the acceptance POE-252
// was filed on is that the pair does not vanish in such an hour. What stops this
// from serving everything is gatedLeg: a window with no realized print inside it
// prices nothing, so a market that has genuinely stopped trading is still
// absent.
//
// Walking the hour's rows first and the remainder sorted keeps the output
// deterministic without moving any pre-POE-252 candidate: the hour-live rows are
// emitted in the order they arrived, exactly as they were, and the shuffled-feed
// property holds because the appended ids are sorted and BestPlays re-sorts by
// key regardless.
//
// THE SCORED HOUR IS WALKED IN TWO SUB-PASSES — hour-live rows, then rescued
// present ones — and the order is load-bearing rather than cosmetic. BestPlays
// keys its per-hour seen map by candidate key and keeps the FIRST candidate
// under each, so if storage ever hands over two rows for one market-hour (the
// repository's PRIMARY KEY forbids it, and both pricing.go and crossquote.go
// defend it anyway) a single pass would let a copy that traded nothing be
// rescued, appended first, and win the key over a copy of the SAME hour that
// traded and priced itself. That would serve a window-priced row where a
// hour-live one existed. Splitting the pass makes the hour-live copy win as it
// did before POE-252; the rescued copy is still emitted, and is still what the
// key resolves to when no live copy exists.
//
// WHAT THE ENUMERATION DOES NOT REACH, and deliberately: a market whose scored
// row is PRESENT and traded at least Config.MinVolumePerHour units but that
// priceIn refuses. windowRescued is false on such a row, so gatedLeg reads the
// price off the scored row and drops the market exactly as it did before
// POE-252 — whether or not the hour was thin enough to have been window-PRICED.
// An ABSENT row and one that traded under the floor are rescued; this one is
// not. A row publishing quantities the feed cannot price is feed drift, and the
// relaxation was filed for liveness.
//
// The two legs are gated separately rather than one being copied from the other,
// and that is what keeps a flip's demand for stock on BOTH sides of the market
// standing after gatedLeg became action-aware (2026-08-23): the buy leg demands
// the item side and the sell leg the quote side, on the SAME row, so the pair of
// per-leg demands spells the old both-sides rule without a case for the shape.
// A direct flip therefore never carries Leg.DepletedSide — a leg's opposite side
// is the other leg's executing side, and both had to be non-empty to get here.
// Everything else the two legs observe is identical: one market, one row family,
// and only the end of the spread they execute on differs, which is read from
// action.
func directCandidates(rows []Row, view windowView, cfg Config) []candidate {
	candidates := make([]candidate, 0, len(rows))
	scored := make(map[string]bool, len(rows))
	var rescuedRows []Row
	for _, r := range rows {
		scored[r.MarketID] = true
		item, quote := orient(r, cfg.QuotePriority)
		// Both legs of a flip trade the one market's item side, so one call
		// answers for the pair.
		if windowRescued(true, r, item, cfg) {
			rescuedRows = append(rescuedRows, r)
			continue
		}
		if c, ok := directFlip(legRow{marketID: r.MarketID, row: r, present: true}, item, quote, view, cfg); ok {
			candidates = append(candidates, c)
		}
	}
	for _, r := range rescuedRows {
		item, quote := orient(r, cfg.QuotePriority)
		if c, ok := directFlip(legRow{marketID: r.MarketID, row: r, present: true}, item, quote, view, cfg); ok {
			candidates = append(candidates, c)
		}
	}

	for _, marketID := range view.markets() {
		if scored[marketID] {
			continue
		}
		span := view.rowsFor(marketID)
		if len(span) == 0 {
			continue
		}
		// Which side is the item is a property of the MARKET, not of an hour:
		// ItemA and ItemB are the same on every row the id carries, so span[0]
		// is read for those two fields ALONE and any row of the span would
		// answer the same. It is not the row the leg reads. Everything an hour
		// could disagree about — prices, stock, volume, tick — comes from the
		// newest CONTRIBUTING window row, which gatedLeg selects for itself
		// against its own volume and price filters and which need not be
		// span[0].
		item, quote := orient(span[0].Row, cfg.QuotePriority)
		if c, ok := directFlip(legRow{marketID: marketID}, item, quote, view, cfg); ok {
			candidates = append(candidates, c)
		}
	}
	return candidates
}

// directFlip builds the buy and the sell of one market's flip, or reports
// ok == false when either leg fails its gate.
func directFlip(r legRow, item, quote string, view windowView, cfg Config) (candidate, bool) {
	buy, ok := gatedLeg("buy", item, quote, r, view, cfg)
	if !ok {
		return candidate{}, false
	}
	sell, ok := gatedLeg("sell", item, quote, r, view, cfg)
	if !ok {
		return candidate{}, false
	}
	return candidate{
		key:  "direct:" + r.marketID,
		mode: ModeDirect,
		legs: []candidateLeg{buy, sell},
		edge: buy.obs.high.price/buy.obs.low.price - 1,
	}, true
}

// gatedLeg builds one leg of a play from the row that would execute it, and
// reports whether that row priced the pair and was alive enough to count.
//
// The gate is liveness, not liquidity: the leg's item must have traded at least
// Config.MinVolumePerHour units, and the side the leg EXECUTES AGAINST must have
// carried stock. At the default of 1 the volume half reads as "a trade happened
// here", which is the weakest true statement about the leg and deliberately so —
// the old floor of 10 dropped a live card market in 11 of 24 measured hours
// (DefaultConfig, and the measurement itself in
// docs/adr/017-no-default-engine-floor-may-hide-a-live-market.md). Liquidity is
// judged later, in chaos, on the play's Turnover — unit volume alone does not
// predict a real edge. A leg failing this kills the whole play — a recipe is
// only as executable as its thinnest step — so both directCandidates and
// crossQuoteCandidates drop the candidate on the first false.
//
// The stock half FOLLOWS THE ACTION, and did not until 2026-08-23. A buy takes
// units off the item side of the book, so it needs item-side stock; a sell hands
// units to the quote side, so it needs QUOTE-side stock. Demanding both of every
// leg — what this did before — deleted exactly the one-sided market with the
// largest edge in it: Journey Tattoo against chaos stood at 1121 chaos of bids
// and zero asks, which is the shape a seller wants and a buyer cannot touch, and
// the sell leg was dropped for the empty side it was never going to trade on.
// The newest-hour rule in BestPlays then removed the whole recipe. The opposite
// side is REPORTED instead, through obs.depletedSide and Leg.DepletedSide, per
// the visibility rule (ADR-017): a market the reader could act on is served and
// marked, never hidden.
//
// A direct flip keeps the both-sides demand all the same, because its two legs
// are gated separately on the one row and between them ask for both — see
// directCandidates.
//
// WHICH HOURS THE LEG READS is what POE-252 added, and there are two questions,
// answered by the same predicate so one Config field disarms both. An hour that
// traded under Config.ThinHourVolume units cannot print a spread — one trade
// collapses the low and the high onto the same number, and the leg then reads
// -0% however long the two sides have stood in the game's book — so such a leg
// is PRICED from the trailing window instead, the extremes the market REALIZED
// over the last Config.WindowPriceHours clock hours, and is marked with the span
// it read. An hour that traded under Config.MinVolumePerHour, or published no
// row at all, additionally has no liveness of its own, and the same window is
// what CARRIES it (windowRescued). At ThinHourVolume 0 nothing is thin, so
// nothing is window-priced and nothing is rescued: the whole feature is off.
//
// The STOCK DEMAND IS UNCHANGED — ADR-017's second amendment stands, and the
// executing side must still be non-empty. What changes on a rescued hour is the
// stock READING, which comes from the newest contributing window row, so it is
// as old as the price it accompanies and no older. Tick, traded volume and quote
// volume come from that same row: one row family, one hour, one story, which is
// also what keeps Play.Depth and Play.Turnover positive on a rescued row rather
// than reporting a market with no depth on exactly the rows this exists to
// serve. Reading the scored row instead would cost the reader the WAIT — the
// desktop's Scale column divides its run by Depth and prints a dash for the
// hours when it cannot — and would put a 0 in front of any armed turnover gate.
//
// A window with no realized print inside it rescues nothing. That is the whole
// of the guard: a market may be priced from a window that is alive, never
// revived from one that is not.
func gatedLeg(action, item, quote string, r legRow, view windowView, cfg Config) (candidateLeg, bool) {
	volume := int64(0)
	if r.present {
		volume = volumeOf(r.row, item)
	}
	rescued := windowRescued(r.present, r.row, item, cfg)

	// ONE condition arms both halves of the window path, so ThinHourVolume: 0
	// disarms pricing and rescue together and a reader can prove the feature off
	// from one field. An absent row counts as volume 0 for the thin test.
	var (
		windowLow, windowHigh pricePoint
		contributors          []StoredRow
		windowVolume          float64
		priced                bool
	)
	if float64(volume) < cfg.ThinHourVolume {
		windowLow, windowHigh, contributors, windowVolume, priced = windowPriceIn(view.rowsFor(r.marketID), view.hour, item, quote, cfg)
	}
	if rescued && !priced {
		return candidateLeg{}, false
	}

	// src is the row every NON-PRICE reading comes from. On an hour-live leg it
	// is the scored row and this whole function is what it was before POE-252;
	// on a rescued one it is the newest contributing window row, which is the
	// row the window's own liveness came from.
	src := r.row
	if rescued {
		src = contributors[0].Row
	}

	low, high, ok := priceIn(src, item, quote)
	if !ok {
		return candidateLeg{}, false
	}

	executes, opposite := stockOf(src, item), stockOf(src, quote)
	if action == "sell" {
		executes, opposite = opposite, executes
	}
	if executes <= 0 {
		return candidateLeg{}, false
	}

	// A row that traded MinVolumePerHour units has a usable vwap unless the
	// quote side reported nothing; that hour then has no fair anchor, and vwapOK
	// carries that through to Leg.FairOK rather than letting a 0 read as a free
	// item — with no anchor, nothing can be called suspect either.
	vwap, vwapOK := vwapIn(src, item, quote)

	o := obs{
		low:          low,
		high:         high,
		vwap:         vwap,
		vwapOK:       vwapOK,
		hourLow:      low,
		hourHigh:     high,
		hourVwap:     vwap,
		hourVwapOK:   vwapOK,
		rescued:      rescued,
		tick:         tickOf(src),
		quoteVolume:  float64(quoteVolumeOf(src, quote)),
		volume:       float64(volumeOf(src, item)),
		stock:        executes,
		depletedSide: opposite <= 0,
	}

	// Only the priced-and-judged channels move. The hour channels above are left
	// as the source row read them, because the fill simulation is calibrated on
	// single-hour prices and must not see a window one (ADR-016, C3) — and on a
	// rescued leg it is not shown them at all, because BestPlays records no entry
	// for such an hour.
	if priced {
		o.low, o.high = windowLow, windowHigh
		// The anchor moves with the extremes, and it is pooled over the very
		// rows that printed them: a window spread judged against one hour's
		// mass would compare two different windows, and Leg.Fair is what the
		// suspect bands read.
		o.vwap, o.vwapOK = windowVwapOf(contributors, item, quote)
		o.windowPriced, o.windowHours, o.windowVolume = true, len(contributors), windowVolume
	}

	return candidateLeg{
		action: action,
		item:   item,
		quote:  quote,
		obs:    o,
	}, true
}
