package trade

import (
	"encoding/json"
	"os"
	"testing"
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
			result := BuildResult("Arc", "20/20", "Test", SearchResponse{}, vector.Listings, vector.DivineRate)
			if result.MedianTop3 != vector.MedianTop3 {
				t.Errorf("MedianTop3 = %v, want %v", result.MedianTop3, vector.MedianTop3)
			}
		})
	}
}

func TestBuildResult_MedianTop10Unchanged(t *testing.T) {
	for _, vector := range readTradePriceVectors(t) {
		t.Run(vector.Name, func(t *testing.T) {
			result := BuildResult("Arc", "20/20", "Test", SearchResponse{}, vector.Listings, vector.DivineRate)
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
	}, 0)
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
