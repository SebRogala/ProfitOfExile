package lab

import (
	"testing"

	"profitofexile/internal/league"
)

// requirePanic fails the test unless fn panics. Used to assert that the cache
// rejects access under a foreign league rather than serving another league's
// cached rows.
func requirePanic(t *testing.T, fn func()) {
	t.Helper()
	defer func() {
		if r := recover(); r == nil {
			t.Fatalf("expected a panic (foreign-league access), got none")
		}
	}()
	fn()
}

// The cache is bound to one league. Reading it under its owning scope must
// return exactly what was stored under that scope — For must be transparent for
// the owning league.
func TestCache_For_OwningLeagueRoundTrips(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	rows := []TransfigureResult{{TransfiguredName: "Spark of Nova"}}
	c.For(scope).SetTransfigure(rows)

	got := c.For(scope).Transfigure()
	if len(got) != 1 || got[0].TransfiguredName != "Spark of Nova" {
		t.Fatalf("owning-league read: got %+v, want the stored row", got)
	}
}

// The tenancy gate: a cache warmed under one league must not serve those rows
// under a different league's scope. This is the leak POE-121 would otherwise
// open — handlers read cache-first and only fall through to the league-scoped
// repository on a miss, so a warm cache that ignored the league would return the
// previous league's rows. Removing the scope check in For makes this read return
// LeagueA's rows instead of panicking, so this test fails.
func TestCache_For_RejectsForeignLeague(t *testing.T) {
	owner := league.Historical("LeagueA")
	other := league.Historical("LeagueB")

	c := NewCache(owner)
	c.For(owner).SetTransfigure([]TransfigureResult{{TransfiguredName: "Spark of Nova"}})

	requirePanic(t, func() {
		_ = c.For(other).Transfigure()
	})
}
