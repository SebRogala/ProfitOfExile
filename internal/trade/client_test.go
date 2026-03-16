package trade

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// testTradeClient creates a Client pointing at the given test server.
func testTradeClient(t *testing.T, server *httptest.Server) *Client {
	t.Helper()
	c := NewClient(TradeConfig{
		UserAgent:  "ProfitOfExile/test",
		LeagueName: "Mirage",
	})
	c.SetBaseURL(server.URL)
	return c
}

func TestSearch_Success(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewEncoder(w).Encode(tradeSearchResp{
			ID:     "abc123",
			Result: []string{"id1", "id2", "id3"},
			Total:  150,
		})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	resp, headers, err := c.Search(context.Background(), "Arc")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if headers == nil {
		t.Fatal("expected non-nil headers")
	}
	if resp.QueryID != "abc123" {
		t.Errorf("QueryID = %q, want %q", resp.QueryID, "abc123")
	}
	if len(resp.IDs) != 3 {
		t.Errorf("len(IDs) = %d, want 3", len(resp.IDs))
	}
	if resp.Total != 150 {
		t.Errorf("Total = %d, want 150", resp.Total)
	}
}

func TestSearch_RequestBody(t *testing.T) {
	var receivedBody []byte
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedBody, _ = io.ReadAll(r.Body)
		json.NewEncoder(w).Encode(tradeSearchResp{ID: "x", Total: 0})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, _, err := c.Search(context.Background(), "Empower Support")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	var body map[string]interface{}
	if err := json.Unmarshal(receivedBody, &body); err != nil {
		t.Fatalf("failed to parse request body: %v", err)
	}

	// Verify sort.price = "asc"
	sortMap, ok := body["sort"].(map[string]interface{})
	if !ok {
		t.Fatal("missing sort in request body")
	}
	if sortMap["price"] != "asc" {
		t.Errorf("sort.price = %v, want %q", sortMap["price"], "asc")
	}

	// Verify query structure
	query, ok := body["query"].(map[string]interface{})
	if !ok {
		t.Fatal("missing query in request body")
	}
	if query["type"] != "Skill Gem" {
		t.Errorf("query.type = %v, want %q", query["type"], "Skill Gem")
	}
	if query["term"] != "Empower Support" {
		t.Errorf("query.term = %v, want %q", query["term"], "Empower Support")
	}
	if query["status"] == nil {
		t.Fatal("missing query.status")
	}

	// Verify trade_filters with priced and collapsed
	filters, ok := query["filters"].(map[string]interface{})
	if !ok {
		t.Fatal("missing query.filters")
	}
	tradeFilters, ok := filters["trade_filters"].(map[string]interface{})
	if !ok {
		t.Fatal("missing trade_filters")
	}
	innerFilters, ok := tradeFilters["filters"].(map[string]interface{})
	if !ok {
		t.Fatal("missing trade_filters.filters")
	}

	saleType, ok := innerFilters["sale_type"].(map[string]interface{})
	if !ok {
		t.Fatal("missing sale_type filter")
	}
	if saleType["option"] != "priced" {
		t.Errorf("sale_type.option = %v, want %q", saleType["option"], "priced")
	}

	collapse, ok := innerFilters["collapse"].(map[string]interface{})
	if !ok {
		t.Fatal("missing collapse filter")
	}
	if collapse["option"] != "true" {
		t.Errorf("collapse.option = %v, want %q", collapse["option"], "true")
	}
}

func TestSearch_UserAgent(t *testing.T) {
	var receivedUA string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedUA = r.Header.Get("User-Agent")
		json.NewEncoder(w).Encode(tradeSearchResp{ID: "x", Total: 0})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, _, err := c.Search(context.Background(), "Arc")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedUA != "ProfitOfExile/test" {
		t.Errorf("User-Agent = %q, want %q", receivedUA, "ProfitOfExile/test")
	}
}

func TestSearch_RateLimited(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusTooManyRequests)
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, _, err := c.Search(context.Background(), "Arc")
	if err == nil {
		t.Fatal("expected error for 429 response, got nil")
	}
	if !errors.Is(err, ErrRateLimited) {
		t.Errorf("error = %v, want ErrRateLimited", err)
	}
}

func TestSearch_ServerError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
		w.Write([]byte("internal server error"))
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, _, err := c.Search(context.Background(), "Arc")
	if err == nil {
		t.Fatal("expected error for 500 response, got nil")
	}
	if !strings.Contains(err.Error(), "500") {
		t.Errorf("error = %q, want it to mention status 500", err.Error())
	}
	if !strings.Contains(err.Error(), "internal server error") {
		t.Errorf("error = %q, want it to include body excerpt", err.Error())
	}
}

