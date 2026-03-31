package trade

import (
	"math"
	"sort"
	"strings"
	"time"
)

// Priority controls queue ordering in the Gate.
type Priority int

const (
	PriorityHigh Priority = iota // interactive lookups
	PriorityLow                  // background scans
)

// TradeConfig holds all tunables for the trade subsystem.
type TradeConfig struct {
	Enabled           bool
	LeagueName        string
	CeilingFactor     float64       // fraction of reported rate limits to use (default: 0.65)
	LatencyPadding    time.Duration // added to rate limit windows for desync safety (default: 1s)
	DefaultSearchRate int           // conservative search req budget before real headers (default: 1 req/5s)
	DefaultFetchRate  int           // conservative fetch req budget before real headers (default: 1 req/2s)
	MaxQueueWait      time.Duration // max time a request can sit in queue before error (default: 30s)
	CacheMaxEntries   int           // LRU cache capacity (default: 200)
	UserAgent         string        // User-Agent header for GGG requests
	SyncWaitBudget    time.Duration // max time handler blocks for fast-path response (default: 500ms)
}

// GateRequest is submitted by the HTTP handler into the Gate's priority queue.
type GateRequest struct {
	Gem         string
	Variant     string
	RequestID   string
	Priority    Priority
	SubmittedAt time.Time
	Result      chan *GateResponse
}

// GateResponse carries the lookup result (or error) back to the handler.
type GateResponse struct {
	Data  *TradeLookupResult
	Error error
}

// TradeLookupResult is the API response for a single gem+variant trade lookup.
type TradeLookupResult struct {
	Gem          string               `json:"gem"`
	Variant      string               `json:"variant"`
	Total        int                  `json:"total"`
	PriceFloor   float64              `json:"priceFloor"`
	PriceCeiling float64              `json:"priceCeiling"`
	PriceSpread  float64              `json:"priceSpread"`
	MedianTop10  float64              `json:"medianTop10"`
	Listings     []TradeListingDetail `json:"listings"`
	Signals      TradeSignals         `json:"signals"`
	DivinePrice  float64              `json:"divinePrice"`  // divine→chaos rate used for normalization
	TradeURL     string               `json:"tradeUrl"`     // link to trade site results page
	FetchedAt    time.Time            `json:"fetchedAt"`
}

// TradeListingDetail holds one trade listing from the GGG fetch response.
type TradeListingDetail struct {
	Price      float64   `json:"price"`
	Currency   string    `json:"currency"`
	ChaosPrice float64   `json:"chaosPrice"` // normalized to chaos using divine rate
	Account    string    `json:"account"`
	IndexedAt  time.Time `json:"indexedAt"`
	GemLevel   int       `json:"gemLevel"`
	GemQuality int       `json:"gemQuality"`
	Corrupted  bool      `json:"corrupted"`
}

// TradeSignals are derived market health indicators computed from the top listings.
type TradeSignals struct {
	SellerConcentration string `json:"sellerConcentration"` // NORMAL, CONCENTRATED, MONOPOLY
	CheapestStaleness   string `json:"cheapestStaleness"`   // FRESH, AGING, STALE
	PriceOutlier        bool   `json:"priceOutlier"`        // cheapest < 50% of median top 10
	UniqueAccounts      int    `json:"uniqueAccounts"`      // distinct sellers in top 10
}

// SearchResponse is the parsed result of a GGG trade search call.
type SearchResponse struct {
	QueryID string
	IDs     []string
	Total   int
}

// CacheKey returns the canonical cache/dedup key for a gem+variant pair.
func CacheKey(gem, variant string) string {
	return gem + "|" + variant
}

// ParseCacheKey splits a cache key back into gem and variant.
func ParseCacheKey(key string) (gem, variant string) {
	parts := strings.SplitN(key, "|", 2)
	if len(parts) == 2 {
		return parts[0], parts[1]
	}
	return key, ""
}

