package exchange

import "math"

// DivineID and ChaosID are the feed ids of the two currencies every liquid
// market is quoted against, and the default Config.QuotePriority in that order.
//
// They are the item ids exactly as the feed spells them, not display names;
// Humanize turns them into "Mod Values" and "Reroll Rare".
const (
	DivineID = "Metadata/Items/Currency/CurrencyModValues"
	ChaosID  = "Metadata/Items/Currency/CurrencyRerollRare"
)

// pricePoint is one realized extreme of an hour on one direction of a market:
// the price, and the integer quantity pair the feed posted it as.
//
// The pair is what the in-game Currency Exchange actually trades in. The game
// posts whole quantities on both sides and nothing else, so "sell 4 for 1
// divine" is an order a player can place and "sell at 0.25 divine each" is not
// — the same fact tickOf reads the other way round when it calls 1/max(x, y)
// the market's smallest representable step. price is the pair divided out, for
// comparing and ranking; the pair is what a reader types into the game.
//
// price is derived FROM the pair by Ratio, in pointOf and nowhere else, so
// Ratio(quoteQty, itemQty) == price holds by construction rather than by two
// code paths agreeing.
//
// The quantities are the FEED's own, oriented to the direction asked for
// (itemQty counts the item, quoteQty the quote), and they are NOT reduced here
// because the feed already publishes them reduced: over the 91,520 priced
// market-hours stored on 2026-08-22 (2026-08-18 16:00 UTC onward) every lowest
// and highest ratio pair had gcd(a, b) == 1, 0 violations. Reducing here anyway
// would also put this file out of step with tickOf, which has read the stored
// pair as reduced since POE-184 — 1/max(x, y) is the true step only on a
// reduced pair — and two readings of one pair is exactly the second source of
// truth the package refuses elsewhere. Drift in that assumption is loud rather
// than silent: Normalize counts every non-reduced pair it stores in
// Stats.NonReduced and warns once per hour, without reducing anything (POE-197).
type pricePoint struct {
	price    float64
	itemQty  int64
	quoteQty int64
}

// priceIn returns the cheapest and the dearest realized price of one item in
// quote units on the row, each with the quantity pair it was posted as, or
// ok == false when the row cannot price that pair.
//
// It is the one place in the engine that decides which stored quantity pair
// belongs to which direction, so no other engine code touches LowestRatio* /
// HighestRatio* or the precomputed float prices. The feed orients both ratio
// pairs as the price of one ItemB in ItemA units, so the reverse direction
// swaps which pair is the low and which is the high (see the Ratio doc):
//
//	item == ItemB, quote == ItemA -> low  = (quote LowestRatioA,  item LowestRatioB)
//	                                 high = (quote HighestRatioA, item HighestRatioB)
//	item == ItemA, quote == ItemB -> low  = (quote HighestRatioB, item HighestRatioA)
//	                                 high = (quote LowestRatioB,  item LowestRatioA)
//
// Which of the four stored quantities is the ITEM side and which the QUOTE side
// is therefore a property of the direction, not of the row: the same market read
// with the two sides swapped hands back the reciprocal price and the transposed
// pair. Config.QuotePriority is what picks the direction (orient), so flipping
// it flips both.
//
// The low and the high are realized extremes of the hour's trades, not two
// sides of a book that existed at the same instant: nobody was necessarily
// offering both at once. A per-hour edge built on them is the hour's optimistic
// reading — both extremes were realized, not necessarily takeable together.
//
// ok is false when the row is not that pair, when PriceValid is false, when
// either Ratio rejects its quantities, or when the result is not a usable
// interval (low <= 0 or high < low). Prices never come from 1/price on a stored
// float, which would be a division by zero on an unpriced row.
func priceIn(r Row, item, quote string) (low, high pricePoint, ok bool) {
	if !r.PriceValid {
		return pricePoint{}, pricePoint{}, false
	}

	var lowOK, highOK bool
	switch {
	case r.ItemB == item && r.ItemA == quote:
		low, lowOK = pointOf(r.LowestRatioA, r.LowestRatioB)
		high, highOK = pointOf(r.HighestRatioA, r.HighestRatioB)
	case r.ItemA == item && r.ItemB == quote:
		low, lowOK = pointOf(r.HighestRatioB, r.HighestRatioA)
		high, highOK = pointOf(r.LowestRatioB, r.LowestRatioA)
	default:
		return pricePoint{}, pricePoint{}, false
	}

	if !lowOK || !highOK || low.price <= 0 || high.price < low.price {
		return pricePoint{}, pricePoint{}, false
	}
	return low, high, true
}