func TestFetch_Success(t *testing.T) {
	indexed := time.Date(2026, 3, 15, 10, 30, 0, 0, time.UTC)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		resp := tradeFetchResp{
			Result: []tradeFetchEntry{
				{
					Listing: tradeFetchListing{
						Indexed: indexed,
						Account: tradeFetchAccount{Name: "seller1"},
						Price:   tradeFetchPrice{Amount: 25.5, Currency: "chaos"},
					},
					Item: tradeFetchItem{
						Corrupted: true,
						Properties: []tradeFetchProperty{
							{Name: "Level", Values: [][]interface{}{{"20", float64(0)}}},
							{Name: "Quality", Values: [][]interface{}{{"+23%", float64(0)}}},
						},
					},
				},
				{
					Listing: tradeFetchListing{
						Indexed: indexed,
						Account: tradeFetchAccount{Name: "seller2"},
						Price:   tradeFetchPrice{Amount: 30.0, Currency: "divine"},
					},
					Item: tradeFetchItem{
						Corrupted: false,
						Properties: []tradeFetchProperty{
							{Name: "Level", Values: [][]interface{}{{"21", float64(0)}}},
							{Name: "Quality", Values: [][]interface{}{{"0", float64(0)}}},
						},
					},
				},
			},
		}
		json.NewEncoder(w).Encode(resp)
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	listings, headers, err := c.Fetch(context.Background(), "qid123", []string{"id1", "id2"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if headers == nil {
		t.Fatal("expected non-nil headers")
	}
	if len(listings) != 2 {
		t.Fatalf("got %d listings, want 2", len(listings))
	}

	l := listings[0]
	if l.Price != 25.5 {
		t.Errorf("Price = %v, want 25.5", l.Price)
	}
	if l.Currency != "chaos" {
		t.Errorf("Currency = %q, want %q", l.Currency, "chaos")
	}
	if l.Account != "seller1" {
		t.Errorf("Account = %q, want %q", l.Account, "seller1")
	}
	if !l.IndexedAt.Equal(indexed) {
		t.Errorf("IndexedAt = %v, want %v", l.IndexedAt, indexed)
	}
	if l.GemLevel != 20 {
		t.Errorf("GemLevel = %d, want 20", l.GemLevel)
	}
	if l.GemQuality != 23 {
		t.Errorf("GemQuality = %d, want 23", l.GemQuality)
	}
	if !l.Corrupted {
		t.Error("Corrupted = false, want true")
	}

	l2 := listings[1]
	if l2.Currency != "divine" {
		t.Errorf("second listing Currency = %q, want %q", l2.Currency, "divine")
	}
	if l2.Corrupted {
		t.Error("second listing Corrupted = true, want false")
	}
	if l2.GemLevel != 21 {
		t.Errorf("second listing GemLevel = %d, want 21", l2.GemLevel)
	}
}

func TestFetch_TruncatesTo10(t *testing.T) {
	var receivedURL string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedURL = r.URL.String()
		json.NewEncoder(w).Encode(tradeFetchResp{})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	ids := make([]string, 15)
	for i := range ids {
		ids[i] = "id" + strings.Repeat("x", i)
	}

	_, _, err := c.Fetch(context.Background(), "qid", ids)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// The URL path should contain a comma-separated list. Count segments.
	// URL format: /api/trade/fetch/{comma-ids}?query={queryID}
	pathParts := strings.SplitN(receivedURL, "?", 2)
	fetchPath := pathParts[0]
	// Extract the IDs portion after "/api/trade/fetch/"
	prefix := "/api/trade/fetch/"
	if !strings.HasPrefix(fetchPath, prefix) {
		t.Fatalf("unexpected path: %s", fetchPath)
	}
	idStr := strings.TrimPrefix(fetchPath, prefix)
	fetchedIDs := strings.Split(idStr, ",")
	if len(fetchedIDs) != 10 {
		t.Errorf("fetched %d IDs, want 10 (truncated from 15)", len(fetchedIDs))
	}
}

func TestFetch_HeadersReturned(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("X-Rate-Limit-Rules", "Ip")
		w.Header().Set("X-Rate-Limit-Ip", "12:4:60,16:12:240")
		w.Header().Set("X-Rate-Limit-Ip-State", "1:4:0,1:12:0")
		json.NewEncoder(w).Encode(tradeFetchResp{})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, headers, err := c.Fetch(context.Background(), "qid", []string{"id1"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	rules := headers.Get("X-Rate-Limit-Rules")
	if rules != "Ip" {
		t.Errorf("X-Rate-Limit-Rules = %q, want %q", rules, "Ip")
	}
	limit := headers.Get("X-Rate-Limit-Ip")
	if limit != "12:4:60,16:12:240" {
		t.Errorf("X-Rate-Limit-Ip = %q, want %q", limit, "12:4:60,16:12:240")
	}
	state := headers.Get("X-Rate-Limit-Ip-State")
	if state != "1:4:0,1:12:0" {
		t.Errorf("X-Rate-Limit-Ip-State = %q, want %q", state, "1:4:0,1:12:0")
	}
}

func TestSearch_RequestURL(t *testing.T) {
	var receivedPath string
	var receivedMethod string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedPath = r.URL.Path
		receivedMethod = r.Method
		json.NewEncoder(w).Encode(tradeSearchResp{ID: "x", Total: 0})
	}))
	defer server.Close()

	c := testTradeClient(t, server)
	_, _, err := c.Search(context.Background(), "Arc")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	expectedPath := "/api/trade/search/Mirage"
	if receivedPath != expectedPath {
		t.Errorf("path = %q, want %q", receivedPath, expectedPath)
	}
	if receivedMethod != http.MethodPost {
		t.Errorf("method = %q, want %q", receivedMethod, http.MethodPost)
	}
}
