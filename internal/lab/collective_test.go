package lab

import (
	"testing"
	"time"
)

func TestRankCollective_ExcludesTRAP(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20", ROI: 50, BasePrice: 10, TransfiguredPrice: 60, Confidence: "OK"},
		{Time: now, TransfiguredName: "Cleave of Rage", BaseName: "Cleave", Variant: "20/20", ROI: 100, BasePrice: 5, TransfiguredPrice: 105, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE", PriceVelocity: 0, ListingVelocity: 0, CV: 10, HistPosition: 50},
		{Name: "Cleave of Rage", Variant: "20/20", Signal: "TRAP", PriceVelocity: 0, ListingVelocity: 0, CV: 200, HistPosition: 50},
	}

	results := RankCollective(transfigure, trends, 0, 50)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1 (TRAP excluded)", len(results))
	}
	if results[0].TransfiguredName != "Spark of Nova" {
		t.Errorf("got %s, want Spark of Nova", results[0].TransfiguredName)
	}
}

func TestRankCollective_DUMPINGPenalized(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Spark of Nova", Variant: "20/20", ROI: 50, BasePrice: 10, TransfiguredPrice: 60, Confidence: "OK"},
		{Time: now, TransfiguredName: "Cleave of Rage", Variant: "20/20", ROI: 100, BasePrice: 5, TransfiguredPrice: 105, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE"},
		{Name: "Cleave of Rage", Variant: "20/20", Signal: "DUMPING"},
	}

	results := RankCollective(transfigure, trends, 0, 50)

	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}
	// Spark: 50 * 1.0 = 50, Cleave: 100 * 0.3 = 30 → Spark first
	if results[0].TransfiguredName != "Spark of Nova" {
		t.Errorf("first result = %s, want Spark of Nova (DUMPING penalized)", results[0].TransfiguredName)
	}
	if results[1].WeightedROI != 30 {
		t.Errorf("Cleave weighted ROI = %f, want 30", results[1].WeightedROI)
	}
}

func TestRankCollective_BudgetFilter(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Cheap Gem", Variant: "20/20", ROI: 20, BasePrice: 10, Confidence: "OK"},
		{Time: now, TransfiguredName: "Expensive Gem", Variant: "20/20", ROI: 100, BasePrice: 60, Confidence: "OK"},
	}

	results := RankCollective(transfigure, nil, 50, 50)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1 (budget filter)", len(results))
	}
	if results[0].TransfiguredName != "Cheap Gem" {
		t.Errorf("got %s, want Cheap Gem", results[0].TransfiguredName)
	}
}

func TestRankCollective_ExcludesNegativeROIAndLowConfidence(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Good Gem", Variant: "20/20", ROI: 50, Confidence: "OK"},
		{Time: now, TransfiguredName: "Negative ROI", Variant: "20/20", ROI: -10, Confidence: "OK"},
		{Time: now, TransfiguredName: "Low Confidence", Variant: "20/20", ROI: 80, Confidence: "LOW"},
	}

	results := RankCollective(transfigure, nil, 0, 50)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].TransfiguredName != "Good Gem" {
		t.Errorf("got %s, want Good Gem", results[0].TransfiguredName)
	}
}

func TestRankCollective_SignalWeighting(t *testing.T) {
	now := time.Now()
	// All same ROI of 100 — sorting determined purely by signal weight.
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Falling Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{Time: now, TransfiguredName: "Recovery Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{Time: now, TransfiguredName: "Rising Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{Time: now, TransfiguredName: "Herd Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Falling Gem", Variant: "20/20", Signal: "FALLING"},
		{Name: "Recovery Gem", Variant: "20/20", Signal: "RECOVERY"},
		{Name: "Rising Gem", Variant: "20/20", Signal: "RISING"},
		{Name: "Herd Gem", Variant: "20/20", Signal: "HERD"},
	}

	results := RankCollective(transfigure, trends, 0, 50)

	if len(results) != 4 {
		t.Fatalf("got %d results, want 4", len(results))
	}

	// Expected order: RECOVERY (1.2), RISING (1.1), HERD (0.8), FALLING (0.6)
	expected := []string{"Recovery Gem", "Rising Gem", "Herd Gem", "Falling Gem"}
	for i, name := range expected {
		if results[i].TransfiguredName != name {
			t.Errorf("position %d: got %s, want %s", i, results[i].TransfiguredName, name)
		}
	}
}

