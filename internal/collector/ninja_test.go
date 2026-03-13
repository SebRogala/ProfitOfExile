package collector

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// testNinjaFetcher creates a NinjaFetcher pointing at the given test server
// with no gem color resolver.
func testNinjaFetcher(t *testing.T, server *httptest.Server) *NinjaFetcher {
	t.Helper()
	f := NewNinjaFetcher(nil)
	f.baseURL = server.URL
	return f
}

// gemLine builds a ninjaGemLine with the given discriminator set.
func gemLine(name, variant string, chaos float64, listings int, discriminator string) ninjaGemLine {
	line := ninjaGemLine{
		Name:         name,
		Variant:      variant,
		ChaosValue:   chaos,
		ListingCount: listings,
	}
	line.TradeFilter.Query.Type.Discriminator = discriminator
	return line
}

func TestFetchGems_validResponse(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			gemLine("Arc", "20/20", 15.5, 300, "gem"),
			gemLine("Vaal Grace", "", 1.2, 50, ""),
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	gems, err := f.FetchGems(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(gems) != 2 {
		t.Fatalf("got %d gems, want 2", len(gems))
	}

	// Verify field extraction for first gem.
	g := gems[0]
	if g.Name != "Arc" {
		t.Errorf("Name = %q, want %q", g.Name, "Arc")
	}
	if g.Variant != "20/20" {
		t.Errorf("Variant = %q, want %q", g.Variant, "20/20")
	}
	if g.Chaos != 15.5 {
		t.Errorf("Chaos = %v, want %v", g.Chaos, 15.5)
	}
	if g.Listings != 300 {
		t.Errorf("Listings = %d, want %d", g.Listings, 300)
	}
	if g.IsTransfigured {
		t.Error("IsTransfigured = true, want false")
	}

	// Verify empty variant is normalised to "default".
	if gems[1].Variant != "default" {
		t.Errorf("empty variant = %q, want %q", gems[1].Variant, "default")
	}
}

func TestFetchGems_corruptedFiltered(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			{Name: "Arc", ChaosValue: 10, ListingCount: 100},
			{Name: "Arc", ChaosValue: 5, ListingCount: 50, Corrupted: true},
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	gems, err := f.FetchGems(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(gems) != 1 {
		t.Fatalf("got %d gems, want 1 (corrupted should be filtered)", len(gems))
	}
	if gems[0].Chaos != 10 {
		t.Errorf("kept gem Chaos = %v, want %v", gems[0].Chaos, 10.0)
	}
}

func TestFetchGems_heistFiltered(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			{Name: "Arc", ChaosValue: 10, ListingCount: 100},
			{Name: "Flame Dash of Trarthus", ChaosValue: 50, ListingCount: 5},
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	gems, err := f.FetchGems(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(gems) != 1 {
		t.Fatalf("got %d gems, want 1 (Heist gem should be filtered)", len(gems))
	}
	if gems[0].Name != "Arc" {
		t.Errorf("remaining gem = %q, want %q", gems[0].Name, "Arc")
	}
}

func TestFetchGems_transfiguredDetection(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			gemLine("Arc of Surging", "", 200, 10, "alt_lightning"),
			gemLine("Cleave", "", 1, 500, "melee"),
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	gems, err := f.FetchGems(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(gems) != 2 {
		t.Fatalf("got %d gems, want 2", len(gems))
	}

	if !gems[0].IsTransfigured {
		t.Error("Arc of Surging should be transfigured (alt_ prefix)")
	}
	if gems[1].IsTransfigured {
		t.Error("Cleave should NOT be transfigured")
	}
}

func TestFetchGems_emptyResponse(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{Lines: []ninjaGemLine{}}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	gems, err := f.FetchGems(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(gems) != 0 {
		t.Errorf("got %d gems, want 0", len(gems))
	}
}

func TestFetchGems_malformedJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("{not valid json"))
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGems(context.Background(), "Standard")
	if err == nil {
		t.Fatal("expected error for malformed JSON, got nil")
	}
	if !strings.Contains(err.Error(), "decode") {
		t.Errorf("error = %q, want it to mention decode", err.Error())
	}
}

func TestFetchGems_httpError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGems(context.Background(), "Standard")
	if err == nil {
		t.Fatal("expected error for 503 response, got nil")
	}
	if !strings.Contains(err.Error(), "503") {
		t.Errorf("error = %q, want it to mention status 503", err.Error())
	}
}

func TestFetchCurrency_validResponse(t *testing.T) {
	payload := ninjaResponse[ninjaCurrencyLine]{
		Lines: []ninjaCurrencyLine{
			{
				CurrencyTypeName: "divine",
				ChaosEquivalent:  210.5,
				Sparkline: struct {
					TotalChange float64 `json:"totalChange"`
				}{TotalChange: -2.3},
			},
			{
				CurrencyTypeName: "exalted",
				ChaosEquivalent:  18.0,
				Sparkline: struct {
					TotalChange float64 `json:"totalChange"`
				}{TotalChange: 1.1},
			},
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	currencies, err := f.FetchCurrency(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(currencies) != 2 {
		t.Fatalf("got %d currencies, want 2", len(currencies))
	}

	// Verify field mapping for Divine.
	c := currencies[0]
	if c.CurrencyID != "divine" {
		t.Errorf("CurrencyID = %q, want %q", c.CurrencyID, "divine")
	}
	if c.Chaos != 210.5 {
		t.Errorf("Chaos = %v, want %v", c.Chaos, 210.5)
	}
	if c.SparklineChange != -2.3 {
		t.Errorf("SparklineChange = %v, want %v", c.SparklineChange, -2.3)
	}

	// Verify second currency.
	if currencies[1].CurrencyID != "exalted" {
		t.Errorf("second CurrencyID = %q, want %q", currencies[1].CurrencyID, "exalted")
	}
}

func TestFetchCurrency_emptyResponse(t *testing.T) {
	payload := ninjaResponse[ninjaCurrencyLine]{Lines: []ninjaCurrencyLine{}}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	currencies, err := f.FetchCurrency(context.Background(), "Standard")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(currencies) != 0 {
		t.Errorf("got %d currencies, want 0", len(currencies))
	}
}

func TestFetchCurrency_malformedJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("{{bad"))
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchCurrency(context.Background(), "Standard")
	if err == nil {
		t.Fatal("expected error for malformed JSON, got nil")
	}
	if !strings.Contains(err.Error(), "decode") {
		t.Errorf("error = %q, want it to mention decode", err.Error())
	}
}

func TestFetchGems_requestIncludesLeague(t *testing.T) {
	var receivedPath string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedPath = r.URL.String()
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchGems(context.Background(), "Mirage")

	if !strings.Contains(receivedPath, "league=Mirage") {
		t.Errorf("request path = %q, want league=Mirage parameter", receivedPath)
	}
	if !strings.Contains(receivedPath, "type=SkillGem") {
		t.Errorf("request path = %q, want type=SkillGem parameter", receivedPath)
	}
}

func TestFetchCurrency_requestIncludesLeague(t *testing.T) {
	var receivedPath string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedPath = r.URL.String()
		json.NewEncoder(w).Encode(ninjaResponse[ninjaCurrencyLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchCurrency(context.Background(), "Mirage")

	if !strings.Contains(receivedPath, "league=Mirage") {
		t.Errorf("request path = %q, want league=Mirage parameter", receivedPath)
	}
	if !strings.Contains(receivedPath, "type=Currency") {
		t.Errorf("request path = %q, want type=Currency parameter", receivedPath)
	}
}
