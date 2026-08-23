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
type obs struct {
	low          pricePoint
	high         pricePoint
	vwap         float64
	vwapOK       bool
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

// directCandidates finds every same-market flip in one hour's rows.
//
// A direct play buys the item at the hour's cheapest realized price and sells it
// at the dearest, on the same market:
//
//	edge = high/low - 1
//
// Both extremes are realized trades from the same hour rather than two live
// sides of a book, so this hour's edge is the optimistic reading (see priceIn):
// it reaches the Play as RoiPctRaw, beside the same round trip after one tick
// of undercut per leg (Play.RoiPct — the ranking is ExpectedRoi's). It is also
// orientation-independent: pricing the market the other way round inverts both
// prices and leaves the ratio unchanged.
//
// A row contributes only when priceIn can price it and BOTH legs pass their own
// gate — at least Config.MinVolumePerHour units traded, and stock on the side
// each leg executes against.
//
// The two legs are gated separately rather than one being copied from the other,
// and that is what keeps a flip's demand for stock on BOTH sides of the market
// standing after gatedLeg became action-aware (2026-08-23): the buy leg demands
// the item side and the sell leg the quote side, on the SAME row, so the pair of
// per-leg demands spells the old both-sides rule without a case for the shape.
// A direct flip therefore never carries Leg.DepletedSide — a leg's opposite side
// is the other leg's executing side, and both had to be non-empty to get here.
// Everything else the two legs observe is identical: one hour of one market, and
// only the end of the spread they execute on differs, which is read from action.
func directCandidates(rows []Row, cfg Config) []candidate {
	candidates := make([]candidate, 0, len(rows))
	for _, r := range rows {
		item, quote := orient(r, cfg.QuotePriority)

		buy, ok := gatedLeg("buy", item, quote, r, cfg)
		if !ok {
			continue
		}
		sell, ok := gatedLeg("sell", item, quote, r, cfg)
		if !ok {
			continue
		}

		candidates = append(candidates, candidate{
			key:  "direct:" + r.MarketID,
			mode: ModeDirect,
			legs: []candidateLeg{buy, sell},
			edge: buy.obs.high.price/buy.obs.low.price - 1,
		})
	}
	return candidates
}

// gatedLeg builds one leg of a play from the row that would execute it, and
// reports whether that row priced the pair and was alive enough to count.
//
// The gate is liveness, not liquidity: the hour must have traded at least
// Config.MinVolumePerHour units of the leg's item on that market, and the side
// the leg EXECUTES AGAINST must have carried stock. At the default of 1 the
// volume half reads as "a trade happened here this hour", which is the weakest
// true statement about the leg and deliberately so — the old floor of 10 dropped
// a live card market in 11 of 24 measured hours (DefaultConfig, and the
// measurement itself in
// docs/adr/017-no-default-engine-floor-may-hide-a-live-market.md). Liquidity is
// judged later, in chaos, on the play's Turnover — unit volume alone does not
// predict a real edge. A leg
// failing this kills the whole play — a recipe is only as executable as its
// thinnest step — so both directCandidates and crossQuoteCandidates drop the
// candidate on the first false.
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
func gatedLeg(action, item, quote string, r Row, cfg Config) (candidateLeg, bool) {
	low, high, ok := priceIn(r, item, quote)
	if !ok {
		return candidateLeg{}, false
	}

	volume := volumeOf(r, item)
	if float64(volume) < cfg.MinVolumePerHour {
		return candidateLeg{}, false
	}

	executes, opposite := stockOf(r, item), stockOf(r, quote)
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
	vwap, vwapOK := vwapIn(r, item, quote)

	return candidateLeg{
		action: action,
		item:   item,
		quote:  quote,
		obs: obs{
			low:          low,
			high:         high,
			vwap:         vwap,
			vwapOK:       vwapOK,
			tick:         tickOf(r),
			quoteVolume:  float64(quoteVolumeOf(r, quote)),
			volume:       float64(volume),
			stock:        executes,
			depletedSide: opposite <= 0,
		},
	}, true
}
