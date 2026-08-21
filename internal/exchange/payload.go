package exchange

// HourPayload is the raw feed response for one hour.
//
// NextChangeID is the feed's cursor, not the hour the payload describes:
// requesting hour 1787119200 answered next_change_id 1787122800, the requested
// hour plus 3600 (observed 2026-08-19). On a 404 it is instead the current
// unpublished hour, which is the hour to retry — that is the value
// ErrNotPublished carries.
type HourPayload struct {
	NextChangeID int64    `json:"next_change_id"`
	Markets      []Market `json:"markets"`
}

// Market is one raw currency-exchange market for one hour, exactly as the feed
// sends it.
//
// MarketPair holds the two item ids (full metadata paths such as
// Metadata/Items/Currency/CurrencyRerollRare) and every map is keyed by those
// ids. All quantities arrive as JSON integers.
//
// The ratio maps are quantity pairs, not prices: LowestRatio
// {CurrencyRerollRare: 196, CurrencyModValues: 1} means 196 chaos buys 1 divine.
// Neither side is guaranteed to be a unit — the feed stores the reduced pair,
// and (5, 43) is a real stored ratio — and which key is which is never a fixed
// MarketPair position. Use Normalize or PriceOf rather than dividing here.
type Market struct {
	League       string           `json:"league"`
	MarketID     string           `json:"market_id"`
	MarketPair   []string         `json:"market_pair"`
	VolumeTraded map[string]int64 `json:"volume_traded"`
	LowestStock  map[string]int64 `json:"lowest_stock"`
	HighestStock map[string]int64 `json:"highest_stock"`
	LowestRatio  map[string]int64 `json:"lowest_ratio"`
	HighestRatio map[string]int64 `json:"highest_ratio"`
}
