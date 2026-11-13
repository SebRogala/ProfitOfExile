package trade

import (
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/price/resolve"
)

const observationLeague = "Temp-POE307"

var observationT0 = time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)

func chaosListing(price float64, level, quality int, corrupted bool) TradeListingDetail {
	return TradeListingDetail{Price: price, Currency: "chaos", GemLevel: level, GemQuality: quality, Corrupted: corrupted}
}

func listingPrices(listings []TradeListingDetail) []float64 {
	prices := make([]float64, 0, len(listings))
	for _, l := range listings {
		prices = append(prices, l.Price)
	}
	return prices
}

// mixedSubmission is set SM: one corrupted 20/20, three clean 20/20 and one
// clean 19/20 listing for the Ballista 20/20 uncorrupted market.
func mixedSubmission() []TradeListingDetail {
	return []TradeListingDetail{
		chaosListing(10, 20, 20, true),
		chaosListing(40, 20, 20, false),
		chaosListing(50, 20, 20, false),
		chaosListing(55, 20, 20, false),
		chaosListing(12, 19, 20, false),
	}
}

func TestListingsInMarket_dropsCorruptedListingsFromAnUncorruptedMarket(t *testing.T) {
	key := resolve.GemKey(observationLeague, "Siege Ballista", "20/20")
	listings := mixedSubmission()[:4]

	got := listingPrices(ListingsInMarket(key, listings))

	if want := []float64{40, 50, 55}; !reflect.DeepEqual(got, want) {
		t.Fatalf("prices in market = %v, want %v", got, want)
	}
}

func TestListingsInMarket_dropsOtherLevelsFromALevelTwentyMarket(t *testing.T) {
	key := resolve.GemKey(observationLeague, "Siege Ballista", "20/20")
	listings := []TradeListingDetail{chaosListing(12, 19, 20, false), chaosListing(40, 20, 20, false)}

	got := listingPrices(ListingsInMarket(key, listings))

	if want := []float64{40}; !reflect.DeepEqual(got, want) {
		t.Fatalf("prices in market = %v, want %v", got, want)
	}
}

func TestListingsInMarket_levelOneMarketKeepsEveryLevel(t *testing.T) {
	key := resolve.GemKey(observationLeague, "Siege Ballista", "1")
	listings := []TradeListingDetail{chaosListing(5, 1, 0, false), chaosListing(7, 7, 13, false)}

	got := listingPrices(ListingsInMarket(key, listings))

	if want := []float64{5, 7}; !reflect.DeepEqual(got, want) {
		t.Fatalf("prices in market = %v, want %v", got, want)
	}
}

func TestObservations_mixedDesktopSubmissionPricesFromTheMarketOnly(t *testing.T) {
	rows := []TradeLookupResult{{Gem: "Siege Ballista", Variant: "20/20", Listings: mixedSubmission(), FetchedAt: observationT0}}

	obs := Observations(observationLeague, rows)
	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}
	price, ok := resolve.MedianCheapest(obs[0].Prices, 3, 200)

	if !ok || price != 50 {
		t.Fatalf("median of cheapest 3 = %v (ok %v), want 50 (12 means the market filter was skipped)", price, ok)
	}
}

func TestObservations_divineListingKeepsItsNativeAmount(t *testing.T) {
	listings := []TradeListingDetail{
		{Price: 0.25, Currency: "divine", ChaosPrice: 50, GemLevel: 20, GemQuality: 20},
		chaosListing(40, 20, 20, false),
		chaosListing(55, 20, 20, false),
	}
	rows := []TradeLookupResult{{Gem: "Siege Ballista", Variant: "20/20", Listings: listings, FetchedAt: observationT0}}

	obs := Observations(observationLeague, rows)
	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}

	want := []resolve.Money{{Amount: 0.25, Currency: "divine"}, {Amount: 40, Currency: "chaos"}, {Amount: 55, Currency: "chaos"}}
	if !reflect.DeepEqual(obs[0].Prices, want) {
		t.Fatalf("prices = %v, want %v", obs[0].Prices, want)
	}
}

func TestObservations_zeroQualitySuffixVariantsCollapseToTheNewestRow(t *testing.T) {
	rows := []TradeLookupResult{
		{Gem: "Siege Ballista", Variant: "1/0", Listings: []TradeListingDetail{chaosListing(30, 1, 0, false)}, FetchedAt: observationT0.Add(-time.Hour)},
		{Gem: "Siege Ballista", Variant: "1", Listings: []TradeListingDetail{chaosListing(40, 1, 0, false)}, FetchedAt: observationT0.Add(-2 * time.Hour)},
	}

	obs := Observations(observationLeague, rows)
	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1 (\"1/0\" and \"1\" are one key)", len(obs))
	}

	want := []resolve.Money{{Amount: 30, Currency: "chaos"}}
	if !reflect.DeepEqual(obs[0].Prices, want) || !obs[0].At.Equal(observationT0.Add(-time.Hour)) {
		t.Fatalf("observation = %v at %v, want %v at %v", obs[0].Prices, obs[0].At, want, observationT0.Add(-time.Hour))
	}
}

func TestObservations_storedConvertedFloorIsNotUsed(t *testing.T) {
	listings := []TradeListingDetail{chaosListing(40, 20, 20, false), chaosListing(50, 20, 20, false), chaosListing(55, 20, 20, false)}
	rows := []TradeLookupResult{{Gem: "Siege Ballista", Variant: "20/20", Listings: listings, PriceFloor: 999, MedianTop10: 999, FetchedAt: observationT0}}

	obs := Observations(observationLeague, rows)
	if len(obs) != 1 {
		t.Fatalf("observations = %d, want 1", len(obs))
	}

	want := []resolve.Money{{Amount: 40, Currency: "chaos"}, {Amount: 50, Currency: "chaos"}, {Amount: 55, Currency: "chaos"}}
	if !reflect.DeepEqual(obs[0].Prices, want) {
		t.Fatalf("prices = %v, want %v", obs[0].Prices, want)
	}
}
