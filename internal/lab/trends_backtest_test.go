package lab

// trends_backtest_test.go — regression tests derived from 40 hours of live production data.
//
// These fixtures were extracted from gem_snapshots via backtest_analysis.py on 2026-03-15.
// Each test embeds a real 8-snapshot slice (30-min cadence) from a production gem and
// asserts that AnalyzeTrends produces the correct signal at each evaluation point.
//
// Covered scenarios:
//   - HERD→DUMPING lifecycle  (Kinetic Blast of Clustering — peak herd then price collapse)
//   - HERD→FALLING lifecycle  (Elemental Hit of the Spectrum — sustained herd)
//   - RISING→HERD transition  (Shock Nova of Procession — gradual climb reaching herd threshold)
//   - STABLE flatline         (Artillery Ballista of Cross Strafe — price/listing inert)

import (
	"math"
	"testing"
	"time"
)

// mustParseUTC parses an RFC3339 timestamp and panics on failure.
func mustParseUTC(s string) time.Time {
	t, err := time.Parse(time.RFC3339Nano, s)
	if err != nil {
		panic("mustParseUTC: " + err.Error())
	}
	return t
}

// backtestFixture describes a real gem's 8-snapshot slice and the expected signal
// that AnalyzeTrends should emit at each evaluation point (using the first 5..8 points
// as rolling history, with the 5th point being the "current" snapshot).
type backtestExpectation struct {
	snapIdx int    // index into points slice (0-based) — this is the "current" snapshot
	signal  string // expected primary signal
}

type backtestFixture struct {
	gemName     string
	variant     string
	gemColor    string
	isTransfigured bool
	points      []PricePoint
	expectations []backtestExpectation
}

