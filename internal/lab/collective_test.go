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
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE", PriceVelocity: 0, ListingVelocity: 0, CV: 10, HistPosition: 50, Sellability: 60},
		{Name: "Cleave of Rage", Variant: "20/20", Signal: "TRAP", PriceVelocity: 0, ListingVelocity: 0, CV: 200, HistPosition: 50, Sellability: 60},
	}

	results := RankCollective(transfigure, trends, 0, 50, "")

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
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE", Sellability: 80},
		{Name: "Cleave of Rage", Variant: "20/20", Signal: "DUMPING", Sellability: 80},
	}

	results := RankCollective(transfigure, trends, 0, 50, "")

	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}
	// Spark: 50 * 0.8 * 1.0 = 40 (STABLE, no saturation)
	// Cleave: 100 * 0.8 * 0.5 = 40 (DUMPING, 0.5 saturation penalty)
	// With equal WeightedROI, order depends on input order (sort is stable).
	// But Spark should rank >= Cleave because Spark has no saturation penalty.
	if results[0].TransfiguredName != "Spark of Nova" && results[0].TransfiguredName != "Cleave of Rage" {
		t.Errorf("unexpected first result = %s", results[0].TransfiguredName)
	}
	// Verify DUMPING penalty: liquidityScore=0.8, saturation=0.5 → 100*0.8*0.5=40
	for _, r := range results {
		if r.TransfiguredName == "Cleave of Rage" {
			expected := 100.0 * 0.8 * 0.5
			if r.WeightedROI != expected {
				t.Errorf("Cleave weighted ROI = %f, want %f", r.WeightedROI, expected)
			}
		}
	}
}

func TestRankCollective_BudgetFilter(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Cheap Gem", Variant: "20/20", ROI: 20, BasePrice: 10, Confidence: "OK"},
		{Time: now, TransfiguredName: "Expensive Gem", Variant: "20/20", ROI: 100, BasePrice: 60, Confidence: "OK"},
	}

	results := RankCollective(transfigure, nil, 50, 50, "")

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

	results := RankCollective(transfigure, nil, 0, 50, "")

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].TransfiguredName != "Good Gem" {
		t.Errorf("got %s, want Good Gem", results[0].TransfiguredName)
	}
}

func TestRankCollective_LiquidityScoring(t *testing.T) {
	now := time.Now()
	// All same ROI but different sellability — sorting by WeightedROI (liquidity-weighted).
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Low Sell Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{Time: now, TransfiguredName: "High Sell Gem", Variant: "20/20", ROI: 100, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Low Sell Gem", Variant: "20/20", Signal: "STABLE", Sellability: 30},
		{Name: "High Sell Gem", Variant: "20/20", Signal: "STABLE", Sellability: 80},
	}

	results := RankCollective(transfigure, trends, 0, 50, "")

	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}

	// High Sell Gem: 100 * 0.8 = 80, Low Sell Gem: 100 * 0.3 = 30 → High first.
	if results[0].TransfiguredName != "High Sell Gem" {
		t.Errorf("first result = %s, want High Sell Gem (higher liquidity)", results[0].TransfiguredName)
	}
	if results[0].WeightedROI != 80 {
		t.Errorf("High Sell weighted ROI = %f, want 80", results[0].WeightedROI)
	}
	if results[1].WeightedROI != 30 {
		t.Errorf("Low Sell weighted ROI = %f, want 30", results[1].WeightedROI)
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

	results := RankCollective(transfigure, nil, 0, 3, "")
	if len(results) != 3 {
		t.Errorf("got %d results, want 3 (limit)", len(results))
	}
}

func TestRankCollective_NoTrendDataDefaultsStable(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Spark of Nova", Variant: "20/20", ROI: 50, Confidence: "OK"},
	}

	results := RankCollective(transfigure, nil, 0, 50, "")

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Signal != "STABLE" {
		t.Errorf("signal = %s, want STABLE (default)", results[0].Signal)
	}
	// No trend data: Sellability defaults to 0 → liquidityScore=0 → WeightedROI=0.
	if results[0].WeightedROI != 0 {
		t.Errorf("weighted ROI = %f, want 0 (no trend data, sellability=0)", results[0].WeightedROI)
	}
}

