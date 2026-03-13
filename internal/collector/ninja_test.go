package collector

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"profitofexile/internal/price/gemcolor"
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

func TestFetchGemsEndpoint_validResponse(t *testing.T) {
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
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
	if len(gems) != 2 {
		t.Fatalf("got %d gems, want 2", len(gems))
	}

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

	if gems[1].Variant != "default" {
		t.Errorf("empty variant = %q, want %q", gems[1].Variant, "default")
	}
}

func TestFetchGemsEndpoint_corruptedFiltered(t *testing.T) {
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
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
	if len(gems) != 1 {
		t.Fatalf("got %d gems, want 1 (corrupted should be filtered)", len(gems))
	}
	if gems[0].Chaos != 10 {
		t.Errorf("kept gem Chaos = %v, want %v", gems[0].Chaos, 10.0)
	}
}

func TestFetchGemsEndpoint_heistFiltered(t *testing.T) {
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
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
	if len(gems) != 1 {
		t.Fatalf("got %d gems, want 1 (Heist gem should be filtered)", len(gems))
	}
	if gems[0].Name != "Arc" {
		t.Errorf("remaining gem = %q, want %q", gems[0].Name, "Arc")
	}
}

func TestFetchGemsEndpoint_transfiguredDetection(t *testing.T) {
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
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
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

func TestFetchGemsEndpoint_emptyResponse(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{Lines: []ninjaGemLine{}}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(result.GemData) != 0 {
		t.Errorf("got %d gems, want 0", len(result.GemData))
	}
}

func TestFetchGemsEndpoint_malformedJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("{not valid json"))
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err == nil {
		t.Fatal("expected error for malformed JSON, got nil")
	}
	if !strings.Contains(err.Error(), "decode") {
		t.Errorf("error = %q, want it to mention decode", err.Error())
	}
}

func TestFetchGemsEndpoint_httpError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err == nil {
		t.Fatal("expected error for 503 response, got nil")
	}
	if !strings.Contains(err.Error(), "503") {
		t.Errorf("error = %q, want it to mention status 503", err.Error())
	}
}

func TestFetchCurrencyEndpoint_validResponse(t *testing.T) {
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
	result, err := f.FetchCurrencyEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	currencies := result.CurrencyData
	if len(currencies) != 2 {
		t.Fatalf("got %d currencies, want 2", len(currencies))
	}

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

	if currencies[1].CurrencyID != "exalted" {
		t.Errorf("second CurrencyID = %q, want %q", currencies[1].CurrencyID, "exalted")
	}
}

func TestFetchCurrencyEndpoint_emptyResponse(t *testing.T) {
	payload := ninjaResponse[ninjaCurrencyLine]{Lines: []ninjaCurrencyLine{}}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchCurrencyEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(result.CurrencyData) != 0 {
		t.Errorf("got %d currencies, want 0", len(result.CurrencyData))
	}
}

func TestFetchCurrencyEndpoint_malformedJSON(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Write([]byte("{{bad"))
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchCurrencyEndpoint(context.Background(), "Standard", "")
	if err == nil {
		t.Fatal("expected error for malformed JSON, got nil")
	}
	if !strings.Contains(err.Error(), "decode") {
		t.Errorf("error = %q, want it to mention decode", err.Error())
	}
}

func TestFetchGemsEndpoint_requestIncludesLeague(t *testing.T) {
	var receivedPath string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedPath = r.URL.String()
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchGemsEndpoint(context.Background(), "Mirage", "")

	if !strings.Contains(receivedPath, "league=Mirage") {
		t.Errorf("request path = %q, want league=Mirage parameter", receivedPath)
	}
	if !strings.Contains(receivedPath, "type=SkillGem") {
		t.Errorf("request path = %q, want type=SkillGem parameter", receivedPath)
	}
}

func TestFetchGemsEndpoint_requestIncludesUserAgent(t *testing.T) {
	var receivedUserAgent string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedUserAgent = r.Header.Get("User-Agent")
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchGemsEndpoint(context.Background(), "Standard", "")

	if receivedUserAgent != ninjaUserAgent {
		t.Errorf("User-Agent = %q, want %q", receivedUserAgent, ninjaUserAgent)
	}
}

