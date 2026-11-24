//go:build integration

package lab

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// lensExceptionalFeedersReader is the capability under test, asserted at run
// time so a Repository without it fails an assertion instead of the build.
type lensExceptionalFeedersReader interface {
	LensExceptionalFeeders(ctx context.Context, scope league.Scope) (ExceptionalSupportSnapshot, error)
}

type lensFeederRow struct {
	league    string
	at        time.Time
	name      string
	variant   string
	corrupted bool
	chaos     *float64
}

func chaosOf(v float64) *float64 { return &v }

func lensFeederTime(hour int) time.Time {
	return time.Date(2099, 3, 14, hour, 0, 0, 0, time.UTC)
}

// registerLensFeederLeagues registers the fixture leagues and deletes only
// their gem_snapshots rows; the row cleanup is registered last so it runs
// before the league deletes (LIFO).
func registerLensFeederLeagues(t *testing.T, pool *pgxpool.Pool, ids ...string) {
	t.Helper()
	for _, id := range ids {
		registerLeague(t, pool, id)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM gem_snapshots WHERE league = ANY($1)`, ids); err != nil {
			t.Logf("cleanup warning: delete gem_snapshots for %v: %v", ids, err)
		}
	})
}

func seedLensFeederRows(t *testing.T, pool *pgxpool.Pool, rows []lensFeederRow) {
	t.Helper()
	for _, r := range rows {
		if _, err := pool.Exec(context.Background(), `
			INSERT INTO gem_snapshots
				(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
			VALUES ($1, $2, $3, $4, $5, false, $6, 10, '')`,
			r.league, r.at, r.name, r.variant, r.corrupted, r.chaos); err != nil {
			t.Fatalf("seed gem_snapshot (league %q, %s %s): %v", r.league, r.name, r.variant, err)
		}
	}
}

// latestLevel3Rows is the acceptance snapshot: Enhance 3=80/3c=12,
// Enlighten 165/20, Empower 150/7, whose per-gem minima are [12, 20, 7].
func latestLevel3Rows(leagueID string, at time.Time) []lensFeederRow {
	return []lensFeederRow{
		{leagueID, at, "Enhance Support", "3", false, chaosOf(80)},
		{leagueID, at, "Enhance Support", "3c", true, chaosOf(12)},
		{leagueID, at, "Enlighten Support", "3", false, chaosOf(165)},
		{leagueID, at, "Enlighten Support", "3c", true, chaosOf(20)},
		{leagueID, at, "Empower Support", "3", false, chaosOf(150)},
		{leagueID, at, "Empower Support", "3c", true, chaosOf(7)},
	}
}

func formatFeederPrices(prices []ExceptionalSupportPrice) string {
	out := ""
	for i, p := range prices {
		if i > 0 {
			out += ", "
		}
		if p.Chaos == nil {
			out += p.Name + "=nil"
		} else {
			out += fmt.Sprintf("%s=%v", p.Name, *p.Chaos)
		}
	}
	return "[" + out + "]"
}

// TestKeepCriticalLensExceptionalFeeders pins the level-3 feeder prices the
// Lens EV trusts. keep:critical — silent: a wrong row choice (older snapshot,
// level-1 or other-level variant, another league, an uncorrupted-only filter or
// a NULL read as zero) returns a plausible wrong feeder price that the Lens EV
// computes from without any error.
func TestKeepCriticalLensExceptionalFeeders(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()

	reader, ok := any(NewRepository(pool)).(lensExceptionalFeedersReader)
	if !ok {
		t.Fatalf("*lab.Repository has no LensExceptionalFeeders(ctx, league.Scope) (ExceptionalSupportSnapshot, error) method")
	}

	older, latest, otherLater := lensFeederTime(1), lensFeederTime(2), lensFeederTime(3)
	const (
		base        = "POE-474-lef-base"
		olderCheap  = "POE-474-lef-older"
		otherLevels = "POE-474-lef-levels"
		scoped      = "POE-474-lef-scoped"
		otherLeague = "POE-474-lef-other"
		missing     = "POE-474-lef-missing"
		nullChaos   = "POE-474-lef-null"
		noSnapshot  = "POE-474-lef-empty"
	)
	registerLensFeederLeagues(t, pool, base, olderCheap, otherLevels, scoped, otherLeague, missing, nullChaos, noSnapshot)

	cases := []struct {
		name     string
		league   string
		seed     []lensFeederRow
		want     [3]*float64
		wantAsOf time.Time
	}{
		{
			name:     "latest level-3 minimum per gem including corrupted 3c",
			league:   base,
			seed:     latestLevel3Rows(base, latest),
			want:     [3]*float64{chaosOf(12), chaosOf(20), chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "older cheaper rows are ignored",
			league: olderCheap,
			seed: append(latestLevel3Rows(olderCheap, latest),
				lensFeederRow{olderCheap, older, "Enhance Support", "3c", true, chaosOf(1)},
				lensFeederRow{olderCheap, older, "Enlighten Support", "3", false, chaosOf(2)},
				lensFeederRow{olderCheap, older, "Empower Support", "3c", true, chaosOf(3)},
			),
			want:     [3]*float64{chaosOf(12), chaosOf(20), chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "level-1 and other-level rows at the latest time are ignored",
			league: otherLevels,
			seed: append(latestLevel3Rows(otherLevels, latest),
				lensFeederRow{otherLevels, latest, "Enhance Support", "1c", true, chaosOf(1)},
				lensFeederRow{otherLevels, latest, "Enlighten Support", "1", false, chaosOf(2)},
				lensFeederRow{otherLevels, latest, "Empower Support", "4c", true, chaosOf(3)},
			),
			want:     [3]*float64{chaosOf(12), chaosOf(20), chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "another league's later cheaper snapshot is ignored",
			league: scoped,
			seed: append(latestLevel3Rows(scoped, latest),
				lensFeederRow{otherLeague, otherLater, "Enhance Support", "3c", true, chaosOf(1)},
				lensFeederRow{otherLeague, otherLater, "Enlighten Support", "3c", true, chaosOf(2)},
				lensFeederRow{otherLeague, otherLater, "Empower Support", "3c", true, chaosOf(3)},
			),
			want:     [3]*float64{chaosOf(12), chaosOf(20), chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "gem absent from the latest snapshot has no price",
			league: missing,
			seed: []lensFeederRow{
				{missing, older, "Enlighten Support", "3c", true, chaosOf(20)},
				{missing, latest, "Enhance Support", "3", false, chaosOf(80)},
				{missing, latest, "Enhance Support", "3c", true, chaosOf(12)},
				{missing, latest, "Empower Support", "3", false, chaosOf(150)},
				{missing, latest, "Empower Support", "3c", true, chaosOf(7)},
			},
			want:     [3]*float64{chaosOf(12), nil, chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "SQL NULL chaos is not a price",
			league: nullChaos,
			seed: []lensFeederRow{
				{nullChaos, latest, "Enhance Support", "3", false, chaosOf(80)},
				{nullChaos, latest, "Enhance Support", "3c", true, nil},
				{nullChaos, latest, "Enlighten Support", "3", false, nil},
				{nullChaos, latest, "Enlighten Support", "3c", true, nil},
				{nullChaos, latest, "Empower Support", "3c", true, chaosOf(7)},
			},
			want:     [3]*float64{chaosOf(80), nil, chaosOf(7)},
			wantAsOf: latest,
		},
		{
			name:   "league without a snapshot has a zero AsOf and no prices",
			league: noSnapshot,
		},
	}

	for _, tc := range cases {
		seedLensFeederRows(t, pool, tc.seed)
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			snap, err := reader.LensExceptionalFeeders(ctx, league.Historical(tc.league))
			if err != nil {
				t.Fatalf("LensExceptionalFeeders(%s): %v", tc.league, err)
			}
			if !snap.AsOf.Equal(tc.wantAsOf) {
				t.Errorf("AsOf = %v, want %v", snap.AsOf, tc.wantAsOf)
			}
			prices := snap.Prices()
			mismatch := len(prices) != len(ExceptionalSupportNames)
			for i := 0; !mismatch && i < len(prices); i++ {
				got, want := prices[i].Chaos, tc.want[i]
				mismatch = prices[i].Name != ExceptionalSupportNames[i] ||
					(got == nil) != (want == nil) ||
					(got != nil && !valuesClose(*got, *want))
			}
			if mismatch {
				wantPrices := make([]ExceptionalSupportPrice, len(ExceptionalSupportNames))
				for i, name := range ExceptionalSupportNames {
					wantPrices[i] = ExceptionalSupportPrice{Name: name, Chaos: tc.want[i]}
				}
				t.Errorf("Prices() = %s, want %s", formatFeederPrices(prices), formatFeederPrices(wantPrices))
			}
		})
	}
}