// productionFixtures are the four representative gems from the 40-hour backtest.
// Timestamps are real production snapshot times (UTC).
var productionFixtures = []backtestFixture{
	{
		// Kinetic Blast of Clustering (BLUE) — HERD→DUMPING lifecycle.
		// Price spikes from ~1110c to 1491c while listings surge 42→188, then collapses.
		// Signal correctly tracks the herd formation and subsequent dump.
		gemName: "Kinetic Blast of Clustering",
		variant: "20/20",
		gemColor: "BLUE",
		isTransfigured: true,
		points: []PricePoint{
			{Time: mustParseUTC("2026-03-15T16:15:22.872914+00:00"), Chaos: 1110.0, Listings: 42},
			{Time: mustParseUTC("2026-03-15T16:45:28.257946+00:00"), Chaos: 996.6,  Listings: 27},
			{Time: mustParseUTC("2026-03-15T17:15:33.560238+00:00"), Chaos: 1105.0, Listings: 28},
			{Time: mustParseUTC("2026-03-15T17:45:38.865870+00:00"), Chaos: 1454.0, Listings: 56},
			{Time: mustParseUTC("2026-03-15T18:15:16.767631+00:00"), Chaos: 1454.0, Listings: 75},
			{Time: mustParseUTC("2026-03-15T18:45:22.171276+00:00"), Chaos: 1454.0, Listings: 123},
			{Time: mustParseUTC("2026-03-15T19:15:27.572683+00:00"), Chaos: 1491.0, Listings: 131},
			{Time: mustParseUTC("2026-03-15T19:45:32.984609+00:00"), Chaos: 1218.0, Listings: 188},
		},
		expectations: []backtestExpectation{
			{snapIdx: 4, signal: "HERD"},    // price+listings both surging
			{snapIdx: 5, signal: "HERD"},    // herd continues
			{snapIdx: 6, signal: "HERD"},    // still accumulating supply
			{snapIdx: 7, signal: "DUMPING"}, // price crashes -18% while listings keep climbing
		},
	},
	{
		// Elemental Hit of the Spectrum (GREEN) — sustained HERD with listing flood.
		// Sustained large listing flood (283→350) at gradually rising price.
		// Time-based velocity (2h window) captures the full price trend, showing
		// consistent price+listing growth that correctly fires HERD throughout.
		// snap[4]: 2h window includes points 0-4, price 554.8→581.6 = RISING (lVel < 10/h)
		// snap[5]: 2h window sees listing surge to 348, HERD fires
		// snap[6-7]: sustained HERD as both velocities stay above thresholds
		gemName: "Elemental Hit of the Spectrum",
		variant: "20/20",
		gemColor: "GREEN",
		isTransfigured: true,
		points: []PricePoint{
			{Time: mustParseUTC("2026-03-15T16:15:22.872914+00:00"), Chaos: 554.8, Listings: 283},
			{Time: mustParseUTC("2026-03-15T16:45:28.257946+00:00"), Chaos: 560.0, Listings: 280},
			{Time: mustParseUTC("2026-03-15T17:15:33.560238+00:00"), Chaos: 581.6, Listings: 292},
			{Time: mustParseUTC("2026-03-15T17:45:38.865870+00:00"), Chaos: 581.6, Listings: 300},
			{Time: mustParseUTC("2026-03-15T18:15:16.767631+00:00"), Chaos: 581.6, Listings: 300},
			{Time: mustParseUTC("2026-03-15T18:45:22.171276+00:00"), Chaos: 581.6, Listings: 348},
			{Time: mustParseUTC("2026-03-15T19:15:27.572683+00:00"), Chaos: 596.4, Listings: 347},
			{Time: mustParseUTC("2026-03-15T19:45:32.984609+00:00"), Chaos: 609.0, Listings: 350},
		},
		expectations: []backtestExpectation{
			{snapIdx: 4, signal: "UNCERTAIN"}, // 2h window: pVel=13.4 but lVel=8.5 (<10/h threshold)
			{snapIdx: 5, signal: "HERD"},   // 2h window: listing surge drives lVel >10/h
			{snapIdx: 6, signal: "HERD"},   // sustained herd: price+listings both above thresholds
			{snapIdx: 7, signal: "HERD"},   // herd continues
		},
	},
	{
		// Shock Nova of Procession (BLUE) — RISING→HERD transition.
		// Steady price climb from 832c to 914c with growing listing pressure.
		// Time-based velocity (2h window) captures broader price trend, showing
		// consistent upward movement that was missed by the old 4-point window.
		// snap[4]: 2h window shows price+listing growth → RISING (lVel still < 10/h)
		// snap[5-6]: 2h window captures sustained listing surge → HERD
		// snap[7]: continued HERD with both velocities above thresholds
		gemName: "Shock Nova of Procession",
		variant: "20/20",
		gemColor: "BLUE",
		isTransfigured: true,
		points: []PricePoint{
			{Time: mustParseUTC("2026-03-15T16:15:22.872914+00:00"), Chaos: 832.2, Listings: 162},
			{Time: mustParseUTC("2026-03-15T16:45:28.257946+00:00"), Chaos: 840.0, Listings: 165},
			{Time: mustParseUTC("2026-03-15T17:15:33.560238+00:00"), Chaos: 872.4, Listings: 172},
			{Time: mustParseUTC("2026-03-15T17:45:38.865870+00:00"), Chaos: 872.4, Listings: 188},
			{Time: mustParseUTC("2026-03-15T18:15:16.767631+00:00"), Chaos: 872.4, Listings: 174},
			{Time: mustParseUTC("2026-03-15T18:45:22.171276+00:00"), Chaos: 872.4, Listings: 200},
			{Time: mustParseUTC("2026-03-15T19:15:27.572683+00:00"), Chaos: 894.6, Listings: 199},
			{Time: mustParseUTC("2026-03-15T19:45:32.984609+00:00"), Chaos: 913.5, Listings: 208},
		},
		expectations: []backtestExpectation{
			{snapIdx: 4, signal: "UNCERTAIN"}, // 2h window: pVel=20.1 but lVel=6.0 (<10/h threshold)
			{snapIdx: 5, signal: "HERD"},   // 2h window: listing surge drives lVel >10/h
			{snapIdx: 6, signal: "HERD"},   // sustained herd
			{snapIdx: 7, signal: "HERD"},   // herd continues with strong listing velocity
		},
	},
	{
		// Artillery Ballista of Cross Strafe (GREEN) — STABLE flatline.
		// Price holds 20c, listings barely tick. No signal noise.
		// Regression guard: velocity should stay near zero, CV stays 0.
		gemName: "Artillery Ballista of Cross Strafe",
		variant: "20/20",
		gemColor: "GREEN",
		isTransfigured: true,
		points: []PricePoint{
			{Time: mustParseUTC("2026-03-15T16:15:22.872914+00:00"), Chaos: 20.0, Listings: 11},
			{Time: mustParseUTC("2026-03-15T16:45:28.257946+00:00"), Chaos: 20.0, Listings: 11},
			{Time: mustParseUTC("2026-03-15T17:15:33.560238+00:00"), Chaos: 20.0, Listings: 11},
			{Time: mustParseUTC("2026-03-15T17:45:38.865870+00:00"), Chaos: 20.0, Listings: 11},
			{Time: mustParseUTC("2026-03-15T18:15:16.767631+00:00"), Chaos: 20.0, Listings: 11},
			{Time: mustParseUTC("2026-03-15T18:45:22.171276+00:00"), Chaos: 20.0, Listings: 12},
			{Time: mustParseUTC("2026-03-15T19:15:27.572683+00:00"), Chaos: 20.0, Listings: 12},
			{Time: mustParseUTC("2026-03-15T19:45:32.984609+00:00"), Chaos: 20.0, Listings: 12},
		},
		expectations: []backtestExpectation{
			{snapIdx: 4, signal: "STABLE"},
			{snapIdx: 5, signal: "STABLE"},
			{snapIdx: 6, signal: "STABLE"},
			{snapIdx: 7, signal: "STABLE"},
		},
	},
}