func TestBuildCompareResults_Recommendations(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Best Gem", BaseName: "Best", Variant: "20/20", ROI: 100, Confidence: "OK"},
		{TransfiguredName: "OK Gem", BaseName: "OK", Variant: "20/20", ROI: 50, Confidence: "OK"},
		{TransfiguredName: "Dump Gem", BaseName: "Dump", Variant: "20/20", ROI: 200, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Best Gem", Variant: "20/20", Signal: "UNCERTAIN"},
		{Name: "OK Gem", Variant: "20/20", Signal: "STABLE"},
		{Name: "Dump Gem", Variant: "20/20", Signal: "DUMPING"},
	}
	sparklines := map[string][]SparklinePoint{
		"Best Gem": {{Time: "2026-03-15T10:00:00Z", Price: 100, Listings: 10}},
	}

	names := []string{"Best Gem", "OK Gem", "Dump Gem"}
	results := BuildCompareResults(names, transfigure, trends, sparklines, "20/20")

	if len(results) != 3 {
		t.Fatalf("got %d results, want 3", len(results))
	}

	// Build map by name for easier assertions.
	byName := make(map[string]CompareResult)
	for _, r := range results {
		byName[r.TransfiguredName] = r
	}

	// Best Gem: ROI 100 * 1.0 (UNCERTAIN) = 100 → BEST
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

func TestBuildCompareResults_SelectsHighestROIVariant(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "1", ROI: 10, ROIPct: 1000, BasePrice: 1, TransfiguredPrice: 11, Confidence: "OK"},
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "1/20", ROI: 30, ROIPct: 1500, BasePrice: 2, TransfiguredPrice: 32, Confidence: "OK"},
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20", ROI: 80, ROIPct: 160, BasePrice: 50, TransfiguredPrice: 130, Confidence: "OK"},
	}

	names := []string{"Spark of Nova"}
	// Run multiple times to confirm determinism.
	for i := 0; i < 10; i++ {
		results := BuildCompareResults(names, transfigure, nil, nil, "")
		if len(results) != 1 {
			t.Fatalf("got %d results, want 1", len(results))
		}
		if results[0].Variant != "20/20" {
			t.Errorf("run %d: variant = %s, want 20/20 (highest ROI)", i, results[0].Variant)
		}
		if results[0].ROI != 80 {
			t.Errorf("run %d: ROI = %f, want 80", i, results[0].ROI)
		}
		if results[0].ROIPct != 160 {
			t.Errorf("run %d: ROIPct = %f, want 160", i, results[0].ROIPct)
		}
	}
}

func TestBuildCompareResults_GemNotFoundInTransfigure(t *testing.T) {
	names := []string{"Unknown Gem"}
	results := BuildCompareResults(names, nil, nil, nil, "20/20")

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Confidence != "LOW" {
		t.Errorf("confidence = %s, want LOW", results[0].Confidence)
	}
	if results[0].ROI != 0 {
		t.Errorf("ROI = %f, want 0", results[0].ROI)
	}
	// Requested variant should be preserved even with no transfigure data.
	if results[0].Variant != "20/20" {
		t.Errorf("variant = %s, want 20/20 (requested variant preserved)", results[0].Variant)
	}
}

func TestRankCollective_SortByPct(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Big Absolute", Variant: "20/20", ROI: 200, ROIPct: 40, BasePrice: 500, TransfiguredPrice: 700, Confidence: "OK"},
		{Time: now, TransfiguredName: "Big Percent", Variant: "20/20", ROI: 15, ROIPct: 1500, BasePrice: 1, TransfiguredPrice: 16, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Big Absolute", Variant: "20/20", Signal: "STABLE", Sellability: 60},
		{Name: "Big Percent", Variant: "20/20", Signal: "STABLE", Sellability: 60},
	}

	// Explicit sort=pct: Big Percent first despite lower absolute ROI.
	// Big Absolute: 40 * 0.6 = 24, Big Percent: 1500 * 0.6 = 900
	results := RankCollective(transfigure, trends, 0, 50, SortPct)
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}
	if results[0].TransfiguredName != "Big Percent" {
		t.Errorf("sort=pct: first = %s, want Big Percent", results[0].TransfiguredName)
	}

	// Explicit sort=chaos: Big Absolute first.
	// Big Absolute: 200 * 0.6 = 120, Big Percent: 15 * 0.6 = 9
	results = RankCollective(transfigure, trends, 0, 50, SortChaos)
	if results[0].TransfiguredName != "Big Absolute" {
		t.Errorf("sort=chaos: first = %s, want Big Absolute", results[0].TransfiguredName)
	}
}

