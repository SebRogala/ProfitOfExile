//go:build integration

package temple

import (
	"context"
	"os"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// integrationScope pins these writes and reads to the seeded Mirage league, the
// way internal/collector's item tests do, so a cleanup can never reach another
// league's rows that happen to share a timestamp.
var integrationScope = league.Historical("Mirage")

func integrationPool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(func() { pool.Close() })

	var exists bool
	if err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'item_snapshots')").
		Scan(&exists); err != nil {
		t.Fatalf("check item_snapshots table: %v", err)
	}
	if !exists {
		t.Skip("item_snapshots not found, skipping (TimescaleDB migrations not applied)")
	}
	return pool
}

// registerLeague adds a second league so a scoping test has one to be excluded
// from. Only Mirage is seeded by the league-control migration, and the FK on
// item_snapshots.league means a row cannot be written for an unregistered one.
func registerLeague(t *testing.T, pool *pgxpool.Pool, id string) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
}

// insertLine writes one item_snapshots row directly, so these tests exercise the
// read path rather than the collector's writer.
func insertLine(t *testing.T, pool *pgxpool.Pool, leagueID string, at time.Time, category, name string, ninjaID int64, chaos, divine float64, listings int) {
	t.Helper()

	_, err := pool.Exec(context.Background(),
		`INSERT INTO item_snapshots (league, time, category, ninja_id, name, chaos, divine, listings)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
		leagueID, at, category, ninjaID, name, chaos, divine, listings)
	if err != nil {
		t.Fatalf("insert %s/%s: %v", category, name, err)
	}
}

// cleanupAt deletes this test's rows at the given stamps, in the leagues the
// test wrote to and no others.
//
// The league predicate is the scope pin this file's header claims: without it a
// stamp that happened to collide with another league's row would take that row
// out too, which is exactly the cross-league reach the pin exists to prevent.
// leagues must therefore name every league the test inserted into — the
// registerLeague cleanup that runs after this one cannot drop a league whose
// item_snapshots rows are still there.
func cleanupAt(t *testing.T, pool *pgxpool.Pool, leagues []string, times ...time.Time) {
	t.Helper()
	t.Cleanup(func() {
		for _, at := range times {
			if _, err := pool.Exec(context.Background(),
				"DELETE FROM item_snapshots WHERE time = $1 AND league = ANY($2)", at, leagues); err != nil {
				t.Logf("cleanup at %v: %v", at, err)
			}
		}
	})
}

func TestNewestLines_readsOnlyEachCategorysNewestSnapshot(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	// Distinct microsecond stamps inside the lookback so the rows cannot collide
	// with anything else the suite writes.
	older := time.Now().UTC().Add(-3 * time.Hour).Truncate(time.Microsecond)
	newer := older.Add(time.Hour)
	// The Vial half is stored only at the older time, which is what proves the
	// newest time is resolved PER CATEGORY: a single global MAX(time) would
	// return no Vial line at all.
	cleanupAt(t, pool, []string{integrationScope.ID()}, older, newer)

	insertLine(t, pool, integrationScope.ID(), older, RoomCategory, "Ancient Doorway (Tier 1)", 990001, 5, 0, 100)
	insertLine(t, pool, integrationScope.ID(), newer, RoomCategory, "Ancient Doorway (Tier 1)", 990001, 12, 0, 140)
	insertLine(t, pool, integrationScope.ID(), older, "Vial", "Vial of Fate", 990002, 1, 0, 343)

	got, err := repo.NewestLines(context.Background(), integrationScope)
	if err != nil {
		t.Fatalf("NewestLines: %v", err)
	}

	byKey := make(map[string]Line, len(got))
	for _, line := range got {
		byKey[lineKey(line)] = line
	}

	room, ok := byKey[RoomCategory+"|Ancient Doorway (Tier 1)"]
	if !ok {
		t.Fatalf("room line missing from %+v", got)
	}
	if room.Chaos != 12 || room.Listings != 140 {
		t.Errorf("room line = chaos %v, listings %d; want the newer snapshot's 12 / 140", room.Chaos, room.Listings)
	}

	vial, ok := byKey["Vial|Vial of Fate"]
	if !ok {
		t.Fatalf("Vial line missing; each category's newest time must be resolved on its own, not from one global MAX(time)")
	}
	if !vial.Time.Equal(older) {
		t.Errorf("vial time = %v, want %v", vial.Time, older)
	}
}

func TestNewestLines_readsOnlyTheScopedLeague(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	const otherLeagueID = "POE-255-Newest-Other-League"
	registerLeague(t, pool, otherLeagueID)

	at := time.Now().UTC().Add(-2 * time.Hour).Truncate(time.Microsecond)
	cleanupAt(t, pool, []string{integrationScope.ID(), otherLeagueID}, at)

	insertLine(t, pool, integrationScope.ID(), at, "Vial", "Vial of Fate", 990010, 1, 0, 343)
	insertLine(t, pool, otherLeagueID, at, "Vial", "Vial of Fate", 990011, 999, 0, 1)

	got, err := repo.NewestLines(context.Background(), integrationScope)
	if err != nil {
		t.Fatalf("NewestLines: %v", err)
	}

	for _, line := range got {
		if line.Chaos == 999 {
			t.Fatalf("read another league's row: %+v", line)
		}
	}
	found := false
	for _, line := range got {
		if line.Name == "Vial of Fate" && line.Time.Equal(at) {
			found = true
		}
	}
	if !found {
		t.Errorf("the scoped league's own row was not returned")
	}
}

func TestNewestLines_readsEveryColumnTheAggregationPrices(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	at := time.Now().UTC().Add(-90 * time.Minute).Truncate(time.Microsecond)
	cleanupAt(t, pool, []string{integrationScope.ID()}, at)

	insertLine(t, pool, integrationScope.ID(), at, "UniqueJewel", "Transcendent Mind", 990020, 318.5, 0.75, 98)

	got, err := repo.NewestLines(context.Background(), integrationScope)
	if err != nil {
		t.Fatalf("NewestLines: %v", err)
	}

	for _, line := range got {
		if line.Name != "Transcendent Mind" {
			continue
		}
		if line.Category != "UniqueJewel" || line.Chaos != 318.5 || line.Divine != 0.75 || line.Listings != 98 || !line.Time.Equal(at) {
			t.Errorf("line = %+v, want category UniqueJewel, chaos 318.5, divine 0.75, listings 98 at %v", line, at)
		}
		return
	}
	t.Fatalf("Transcendent Mind not returned")
}

func TestWindowPrices_returnsEveryPrintInsideTheWindowAndNoneOlder(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	inside := []time.Time{
		time.Now().UTC().Add(-1 * time.Hour).Truncate(time.Microsecond),
		time.Now().UTC().Add(-5 * time.Hour).Truncate(time.Microsecond),
		time.Now().UTC().Add(-23 * time.Hour).Truncate(time.Microsecond),
	}
	// Past the trailing window. Its price is far from the others so an unbounded
	// read moves the median it is excluded from.
	outside := time.Now().UTC().Add(-(WindowHours + 2) * time.Hour).Truncate(time.Microsecond)
	cleanupAt(t, pool, []string{integrationScope.ID()}, append(append([]time.Time{}, inside...), outside)...)

	for i, at := range inside {
		insertLine(t, pool, integrationScope.ID(), at, "Vial", "Vial of the Ghost", 990030, float64(1500+i*100), 4, 7)
	}
	insertLine(t, pool, integrationScope.ID(), outside, "Vial", "Vial of the Ghost", 990030, 99999, 4, 7)

	got, err := repo.WindowPrices(context.Background(), integrationScope,
		[]Line{{Category: "Vial", Name: "Vial of the Ghost"}})
	if err != nil {
		t.Fatalf("WindowPrices: %v", err)
	}

	prints := got["Vial|Vial of the Ghost"]
	if len(prints) != len(inside) {
		t.Fatalf("prints = %d (%+v), want the %d inside the %d-hour window", len(prints), prints, len(inside), WindowHours)
	}
	for _, p := range prints {
		if p.Chaos == 99999 {
			t.Errorf("a print from outside the window was returned: %+v", p)
		}
	}
}

func TestWindowPrices_readsOnlyTheScopedLeague(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	const otherLeagueID = "POE-255-Window-Other-League"
	registerLeague(t, pool, otherLeagueID)

	at := time.Now().UTC().Add(-2 * time.Hour).Truncate(time.Microsecond)
	cleanupAt(t, pool, []string{integrationScope.ID(), otherLeagueID}, at)

	insertLine(t, pool, integrationScope.ID(), at, "Vial", "Vial of Sacrifice", 990040, 422.3, 1, 11)
	insertLine(t, pool, otherLeagueID, at, "Vial", "Vial of Sacrifice", 990041, 88888, 1, 11)

	got, err := repo.WindowPrices(context.Background(), integrationScope,
		[]Line{{Category: "Vial", Name: "Vial of Sacrifice"}})
	if err != nil {
		t.Fatalf("WindowPrices: %v", err)
	}

	for _, p := range got["Vial|Vial of Sacrifice"] {
		if p.Chaos == 88888 {
			t.Fatalf("read another league's print: %+v", p)
		}
	}
}

func TestWindowPrices_returnsNothingForNamesItWasNotAsked(t *testing.T) {
	// The window query is bounded by its name list, which is what keeps it one
	// small read rather than a scan of every unique in the league.
	pool := integrationPool(t)
	repo := NewRepository(pool)

	at := time.Now().UTC().Add(-2 * time.Hour).Truncate(time.Microsecond)
	cleanupAt(t, pool, []string{integrationScope.ID()}, at)

	insertLine(t, pool, integrationScope.ID(), at, "Vial", "Vial of Fate", 990050, 1, 0, 343)
	insertLine(t, pool, integrationScope.ID(), at, "Vial", "Vial of Dominance", 990051, 1, 0, 107)

	got, err := repo.WindowPrices(context.Background(), integrationScope,
		[]Line{{Category: "Vial", Name: "Vial of Fate"}})
	if err != nil {
		t.Fatalf("WindowPrices: %v", err)
	}

	if len(got["Vial|Vial of Fate"]) == 0 {
		t.Errorf("the requested name returned no prints")
	}
	if prints, ok := got["Vial|Vial of Dominance"]; ok {
		t.Errorf("returned %d prints for a name that was not asked for: %+v", len(prints), prints)
	}
}

func TestWindowPrices_noLinesMeansNoQueryAndAnEmptyResult(t *testing.T) {
	pool := integrationPool(t)
	repo := NewRepository(pool)

	got, err := repo.WindowPrices(context.Background(), integrationScope, nil)
	if err != nil {
		t.Fatalf("WindowPrices: %v", err)
	}
	if len(got) != 0 {
		t.Errorf("result = %+v, want empty — a confident market must not run the window query at all", got)
	}
	if got == nil {
		t.Errorf("result is nil; Build indexes it and a nil map would be a second shape to handle")
	}
}