// TestAnalyzeTrends_Backtest runs AnalyzeTrends on each production fixture
// and asserts the expected signal at every evaluation point.
func TestAnalyzeTrends_Backtest(t *testing.T) {
	for _, fix := range productionFixtures {
		fix := fix // capture
		t.Run(fix.gemName, func(t *testing.T) {
			for _, exp := range fix.expectations {
				exp := exp
				t.Run("", func(t *testing.T) {
					snapIdx := exp.snapIdx
					if snapIdx >= len(fix.points) {
						t.Fatalf("snapIdx %d out of range (len=%d)", snapIdx, len(fix.points))
					}

					currentPt := fix.points[snapIdx]

					// current = just the gem at this snapshot
					current := []GemPrice{
						{
							Name:           fix.gemName,
							Variant:        fix.variant,
							Chaos:          currentPt.Chaos,
							Listings:       currentPt.Listings,
							IsTransfigured: fix.isTransfigured,
							GemColor:       fix.gemColor,
						},
					}

					// history = all points up to and including current snapshot
					history := []GemPriceHistory{
						{
							Name:     fix.gemName,
							Variant:  fix.variant,
							GemColor: fix.gemColor,
							Points:   fix.points[:snapIdx+1],
						},
					}

					results := AnalyzeTrends(currentPt.Time, current, history, nil, 0)

					if len(results) != 1 {
						t.Fatalf("snap[%d]: got %d results, want 1", snapIdx, len(results))
					}
					r := results[0]

					if r.Signal != exp.signal {
						t.Errorf("snap[%d] chaos=%.1f lst=%d: signal=%s, want %s (pVel=%.2f lVel=%.2f cv=%.2f)",
							snapIdx, currentPt.Chaos, currentPt.Listings,
							r.Signal, exp.signal,
							r.PriceVelocity, r.ListingVelocity, r.CV)
					}
				})
			}
		})
	}
}

// TestAnalyzeTrends_Backtest_HERD_to_DUMPING validates the full 8-point lifecycle
// of Kinetic Blast of Clustering in a single test: HERD×3 then DUMPING×1.
func TestAnalyzeTrends_Backtest_HERD_to_DUMPING(t *testing.T) {
	fix := productionFixtures[0] // Kinetic Blast of Clustering
	expectedSequence := []string{"HERD", "HERD", "HERD", "DUMPING"}

	for i, want := range expectedSequence {
		snapIdx := 4 + i
		currentPt := fix.points[snapIdx]
		current := []GemPrice{{
			Name: fix.gemName, Variant: fix.variant,
			Chaos: currentPt.Chaos, Listings: currentPt.Listings,
			IsTransfigured: true, GemColor: fix.gemColor,
		}}
		history := []GemPriceHistory{{
			Name: fix.gemName, Variant: fix.variant, GemColor: fix.gemColor,
			Points: fix.points[:snapIdx+1],
		}}

		results := AnalyzeTrends(currentPt.Time, current, history, nil, 0)
		if len(results) != 1 {
			t.Fatalf("step %d: no result", i)
		}
		if results[0].Signal != want {
			t.Errorf("step %d (snap[%d] %.0fc lst=%d): signal=%s, want %s",
				i, snapIdx, currentPt.Chaos, currentPt.Listings,
				results[0].Signal, want)
		}
	}
}

