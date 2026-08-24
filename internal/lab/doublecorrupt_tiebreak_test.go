package lab

import "testing"

// The double-corruption tiebreaker in BuildCompareResults (POE-125): when the
// Font score names no winner, double-corrupt profit decides — and only then.

// tiebreakGem builds one compare candidate. signal/sellUrgency drive the
// existing disqualifiers; roi drives the score-based ranking.
type tiebreakGem struct {
	name        string
	roi         float64
	signal      string
	sellUrgency string
	dcProfit    float64
}

func tiebreakInput(gems ...tiebreakGem) ([]string, []TransfigureResult, []GemSignal, map[string]DoubleCorruptResult) {
	names := make([]string, 0, len(gems))
	transfigure := make([]TransfigureResult, 0, len(gems))
	signals := make([]GemSignal, 0, len(gems))
	dc := make(map[string]DoubleCorruptResult, len(gems))
	for _, g := range gems {
		names = append(names, g.name)
		transfigure = append(transfigure, TransfigureResult{
			TransfiguredName: g.name, BaseName: "Base", Variant: "20/20",
			GemColor: "BLUE", BasePrice: 10, TransfiguredPrice: 10 + g.roi,
			ROI: g.roi, ROIPct: g.roi * 10, Confidence: "OK",
		})
		signal := g.signal
		if signal == "" {
			signal = "STABLE"
		}
		signals = append(signals, GemSignal{
			Name: g.name, Variant: "20/20", Signal: signal,
			SellUrgency: g.sellUrgency, Sellability: 70, Tier: "MID",
		})
		if g.dcProfit != 0 {
			dc[g.name] = DoubleCorruptResult{
				Name: g.name, InputVariant: "20/20",
				EV: g.dcProfit + 10, EVRaw: g.dcProfit + 10, Profit: g.dcProfit,
				PricedProbability: 0.79, UnpricedProbability: 0.21,
				Model: DoubleCorruptModelEstimated,
			}
		}
	}
	return names, transfigure, signals, dc
}

func compareByName(t *testing.T, results []CompareResult) map[string]CompareResult {
	t.Helper()
	out := make(map[string]CompareResult, len(results))
	for _, r := range results {
		out[r.TransfiguredName] = r
	}
	return out
}

func TestBuildCompareResults_TiebreakPromotesTheBestDoubleCorruptCandidateWhenNoneWon(t *testing.T) {
	// The top-ranked gem is disqualified into AVOID, so the score pass names no
	// BEST at all. Of the two survivors, the higher double-corrupt profit wins —
	// note it is the LOWER-ROI one, so ROI cannot be what decided this.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Top ROI Caution", roi: 900, signal: "CAUTION"},
		tiebreakGem{name: "Middling", roi: 400, sellUrgency: "SELL_NOW"},
		tiebreakGem{name: "Weak Now Rich Corrupted", roi: 20, dcProfit: 800},
		tiebreakGem{name: "Weak Now Poor Corrupted", roi: 50, dcProfit: 30},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)
	byName := compareByName(t, results)

	winner := byName["Weak Now Rich Corrupted"]
	if winner.Recommendation != "BEST" {
		t.Errorf("recommendation = %q, want BEST for the highest double-corrupt profit", winner.Recommendation)
	}
	if !winner.DoubleCorruptTiebreak {
		t.Error("DoubleCorruptTiebreak = false — a tiebroken pick must say so, not read as an ordinary win")
	}
	if byName["Weak Now Poor Corrupted"].Recommendation == "BEST" {
		t.Error("the lower double-corrupt profit was promoted too")
	}
}

func TestBuildCompareResults_TiebreakDoesNotFireWhenTheScorePassNamedAWinner(t *testing.T) {
	// A clean top-ranked candidate takes BEST on its Font score. The other gem
	// has far higher double-corrupt profit and must not overturn it.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Clean Winner", roi: 900},
		tiebreakGem{name: "Corrupt Monster", roi: 10, dcProfit: 99999},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)
	byName := compareByName(t, results)

	if byName["Clean Winner"].Recommendation != "BEST" {
		t.Errorf("Clean Winner = %q, want BEST", byName["Clean Winner"].Recommendation)
	}
	if byName["Corrupt Monster"].Recommendation == "BEST" {
		t.Error("the tiebreaker overturned a Font score that already named a winner")
	}
	for _, r := range results {
		if r.DoubleCorruptTiebreak {
			t.Errorf("%q carries DoubleCorruptTiebreak although the score pass decided", r.TransfiguredName)
		}
	}
}

func TestBuildCompareResults_TiebreakNeverPromotesADisqualifiedCandidate(t *testing.T) {
	// Every candidate is AVOID. The disqualifiers are about the gem's own
	// market, and a high double-corrupt profit does not clear them.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Caution Rich Corrupted", roi: 500, signal: "CAUTION", dcProfit: 5000},
		tiebreakGem{name: "Sell Now", roi: 400, sellUrgency: "SELL_NOW", dcProfit: 3000},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)

	for _, r := range results {
		if r.Recommendation != "AVOID" {
			t.Errorf("%q = %q, want AVOID", r.TransfiguredName, r.Recommendation)
		}
		if r.DoubleCorruptTiebreak {
			t.Errorf("%q was tiebroken into a recommendation despite being disqualified", r.TransfiguredName)
		}
	}
}

