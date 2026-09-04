package exchange

import (
	"math"
	"sort"
	"time"
)

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

// windowView is what ONE scored hour may look back over: every market's rows
// from the span BestPlays already holds, indexed by market id with each market's
// rows ordered newest hour first, plus the hour doing the looking.
//
// It adds NO query. byMarket is built once per BestPlays call out of the same
// grouped rows the per-hour loop walks, so the engine still reads the hypertable
// exactly once per recompute (ADR-016's last consequence); only hour changes as
// the loop moves back through the span.
//
// The zero value is a view with no history at all, under which windowPriceIn
// never returns ok and every leg prices from its own hour. That is what the
// per-hour unit tests pass, and it is why a caller that has no span in hand
// cannot accidentally serve a window price.
type windowView struct {
	hour     time.Time
	byMarket map[string][]StoredRow
}

// rowsFor returns one market's span rows, newest hour first, or nil when the
// span carries none.
//
// The empty id is never a market: crossQuoteCandidates reaches here with a zero
// Row when a triangle's third market did not trade in the scored hour, and an
// index that happened to hold a row under "" would hand that route somebody
// else's history.
func (v windowView) rowsFor(marketID string) []StoredRow {
	if marketID == "" {
		return nil
	}
	return v.byMarket[marketID]
}

// markets returns every market id the span carries, ascending.
//
// It is what directCandidates enumerates since POE-252's liveness change: a
// market with no row in the scored hour is still a market the reader can act on
// when the window behind it priced, so the hour's own rows are no longer the
// whole list of candidates. Sorted because that list must not depend on the
// order storage returned rows in — the property
// TestCorpus_shuffledFeedOrder_producesTheSameWireBytes holds the engine to.
//
// It is derived from byMarket on every call rather than stored beside it, so a
// view built by hand in a test cannot carry a market list that disagrees with
// its own index. The zero view has no markets, which is why the per-hour unit
// tests see the pre-POE-252 enumeration.
func (v windowView) markets() []string {
	return sortedIDs(v.byMarket)
}

// windowHistoryOf indexes stored rows by market id, newest hour first.
//
// The sort is done here rather than trusted from the caller, so a window's
// answer cannot depend on the order storage happened to return rows in — the
// same property TestCorpus_shuffledFeedOrder_producesTheSameWireBytes holds the
// whole engine to.
func windowHistoryOf(rows []StoredRow) map[string][]StoredRow {
	history := make(map[string][]StoredRow)
	for _, stored := range rows {
		history[stored.MarketID] = append(history[stored.MarketID], stored)
	}
	for _, market := range history {
		sort.SliceStable(market, func(i, j int) bool { return market[i].Hour.After(market[j].Hour) })
	}
	return history
}

// windowContributors returns the rows of one market that may price a trailing
// window ending at hour: the rows inside the CLOSED clock span
// [hour - (Config.WindowPriceHours-1)h, hour] that traded at least
// Config.MinVolumePerHour units of item and that priceIn can price.
//
// Three filters, each answering a different way a window could lie.
//
// The span is counted in CLOCK hours rather than in traded ones, so the oldest
// print a window can serve is WindowPriceHours-1 hours old whatever the market's
// gaps look like: a pair that traded six times in thirty hours is not priced off
// a thirty-hour-old print.
//
// The volume floor is what keeps a stale ratio out of the extremes. The feed
// republishes an hour nobody traded in with the market's LAST ratios rather than
// with nulls (Row carries plain int64 quantities and no nullable price), so
// letting an untraded row in would serve a price the window never printed. It is
// the same floor gatedLeg reads on the scored hour, so both readings of "this
// hour traded" are one number.
//
// A row priceIn refuses contributes neither an extreme nor its volume: the
// window's summed volume is what the extremes were drawn from, not what the
// market did.
//
// One row per market-hour is what the feed publishes; a duplicated hour
// contributes once, and the winner is the first row of that hour that CLEARS
// THE FILTERS ABOVE rather than simply the first one seen — the check compares
// against the last row ADMITTED, so a duplicate whose first copy was dropped for
// trading nothing or for not pricing lets the next copy be considered. That is
// the rule crossQuoteCandidates already applies to a duplicated pair.
//
// The span is what caps the slice, not the market's history: at most one row per
// clock hour survives, so a six-hour window read out of a day of rows allocates
// six.
func windowContributors(rows []StoredRow, hour time.Time, item, quote string, cfg Config) []StoredRow {
	oldest := hour.Add(-time.Duration(cfg.WindowPriceHours-1) * time.Hour)
	capacity := len(rows)
	if cfg.WindowPriceHours >= 0 && cfg.WindowPriceHours < capacity {
		capacity = cfg.WindowPriceHours
	}
	contributors := make([]StoredRow, 0, capacity)
	for _, stored := range rows {
		at := stored.Hour.UTC()
		if at.After(hour) || at.Before(oldest) {
			continue
		}
		if len(contributors) > 0 && contributors[len(contributors)-1].Hour.UTC().Equal(at) {
			continue
		}
		if float64(volumeOf(stored.Row, item)) < cfg.MinVolumePerHour {
			continue
		}
		if _, _, ok := priceIn(stored.Row, item, quote); !ok {
			continue
		}
		contributors = append(contributors, stored)
	}
	return contributors
}

