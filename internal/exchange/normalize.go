package exchange

import (
	"fmt"
	"log/slog"
)

// reasonZeroRatio marks a row whose ratio quantities cannot produce a price.
const reasonZeroRatio = "zero_ratio"

// Row is one normalized market for one hour.
//
// ItemA and ItemB follow market_pair order, and every A/B field is keyed the
// same way. LowestPriceBInA and HighestPriceBInA are the price of one ItemB in
// units of ItemA (LowestRatioA/LowestRatioB and HighestRatioA/HighestRatioB).
// Both prices are 0 when PriceValid is false; volume and stock stay usable on
// such a row.
//
// The feed's two ratio maps are oriented as the lowest and the highest price of
// one ItemB in ItemA units: LowestPriceBInA <= HighestPriceBInA held on all 23
// priced rows of the recorded fixture hour (0 violations). Deriving the reverse
// direction therefore swaps lowest and highest — the lowest price of one ItemA
// in ItemB units comes from the HIGHEST ratio pair, via
// Ratio(row.HighestRatioB, row.HighestRatioA).
type Row struct {
	League   string
	MarketID string
	ItemA    string
	ItemB    string

	VolumeA int64
	VolumeB int64

	LowestStockA  int64
	LowestStockB  int64
	HighestStockA int64
	HighestStockB int64

	LowestRatioA  int64
	LowestRatioB  int64
	HighestRatioA int64
	HighestRatioB int64

	// PriceValid is false when a ratio quantity was missing or non-positive.
	PriceValid bool
	// InvalidReason explains a false PriceValid; empty when PriceValid is true.
	InvalidReason string

	LowestPriceBInA  float64
	HighestPriceBInA float64
}

// Stats summarizes one Normalize call.
//
// Rows is the number of returned rows, Invalid the subset kept with
// PriceValid == false, Skipped the malformed markets dropped entirely, and
// Leagues the kept-row count per league (no league filtering happens here).
//
// NonReduced counts PAIRS, not rows: a kept row carries a lowest and a highest
// ratio pair, each judged on its own, so one row contributes 0, 1 or 2. It
// reports feed drift rather than pricing eligibility — a row whose other pair
// made it Invalid still has its other pair counted — and nothing is rewritten
// when it fires (see isReduced).
type Stats struct {
	Rows       int
	Invalid    int
	Skipped    int
	NonReduced int
	Leagues    map[string]int
}

// Normalize turns a raw hour payload into rows and per-hour counters.
//
// This is the one place prices are derived from the feed's quantity pairs;
// callers store or score the returned rows and never recompute prices.
//
// A malformed market (market_pair length other than 2, empty market_id, or a
// pair id missing from any of the five quantity maps) is skipped, counted in
// Stats.Skipped and reported once per hour with slog.Warn: one bad row never
// fails an hour.
//
// A market whose ratio quantities are not all positive is kept with
// PriceValid == false, InvalidReason == "zero_ratio" and both prices 0, and is
// counted in Stats.Invalid. One flag covers both ratio maps deliberately: in the
// live payload for hour 1787119200 all 209 zero-ratio Allflame rows carried
// zeros in both maps, so a partial zero is treated conservatively as invalid.
//
// Every quantity pair a kept row carries is also checked against the feed's
// promise that it publishes pairs in lowest terms: a pair that is not is
// counted in Stats.NonReduced and reported once per hour with slog.Warn. The
// pair is stored exactly as the feed sent it either way — the counter is a
// drift alarm, not a repair (isReduced; pricePoint in pricing.go).
func Normalize(p *HourPayload) ([]Row, Stats) {
	stats := Stats{Leagues: make(map[string]int)}
	if p == nil || len(p.Markets) == 0 {
		return nil, stats
	}

	// First offending pair, for the once-per-hour warn.
	var firstNonReducedMarket, firstNonReducedPair, firstNonReducedSide string

	rows := make([]Row, 0, len(p.Markets))
	for i := range p.Markets {
		m := &p.Markets[i]
		if !wellFormed(m) {
			stats.Skipped++
			continue
		}

		itemA, itemB := m.MarketPair[0], m.MarketPair[1]
		row := Row{
			League:        m.League,
			MarketID:      m.MarketID,
			ItemA:         itemA,
			ItemB:         itemB,
			VolumeA:       m.VolumeTraded[itemA],
			VolumeB:       m.VolumeTraded[itemB],
			LowestStockA:  m.LowestStock[itemA],
			LowestStockB:  m.LowestStock[itemB],
			HighestStockA: m.HighestStock[itemA],
			HighestStockB: m.HighestStock[itemB],
			LowestRatioA:  m.LowestRatio[itemA],
			LowestRatioB:  m.LowestRatio[itemB],
			HighestRatioA: m.HighestRatio[itemA],
			HighestRatioB: m.HighestRatio[itemB],
		}

		// Both pairs are judged, independently of each other and of whether the
		// row prices: the counter reports what the feed sent, not what the
		// engine can use.
		for side, pair := range [2][2]int64{
			{row.LowestRatioA, row.LowestRatioB},
			{row.HighestRatioA, row.HighestRatioB},
		} {
			if isReduced(pair[0], pair[1]) {
				continue
			}
			stats.NonReduced++
			if stats.NonReduced == 1 {
				firstNonReducedMarket = row.MarketID
				firstNonReducedPair = fmt.Sprintf("%d/%d", pair[0], pair[1])
				if side == 0 {
					firstNonReducedSide = "lowest"
				} else {
					firstNonReducedSide = "highest"
				}
			}
		}

		lowest, lowestOK := PriceOf(m.LowestRatio, itemB, itemA)
		highest, highestOK := PriceOf(m.HighestRatio, itemB, itemA)
		if lowestOK && highestOK {
			row.PriceValid = true
			row.LowestPriceBInA = lowest
			row.HighestPriceBInA = highest
		} else {
			row.InvalidReason = reasonZeroRatio
			stats.Invalid++
		}

		rows = append(rows, row)
		stats.Leagues[m.League]++
	}

	stats.Rows = len(rows)
	if stats.Skipped > 0 {
		slog.Warn("currency-exchange: skipped malformed markets",
			"next_change_id", p.NextChangeID, "skipped", stats.Skipped, "markets", len(p.Markets))
	}
	if stats.NonReduced > 0 {
		slog.Warn("currency-exchange: non-reduced quantity pairs in feed",
			"next_change_id", p.NextChangeID, "non_reduced", stats.NonReduced,
			"first_market_id", firstNonReducedMarket, "first_pair", firstNonReducedPair,
			"first_pair_side", firstNonReducedSide)
	}
	return rows, stats
}

