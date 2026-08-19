package exchange

import "log/slog"

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
type Stats struct {
	Rows    int
	Invalid int
	Skipped int
	Leagues map[string]int
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
func Normalize(p *HourPayload) ([]Row, Stats) {
	stats := Stats{Leagues: make(map[string]int)}
	if p == nil || len(p.Markets) == 0 {
		return nil, stats
	}

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
	return rows, stats
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