// windowPriceIn returns the cheapest and the dearest price of one item in quote
// units REALIZED anywhere in the trailing window ending at hour, the rows that
// contributed one, and the item-side volume those rows traded between them.
//
// The contributing ROWS come back rather than a count of them because the leg
// needs both readings of one window — the extremes and the pooled anchor those
// extremes are judged against (windowVwapOf) — and selecting the window twice
// would be two answers to "which rows priced this hour". The count the wire
// carries is len(contributors).
//
// It is the answer to a thin hour, and it is a min/max over per-ROW priceIn
// results rather than a second reading of the stored columns: orientation stays
// in priceIn, which already swaps which stored pair is the low and which the
// high when the direction is reversed, so the reverse direction of a window is
// the reciprocal interval with the pairs transposed and nothing here knows that.
//
// The two points it returns are WHOLE pricePoints lifted out of the rows that
// printed them — never a price synthesized across hours. The integer pair is
// what a reader types into the game and what
// docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md sizes the whole row from, so a price
// without its own posting pair would break the row's arithmetic as well as its
// meaning.
//
// The two prices are realized extremes of DIFFERENT hours, which is one step
// further from a book than priceIn's own caveat: nobody was necessarily offering
// both at once, and here they were not even offered in the same hour. That is
// the trade-off the mark and its span disclose (Play.WindowPriced), and the
// clock span is what bounds it.
//
// ok is false on exactly two refusals: no row contributed, or the contributing
// rows traded less than Config.MinWindowVolume between them. priceIn's third
// refusal — a result that is not a usable interval — cannot fire here, because
// every contributor already cleared priceIn: the min of positive lows is
// positive, and the max of the highs is at or above it since each row's own high
// is at or above its own low.
func windowPriceIn(rows []StoredRow, hour time.Time, item, quote string, cfg Config) (low, high pricePoint, contributors []StoredRow, volume float64, ok bool) {
	contributors = windowContributors(rows, hour, item, quote, cfg)
	if len(contributors) == 0 {
		return pricePoint{}, pricePoint{}, nil, 0, false
	}

	for i, stored := range contributors {
		rowLow, rowHigh, priced := priceIn(stored.Row, item, quote)
		if !priced {
			continue
		}
		if i == 0 || rowLow.price < low.price {
			low = rowLow
		}
		if i == 0 || rowHigh.price > high.price {
			high = rowHigh
		}
		volume += float64(volumeOf(stored.Row, item))
	}

	if volume < cfg.MinWindowVolume {
		return pricePoint{}, pricePoint{}, nil, 0, false
	}
	return low, high, contributors, volume, true
}

// windowVwapOf returns the POOLED volume-weighted price of one item in quote
// units over the rows a window contributed — the quote units they traded over
// the item units they traded — or ok == false when either sum is not positive.
//
// It takes windowPriceIn's contributors rather than the span and the hour, so
// the window is selected once per leg: it is vwapIn's statement read over the
// SAME rows the extremes were drawn from, and re-deriving that set here would
// let the anchor and the extremes disagree about which hours priced.
//
// Pooled rather than an average of the hours' own volume-weighted prices,
// because a one-card hour would otherwise outvote a thousand-card one. That is
// what makes it the anchor a window-priced leg is JUDGED against (Leg.Fair, and
// the suspect bands that compare to it); anchoring a window's extremes to a
// single hour's mass would compare two different windows.
func windowVwapOf(contributors []StoredRow, item, quote string) (float64, bool) {
	itemVolume, quoteVolume := int64(0), int64(0)
	for _, stored := range contributors {
		itemVolume += volumeOf(stored.Row, item)
		quoteVolume += quoteVolumeOf(stored.Row, quote)
	}
	if itemVolume <= 0 || quoteVolume <= 0 {
		return 0, false
	}
	return float64(quoteVolume) / float64(itemVolume), true
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
