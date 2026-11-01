package trade

import "testing"

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
