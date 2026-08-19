package exchange

// obs is what ONE hour observed about one leg's market.
//
// It is the unit the aggregator takes medians of, so every number a Play shows
// traces back to a slice of these. low and high are the hour's cheapest and
// dearest realized price of the leg's item in its quote (priceIn); vwap is the
// price its traded mass actually cleared at (vwapIn), and vwapOK says whether
// the hour had one at all — an hour whose quote side reported no volume carries
// vwap 0, which is a missing reading rather than a price of zero, and the
// aggregator must leave it out instead of averaging it in; tick is the coarsest
// step the market's quantity pairs can express (tickOf); quoteVolume and volume
// are the two sides' traded units; stock is liveness only — lowest/highest stock
// are the hour's min and max of total book size and say nothing about the
// extreme (corr <= 0.13 against the edge), so nothing scores on them.
type obs struct {
	low         float64
	high        float64
	vwap        float64
	vwapOK      bool
	tick        float64
	quoteVolume float64
	volume      float64
	stock       int64
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
// sides of a book, so this hour's edge is the optimistic reading (see priceIn);
// it survives onto the Play as RoiPctNewestHour, while what the Play ranks on is
// the cross-hour median. It is also orientation-independent: pricing the market
// the other way round inverts both prices and leaves the ratio unchanged.
//
// A row contributes only when priceIn can price it and the traded side is alive
// — at least Config.MinVolumePerHour units traded and stock on both sides of the
// market.
func directCandidates(rows []Row, cfg Config) []candidate {
	candidates := make([]candidate, 0, len(rows))
	for _, r := range rows {
		item, quote := orient(r, cfg.QuotePriority)

		buy, ok := gatedLeg("buy", item, quote, r, cfg)
		if !ok {
			continue
		}
		// Both sides of a flip are the same market in the same hour, so the
		// sell leg observes exactly what the buy leg did; only the side of the
		// spread it executes on differs, and that is read from action.
		sell := buy
		sell.action = "sell"

		candidates = append(candidates, candidate{
			key:  "direct:" + r.MarketID,
			mode: ModeDirect,
			legs: []candidateLeg{buy, sell},
			edge: buy.obs.high/buy.obs.low - 1,
		})
	}
	return candidates
}

// gatedLeg builds one leg of a play from the row that would execute it, and
// reports whether that row priced the pair and was alive enough to count.
//
// The gate is liveness, not liquidity: the hour must have traded at least
// Config.MinVolumePerHour units of the leg's item on that market, and both sides
// of the market must have carried stock. Liquidity is judged later, in chaos, on
// the play's Turnover — unit volume alone does not predict a real edge. A leg
// failing this kills the whole play — a recipe is only as executable as its
// thinnest step — so both directCandidates and crossQuoteCandidates drop the
// candidate on the first false.
func gatedLeg(action, item, quote string, r Row, cfg Config) (candidateLeg, bool) {
	low, high, ok := priceIn(r, item, quote)
	if !ok {
		return candidateLeg{}, false
	}

	volume := volumeOf(r, item)
	if float64(volume) < cfg.MinVolumePerHour || stockOf(r, item) <= 0 || stockOf(r, quote) <= 0 {
		return candidateLeg{}, false
	}

	// A row that traded MinVolumePerHour units has a usable vwap unless the
	// quote side reported nothing; that hour then contributes no fair anchor and
	// no turnover, and vwapOK keeps it out of the leg's Fair median rather than
	// pulling that median toward zero.
	vwap, vwapOK := vwapIn(r, item, quote)

	return candidateLeg{
		action: action,
		item:   item,
		quote:  quote,
		obs: obs{
			low:         low,
			high:        high,
			vwap:        vwap,
			vwapOK:      vwapOK,
			tick:        tickOf(r),
			quoteVolume: float64(quoteVolumeOf(r, quote)),
			volume:      float64(volume),
			stock:       stockOf(r, item),
		},
	}, true
}
