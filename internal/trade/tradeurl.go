package trade

import (
	"net/url"
	"strings"
)

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
