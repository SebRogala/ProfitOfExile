//go:build integration

package lab

import (
	"context"
	"reflect"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// Double-corruption result persistence (POE-125). The helpers (labIntegrationPool,
// registerLeague, cleanupAtTime, futureTime) live in repository_integration_test.go.
//
// Every assertion here is designed to fail if the league predicate is removed
// from — or points at the wrong league in — the query it covers, which is the
// same contract the sibling repository integration tests keep.

func seedDoubleCorruptResult(t *testing.T, pool *pgxpool.Pool, leagueID string, tm time.Time,
	name, inputVariant string, profit float64) {
	t.Helper()
	repo := NewRepository(pool)
	row := DoubleCorruptResult{
		Time: tm, Name: name, InputVariant: inputVariant, Color: "BLUE",
		InputCost: 10, EV: profit + 10, EVRaw: profit + 10, Profit: profit,
		LiquidityRisk: "LOW", Model: DoubleCorruptModelEstimated,
		Outcomes: []DoubleCorruptOutcome{
			{Name: name, Variant: "21/20c", Probability: 0.5, Chaos: 300, Priced: true},
		},
	}
	if _, err := repo.SaveDoubleCorruptResults(context.Background(), league.Historical(leagueID),
		[]DoubleCorruptResult{row}); err != nil {
		t.Fatalf("seed double corrupt result (%s/%s): %v", leagueID, name, err)
	}
}

func TestSaveDoubleCorruptResults_storesScopeLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-125-save-dc"
	registerLeague(t, pool, leagueID)
	tm := futureTime(53)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	row := DoubleCorruptResult{
		Time: tm, Name: "Arc of Surging", InputVariant: "20/20", Color: "BLUE",
		LiquidityRisk: "LOW", Model: DoubleCorruptModelEstimated,
	}
	if _, err := repo.SaveDoubleCorruptResults(ctx, league.Historical(leagueID),
		[]DoubleCorruptResult{row}); err != nil {
		t.Fatalf("SaveDoubleCorruptResults: %v", err)
	}

	var stored string
	if err := pool.QueryRow(ctx,
		`SELECT league FROM double_corrupt_snapshots WHERE time = $1 AND name = $2 AND input_variant = $3`,
		tm, row.Name, row.InputVariant).Scan(&stored); err != nil {
		t.Fatalf("read stored league: %v", err)
	}
	if stored != leagueID {
		t.Errorf("stored league = %q, want %q (a hardcoded 'Mirage' write would fail here)", stored, leagueID)
	}
}

func TestSaveDoubleCorruptResults_roundTripsTheOutcomeBreakdown(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-125-roundtrip-dc"
	registerLeague(t, pool, leagueID)
	tm := futureTime(54)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	row := DoubleCorruptResult{
		Time: tm, Name: "Arc of Surging", InputVariant: "20/20", Color: "BLUE",
		InputCost: 100, TempleOverheadChaos: 7, HasVaalVersion: true,
		EV: 140.5, EVRaw: 162.25, Profit: 55.25,
		PricedProbability: 0.75, UnpricedProbability: 0.25, ThinOutcomeCells: 2,
		LiquidityRisk: "MEDIUM", Model: DoubleCorruptModelEstimated,
		Outcomes: []DoubleCorruptOutcome{
			{Name: "Vaal Arc (Arc of Surging)", Variant: "21/20c", Probability: 0.0625,
				Chaos: 1000, Adjusted: 900, Listings: 3, Priced: true, Thin: true},
			{Name: "Arc of Surging", Variant: "20c", Probability: 0.15625},
		},
	}
	if _, err := repo.SaveDoubleCorruptResults(ctx, league.Historical(leagueID),
		[]DoubleCorruptResult{row}); err != nil {
		t.Fatalf("SaveDoubleCorruptResults: %v", err)
	}

	got, err := repo.LatestDoubleCorruptResults(ctx, league.Historical(leagueID), "20/20", 10)
	if err != nil {
		t.Fatalf("LatestDoubleCorruptResults: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("got %d results, want 1", len(got))
	}
	if !reflect.DeepEqual(got[0].Outcomes, row.Outcomes) {
		t.Errorf("outcomes round-tripped as %+v, want %+v", got[0].Outcomes, row.Outcomes)
	}
	if !got[0].HasVaalVersion || got[0].TempleOverheadChaos != 7 ||
		got[0].Model != DoubleCorruptModelEstimated || got[0].LiquidityRisk != "MEDIUM" {
		t.Errorf("scalar columns round-tripped as hasVaal=%v overhead=%v model=%q risk=%q",
			got[0].HasVaalVersion, got[0].TempleOverheadChaos, got[0].Model, got[0].LiquidityRisk)
	}
	if got[0].EV != 140.5 || got[0].EVRaw != 162.25 || got[0].Profit != 55.25 ||
		got[0].PricedProbability != 0.75 || got[0].UnpricedProbability != 0.25 ||
		got[0].ThinOutcomeCells != 2 {
		t.Errorf("EV columns round-tripped as ev=%v evRaw=%v profit=%v priced=%v unpriced=%v thin=%d",
			got[0].EV, got[0].EVRaw, got[0].Profit,
			got[0].PricedProbability, got[0].UnpricedProbability, got[0].ThinOutcomeCells)
	}
}

func TestLatestDoubleCorruptResults_returnsOnlyScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	mine, theirs := "POE-125-read-mine", "POE-125-read-theirs"
	registerLeague(t, pool, mine)
	registerLeague(t, pool, theirs)
	tm := futureTime(55)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	seedDoubleCorruptResult(t, pool, mine, tm, "Arc of Surging", "20/20", 50)
	seedDoubleCorruptResult(t, pool, theirs, tm, "Spark of the Nova", "20/20", 9000)

	got, err := repo.LatestDoubleCorruptResults(ctx, league.Historical(mine), "", 10)
	if err != nil {
		t.Fatalf("LatestDoubleCorruptResults: %v", err)
	}
	if len(got) != 1 || got[0].Name != "Arc of Surging" {
		t.Fatalf("got %d results %+v, want only this league's row", len(got), got)
	}
}

