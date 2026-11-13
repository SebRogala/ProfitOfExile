package trade

import (
	"encoding/json"
	"os"
	"testing"
	"time"

	"profitofexile/internal/league"
)

type tradePriceVector struct {
	Name        string               `json:"name"`
	DivineRate  float64              `json:"divineRate"`
	Listings    []TradeListingDetail `json:"listings"`
	MedianTop3  float64              `json:"medianTop3"`
	MedianTop10 float64              `json:"medianTop10"`
}

func readTradePriceVectors(t *testing.T) []tradePriceVector {
	t.Helper()
	data, err := os.ReadFile("testdata/trade-price-vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var vectors []tradePriceVector
	if err := json.Unmarshal(data, &vectors); err != nil {
		t.Fatal(err)
	}
	return vectors
}

func TestBuildResult_TradePriceVectors(t *testing.T) {
	for _, vector := range readTradePriceVectors(t) {
		t.Run(vector.Name, func(t *testing.T) {
			result := BuildResult("Arc", "20/20", "Test", SearchResponse{}, vector.Listings, 0, vector.DivineRate)
			if result.MedianTop3 != vector.MedianTop3 {
				t.Errorf("MedianTop3 = %v, want %v", result.MedianTop3, vector.MedianTop3)
			}
		})
	}
}

func TestBuildResult_MedianTop10Unchanged(t *testing.T) {
	for _, vector := range readTradePriceVectors(t) {
		t.Run(vector.Name, func(t *testing.T) {
			result := BuildResult("Arc", "20/20", "Test", SearchResponse{}, vector.Listings, 0, vector.DivineRate)
			if result.MedianTop10 != vector.MedianTop10 {
				t.Errorf("MedianTop10 = %v, want %v", result.MedianTop10, vector.MedianTop10)
			}
		})
	}
}

func TestBuildResult_JSONIncludesTradePrice(t *testing.T) {
	result := BuildResult("Arc", "20/20", "Test", SearchResponse{}, []TradeListingDetail{
		{Price: 40, Currency: "chaos"},
		{Price: 50, Currency: "chaos"},
		{Price: 55, Currency: "chaos"},
		{Price: 60, Currency: "chaos"},
		{Price: 70, Currency: "chaos"},
	}, 0, 0)
	data, err := json.Marshal(result)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(data, &fields); err != nil {
		t.Fatal(err)
	}
	var price float64
	if value, ok := fields["medianTop3"]; !ok {
		t.Fatal("JSON is missing medianTop3")
	} else if err := json.Unmarshal(value, &price); err != nil {
		t.Fatal(err)
	}
	if price != 50 {
		t.Errorf("JSON medianTop3 = %v, want 50", price)
	}
}

func TestBuildResultCarriesLeague(t *testing.T) {
	for _, queryID := range []string{"", "query"} {
		t.Run(queryID, func(t *testing.T) {
			result := BuildResult("Arc", "20/20", "New League", SearchResponse{QueryID: queryID}, nil, 0, 0)
			data, err := json.Marshal(result)
			if err != nil {
				t.Fatal(err)
			}
			var fields map[string]any
			if err := json.Unmarshal(data, &fields); err != nil {
				t.Fatal(err)
			}
			if fields["league"] != "New League" {
				t.Fatalf("serialized league = %v, want New League", fields["league"])
			}
		})
	}
}

var quoteNow = time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)

func chaosListings(prices ...float64) []TradeListingDetail {
	out := make([]TradeListingDetail, len(prices))
	for i, p := range prices {
		out[i] = TradeListingDetail{Price: p, Currency: "chaos", ChaosPrice: p}
	}
	return out
}

func TestUsableQuote_ThreeChaosListingsPriceAtTheMedianTop3(t *testing.T) {
	r := BuildResult("g", "v", "L", SearchResponse{}, chaosListings(400, 412, 500, 900), 0, 0)
	r.FetchedAt = quoteNow.Add(-time.Hour)

	q := UsableQuote(r, quoteNow)

	if q.Chaos == nil || *q.Chaos != 412 {
		t.Fatalf("chaos = %v, want 412", q.Chaos)
	}
	if q.LowConfidence || q.Listings != 4 {
		t.Errorf("lowConfidence=%v listings=%d, want false, 4", q.LowConfidence, q.Listings)
	}
}

func TestUsableQuote_FewerThanThreeListingsKeepThePriceAsLowConfidence(t *testing.T) {
	r := BuildResult("g", "v", "L", SearchResponse{}, chaosListings(90, 100), 0, 0)
	r.FetchedAt = quoteNow

	q := UsableQuote(r, quoteNow)

	if q.Chaos == nil || *q.Chaos != 95 || !q.LowConfidence {
		t.Fatalf("quote = %+v (chaos %v), want 95 low confidence", q, q.Chaos)
	}
}