// TestAnalyzeTrends_Backtest_STABLE_CV_zero verifies that the Artillery Ballista
// fixture produces zero CV and zero price velocity throughout — a regression guard
// to confirm that flat price history never produces spurious TRAP signals.
func TestAnalyzeTrends_Backtest_STABLE_CV_zero(t *testing.T) {
	fix := productionFixtures[3] // Artillery Ballista of Cross Strafe

	for _, snapIdx := range []int{4, 5, 6, 7} {
		currentPt := fix.points[snapIdx]
		current := []GemPrice{{
			Name: fix.gemName, Variant: fix.variant,
			Chaos: currentPt.Chaos, Listings: currentPt.Listings,
			IsTransfigured: true, GemColor: fix.gemColor,
		}}
		history := []GemPriceHistory{{
			Name: fix.gemName, Variant: fix.variant, GemColor: fix.gemColor,
			Points: fix.points[:snapIdx+1],
		}}

		results := AnalyzeTrends(currentPt.Time, current, history, nil, 0)
		if len(results) != 1 {
			t.Fatalf("snap[%d]: no result", snapIdx)
		}
		r := results[0]

		if r.CV != 0 {
			t.Errorf("snap[%d]: CV=%f, want 0 (flat price → zero CV)", snapIdx, r.CV)
		}
		if math.Abs(r.PriceVelocity) > 0.001 {
			t.Errorf("snap[%d]: PriceVelocity=%f, want ~0 (flat price)", snapIdx, r.PriceVelocity)
		}
		if r.Signal == "TRAP" {
			t.Errorf("snap[%d]: flat price incorrectly classified as TRAP", snapIdx)
		}
	}
}

// TestAnalyzeTrends_Backtest_RISING_before_HERD validates that Shock Nova of Procession
// correctly emits RISING at snap[4] (listing velocity still below HERD threshold)
// then transitions to HERD at snap[5] once the 2h window captures enough listing growth.
func TestAnalyzeTrends_Backtest_UNCERTAIN_before_HERD(t *testing.T) {
	fix := productionFixtures[2] // Shock Nova of Procession

	// snap[4]: UNCERTAIN — 2h window: price velocity >5/h but listing velocity <10/h
	{
		snapIdx := 4
		currentPt := fix.points[snapIdx]
		current := []GemPrice{{
			Name: fix.gemName, Variant: fix.variant,
			Chaos: currentPt.Chaos, Listings: currentPt.Listings,
			IsTransfigured: true, GemColor: fix.gemColor,
		}}
		history := []GemPriceHistory{{
			Name: fix.gemName, Variant: fix.variant, GemColor: fix.gemColor,
			Points: fix.points[:snapIdx+1],
		}}
		results := AnalyzeTrends(currentPt.Time, current, history, nil, 0)
		if len(results) != 1 {
			t.Fatal("snap[4]: no result")
		}
		r := results[0]
		if r.Signal != "UNCERTAIN" {
			t.Errorf("snap[4] pre-HERD: signal=%s, want UNCERTAIN (listing vel not yet >10/h)", r.Signal)
		}
		if r.ListingVelocity >= 10 {
			t.Errorf("snap[4]: listing velocity %.2f should be <10/h (not yet HERD)", r.ListingVelocity)
		}
	}

	// snap[5]: HERD — 2h window captures listing surge >10/h
	{
		snapIdx := 5
		currentPt := fix.points[snapIdx]
		current := []GemPrice{{
			Name: fix.gemName, Variant: fix.variant,
			Chaos: currentPt.Chaos, Listings: currentPt.Listings,
			IsTransfigured: true, GemColor: fix.gemColor,
		}}
		history := []GemPriceHistory{{
			Name: fix.gemName, Variant: fix.variant, GemColor: fix.gemColor,
			Points: fix.points[:snapIdx+1],
		}}
		results := AnalyzeTrends(currentPt.Time, current, history, nil, 0)
		if len(results) != 1 {
			t.Fatal("snap[5]: no result")
		}
		r := results[0]
		if r.Signal != "HERD" {
			t.Errorf("snap[5] HERD threshold crossed: signal=%s, want HERD", r.Signal)
		}
		if r.PriceVelocity <= 5 {
			t.Errorf("snap[5]: price velocity %.2f should be >5/h for HERD", r.PriceVelocity)
		}
		if r.ListingVelocity <= 10 {
			t.Errorf("snap[5]: listing velocity %.2f should be >10/h for HERD", r.ListingVelocity)
		}
	}
}