func TestRankCollective_Limit(t *testing.T) {
	now := time.Now()
	transfigure := make([]TransfigureResult, 10)
	for i := range transfigure {
		transfigure[i] = TransfigureResult{
			Time: now, TransfiguredName: "Gem", Variant: "20/20",
			ROI: float64(i + 1), Confidence: "OK",
		}
	}

	results := RankCollective(transfigure, nil, 0, 3)
	if len(results) != 3 {
		t.Errorf("got %d results, want 3 (limit)", len(results))
	}
}

func TestRankCollective_NoTrendDataDefaultsStable(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Spark of Nova", Variant: "20/20", ROI: 50, Confidence: "OK"},
	}

	results := RankCollective(transfigure, nil, 0, 50)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Signal != "STABLE" {
		t.Errorf("signal = %s, want STABLE (default)", results[0].Signal)
	}
	if results[0].WeightedROI != 50 {
		t.Errorf("weighted ROI = %f, want 50 (1.0 weight)", results[0].WeightedROI)
	}
}

func TestBuildCompareResults_Recommendations(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Best Gem", BaseName: "Best", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{TransfiguredName: "OK Gem", BaseName: "OK", Variant: "20/20", ROI: 50, Confidence: "OK"},
		{TransfiguredName: "Dump Gem", BaseName: "Dump", Variant: "20/20", ROI: 200, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Best Gem", Variant: "20/20", Signal: "RISING"},
		{Name: "OK Gem", Variant: "20/20", Signal: "STABLE"},
		{Name: "Dump Gem", Variant: "20/20", Signal: "DUMPING"},
	}
	sparklines := map[string][]SparklinePoint{
		"Best Gem": {{Time: "2026-03-15T10:00:00Z", Price: 100, Listings: 10}},
	}

	names := []string{"Best Gem", "OK Gem", "Dump Gem"}
	results := BuildCompareResults(names, transfigure, trends, sparklines)

	if len(results) != 3 {
		t.Fatalf("got %d results, want 3", len(results))
	}

	// Build map by name for easier assertions.
	byName := make(map[string]CompareResult)
	for _, r := range results {
		byName[r.TransfiguredName] = r
	}

	// Best Gem: ROI 100 * 1.1 (RISING) = 110 → BEST
	if byName["Best Gem"].Recommendation != "BEST" {
		t.Errorf("Best Gem recommendation = %s, want BEST", byName["Best Gem"].Recommendation)
	}
	// OK Gem: ROI 50 * 1.0 (STABLE) = 50 → OK
	if byName["OK Gem"].Recommendation != "OK" {
		t.Errorf("OK Gem recommendation = %s, want OK", byName["OK Gem"].Recommendation)
	}
	// Dump Gem: DUMPING → AVOID
	if byName["Dump Gem"].Recommendation != "AVOID" {
		t.Errorf("Dump Gem recommendation = %s, want AVOID", byName["Dump Gem"].Recommendation)
	}

	// Check sparkline attached.
	if len(byName["Best Gem"].Sparkline) != 1 {
		t.Errorf("Best Gem sparkline length = %d, want 1", len(byName["Best Gem"].Sparkline))
	}
	// OK Gem has no sparkline data — should be empty slice, not nil.
	if byName["OK Gem"].Sparkline == nil {
		t.Error("OK Gem sparkline should be empty slice, not nil")
	}
}

func TestSignalWeight(t *testing.T) {
	tests := []struct {
		signal string
		want   float64
	}{
		{"TRAP", 0},
		{"DUMPING", 0.3},
		{"FALLING", 0.6},
		{"HERD", 0.8},
		{"STABLE", 1.0},
		{"RISING", 1.1},
		{"RECOVERY", 1.2},
		{"UNKNOWN", 1.0},
	}
	for _, tt := range tests {
		got := signalWeight(tt.signal)
		if got != tt.want {
			t.Errorf("signalWeight(%s) = %f, want %f", tt.signal, got, tt.want)
		}
	}
}
