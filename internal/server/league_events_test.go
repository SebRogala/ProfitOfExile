package server

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/collector"
	"profitofexile/internal/league"
)

// scopeRow / scopeDB build a resolved league.Scope with a chosen id and
// revision. Scope's fields are unexported and Historical() forces revision 0,
// so Resolve against a fake row is the only way to get a non-zero revision.
type scopeRow struct {
	id  string
	rev int64
}

func (r scopeRow) Scan(dest ...any) error {
	*dest[0].(*string) = r.id
	*dest[1].(*int64) = r.rev
	return nil
}

type scopeDB struct{ row scopeRow }

func (d scopeDB) QueryRow(context.Context, string, ...any) pgx.Row { return d.row }

func mustScope(t *testing.T, id string, rev int64) league.Scope {
	t.Helper()
	s, err := league.Resolve(context.Background(), scopeDB{row: scopeRow{id: id, rev: rev}})
	if err != nil {
		t.Fatalf("build scope: %v", err)
	}
	return s
}

func TestLeagueEventGuard_AcceptsMatchingLeagueAndRevision(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	if !g.Accept("Mirage", true, 7, true) {
		t.Fatal("matching league+revision was rejected")
	}
	if got := g.Rejected(); got != 0 {
		t.Errorf("rejected count = %d, want 0", got)
	}
}

func TestLeagueEventGuard_RejectsWrongLeague(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	if g.Accept("Standard", true, 7, true) {
		t.Fatal("event for a different league was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

// TestLeagueEventGuard_RejectsWrongRevision is the mutation anchor: dropping the
// revision comparison in Accept makes this event pass, failing the assertion.
func TestLeagueEventGuard_RejectsWrongRevision(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	if g.Accept("Mirage", true, 8, true) {
		t.Fatal("same league with a bumped revision was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

func TestLeagueEventGuard_RejectsAbsentLeagueField(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	// An unstamped legacy event: league field missing (leagueOK false).
	if g.Accept("", false, 7, true) {
		t.Fatal("event missing its league stamp was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

func TestLeagueEventGuard_RejectsAbsentRevisionField(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	// Revision stamp absent (revisionOK false) even though league matches.
	if g.Accept("Mirage", true, 0, false) {
		t.Fatal("event missing its revision stamp was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

func TestLeagueEventGuard_AcceptRawAcceptsStampedWireEvent(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	raw := []byte(`{"topic":"poe/collector/gems","league":"Mirage","leagueRevision":7,"endpoint":"ninja_gems"}`)
	if !g.AcceptRaw(raw) {
		t.Fatal("a correctly stamped wire event was rejected")
	}
}

func TestLeagueEventGuard_AcceptRawRejectsWrongRevisionWireEvent(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	raw := []byte(`{"topic":"poe/collector/gems","league":"Mirage","leagueRevision":9,"endpoint":"ninja_gems"}`)
	if g.AcceptRaw(raw) {
		t.Fatal("a wire event with a bumped revision was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

func TestLeagueEventGuard_AcceptRawRejectsUnstampedWireEvent(t *testing.T) {
	g := NewLeagueEventGuard(mustScope(t, "Mirage", 7))

	// Old-format event with neither league nor leagueRevision stamped.
	raw := []byte(`{"topic":"poe/collector/gems","endpoint":"ninja_gems"}`)
	if g.AcceptRaw(raw) {
		t.Fatal("an unstamped wire event was accepted")
	}
	if got := g.Rejected(); got != 1 {
		t.Errorf("rejected count = %d, want 1", got)
	}
}

// TestLeagueStampFieldNamesRoundTripFromCollector closes the cross-package field
// -name seam: the collector stamps events (collector.StampScope, used by every
// publish path) and the server reads them back (extractLeagueStamp via
// AcceptRaw). Both sides are hand-authored key literals in different packages,
// so a rename on either side would silently drop every event with no other test
// failing. Marshalling an actual collector-stamped payload and running it
// through the real server guard ties the two literals together: renaming the key
// in StampScope (collector side) or the json tag in extractLeagueStamp (server
// side) makes AcceptRaw return false and fails this test. The revision is
// non-zero so the leagueRevision int64<->float64 wire round-trip is exercised.
func TestLeagueStampFieldNamesRoundTripFromCollector(t *testing.T) {
	scope := mustScope(t, "Mirage", 7)

	raw, err := json.Marshal(collector.StampScope(map[string]any{
		"topic":    "poe/collector/gems",
		"endpoint": "ninja_gems",
	}, scope))
	if err != nil {
		t.Fatalf("marshal collector-stamped payload: %v", err)
	}

	g := NewLeagueEventGuard(scope)
	if !g.AcceptRaw(raw) {
		t.Fatalf("collector-stamped event rejected by server guard; stamp field names drifted between collector and server: %s", raw)
	}
}
