package exchange

// candidate is one play as it was observed in ONE hour: the recipe plus that
// hour's edge, prices, volumes and stocks. BestPlays aggregates candidates
// sharing a key across the hours of its window into a Play.
//
// hour is not a field: the aggregator always knows which hour's rows it passed
// to the unit that produced the candidate.
type candidate struct {
	key  string
	mode Mode
	legs []Leg
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
// sides of a book, so the edge is an upper bound on what was actually takeable
// (see priceIn). It is also orientation-independent: pricing the market the
// other way round inverts both prices and leaves the ratio unchanged.
//
// A row contributes only when priceIn can price it and the traded side is deep
// enough to matter — at least Config.MinVolumePerHour units traded and stock on
// both sides of the market.
func directCandidates(rows []Row, cfg Config) []candidate {
	candidates := make([]candidate, 0, len(rows))
	for _, r := range rows {
		item, quote := orient(r, cfg.QuotePriority)
		low, high, ok := priceIn(r, item, quote)
		if !ok {
			continue
		}

		buy, ok := gatedLeg("buy", item, quote, low, r, cfg)
		if !ok {
			continue
		}
		sell := buy
		sell.Action = "sell"
		sell.Price = high

		candidates = append(candidates, candidate{
			key:  "direct:" + r.MarketID,
			mode: ModeDirect,
			legs: []Leg{buy, sell},
			edge: high/low - 1,
		})
	}
	return candidates
}

// gatedLeg builds one leg of a play from the row that would execute it, and
// reports whether that row is deep enough to count.
//
// The gate is depth, not price: the hour must have traded at least
// Config.MinVolumePerHour units of the leg's item on that market, and both sides
// of the market must have carried stock. A leg failing it kills the whole play —
// a recipe is only as executable as its thinnest step — so both directCandidates
// and crossQuoteCandidates drop the candidate on the first false.
func gatedLeg(action, item, quote string, price float64, r Row, cfg Config) (Leg, bool) {
	volume := volumeOf(r, item)
	if float64(volume) < cfg.MinVolumePerHour || stockOf(r, item) <= 0 || stockOf(r, quote) <= 0 {
		return Leg{}, false
	}
	return Leg{
		Action: action,
		Item:   item,
		Quote:  quote,
		Price:  price,
		Volume: float64(volume),
		Stock:  stockOf(r, item),
	}, true
}
