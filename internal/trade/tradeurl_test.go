package trade

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestLeagueFromTradeURL(t *testing.T) {
	for _, tc := range []struct{ name, input, want string }{
		{"matching", "https://www.pathofexile.com/trade/search/Mirage/abc", "Mirage"},
		{"other league", "https://www.pathofexile.com/trade/search/Settlers/abc", "Settlers"},
		{"escaped spaces", "https://www.pathofexile.com/trade/search/New%20League/abc", "New League"},
		{"raw spaces", "https://www.pathofexile.com/trade/search/New League/abc", "New League"},
		{"decode once", "https://www.pathofexile.com/trade/search/New%2520League/abc", "New%20League"},
		{"plus literal", "https://www.pathofexile.com/trade/search/New+League/abc", "New+League"},
		{"empty zero result", "", ""},
		{"malformed escape", "https://www.pathofexile.com/trade/search/New%ZZLeague/abc", ""},
		{"malformed URL", "://[", ""},
		{"missing league", "https://www.pathofexile.com/trade/search//abc", ""},
		{"missing query", "https://www.pathofexile.com/trade/search/Mirage", ""},
		{"empty query", "https://www.pathofexile.com/trade/search/Mirage/", ""},
		{"wrong path", "https://www.pathofexile.com/trade/exchange/Mirage/abc", ""},
		{"relative URL", "/trade/search/Mirage/abc", ""},
		{"wrong host", "https://example.com/trade/search/Mirage/abc", ""},
		{"extra path", "https://www.pathofexile.com/trade/search/Mirage/abc/extra", ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := LeagueFromTradeURL(tc.input); got != tc.want {
				t.Fatalf("LeagueFromTradeURL(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}
}

func TestKeepHappy_SearchPostsToPathEscapedLeague(t *testing.T) {
	vectors := readTradeURLVectors(t)
	if len(vectors) == 0 {
		t.Fatal("no trade URL vectors")
	}
	for _, vector := range vectors {
		t.Run(vector.Name, func(t *testing.T) {
			var gotPath string
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gotPath = r.URL.EscapedPath()
				json.NewEncoder(w).Encode(tradeSearchResp{ID: vector.QueryID})
			}))
			defer server.Close()
			c := NewClient(TradeConfig{LeagueName: vector.League})
			c.SetBaseURL(server.URL)

			if _, _, err := c.Search(context.Background(), "Arc", "20/20"); err != nil {
				t.Fatal(err)
			}
			if gotPath != vector.APISearchPath {
				t.Fatalf("search path = %q, want %q", gotPath, vector.APISearchPath)
			}
		})
	}
}
