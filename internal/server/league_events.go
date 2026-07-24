package server

import (
	"encoding/json"
	"log/slog"
	"sync/atomic"

	"profitofexile/internal/league"
)

// LeagueEventGuard validates that an inbound Mercure event was stamped with the
// process-active league identity before the server processes it as current
// data. The collector stamps every scoped publish with its resolved league and
// revision; an event carrying a different league or a bumped revision (e.g. a
// stale publisher lingering across a rolling deploy) must be rejected, not
// applied. Rejections are counted so a sustained mismatch is observable.
type LeagueEventGuard struct {
	scope    league.Scope
	rejected atomic.Int64
}

// NewLeagueEventGuard returns a guard bound to the process-active scope.
func NewLeagueEventGuard(scope league.Scope) *LeagueEventGuard {
	return &LeagueEventGuard{scope: scope}
}

// Accept reports whether a stamped event belongs to the active scope. Both the
// league id and the revision must be present and equal to the active scope's:
// leagueOK/revisionOK are false when the field was absent on the wire, which is
// itself a mismatch — an unstamped legacy event must not be trusted as current.
// On any mismatch it increments the rejection counter and logs; a false result
// means the caller must drop the event without processing it.
func (g *LeagueEventGuard) Accept(eventLeague string, leagueOK bool, eventRevision int64, revisionOK bool) bool {
	if leagueOK && revisionOK &&
		eventLeague == g.scope.ID() &&
		eventRevision == g.scope.Revision() {
		return true
	}
	n := g.rejected.Add(1)
	slog.Warn("mercure: rejected event for non-active league",
		"event_league", eventLeague,
		"league_present", leagueOK,
		"event_revision", eventRevision,
		"revision_present", revisionOK,
		"active_league", g.scope.ID(),
		"active_revision", g.scope.Revision(),
		"rejected_total", n,
	)
	return false
}

// AcceptRaw extracts the league stamp from a raw Mercure event body and applies
// Accept. Unparseable JSON, or a body missing either stamp, is treated as a
// mismatch.
func (g *LeagueEventGuard) AcceptRaw(raw []byte) bool {
	leagueID, leagueOK, revision, revisionOK := extractLeagueStamp(raw)
	return g.Accept(leagueID, leagueOK, revision, revisionOK)
}

// Rejected returns the number of events rejected so far.
func (g *LeagueEventGuard) Rejected() int64 { return g.rejected.Load() }

// extractLeagueStamp pulls the stamped league id and revision from a raw event
// body. Each value is returned with a presence flag; pointer fields distinguish
// an absent stamp from a present zero value so an unstamped event fails
// validation rather than matching a zero-revision scope by accident.
func extractLeagueStamp(raw []byte) (leagueID string, leagueOK bool, revision int64, revisionOK bool) {
	var m struct {
		League         *string  `json:"league"`
		LeagueRevision *float64 `json:"leagueRevision"`
	}
	if err := json.Unmarshal(raw, &m); err != nil {
		return "", false, 0, false
	}
	if m.League != nil {
		leagueID, leagueOK = *m.League, true
	}
	if m.LeagueRevision != nil {
		revision, revisionOK = int64(*m.LeagueRevision), true
	}
	return leagueID, leagueOK, revision, revisionOK
}