func TestRankCollective_BudgetAwareDefaultSort(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Big Absolute", Variant: "20/20", ROI: 40, ROIPct: 100, BasePrice: 40, TransfiguredPrice: 80, Confidence: "OK"},
		{Time: now, TransfiguredName: "Big Percent", Variant: "20/20", ROI: 10, ROIPct: 1000, BasePrice: 1, TransfiguredPrice: 11, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Big Absolute", Variant: "20/20", Signal: "STABLE", Sellability: 60},
		{Name: "Big Percent", Variant: "20/20", Signal: "STABLE", Sellability: 60},
	}

	// Budget <= 50, no explicit sort → defaults to pct.
	results := RankCollective(transfigure, trends, 50, 50, "")
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}
	if results[0].TransfiguredName != "Big Percent" {
		t.Errorf("budget-aware default: first = %s, want Big Percent", results[0].TransfiguredName)
	}

	// Budget > 50, no explicit sort → defaults to chaos.
	results = RankCollective(transfigure, trends, 100, 50, "")
	if results[0].TransfiguredName != "Big Absolute" {
		t.Errorf("budget>50 default: first = %s, want Big Absolute", results[0].TransfiguredName)
	}

	// Budget = 0 (no budget filter), no explicit sort → defaults to chaos.
	results = RankCollective(transfigure, trends, 0, 50, "")
	if results[0].TransfiguredName != "Big Absolute" {
		t.Errorf("budget=0 default: first = %s, want Big Absolute (chaos sort)", results[0].TransfiguredName)
	}
}

func TestRankCollective_ROIPctPopulated(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Spark of Nova", Variant: "20/20", ROI: 50, ROIPct: 500, BasePrice: 10, TransfiguredPrice: 60, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE", Sellability: 100},
	}

	results := RankCollective(transfigure, trends, 0, 50, "")
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].ROIPct != 500 {
		t.Errorf("ROIPct = %f, want 500", results[0].ROIPct)
	}
	// Sellability=100, no saturation → WeightedROIPct = 500 * 1.0 = 500
	if results[0].WeightedROIPct != 500 {
		t.Errorf("WeightedROIPct = %f, want 500 (liquidityScore=1.0)", results[0].WeightedROIPct)
	}
}

func TestDeriveSellConfidence_DUMPINGLiquidMarket(t *testing.T) {
	// DUMPING on a liquid, stable market should be FAIR (not RISKY).
	got := deriveSellConfidence(35, 20, "DUMPING")
	if got != "FAIR" {
		t.Errorf("deriveSellConfidence(35 listings, 20%% CV, DUMPING) = %s, want FAIR", got)
	}

	// DUMPING on a thin market is still RISKY.
	got = deriveSellConfidence(5, 20, "DUMPING")
	if got != "RISKY" {
		t.Errorf("deriveSellConfidence(5 listings, 20%% CV, DUMPING) = %s, want RISKY", got)
	}

	// DUMPING on a liquid but volatile market is still RISKY.
	got = deriveSellConfidence(35, 50, "DUMPING")
	if got != "RISKY" {
		t.Errorf("deriveSellConfidence(35 listings, 50%% CV, DUMPING) = %s, want RISKY", got)
	}

	// TRAP is always RISKY regardless of market health.
	got = deriveSellConfidence(100, 5, "TRAP")
	if got != "RISKY" {
		t.Errorf("deriveSellConfidence(100 listings, 5%% CV, TRAP) = %s, want RISKY", got)
	}
}

