package trade

import (
	"strconv"
	"strings"

	"profitofexile/internal/price/resolve"
)

// ListingsInMarket keeps the listings that belong to the key's market.
//
// The rule mirrors buildSearchQuery's market definition (client.go): the
// corruption state must match, the level is pinned only at 20 and above, and
// the quality only at exactly 20. A change to the search's market must change
// this filter too (accepted coupling; POE-295 owner question Q6).
func ListingsInMarket(key resolve.Key, listings []TradeListingDetail) []TradeListingDetail {
	level, quality := levelQuality(key.Variant)
	kept := make([]TradeListingDetail, 0, len(listings))
	for _, l := range listings {
		if l.Corrupted != key.Corrupted {
			continue
		}
		if level >= 20 && l.GemLevel != level {
			continue
		}
		if quality == 20 && l.GemQuality != 20 {
			continue
		}
		kept = append(kept, l)
	}
	return kept
}

// Observations turns stored trade lookups into one trade observation per key,
// carrying each in-market listing's native amount and currency. The stored
// converted columns (price floor, medians, divine rate) are never read (L13).
// Rows that map to the same key keep the newest FetchedAt.
func Observations(league string, rows []TradeLookupResult) []resolve.Observation {
	index := make(map[resolve.Key]int, len(rows))
	out := make([]resolve.Observation, 0, len(rows))
	for _, row := range rows {
		key := resolve.GemKey(league, row.Gem, row.Variant)
		if i, seen := index[key]; seen && !row.FetchedAt.After(out[i].At) {
			continue
		}
		listings := ListingsInMarket(key, row.Listings)
		prices := make([]resolve.Money, 0, len(listings))
		for _, l := range listings {
			prices = append(prices, resolve.Money{Amount: l.Price, Currency: l.Currency})
		}
		obs := resolve.Observation{Key: key, Source: resolve.SourceTrade, At: row.FetchedAt, Prices: prices}
		if i, seen := index[key]; seen {
			out[i] = obs
			continue
		}
		index[key] = len(out)
		out = append(out, obs)
	}
	return out
}

// levelQuality reads "level" or "level/quality" from a normalized variant.
// An unreadable part reads as 0, which pins nothing in ListingsInMarket.
func levelQuality(variant string) (level, quality int) {
	levelPart, qualityPart, _ := strings.Cut(variant, "/")
	level, _ = strconv.Atoi(levelPart)
	quality, _ = strconv.Atoi(qualityPart)
	return level, quality
}