// ComputeSignals derives market health signals from the top listings.
func ComputeSignals(listings []TradeListingDetail) TradeSignals {
	if len(listings) == 0 {
		return TradeSignals{
			SellerConcentration: "NORMAL",
			CheapestStaleness:   "FRESH",
			PriceOutlier:        false,
			UniqueAccounts:      0,
		}
	}

	// Count unique accounts.
	seen := make(map[string]struct{}, len(listings))
	for _, l := range listings {
		seen[l.Account] = struct{}{}
	}
	unique := len(seen)

	// Seller concentration.
	var concentration string
	switch {
	case unique >= 8:
		concentration = "NORMAL"
	case unique >= 5:
		concentration = "CONCENTRATED"
	default:
		concentration = "MONOPOLY"
	}

	// Cheapest listing staleness (listings are assumed price-sorted asc).
	age := time.Since(listings[0].IndexedAt)
	var staleness string
	switch {
	case age < time.Hour:
		staleness = "FRESH"
	case age < 6*time.Hour:
		staleness = "AGING"
	default:
		staleness = "STALE"
	}

	// Price outlier: cheapest < 50% of median of top 10 (using normalized chaos prices).
	median := medianChaosPrice(listings)
	outlier := listings[0].ChaosPrice < median*0.5

	return TradeSignals{
		SellerConcentration: concentration,
		CheapestStaleness:   staleness,
		PriceOutlier:        outlier,
		UniqueAccounts:      unique,
	}
}

// BuildResult assembles a TradeLookupResult from raw search + fetch data.
// NormalizeToChaos converts a listing price to chaos using the divine rate.
// Handles "divine" currency; everything else assumed to be chaos-equivalent.
func NormalizeToChaos(price float64, currency string, divineRate float64) float64 {
	if currency == "divine" && divineRate > 0 {
		return math.Round(price*divineRate*10) / 10 // round to 1 decimal
	}
	return price
}

func BuildResult(gem, variant, league string, sr SearchResponse, listings []TradeListingDetail, divineRate float64) *TradeLookupResult {
	// Normalize all listing prices to chaos.
	for i := range listings {
		listings[i].ChaosPrice = NormalizeToChaos(listings[i].Price, listings[i].Currency, divineRate)
	}

	// Re-sort by chaos price (divine listings may have shifted position).
	sort.Slice(listings, func(i, j int) bool {
		return listings[i].ChaosPrice < listings[j].ChaosPrice
	})

	tradeURL := ""
	if sr.QueryID != "" && league != "" {
		tradeURL = "https://www.pathofexile.com/trade/search/" + league + "/" + sr.QueryID
	}

	result := &TradeLookupResult{
		Gem:         gem,
		Variant:     variant,
		Total:       sr.Total,
		Listings:    listings,
		DivinePrice: divineRate,
		TradeURL:    tradeURL,
		FetchedAt:   time.Now(),
	}

	if len(listings) > 0 {
		result.PriceFloor = listings[0].ChaosPrice

		ceiling := listings[0].ChaosPrice
		for _, l := range listings {
			if l.ChaosPrice > ceiling {
				ceiling = l.ChaosPrice
			}
		}
		result.PriceCeiling = ceiling
		result.PriceSpread = ceiling - listings[0].ChaosPrice
		result.MedianTop10 = medianChaosPrice(listings)
	}

	result.Signals = ComputeSignals(listings)

	return result
}

// medianPrice returns the median price from a slice of listings.
func medianChaosPrice(listings []TradeListingDetail) float64 {
	if len(listings) == 0 {
		return 0
	}

	prices := make([]float64, len(listings))
	for i, l := range listings {
		prices[i] = l.ChaosPrice
	}
	sort.Float64s(prices)

	n := len(prices)
	if n%2 == 0 {
		return math.Round(((prices[n/2-1]+prices[n/2])/2)*100) / 100
	}
	return prices[n/2]
}