func TestLatestDoubleCorruptResults_narrowsToTheRequestedInputVariant(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-125-variant-dc"
	registerLeague(t, pool, leagueID)
	tm := futureTime(56)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	seedDoubleCorruptResult(t, pool, leagueID, tm, "Arc of Surging", "20/20", 50)
	seedDoubleCorruptResult(t, pool, leagueID, tm, "Arc of Surging", "1/20", 4000)

	got, err := repo.LatestDoubleCorruptResults(ctx, league.Historical(leagueID), "20/20", 10)
	if err != nil {
		t.Fatalf("LatestDoubleCorruptResults: %v", err)
	}
	if len(got) != 1 || got[0].InputVariant != "20/20" {
		t.Fatalf("got %+v, want only the 20/20 row — a 1/20 gem is a different market", got)
	}
}

func TestDoubleCorruptResultsByNames_returnsOnlyTheNamedGemsOfTheScopedLeague(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	mine, theirs := "POE-125-names-mine", "POE-125-names-theirs"
	registerLeague(t, pool, mine)
	registerLeague(t, pool, theirs)
	tm := futureTime(57)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	seedDoubleCorruptResult(t, pool, mine, tm, "Arc of Surging", "20/20", 50)
	seedDoubleCorruptResult(t, pool, mine, tm, "Spark of the Nova", "20/20", 30)
	seedDoubleCorruptResult(t, pool, theirs, tm, "Arc of Surging", "20/20", 9000)

	got, err := repo.DoubleCorruptResultsByNames(ctx, league.Historical(mine), "20/20",
		[]string{"Arc of Surging"})
	if err != nil {
		t.Fatalf("DoubleCorruptResultsByNames: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("got %d results %+v, want 1", len(got), got)
	}
	if got[0].Name != "Arc of Surging" || got[0].Profit != 50 {
		t.Errorf("got %q at profit %v, want this league's Arc of Surging at 50",
			got[0].Name, got[0].Profit)
	}
}

func TestDoubleCorruptResultsByNames_narrowsToTheRequestedInputVariant(t *testing.T) {
	// One gem name, two input variants. A compare request is about one market,
	// so the 1/20 row must not answer for the 20/20 one.
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-125-names-variant"
	registerLeague(t, pool, leagueID)
	tm := futureTime(58)
	cleanupAtTime(t, pool, tm, "double_corrupt_snapshots")

	seedDoubleCorruptResult(t, pool, leagueID, tm, "Arc of Surging", "20/20", 50)
	seedDoubleCorruptResult(t, pool, leagueID, tm, "Arc of Surging", "1/20", 9000)

	got, err := repo.DoubleCorruptResultsByNames(ctx, league.Historical(leagueID), "20/20",
		[]string{"Arc of Surging"})
	if err != nil {
		t.Fatalf("DoubleCorruptResultsByNames: %v", err)
	}
	if len(got) != 1 {
		t.Fatalf("got %d results %+v, want only the 20/20 row", len(got), got)
	}
	if got[0].InputVariant != "20/20" || got[0].Profit != 50 {
		t.Errorf("got input variant %q at profit %v, want 20/20 at 50",
			got[0].InputVariant, got[0].Profit)
	}
}

func TestDoubleCorruptResultsByNames_rejectsAnUnnamedInputVariant(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	repo := NewRepository(pool)

	leagueID := "POE-125-novariant-dc"
	registerLeague(t, pool, leagueID)

	if _, err := repo.DoubleCorruptResultsByNames(ctx, league.Historical(leagueID), "",
		[]string{"Arc of Surging"}); err == nil {
		t.Fatal("no error for an empty input variant — the compare join must stay inside one market")
	}
}
