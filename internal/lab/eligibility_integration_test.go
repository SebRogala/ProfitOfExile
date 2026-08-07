//go:build integration

package lab

import (
	"context"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// The Dedication picker was the surface where the eligibility rules diverged
// furthest: it applied the support and Heist rules in SQL and the colour rule
// nowhere, so it offered gems the craft can never hand out. Measured 2026-08-05
// on the local Allflame snapshot, the skill half returned Pact of Beidat, Ghorr,
// K'Tash and Lycia — the four WHITE 3.29 gems isDedicationOutcome excludes from
// the pool, the tiers and the rankings.
func TestCorruptedGemNamesAutocomplete_excludesWhiteGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-142-ac-white"
	registerLeague(t, pool, leagueID)

	tm := futureTime(42)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	seedGemSnapshot(t, pool, leagueID, tm, "POE142 Pact of Beidat", "21/23c", false, true, 34220, 20, "WHITE")
	seedGemSnapshot(t, pool, leagueID, tm, "POE142 Cyclone", "21/23c", false, true, 300, 12, "GREEN")

	names, err := repo.CorruptedGemNamesAutocomplete(ctx, league.Historical(leagueID), false, 50)
	if err != nil {
		t.Fatalf("CorruptedGemNamesAutocomplete: %v", err)
	}

	if len(names) != 1 || names[0] != "POE142 Cyclone" {
		t.Errorf("names = %v, want only [POE142 Cyclone] — a white gem has no attribute colour "+
			"and the Dedication cannot hand one out", names)
	}
}

// The other reason the picker is not simply isDedicationOutcome in SQL, and the
// case that the WHITE/three-colour distinction turns on.
//
// The collector writes a NULL gem_color for every name the gemcolor resolver has
// not met yet (internal/collector/repository.go), and the empty string reaches
// the same query through any row seeded without one. Both are a data gap rather
// than a game rule, so
// the picker must still offer the name — a typeahead partitions nothing by
// colour, and a player typing a name the resolver is behind on would otherwise
// get zero results.
//
// Measured 2026-08-05 on the 05:28 Allflame snapshot, taken while the resolver
// was still behind: gating this query on the three attribute colours instead
// dropped Dark Bargain and Mana-Infused Staff, both NULL there and both resolved
// BLUE by the 12:39 snapshot.
func TestCorruptedGemNamesAutocomplete_offersUnresolvedColourGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-142-ac-nullcolour"
	registerLeague(t, pool, leagueID)

	tm := futureTime(44)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	seedGemSnapshotNoColor(t, pool, leagueID, tm, "POE142 Dark Bargain", "21/23c", false, true, 400, 9)
	seedGemSnapshot(t, pool, leagueID, tm, "POE142 Mana-Infused Staff", "21/23c", false, true, 350, 7, "")
	seedGemSnapshot(t, pool, leagueID, tm, "POE142 Pact of Ghorr", "21/23c", false, true, 12000, 3, "WHITE")

	names, err := repo.CorruptedGemNamesAutocomplete(ctx, league.Historical(leagueID), false, 50)
	if err != nil {
		t.Fatalf("CorruptedGemNamesAutocomplete: %v", err)
	}

	want := []string{"POE142 Dark Bargain", "POE142 Mana-Infused Staff"}
	if len(names) != len(want) || names[0] != want[0] || names[1] != want[1] {
		t.Errorf("names = %v, want %v — an unresolved colour is the resolver being behind, "+
			"not a reason the Dedication cannot hand the gem out; only the WHITE gem is a rule", names, want)
	}
}