// pointOf divides one quantity pair into its price and keeps both together.
//
// It is the only constructor of a pricePoint, which is what makes the pair and
// the price inseparable: there is no way to hand a caller a price whose pair
// says something else. A pair Ratio refuses (either quantity not positive)
// yields the zero point and false rather than a price of 0 or +Inf.
func pointOf(quoteQty, itemQty int64) (pricePoint, bool) {
	price, ok := Ratio(quoteQty, itemQty)
	if !ok {
		return pricePoint{}, false
	}
	return pricePoint{price: price, itemQty: itemQty, quoteQty: quoteQty}, true
}

// vwapIn returns the hour's realized volume-weighted average price of one item
// in quote units — the quote units traded divided by the item units traded — or
// ok == false when the row traded none of either side.
//
// The feed publishes volume_traded for BOTH sides of a market, so their ratio is
// the price the hour's mass actually changed hands at, as opposed to priceIn's
// two extremes. Measured over 30,534 priced Allflame market-hours it fell inside
// [low, high] on every market checked (Divine Vessel: 0.219c against a 0.0286 /
// 1.00 extreme pair), which makes it the fair-value anchor a play is judged
// against rather than a second spread.
//
// Like priceIn this is a direction mapper, so it lives here: nothing outside
// this file decides which stored volume belongs to which side.
func vwapIn(r Row, item, quote string) (float64, bool) {
	if item == quote {
		return 0, false
	}
	itemVolume, quoteVolume := volumeOf(r, item), volumeOf(r, quote)
	if itemVolume <= 0 || quoteVolume <= 0 {
		return 0, false
	}
	return float64(quoteVolume) / float64(itemVolume), true
}

// tickOf returns the coarsest price step the row can express, as a fraction of
// the price — 0.5 meaning the next representable price is 50% away.
//
// The feed quotes each side of a market as a reduced integer quantity pair, so
// on a pair (x, y) the smallest representable move is one unit of the larger
// quantity: 1/max(x, y). The row carries two pairs (the hour's lowest and its
// highest ratio) and the coarser of the two bounds everything derived from it.
// This is the consumer that a non-reduced pair would quietly mislead — it would
// understate the step — so Normalize counts such pairs in Stats.NonReduced and
// warns rather than reducing them under this function (POE-197).
//
// This is the single strongest predictor of an apparent spread: over 26 hours of
// Allflame, corr(ln edge, ln tick) = +0.42, median tick 14.3%, p75 50%. A 1:2
// market that also printed 1:1 shows a 100% "edge" that is one integer step, and
// the same item quoted in divine shows 100% where its chaos market shows 7%.
//
// A row whose quantities are not positive cannot resolve any price; it returns 0
// and is unreachable in practice because such a row has PriceValid == false and
// never reaches a leg.
func tickOf(r Row) float64 {
	lowest := maxQuantity(r.LowestRatioA, r.LowestRatioB)
	highest := maxQuantity(r.HighestRatioA, r.HighestRatioB)
	if lowest <= 0 || highest <= 0 {
		return 0
	}
	return math.Max(1/float64(lowest), 1/float64(highest))
}

// maxQuantity returns the larger of two stored ratio quantities.
func maxQuantity(a, b int64) int64 {
	if a > b {
		return a
	}
	return b
}

// quoteVolumeOf returns the units of the QUOTE currency that changed hands on
// the row during its hour.
//
// It is volumeOf read from the other side, named apart because the two answer
// different questions: item volume is the depth of the recipe (how many units a
// leg can absorb), quote volume priced in chaos is its liquidity (how much value
// flows through it per hour). Unit volume does not predict a real edge
// (corr(ln edge, ln units) = +0.06) while chaos-denominated turnover does
// (−0.30), which is why both readings exist.
func quoteVolumeOf(r Row, quote string) int64 {
	return volumeOf(r, quote)
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