func TestFetchCurrencyEndpoint_requestIncludesUserAgent(t *testing.T) {
	var receivedUserAgent string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedUserAgent = r.Header.Get("User-Agent")
		json.NewEncoder(w).Encode(ninjaResponse[ninjaCurrencyLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchCurrencyEndpoint(context.Background(), "Standard", "")

	if receivedUserAgent != ninjaUserAgent {
		t.Errorf("User-Agent = %q, want %q", receivedUserAgent, ninjaUserAgent)
	}
}

// newTestResolver creates a gemcolor.Resolver pre-seeded with the given
// name->color mappings. Colors are specified as strings (e.g. "RED", "BLUE").
func newTestResolver(colors map[string]string) *gemcolor.Resolver {
	m := make(map[string]gemcolor.Color, len(colors))
	for name, c := range colors {
		m[name] = gemcolor.Color(c)
	}
	return gemcolor.NewResolverFromMap(m)
}

func TestFetchGemsEndpoint_resolverPopulatesGemColor(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			gemLine("Arc", "20/20", 15.5, 300, "gem"),
			gemLine("Cleave", "", 5.0, 100, "melee"),
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	resolver := newTestResolver(map[string]string{
		"Arc":    "BLUE",
		"Cleave": "RED",
	})

	f := NewNinjaFetcher(resolver)
	f.baseURL = server.URL
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
	if len(gems) != 2 {
		t.Fatalf("got %d gems, want 2", len(gems))
	}

	if gems[0].GemColor != "BLUE" {
		t.Errorf("Arc GemColor = %q, want %q", gems[0].GemColor, "BLUE")
	}
	if gems[1].GemColor != "RED" {
		t.Errorf("Cleave GemColor = %q, want %q", gems[1].GemColor, "RED")
	}
}

func TestFetchGemsEndpoint_resolverUnresolvedGemColorEmpty(t *testing.T) {
	payload := ninjaResponse[ninjaGemLine]{
		Lines: []ninjaGemLine{
			gemLine("Unknown Gem", "", 10.0, 50, "gem"),
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	resolver := newTestResolver(map[string]string{})

	f := NewNinjaFetcher(resolver)
	f.baseURL = server.URL
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	gems := result.GemData
	if len(gems) != 1 {
		t.Fatalf("got %d gems, want 1", len(gems))
	}
	if gems[0].GemColor != "" {
		t.Errorf("GemColor = %q, want empty string for unresolved gem", gems[0].GemColor)
	}
}

func TestFetchGemsEndpoint_contextCancellation(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		<-r.Context().Done()
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := f.FetchGemsEndpoint(ctx, "Standard", "")
	if err == nil {
		t.Fatal("expected error for cancelled context, got nil")
	}
}

func TestFetchCurrencyEndpoint_requestIncludesLeague(t *testing.T) {
	var receivedPath string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedPath = r.URL.String()
		json.NewEncoder(w).Encode(ninjaResponse[ninjaCurrencyLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, _ = f.FetchCurrencyEndpoint(context.Background(), "Mirage", "")

	if !strings.Contains(receivedPath, "league=Mirage") {
		t.Errorf("request path = %q, want league=Mirage parameter", receivedPath)
	}
	if !strings.Contains(receivedPath, "type=Currency") {
		t.Errorf("request path = %q, want type=Currency parameter", receivedPath)
	}
}

func TestFetchGemsEndpoint_304NotModified(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("If-None-Match") == `"abc123"` {
			w.Header().Set("ETag", `"abc123"`)
			w.Header().Set("Age", "500")
			w.WriteHeader(http.StatusNotModified)
			return
		}
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", `"abc123"`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !result.NotModified {
		t.Error("expected NotModified = true")
	}
	if result.ETag != `"abc123"` {
		t.Errorf("ETag = %q, want %q", result.ETag, `"abc123"`)
	}
	if result.Age != 500 {
		t.Errorf("Age = %d, want 500", result.Age)
	}
}

func TestFetchGemsEndpoint_parsesETagAndAge(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("ETag", `"xyz789"`)
		w.Header().Set("Age", "1200")
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{
			Lines: []ninjaGemLine{gemLine("Arc", "20/20", 10, 100, "gem")},
		})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.NotModified {
		t.Error("expected NotModified = false for 200 response")
	}
	if result.ETag != `"xyz789"` {
		t.Errorf("ETag = %q, want %q", result.ETag, `"xyz789"`)
	}
	if result.Age != 1200 {
		t.Errorf("Age = %d, want 1200", result.Age)
	}
	if len(result.GemData) != 1 {
		t.Errorf("got %d gems, want 1", len(result.GemData))
	}
}

func TestFetchGemsEndpoint_noIfNoneMatchWhenEtagEmpty(t *testing.T) {
	var receivedIfNoneMatch string
	var headerPresent bool

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedIfNoneMatch = r.Header.Get("If-None-Match")
		_, headerPresent = r.Header["If-None-Match"]
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{
			Lines: []ninjaGemLine{gemLine("Arc", "20/20", 10, 100, "gem")},
		})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if headerPresent {
		t.Errorf("If-None-Match header should not be present when etag is empty, got %q", receivedIfNoneMatch)
	}
}

func TestFetchGemsEndpoint_ifNoneMatchHeaderSentWithEtag(t *testing.T) {
	var receivedIfNoneMatch string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedIfNoneMatch = r.Header.Get("If-None-Match")
		w.Header().Set("ETag", `"test-etag"`)
		w.WriteHeader(http.StatusNotModified)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	_, err := f.FetchGemsEndpoint(context.Background(), "Standard", `"test-etag"`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedIfNoneMatch != `"test-etag"` {
		t.Errorf("If-None-Match = %q, want %q", receivedIfNoneMatch, `"test-etag"`)
	}
}

func TestFetchGemsEndpoint_missingAgeHeaderReturnsZero(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// No Age header set.
		w.Header().Set("ETag", `"no-age"`)
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{
			Lines: []ninjaGemLine{gemLine("Arc", "20/20", 10, 100, "gem")},
		})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result.Age != 0 {
		t.Errorf("Age = %d, want 0 when Age header is missing", result.Age)
	}
}

func TestFetchGemsEndpoint_invalidAgeHeaderReturnsZero(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Age", "not-a-number")
		w.Header().Set("ETag", `"invalid-age"`)
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{
			Lines: []ninjaGemLine{gemLine("Arc", "20/20", 10, 100, "gem")},
		})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result.Age != 0 {
		t.Errorf("Age = %d, want 0 when Age header is invalid", result.Age)
	}
}

func TestFetchGemsEndpoint_etagCapturedFromResponse(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("ETag", `"captured-etag"`)
		w.Header().Set("Age", "100")
		json.NewEncoder(w).Encode(ninjaResponse[ninjaGemLine]{
			Lines: []ninjaGemLine{gemLine("Arc", "20/20", 10, 100, "gem")},
		})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchGemsEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result.ETag != `"captured-etag"` {
		t.Errorf("ETag = %q, want %q", result.ETag, `"captured-etag"`)
	}
}

func TestFetchCurrencyEndpoint_populatedWithETagAndAge(t *testing.T) {
	payload := ninjaResponse[ninjaCurrencyLine]{
		Lines: []ninjaCurrencyLine{
			{
				CurrencyTypeName: "divine",
				ChaosEquivalent:  210.5,
				Sparkline: struct {
					TotalChange float64 `json:"totalChange"`
				}{TotalChange: -2.3},
			},
		},
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("ETag", `"currency-etag"`)
		w.Header().Set("Age", "900")
		json.NewEncoder(w).Encode(payload)
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchCurrencyEndpoint(context.Background(), "Standard", "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if result.NotModified {
		t.Error("expected NotModified = false for 200 response")
	}
	if result.ETag != `"currency-etag"` {
		t.Errorf("ETag = %q, want %q", result.ETag, `"currency-etag"`)
	}
	if result.Age != 900 {
		t.Errorf("Age = %d, want 900", result.Age)
	}
	if len(result.CurrencyData) != 1 {
		t.Fatalf("got %d currencies, want 1", len(result.CurrencyData))
	}
	if result.CurrencyData[0].CurrencyID != "divine" {
		t.Errorf("CurrencyID = %q, want %q", result.CurrencyData[0].CurrencyID, "divine")
	}
	if result.CurrencyData[0].Chaos != 210.5 {
		t.Errorf("Chaos = %v, want %v", result.CurrencyData[0].Chaos, 210.5)
	}
}

func TestFetchCurrencyEndpoint_304NotModified(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("If-None-Match") == `"curr-etag"` {
			w.Header().Set("ETag", `"curr-etag"`)
			w.Header().Set("Age", "600")
			w.WriteHeader(http.StatusNotModified)
			return
		}
		json.NewEncoder(w).Encode(ninjaResponse[ninjaCurrencyLine]{})
	}))
	defer server.Close()

	f := testNinjaFetcher(t, server)
	result, err := f.FetchCurrencyEndpoint(context.Background(), "Standard", `"curr-etag"`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !result.NotModified {
		t.Error("expected NotModified = true for 304 response")
	}
	if result.ETag != `"curr-etag"` {
		t.Errorf("ETag = %q, want %q", result.ETag, `"curr-etag"`)
	}
	if result.Age != 600 {
		t.Errorf("Age = %d, want 600", result.Age)
	}
}
