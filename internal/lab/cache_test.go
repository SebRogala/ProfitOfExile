package lab

import (
	"testing"
	"time"

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

// Sparklines are keyed by name *and* variant: 1/0 and 20/20 are different
// markets whose prices must never be served for one another.
func TestCache_SetSparklines_RoundTripsPerNameAndVariant(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	twenty := []SparklinePoint{{Time: "2026-07-20T11:00:00Z", Price: 120, Listings: 4}}
	one := []SparklinePoint{{Time: "2026-07-20T11:00:00Z", Price: 3, Listings: 9}}
	c.For(scope).SetSparklines(map[sparklineKey][]SparklinePoint{
		{name: "Spark of Nova", variant: "20/20"}: twenty,
		{name: "Spark of Nova", variant: "1"}:     one,
	}, nil, time.Time{})

	got := c.For(scope).Sparklines("Spark of Nova", "20/20")
	if len(got) != 1 || got[0].Price != 120 || got[0].Listings != 4 {
		t.Fatalf("20/20 read: got %+v, want the stored 120c point", got)
	}
	if got := c.For(scope).Sparklines("Spark of Nova", "1"); len(got) != 1 || got[0].Price != 3 {
		t.Fatalf("1 read: got %+v, want the stored 3c point", got)
	}
}

// Corrupted gems live in their own map. Serving a corrupted 21/23c series from
// the non-corrupted read would put Dedication prices on a normal gem's chart.
func TestCache_SetSparklines_KeepsCorruptedSeriesOutOfTheNormalRead(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	corrupted := []SparklinePoint{{Time: "2026-07-20T11:00:00Z", Price: 400, Listings: 2}}
	c.For(scope).SetSparklines(nil, map[sparklineKey][]SparklinePoint{
		{name: "Spark of Nova", variant: "21/23c"}: corrupted,
	}, time.Time{})

	if got := c.For(scope).Sparklines("Spark of Nova", "21/23c"); got != nil {
		t.Fatalf("normal read of a corrupted key: got %+v, want nil", got)
	}
	got := c.For(scope).SparklinesCorrupted("Spark of Nova", "21/23c")
	if len(got) != 1 || got[0].Price != 400 {
		t.Fatalf("corrupted read: got %+v, want the stored 400c point", got)
	}
}

// Same tenancy gate as Transfigure: handlers read sparklines cache-first, so a
// cache warmed under LeagueA must never serve those series under LeagueB.
func TestCache_Sparklines_RejectsForeignLeague(t *testing.T) {
	owner := league.Historical("LeagueA")
	other := league.Historical("LeagueB")

	c := NewCache(owner)
	c.For(owner).SetSparklines(map[sparklineKey][]SparklinePoint{
		{name: "Spark of Nova", variant: "20/20"}: {{Time: "2026-07-20T11:00:00Z", Price: 120}},
	}, nil, time.Time{})

	requirePanic(t, func() {
		_ = c.For(other).Sparklines("Spark of Nova", "20/20")
	})
}

// A cold cache must report false so handlers fall back to the database instead
// of serving an empty sparkline for every gem.
func TestCache_HasSparklines_FalseBeforeFirstPopulation(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	if c.For(scope).HasSparklines() {
		t.Fatalf("HasSparklines on a fresh cache: got true, want false")
	}
}

// Once populated, a missing key means the gem genuinely has no recent points —
// the signal handlers use to stop falling back.
func TestCache_HasSparklines_TrueAfterPopulation(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	c.For(scope).SetSparklines(map[sparklineKey][]SparklinePoint{
		{name: "Spark of Nova", variant: "20/20"}: {{Time: "2026-07-20T11:00:00Z", Price: 120}},
	}, nil, time.Time{})

	if !c.For(scope).HasSparklines() {
		t.Fatalf("HasSparklines after a populated write: got false, want true")
	}
}

// The mark drives the next incremental read, so it must come back exactly as
// stored rather than as the write's wall-clock time.
func TestCache_SparklineHighWater_ReturnsStoredMark(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	mark := time.Date(2026, 7, 20, 11, 30, 0, 0, time.UTC)
	c.For(scope).SetSparklines(nil, nil, mark)

	if got := c.For(scope).SparklineHighWater(); !got.Equal(mark) {
		t.Fatalf("SparklineHighWater: got %s, want the stored %s", got, mark)
	}
}
