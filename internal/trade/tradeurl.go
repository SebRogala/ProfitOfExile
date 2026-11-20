package trade

import (
	"net/url"
	"strings"
)

// leaguePathSegment is the single league path-escaping rule for trade URLs.
func leaguePathSegment(league string) string {
	return url.PathEscape(league)
}

// tradeSearchURL returns the trade-site search prefix for league, path-escaped.
func tradeSearchURL(league string) string {
	return "https://www.pathofexile.com/trade/search/" + leaguePathSegment(league)
}

// tradeAPISearchURL returns the trade API search endpoint under baseURL for league.
func tradeAPISearchURL(baseURL, league string) string {
	return baseURL + "/api/trade/search/" + leaguePathSegment(league)
}

// LeagueFromTradeURL recovers the captured identity of installed clients that
// predate the explicit league field. Invalid or absent search URLs fail closed.
func LeagueFromTradeURL(raw string) string {
	u, err := url.Parse(raw)
	if err != nil || u.Scheme != "https" || u.Host != "www.pathofexile.com" || u.User != nil {
		return ""
	}
	// Split the escaped path so PathUnescape decodes the league exactly once.
	parts := strings.Split(u.EscapedPath(), "/")
	if len(parts) != 5 || parts[0] != "" || parts[1] != "trade" || parts[2] != "search" || parts[3] == "" || parts[4] == "" {
		return ""
	}
	identity, err := url.PathUnescape(parts[3])
	if err != nil {
		return ""
	}
	return identity
}