// seedGemSnapshotNoColor seeds a row with gem_color SQL NULL, which is what the
// collector writes for a name the gemcolor resolver has not placed yet. The
// shared seedGemSnapshot takes a string and so can only ever write the empty
// string, and the two are not the same value to a `gem_color <> 'WHITE'` term.
func seedGemSnapshotNoColor(t *testing.T, pool *pgxpool.Pool, leagueID string, tm time.Time,
	name, variant string, isTransfigured, isCorrupted bool, chaos float64, listings int) {
	t.Helper()
	_, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots
			(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL)`,
		leagueID, tm, name, variant, isCorrupted, isTransfigured, chaos, listings)
	if err != nil {
		t.Fatalf("seed gem_snapshot with NULL colour (league %q, %s): %v", leagueID, name, err)
	}
}

// The Font (normal-mode) picker is the other half of the same failure, and it
// was the worse half: GemNamesAutocomplete carried no eligibility rule at all
// beyond is_transfigured, which made it the widest unfiltered name source
// feeding a gem icon. Measured 2026-08-05 on the latest local Allflame snapshot
// it offered 247 names, 45 of them corrupted "Vaal <Base> (<Transfigured>)"
// market identities the Font cannot hand out and /api/gem-icon has no entry for.
//
// One subtest per rule, each seeding the ineligible name beside an eligible
// control under its own league: dropping any single SQL fragment fails exactly
// the subtest that names it, and a query broken outright fails every one of them
// on the control instead.
func TestGemNamesAutocomplete_excludesGemsTheFontCannotHandOut(t *testing.T) {
	const control = "POE145 Cyclone of Tumult"

	cases := []struct {
		rule      string
		leagueID  string
		day       int
		name      string
		corrupted bool
		color     string
		why       string
	}{
		{
			rule:      "corrupted",
			leagueID:  "POE-145-fac-corrupted",
			day:       45,
			name:      "Vaal POE145 Arc (POE145 Arc of Surging)",
			corrupted: true,
			color:     "BLUE",
			why:       "the Font rerolls an uncorrupted transfigured gem; a corrupted one is not an outcome",
		},
		{
			rule:     "support",
			leagueID: "POE-145-fac-support",
			day:      46,
			name:     "POE145 Empower Support",
			color:    "RED",
			why:      "both lab gem crafts hand out skill gems only",
		},
		{
			rule:     "heist",
			leagueID: "POE-145-fac-heist",
			day:      47,
			name:     "POE145 Sunder of Trarthus",
			color:    "RED",
			why:      "a Trarthus gem drops only from Heist blueprints",
		},
		{
			rule:     "white",
			leagueID: "POE-145-fac-white",
			day:      48,
			name:     "POE145 Pact of Beidat",
			color:    "WHITE",
			why:      "the craft hands out a gem of the same colour, and white has none",
		},
	}

	for _, tc := range cases {
		t.Run(tc.rule, func(t *testing.T) {
			pool := labIntegrationPool(t)
			ctx := context.Background()
			repo := NewRepository(pool)

			registerLeague(t, pool, tc.leagueID)
			tm := futureTime(tc.day)
			cleanupAtTime(t, pool, tm, "gem_snapshots")

			seedGemSnapshot(t, pool, tc.leagueID, tm, tc.name, "20/20", true, tc.corrupted, 400, 11, tc.color)
			seedGemSnapshot(t, pool, tc.leagueID, tm, control, "20/20", true, false, 300, 12, "GREEN")

			names, err := repo.GemNamesAutocomplete(ctx, league.Historical(tc.leagueID), "POE145", 50)
			if err != nil {
				t.Fatalf("GemNamesAutocomplete: %v", err)
			}

			if len(names) != 1 || names[0] != control {
				t.Errorf("names = %v, want only [%s] — %q must not be offered: %s",
					names, control, tc.name, tc.why)
			}
		})
	}
}

// The boundary the WHITE rule turns on, pinned separately on this surface for
// the same reason it is pinned on the Dedication picker: an unresolved colour is
// the gemcolor resolver being behind, not a game rule. The collector writes a
// NULL gem_color for a name it has not placed yet, and reaching for
// sqlHasAttributeColor here would blank the Font picker for exactly the new gems
// at a league start.
func TestGemNamesAutocomplete_offersUnresolvedColourGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-145-fac-nullcolour"
	registerLeague(t, pool, leagueID)

	tm := futureTime(49)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	seedGemSnapshotNoColor(t, pool, leagueID, tm, "POE145 Chain Hook of Angling", "20/20", true, false, 400, 9)
	seedGemSnapshot(t, pool, leagueID, tm, "POE145 Pact of Lycia", "20/20", true, false, 12000, 3, "WHITE")

	names, err := repo.GemNamesAutocomplete(ctx, league.Historical(leagueID), "POE145", 50)
	if err != nil {
		t.Fatalf("GemNamesAutocomplete: %v", err)
	}

	if len(names) != 1 || names[0] != "POE145 Chain Hook of Angling" {
		t.Errorf("names = %v, want only [POE145 Chain Hook of Angling] — a NULL colour is a data gap "+
			"the resolver will fill, not a reason the Font cannot hand the gem out", names)
	}
}

// The rule above only bites because the query reads one snapshot. gem_color is a
// per-row value the collector writes from whatever the gemcolor resolver knew at
// insert time (internal/collector/repository.go), not a lookup of current state,
// so a name the resolver placed late carries NULL on every row before that — and
// an unbounded DISTINCT over the league sees those rows for the rest of it.
//
// Measured 2026-08-05 on the local DB: Pact of Beidat, Ghorr, K'Tash and Lycia
// each carry 8-10 NULL rows and ~100 WHITE ones under Allflame, so the
// readmission this pins is live in the data. It misses this surface today only
// because none of the four is transfigured.
//
// The single-row seed of the white subtest above cannot reach it: one row is
// both the earliest and the latest.
func TestGemNamesAutocomplete_excludesAGemColouredWhiteAfterAnEarlierSnapshot(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-145-fac-latecolour"
	registerLeague(t, pool, leagueID)

	early, latest := futureTime(50), futureTime(51)
	cleanupAtTime(t, pool, early, "gem_snapshots")
	cleanupAtTime(t, pool, latest, "gem_snapshots")

	const white = "POE145 Pact of Beidat"
	const control = "POE145 Cyclone of Tumult"

	// The same gem twice: unresolved when it was first snapshotted, WHITE once
	// the resolver caught up.
	seedGemSnapshotNoColor(t, pool, leagueID, early, white, "20/20", true, false, 12000, 3)
	seedGemSnapshot(t, pool, leagueID, latest, white, "20/20", true, false, 12000, 3, "WHITE")
	seedGemSnapshot(t, pool, leagueID, latest, control, "20/20", true, false, 300, 12, "GREEN")

	names, err := repo.GemNamesAutocomplete(ctx, league.Historical(leagueID), "POE145", 50)
	if err != nil {
		t.Fatalf("GemNamesAutocomplete: %v", err)
	}

	if len(names) != 1 || names[0] != control {
		t.Errorf("names = %v, want only [%s] — %q is WHITE at the latest snapshot, and the NULL "+
			"colour on its earlier row is the resolver being behind then, not an answer about it now",
			names, control, white)
	}
}

// Which bound, and why this one. CorruptedGemNamesAutocomplete uses `time >
// NOW() - INTERVAL '2 hours'`; this query cannot, because it also fills the warm
// pool the picker answers from (Cache.SetGemNamePool). A pool that empties when
// the collector falls two hours behind blanks the picker while the compare card
// behind it still answers from that same snapshot — the analysis path reads
// MAX(time) and has no staleness cutoff at all.
//
// Seeded in the past rather than through futureTime for that reason: the point
// is a snapshot old enough that an interval bound would have dropped it.
func TestGemNamesAutocomplete_offersTheLatestSnapshotHoweverStale(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-145-fac-stale"
	registerLeague(t, pool, leagueID)

	// Before this project had a database, so no real row shares the timestamp
	// cleanupAtTime deletes across leagues.
	stale := time.Date(2019, 3, 14, 9, 26, 53, 0, time.UTC)
	cleanupAtTime(t, pool, stale, "gem_snapshots")

	const name = "POE145 Cyclone of Tumult"
	seedGemSnapshot(t, pool, leagueID, stale, name, "20/20", true, false, 300, 12, "GREEN")

	names, err := repo.GemNamesAutocomplete(ctx, league.Historical(leagueID), "POE145", 50)
	if err != nil {
		t.Fatalf("GemNamesAutocomplete: %v", err)
	}

	if len(names) != 1 || names[0] != name {
		t.Errorf("names = %v, want [%s] — the picker offers the latest snapshot's gems no matter "+
			"how old it is; going empty on a stalled collector leaves the player unable to type "+
			"the gem the compare card would still price", names, name)
	}
}

// The half of the picker every test above misses. GemNamesAutocomplete is the
// COLD path; once a transfigure tick lands, GemNamesSearch answers from the warm
// pool and the database is not consulted again for the life of the process
// (POE-152). The pool used to be derived from the transfigure results, whose
// gate is isTransfigurePairCandidate — no support rule, no WHITE rule — so the
// two halves applied different eligibility and only the unused one was tested.
//
// This runs the tick rather than calling the setter, because the wiring is the
// thing under test: what fails here is RunTransfigure filling the pool from
// anything other than GemNamesAutocomplete.
//
// One subtest per rule the transfigure gate lacks, each with its own league.
func TestRunTransfigure_warmPoolExcludesGemsTheFontCannotHandOut(t *testing.T) {
	const control = "POE145 Cyclone of Tumult"

	cases := []struct {
		rule     string
		leagueID string
		day      int
		name     string
		color    string
		why      string
	}{
		{
			rule:     "support",
			leagueID: "POE-145-warm-support",
			day:      52,
			name:     "POE145 Empower Support",
			color:    "RED",
			why:      "both lab gem crafts hand out skill gems only",
		},
		{
			rule:     "white",
			leagueID: "POE-145-warm-white",
			day:      53,
			name:     "POE145 Pact of Beidat",
			color:    "WHITE",
			why:      "the craft hands out a gem of the same colour, and white has none",
		},
	}

	for _, tc := range cases {
		t.Run(tc.rule, func(t *testing.T) {
			pool := labIntegrationPool(t)
			ctx := context.Background()
			repo := NewRepository(pool)
			scope := league.Historical(tc.leagueID)

			registerLeague(t, pool, tc.leagueID)
			tm := futureTime(tc.day)
			cleanupAtTime(t, pool, tm, "gem_snapshots", "transfigure_results")

			seedGemSnapshot(t, pool, tc.leagueID, tm, tc.name, "20/20", true, false, 400, 11, tc.color)
			seedGemSnapshot(t, pool, tc.leagueID, tm, control, "20/20", true, false, 300, 12, "GREEN")

			cache := NewCache(scope)
			if err := NewAnalyzer(repo, nil, cache, nil).RunTransfigure(ctx, scope); err != nil {
				t.Fatalf("RunTransfigure: %v", err)
			}

			names, ok := cache.For(scope).GemNamesSearch("POE145", 50)
			if !ok {
				t.Fatal("GemNamesSearch reports cold after a completed tick: the tick did not store the pool, " +
					"so every keystroke goes back to the hypertable")
			}
			if len(names) != 1 || names[0] != control {
				t.Errorf("warm pool = %v, want only [%s] — %q must not be offered: %s",
					names, control, tc.name, tc.why)
			}
		})
	}
}

// The counterpart, and the reason the picker is not simply isDedicationOutcome
// in SQL: a Vaal gem carries an attribute colour, is a legal Dedication feed,
// and BuildDedicationCorpus documents the compare path as answering for it and
// marking it not-an-outcome. Dropping it from the picker would take away the one
// non-outcome a player has a reason to look up.
func TestCorruptedGemNamesAutocomplete_offersVaalGems(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-142-ac-vaal"
	registerLeague(t, pool, leagueID)

	tm := futureTime(43)
	cleanupAtTime(t, pool, tm, "gem_snapshots")

	seedGemSnapshot(t, pool, leagueID, tm, "Vaal POE142 Blade Vortex", "21/20c", false, true, 500, 14, "GREEN")

	names, err := repo.CorruptedGemNamesAutocomplete(ctx, league.Historical(leagueID), false, 50)
	if err != nil {
		t.Fatalf("CorruptedGemNamesAutocomplete: %v", err)
	}

	if len(names) != 1 || names[0] != "Vaal POE142 Blade Vortex" {
		t.Errorf("names = %v, want [Vaal POE142 Blade Vortex] — the compare path answers for Vaal gems", names)
	}
}

// eligibility.go carries each rule twice, once in Go and once as a SQL fragment,
// because a query cannot call a Go function. Nothing but this test stops the two
// halves drifting — and they had already drifted before POE-142: the picker's
// `LIKE '%Support%'` matched a substring where isSupportGemName matches a suffix.
func TestSQLExclusionsMatchGoPredicates(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()

	rules := []struct {
		name     string
		fragment string
		keep     func(GemPrice) bool
	}{
		{"sqlNotSupportGem vs isSupportGemName", sqlNotSupportGem,
			func(g GemPrice) bool { return !isSupportGemName(g.Name) }},
		{"sqlNotHeistOnly vs isHeistOnlyGemName", sqlNotHeistOnly,
			func(g GemPrice) bool { return !isHeistOnlyGemName(g.Name) }},
		{"sqlHasAttributeColor vs hasAttributeColor", sqlHasAttributeColor, hasAttributeColor},
		{"sqlNotColorlessByRule vs isColorlessByRule", sqlNotColorlessByRule,
			func(g GemPrice) bool { return !isColorlessByRule(g) }},
	}

	// "Support Cyclone" and "Supportive" are the cases the pre-POE-142
	// `LIKE '%Support%'` and `strings.Contains` forms got wrong; the NULL colour
	// is the one the fragment's COALESCE exists for. The two colour fragments
	// part company on exactly that NULL — sqlNotColorlessByRule keeps it,
	// sqlHasAttributeColor drops it — which is why they are separate constants
	// and both are pinned here.
	gemNames := []string{
		"Cyclone", "Empower Support", "Support Cyclone", "Supportive",
		"Sunder of Trarthus", "Trarthus Ire", "Vaal Arc", "Pact of Beidat",
	}
	gemColors := []*string{ptr("RED"), ptr("GREEN"), ptr("BLUE"), ptr("WHITE"), ptr(""), nil}

	for _, rule := range rules {
		for _, name := range gemNames {
			for _, color := range gemColors {
				g := GemPrice{Name: name}
				shown := "NULL"
				if color != nil {
					g.GemColor = *color
					shown = `"` + *color + `"`
				}

				var sqlKeeps bool
				if err := pool.QueryRow(ctx,
					`SELECT `+rule.fragment+` FROM (SELECT $1::text AS name, $2::text AS gem_color) g`,
					name, color).Scan(&sqlKeeps); err != nil {
					t.Fatalf("evaluate %q for name %q colour %s: %v", rule.fragment, name, shown, err)
				}

				if want := rule.keep(g); sqlKeeps != want {
					t.Errorf("%s disagree on name %q colour %s: SQL keeps=%v, Go keeps=%v",
						rule.name, name, shown, sqlKeeps, want)
				}
			}
		}
	}
}

func ptr(s string) *string { return &s }

// The counter reaching an operator is the whole point of counting, and the wiring
// between the two is not something the pure predicate's tests can see: the count
// is computed in RunDedication and dies there unless it is logged.
//
// Integration rather than unit because RunDedication needs a real Repository —
// it loads the snapshot, persists results and refills two autocomplete pools —
// and because the seam under test is the analyzer's logger, which NewAnalyzer
// binds from the slog default at construction. Capturing after that point
// records nothing, so the capture is installed first and this test would still
// pass with the warning removed if it were arranged the other way round.
func TestRunDedication_warnsWithTheNamesDroppedForAnUnresolvedColour(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-161-dedication-colourgap"
	registerLeague(t, pool, leagueID)
	scope := league.Historical(leagueID)

	tm := futureTime(52)
	cleanupAtTime(t, pool, tm, "gem_snapshots", "dedication_snapshots")

	// One gem the pools should hold and do not, beside a coloured control so the
	// tick has a pool to analyse and the assertion reads on the gap alone.
	seedGemSnapshotNoColor(t, pool, leagueID, tm, "POE161 Mana-Infused Staff", "21/23c", false, true, 350, 7)
	seedGemSnapshot(t, pool, leagueID, tm, "POE161 Cyclone", "21/23c", false, true, 300, 12, "GREEN")

	logs := captureLogs(t)

	if err := NewAnalyzer(repo, nil, NewCache(scope), nil).RunDedication(ctx, scope); err != nil {
		t.Fatalf("RunDedication: %v", err)
	}

	var dropped []string
	for _, r := range logs.warnings() {
		if names, ok := logAttr(r, "names"); ok {
			if list, isList := names.Any().([]string); isList {
				dropped = list
			}
		}
	}

	if len(dropped) != 1 || dropped[0] != "POE161 Mana-Infused Staff" {
		t.Errorf("names logged for the colour gap = %v, want [POE161 Mana-Infused Staff] — the "+
			"gem left the Dedication pools, so EV, pWin and input cost were computed without it "+
			"and nothing else in the run says so", dropped)
	}
}