func TestRankCollective_DUMPINGLiquidMarketReducedPenalty(t *testing.T) {
	now := time.Now()
	transfigure := []TransfigureResult{
		{Time: now, TransfiguredName: "Liquid Dump Gem", Variant: "20/20", ROI: 100, BasePrice: 5, TransfiguredPrice: 105, TransfiguredListings: 30, Confidence: "OK"},
		{Time: now, TransfiguredName: "Thin Dump Gem", Variant: "20/20", ROI: 100, BasePrice: 5, TransfiguredPrice: 105, TransfiguredListings: 5, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Liquid Dump Gem", Variant: "20/20", Signal: "DUMPING", Sellability: 80},
		{Name: "Thin Dump Gem", Variant: "20/20", Signal: "DUMPING", Sellability: 80},
	}

	results := RankCollective(transfigure, trends, 0, 50, "")
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}

	byName := make(map[string]CollectiveResult)
	for _, r := range results {
		byName[r.TransfiguredName] = r
	}

	// Liquid market: 100 * 0.8 * (1 - 0.15) = 68
	liquidExpected := 100.0 * 0.8 * 0.85
	if byName["Liquid Dump Gem"].WeightedROI != liquidExpected {
		t.Errorf("Liquid Dump Gem WeightedROI = %f, want %f", byName["Liquid Dump Gem"].WeightedROI, liquidExpected)
	}

	// Thin market: 100 * 0.8 * (1 - 0.5) = 40
	thinExpected := 100.0 * 0.8 * 0.5
	if byName["Thin Dump Gem"].WeightedROI != thinExpected {
		t.Errorf("Thin Dump Gem WeightedROI = %f, want %f", byName["Thin Dump Gem"].WeightedROI, thinExpected)
	}

	// Liquid market should rank higher.
	if results[0].TransfiguredName != "Liquid Dump Gem" {
		t.Errorf("first result = %s, want Liquid Dump Gem", results[0].TransfiguredName)
	}
}

func TestBuildCompareResults_DUMPINGLiquidNotAvoided(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Liquid Gem", BaseName: "Liquid", Variant: "20/20", ROI: 200, Confidence: "OK"},
	}
	trends := []TrendResult{
		{Name: "Liquid Gem", Variant: "20/20", Signal: "DUMPING", CurrentListings: 30},
	}

	results := BuildCompareResults([]string{"Liquid Gem"}, transfigure, trends, nil, "20/20")
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	// DUMPING with 30 listings should be BEST (not AVOID) when it's the only gem.
	if results[0].Recommendation == "AVOID" {
		t.Errorf("Liquid DUMPING gem recommendation = AVOID, should not be AVOID with 30 listings")
	}
}

func TestBuildCompareResults_ColorBaseROI(t *testing.T) {
	// Two gems of the same color: cheapest base should be used for ROI.
	transfigure := []TransfigureResult{
		{TransfiguredName: "Expensive Trans", BaseName: "Expensive Base", Variant: "20/20",
			GemColor: "RED", BasePrice: 100, TransfiguredPrice: 500, ROI: 400, ROIPct: 400, Confidence: "OK"},
		{TransfiguredName: "Cheap Trans", BaseName: "Cheap Base", Variant: "20/20",
			GemColor: "RED", BasePrice: 5, TransfiguredPrice: 200, ROI: 195, ROIPct: 3900, Confidence: "OK"},
	}

	results := BuildCompareResults([]string{"Expensive Trans"}, transfigure, nil, nil, "20/20")
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	// ROI should use cheapest RED 20/20 base (5c), not specific base (100c).
	expectedROI := 500.0 - 5.0 // 495
	if results[0].ROI != expectedROI {
		t.Errorf("ROI = %f, want %f (cheapest color base)", results[0].ROI, expectedROI)
	}
	if results[0].BasePrice != 5 {
		t.Errorf("BasePrice = %f, want 5 (cheapest color base)", results[0].BasePrice)
	}
	expectedROIPct := (expectedROI / 5.0) * 100 // 9900
	if results[0].ROIPct != expectedROIPct {
		t.Errorf("ROIPct = %f, want %f", results[0].ROIPct, expectedROIPct)
	}
}

func TestSignalWeight(t *testing.T) {
	tests := []struct {
		signal string
		want   float64
	}{
		{"TRAP", 0},
		{"DUMPING", 0.3},
		{"UNCERTAIN", 1.0},
		{"HERD", 0.8},
		{"STABLE", 1.0},

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