// isReduced reports whether a feed quantity pair is already in lowest terms,
// i.e. gcd(a, b) == 1.
//
// It DETECTS and never rewrites: the pair Normalize stores is the feed's own,
// because the engine reads a pair exactly once and reducing it here would be
// the second source of truth the package refuses — see the pricePoint doc in
// pricing.go, and tickOf, which has read the stored pair as reduced since
// POE-184.
//
// A pair with either side non-positive reads as reduced because it cannot be
// judged: gcd(0, n) == n would report every zero-ratio pair as feed drift, and
// such a pair is already reported as Stats.Invalid with reasonZeroRatio.
func isReduced(a, b int64) bool {
	if a <= 0 || b <= 0 {
		return true
	}
	for b != 0 {
		a, b = b, a%b
	}
	return a == 1
}

// Ratio prices one item in quote units from a pair of feed quantities: it
// returns quoteQty/itemQty, or 0 and false when either quantity is not positive
// rather than dividing by zero.
//
// This is the single quantity-based guard in the package. Engines call it on the
// quantities persisted on a Row instead of dividing themselves, which is how
// they get a direction Normalize did not precompute:
// Ratio(row.LowestRatioB, row.LowestRatioA) is the price of one ItemA in ItemB
// units derived from the lowest ratio pair. Because the feed's lowest/highest
// ratios are oriented as the price of one ItemB in ItemA units (observed: 0
// violations over the 23 priced rows of the recorded fixture hour), that reverse
// direction inverts the ordering too — the LOWEST price of one ItemA in ItemB
// units comes from the row's highest ratio pair.
func Ratio(quoteQty, itemQty int64) (float64, bool) {
	if itemQty <= 0 || quoteQty <= 0 {
		return 0, false
	}
	return float64(quoteQty) / float64(itemQty), true
}

// PriceOf looks both quantities up in one of the feed's ratio maps and hands
// them to Ratio, so a key the map does not carry reads as the unusable
// quantity 0.
//
// It returns the price of one item in quote units (ratio[quote] / ratio[item]),
// or 0 and false when either quantity is missing or not positive. Normalize uses
// it to price a raw Market; a caller holding a stored Row calls Ratio directly.
// The lowest/highest orientation note on Ratio applies here too.
func PriceOf(ratio map[string]int64, item, quote string) (float64, bool) {
	return Ratio(ratio[quote], ratio[item])
}

// wellFormed reports whether a market carries the identifiers and quantity map
// entries Normalize needs. It does not judge the quantities themselves.
func wellFormed(m *Market) bool {
	if m.MarketID == "" || len(m.MarketPair) != 2 {
		return false
	}
	maps := []map[string]int64{
		m.VolumeTraded,
		m.LowestStock,
		m.HighestStock,
		m.LowestRatio,
		m.HighestRatio,
	}
	for _, id := range m.MarketPair {
		for _, quantities := range maps {
			if _, ok := quantities[id]; !ok {
				return false
			}
		}
	}
	return true
}