func TestUsableQuote_ExactlyTwentyFourHoursOldIsUsable(t *testing.T) {
	r := BuildResult("g", "v", "L", SearchResponse{}, chaosListings(5, 5, 5), 0, 0)
	r.FetchedAt = quoteNow.Add(-UsableQuoteMaxAge)

	if q := UsableQuote(r, quoteNow); q.Chaos == nil {
		t.Fatal("chaos = nil at exactly 24 h, want usable")
	}
}

func TestUsableQuote_OlderThanTwentyFourHoursHasNoPriceButKeepsFetchedAt(t *testing.T) {
	r := BuildResult("g", "v", "L", SearchResponse{}, chaosListings(5, 5, 5), 0, 0)
	r.FetchedAt = quoteNow.Add(-UsableQuoteMaxAge - time.Second)

	q := UsableQuote(r, quoteNow)

	if q.Chaos != nil {
		t.Errorf("chaos = %v, want nil", *q.Chaos)
	}
	if q.FetchedAt == nil || !q.FetchedAt.Equal(r.FetchedAt) {
		t.Errorf("fetchedAt = %v, want %v", q.FetchedAt, r.FetchedAt)
	}
}

func TestUsableQuote_EmptyResultHasNoPriceButKeepsFetchedAt(t *testing.T) {
	r := BuildResult("g", "v", "L", SearchResponse{}, nil, 0, 0)
	r.FetchedAt = quoteNow

	q := UsableQuote(r, quoteNow)

	if q.Chaos != nil || q.FetchedAt == nil || q.Listings != 0 {
		t.Fatalf("quote = %+v, want chaos nil, fetchedAt kept, 0 listings", q)
	}
}

func TestUsableQuote_NoObservationHasNoPriceAndNoFetchedAt(t *testing.T) {
	q := UsableQuote(nil, quoteNow)

	if q.Chaos != nil || q.FetchedAt != nil {
		t.Fatalf("quote = %+v, want all null", q)
	}
}

func TestUsableQuote_DivineListingWithoutARateIsUnusable(t *testing.T) {
	listings := []TradeListingDetail{{Price: 5, Currency: "divine"}, {Price: 5, Currency: "divine"}, {Price: 6, Currency: "divine"}}
	c := NewTradeCache(10, league.Historical("Test"))
	r := BuildResult("g", "v", "Test", SearchResponse{}, listings, 0, 0)
	r.FetchedAt = quoteNow
	c.Set("k", r)
	cached, _ := c.Get("k")

	q := UsableQuote(cached, quoteNow)

	if q.Chaos != nil {
		t.Fatalf("chaos = %v, want nil: a divine price without a rate is not chaos", *q.Chaos)
	}
}

func TestUsableQuote_DivineListingWithARateIsUsable(t *testing.T) {
	listings := []TradeListingDetail{{Price: 2, Currency: "divine"}, {Price: 400, Currency: "chaos"}, {Price: 500, Currency: "chaos"}}
	r := BuildResult("g", "v", "L", SearchResponse{}, listings, 0, 200)
	r.FetchedAt = quoteNow

	q := UsableQuote(r, quoteNow)

	if q.Chaos == nil || *q.Chaos != 400 {
		t.Fatalf("chaos = %v, want 400 (median of 400, 400, 500)", q.Chaos)
	}
}

func TestUsableQuote_MixedChaosAndDivineWithoutARateIsUnusable(t *testing.T) {
	listings := []TradeListingDetail{{Price: 100, Currency: "chaos"}, {Price: 110, Currency: "chaos"}, {Price: 1, Currency: "divine"}}
	r := BuildResult("g", "v", "L", SearchResponse{}, listings, 0, 0)
	r.FetchedAt = quoteNow

	if q := UsableQuote(r, quoteNow); q.Chaos != nil {
		t.Fatalf("chaos = %v, want nil", *q.Chaos)
	}
}

func TestUsableQuote_OtherCurrencyListingIsUnusable(t *testing.T) {
	listings := []TradeListingDetail{{Price: 100, Currency: "chaos"}, {Price: 110, Currency: "chaos"}, {Price: 3, Currency: "exalted"}}
	r := BuildResult("g", "v", "L", SearchResponse{}, listings, 0, 200)
	r.FetchedAt = quoteNow

	if q := UsableQuote(r, quoteNow); q.Chaos != nil {
		t.Fatalf("chaos = %v, want nil", *q.Chaos)
	}
}
