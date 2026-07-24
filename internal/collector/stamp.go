package collector

import "profitofexile/internal/league"

// StampScope writes the cross-process league identity onto a Mercure payload
// under the field names the server's LeagueEventGuard reads back
// (internal/server extractLeagueStamp: "league" and "leagueRevision"). The two
// key literals are a hand-authored contract that spans two packages, so every
// collector publish path funnels through this one definition: a rename here (or
// on the server reader) breaks the round-trip test in internal/server rather
// than silently dropping every event on the wire.
//
// It mutates and returns payload so callers can inline it inside json.Marshal.
func StampScope(payload map[string]any, scope league.Scope) map[string]any {
	payload["league"] = scope.ID()
	payload["leagueRevision"] = scope.Revision()
	return payload
}