func TestBuildCompareResults_TiebreakIgnoresANonProfitableDoubleCorrupt(t *testing.T) {
	// Losing chaos on the craft is a reason not to corrupt, never a reason to
	// pick a gem — so the comparison stays without a winner, as it was before.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Top ROI Caution", roi: 900, signal: "CAUTION"},
		tiebreakGem{name: "Corrupting Loses Money", roi: 20, dcProfit: -400},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)

	for _, r := range results {
		if r.Recommendation == "BEST" {
			t.Errorf("%q was promoted on a negative double-corrupt profit", r.TransfiguredName)
		}
		if r.DoubleCorruptTiebreak {
			t.Errorf("%q carries the tiebreak flag on a negative profit", r.TransfiguredName)
		}
	}
}

func TestBuildCompareResults_TiebreakStaysOffWithoutDoubleCorruptData(t *testing.T) {
	// The pre-POE-125 shape: a variant the calculator does not model passes nil,
	// and a comparison with no surviving winner keeps naming none.
	names, transfigure, signals, _ := tiebreakInput(
		tiebreakGem{name: "Top ROI Caution", roi: 900, signal: "CAUTION"},
		tiebreakGem{name: "Survivor", roi: 20},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", nil)

	for _, r := range results {
		if r.DoubleCorruptTiebreak {
			t.Errorf("%q was tiebroken with no double-corruption data supplied", r.TransfiguredName)
		}
	}
	if byName := compareByName(t, results); byName["Survivor"].Recommendation == "BEST" {
		t.Error("Survivor = BEST without the tiebreaker — the score pass promotes only the top-ranked candidate")
	}
}

func TestBuildCompareResults_CarriesDoubleCorruptEVOnEveryPricedCandidate(t *testing.T) {
	// The numbers reach the UI whether or not the tiebreaker fires: "weak as
	// 20/20, strong double-corrupt candidate" is a badge on a candidate, not a
	// property of the winner. The model marker travels with them so no surface
	// can present community-sourced odds as confirmed.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Clean Winner", roi: 900},
		tiebreakGem{name: "Corrupt Candidate", roi: 10, dcProfit: 700},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)
	byName := compareByName(t, results)

	candidate := byName["Corrupt Candidate"]
	if candidate.DoubleCorruptProfit != 700 || candidate.DoubleCorruptEV != 710 {
		t.Errorf("profit/EV = %v/%v, want 700/710", candidate.DoubleCorruptProfit, candidate.DoubleCorruptEV)
	}
	if candidate.DoubleCorruptModel != DoubleCorruptModelEstimated {
		t.Errorf("model marker = %q, want %q", candidate.DoubleCorruptModel, DoubleCorruptModelEstimated)
	}
	if winner := byName["Clean Winner"]; winner.DoubleCorruptProfit != 0 || winner.DoubleCorruptModel != "" {
		t.Errorf("a gem the calculator did not price carries profit %v model %q, want zero values",
			winner.DoubleCorruptProfit, winner.DoubleCorruptModel)
	}
}

func TestBuildCompareResults_CarriesTheShareOfOutcomesTheEVCovers(t *testing.T) {
	// The EV is a floor over the priced share of the distribution, not the whole
	// expectation, so the share has to travel with it. Left behind here, the
	// comparator has no honest way to qualify the number it prints.
	names, transfigure, signals, dc := tiebreakInput(
		tiebreakGem{name: "Corrupt Candidate", roi: 10, dcProfit: 700},
	)

	results := BuildCompareResults(names, transfigure, signals, nil, nil, "20/20", dc)

	candidate := compareByName(t, results)["Corrupt Candidate"]
	if candidate.DoubleCorruptPricedProbability != 0.79 {
		t.Errorf("DoubleCorruptPricedProbability = %v, want 0.79", candidate.DoubleCorruptPricedProbability)
	}
}

// The Dedication twin (BuildDedicationCompareResults) carries a near-identical
// recommendation loop that must NOT grow this tiebreaker — a gem it compares is
// already corrupted and cannot be double-corrupted at all.
//
// No test covers that here, deliberately: the rule is enforced by the signature,
// not by a branch. BuildDedicationCompareResults takes no double-corruption
// argument, so every candidate it builds carries zero-valued fields no matter
// what the loop does, and a test asserting those zeros would be asserting Go's
// zero value rather than any decision this package makes — a test that cannot
// fail for a code-level reason. The smallest refactor that would create a seam
// is giving that function the same map parameter, which is the very coupling the
// rule exists to prevent. The constraint is stated at applyDoubleCorruptTiebreak
// instead, next to the code a future edit would touch.
