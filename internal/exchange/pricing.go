package exchange

// DivineID and ChaosID are the feed ids of the two currencies every liquid
// market is quoted against, and the default Config.QuotePriority in that order.
//
// They are the item ids exactly as the feed spells them, not display names;
// Humanize turns them into "Mod Values" and "Reroll Rare".
const (
	DivineID = "Metadata/Items/Currency/CurrencyModValues"
	ChaosID  = "Metadata/Items/Currency/CurrencyRerollRare"
)

// priceIn returns the cheapest and the dearest realized price of one item in
// quote units on the row, or ok == false when the row cannot price that pair.
//
// It is the one place in the engine that decides which stored quantity pair
// belongs to which direction, so no other engine code touches LowestRatio* /
// HighestRatio* or the precomputed float prices. The feed orients both ratio
// pairs as the price of one ItemB in ItemA units, so the reverse direction
// swaps which pair is the low and which is the high (see the Ratio doc):
//
//	item == ItemB, quote == ItemA -> low  = Ratio(LowestRatioA, LowestRatioB)
//	                                 high = Ratio(HighestRatioA, HighestRatioB)
//	item == ItemA, quote == ItemB -> low  = Ratio(HighestRatioB, HighestRatioA)
//	                                 high = Ratio(LowestRatioB, LowestRatioA)
//
// The low and the high are realized extremes of the hour's trades, not two
// sides of a book that existed at the same instant: nobody was necessarily
// offering both at once. Every edge built on them is therefore an upper bound.
//
// ok is false when the row is not that pair, when PriceValid is false, when
// either Ratio rejects its quantities, or when the result is not a usable
// interval (low <= 0 or high < low). Prices never come from 1/price on a stored
// float, which would be a division by zero on an unpriced row.
func priceIn(r Row, item, quote string) (low, high float64, ok bool) {
	if !r.PriceValid {
		return 0, 0, false
	}

	var lowOK, highOK bool
	switch {
	case r.ItemB == item && r.ItemA == quote:
		low, lowOK = Ratio(r.LowestRatioA, r.LowestRatioB)
		high, highOK = Ratio(r.HighestRatioA, r.HighestRatioB)
	case r.ItemA == item && r.ItemB == quote:
		low, lowOK = Ratio(r.HighestRatioB, r.HighestRatioA)
		high, highOK = Ratio(r.LowestRatioB, r.LowestRatioA)
	default:
		return 0, 0, false
	}

	if !lowOK || !highOK || low <= 0 || high < low {
		return 0, 0, false
	}
	return low, high, true
}

// volumeOf returns the units of item traded on the row during its hour, or 0
// when the row does not carry that item. It is the depth signal every leg gate
// is measured against.
func volumeOf(r Row, item string) int64 {
	switch item {
	case r.ItemA:
		return r.VolumeA
	case r.ItemB:
		return r.VolumeB
	default:
		return 0
	}
}

// stockOf returns the row's HighestStock for item, or 0 when the row does not
// carry that item. The highest stock is used rather than the lowest because the
// gate asks whether the side was offered at all during the hour, not how thin
// it got.
func stockOf(r Row, item string) int64 {
	switch item {
	case r.ItemA:
		return r.HighestStockA
	case r.ItemB:
		return r.HighestStockB
	default:
		return 0
	}
}

// orient picks which side of a market is the traded item and which is the
// currency it is priced in.
//
// The quote is the side that appears earliest in priority — with the default
// [DivineID, ChaosID] a divine/chaos market quotes in divine, and anything
// quoted against chaos keeps chaos as the quote. When neither side appears in
// priority the market has no preferred currency and ItemB is the quote, which
// is the orientation the feed's own prices already carry.
//
// Edges do not depend on this choice (high/low is the same ratio either way);
// it decides how a play reads to a human and which side the volume gate is
// applied to.
func orient(r Row, priority []string) (item, quote string) {
	rankA := priorityRank(r.ItemA, priority)
	rankB := priorityRank(r.ItemB, priority)
	if rankA >= 0 && (rankB < 0 || rankA < rankB) {
		return r.ItemB, r.ItemA
	}
	return r.ItemA, r.ItemB
}

// priorityRank returns the position of id in priority, or -1 when it is absent.
func priorityRank(id string, priority []string) int {
	for i, candidate := range priority {
		if candidate == id {
			return i
		}
	}
	return -1
}
