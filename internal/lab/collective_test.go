package lab

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestBuildCompareResults_Recommendations(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Best Gem", BaseName: "Best", Variant: "20/20", ROI: 200, Confidence: "OK"},
		{TransfiguredName: "OK Gem", BaseName: "OK", Variant: "20/20", ROI: 50, Confidence: "OK"},
		{TransfiguredName: "Dump Gem", BaseName: "Dump", Variant: "20/20", ROI: 150, Confidence: "OK"},
	}
	signals := []GemSignal{
		{Name: "Best Gem", Variant: "20/20", Signal: "UNCERTAIN"},
		{Name: "OK Gem", Variant: "20/20", Signal: "STABLE"},
		{Name: "Dump Gem", Variant: "20/20", Signal: "DUMPING"},
	}
	sparklines := map[string][]SparklinePoint{
		"Best Gem": {{Time: "2026-03-15T10:00:00Z", Price: 100, Listings: 10}},
	}

	names := []string{"Best Gem", "OK Gem", "Dump Gem"}
	results := BuildCompareResults(names, transfigure, signals, nil, sparklines, "20/20", nil)

	if len(results) != 3 {
		t.Fatalf("got %d results, want 3", len(results))
	}

	// Build map by name for easier assertions.
	byName := make(map[string]CompareResult)
	for _, r := range results {
		byName[r.TransfiguredName] = r
	}

	// Best Gem: ROI 200 * 1.0 (UNCERTAIN weight) * 0.5 (default sell) = 100 → highest score → BEST
	if byName["Best Gem"].Recommendation != "BEST" {
		t.Errorf("Best Gem recommendation = %s, want BEST", byName["Best Gem"].Recommendation)
	}
	// OK Gem: ROI 50 * 1.0 (STABLE) * 0.5 = 25 → OK
	if byName["OK Gem"].Recommendation != "OK" {
		t.Errorf("OK Gem recommendation = %s, want OK", byName["OK Gem"].Recommendation)
	}
	// Dump Gem: DUMPING on thin market (TransListings=0 < 15) → AVOID
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
		results := BuildCompareResults(names, transfigure, nil, nil, nil, "", nil)
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
	results := BuildCompareResults(names, nil, nil, nil, nil, "20/20", nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	// NO_DATA, not LOW: LOW means "priced, but thin market". This gem has no price
	// at all, and the UI must render its zeros as unknown rather than as 0c.
	if results[0].Confidence != ConfidenceNoData {
		t.Errorf("confidence = %s, want %s", results[0].Confidence, ConfidenceNoData)
	}
	if results[0].ROI != 0 {
		t.Errorf("ROI = %f, want 0", results[0].ROI)
	}
	// Requested variant should be preserved even with no transfigure data.
	if results[0].Variant != "20/20" {
		t.Errorf("variant = %s, want 20/20 (requested variant preserved)", results[0].Variant)
	}
}

func TestBuildCompareResults_DUMPINGLiquidNotAvoided(t *testing.T) {
	transfigure := []TransfigureResult{
		{TransfiguredName: "Liquid Gem", BaseName: "Liquid", Variant: "20/20", ROI: 200, Confidence: "OK"},
	}
	signals := []GemSignal{
		{Name: "Liquid Gem", Variant: "20/20", Signal: "DUMPING"},
	}
	features := []GemFeature{
		{Name: "Liquid Gem", Variant: "20/20", Listings: 30},
	}

	results := BuildCompareResults([]string{"Liquid Gem"}, transfigure, signals, features, nil, "20/20", nil)
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

	results := BuildCompareResults([]string{"Expensive Trans"}, transfigure, nil, nil, nil, "20/20", nil)
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

func TestBuildCompareResults_SellConfidenceFromSignal(t *testing.T) {
	// Compare endpoint should use GemSignal.SellConfidence, not inline derivation.
	transfigure := []TransfigureResult{
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20",
			ROI: 100, TransfiguredPrice: 200, Confidence: "OK"},
	}
	signals := []GemSignal{
		{Name: "Spark of Nova", Variant: "20/20", Signal: "STABLE",
			SellConfidence: "RISKY", TradeConfidenceNote: "CASCADE regime: always RISKY",
			QuickSellPrice: 170, RiskAdjustedValue: 150},
	}

	results := BuildCompareResults([]string{"Spark of Nova"}, transfigure, signals, nil, nil, "20/20", nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	r := results[0]
	if r.SellConfidence != "RISKY" {
		t.Errorf("SellConfidence = %q, want RISKY (from GemSignal)", r.SellConfidence)
	}
	if r.SellConfidenceReason != "CASCADE regime: always RISKY" {
		t.Errorf("SellConfidenceReason = %q, want 'CASCADE regime: always RISKY'", r.SellConfidenceReason)
	}
	if r.QuickSellPrice != 170 {
		t.Errorf("QuickSellPrice = %f, want 170 (from GemSignal)", r.QuickSellPrice)
	}
	if r.RiskAdjustedPrice != 150 {
		t.Errorf("RiskAdjustedPrice = %f, want 150 (from GemSignal.RiskAdjustedValue)", r.RiskAdjustedPrice)
	}
}

func TestSignalWeight(t *testing.T) {
	tests := []struct {
		signal string
		want   float64
	}{
		{"CAUTION", 1.0},
		{"DUMPING", 0.85},
		{"UNCERTAIN", 1.0},
		{"HERD", 0.95},
		{"STABLE", 1.0},
		{"RECOVERY", 1.05},
		{"UNKNOWN", 1.0},
	}
	for _, tt := range tests {
		got := signalWeight(tt.signal)
		if got != tt.want {
			t.Errorf("signalWeight(%s) = %f, want %f", tt.signal, got, tt.want)
		}
	}
}

func TestBuildCompareResults_NoBaseRowDoesNotBecomeTheCheapestBase(t *testing.T) {
	// A NO_BASE row carries BasePrice 0 (unknown). Seeding the per-color cheapest-base
	// map with it pins that color/variant at 0 forever — no later real price is
	// "cheaper" — which silently drops the whole cheapest-color-base rule and prices
	// every gem off its own base instead.
	transfigure := []TransfigureResult{
		{TransfiguredName: "Unpriced Base Trans", BaseName: "Unpriced Base", Variant: "20/20",
			GemColor: "RED", TransfiguredPrice: 300, Confidence: ConfidenceNoBase},
		{TransfiguredName: "Expensive Trans", BaseName: "Expensive Base", Variant: "20/20",
			GemColor: "RED", BasePrice: 100, TransfiguredPrice: 500, ROI: 400, ROIPct: 400, Confidence: "OK"},
		{TransfiguredName: "Cheap Trans", BaseName: "Cheap Base", Variant: "20/20",
			GemColor: "RED", BasePrice: 5, TransfiguredPrice: 200, ROI: 195, ROIPct: 3900, Confidence: "OK"},
	}

	results := BuildCompareResults([]string{"Expensive Trans"}, transfigure, nil, nil, nil, "20/20", nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	// Lab cost basis is the cheapest RED 20/20 base (5c), not this gem's own base (100c).
	if results[0].BasePrice != 5 {
		t.Errorf("BasePrice = %f, want 5 (cheapest real color base, unaffected by the NO_BASE row)",
			results[0].BasePrice)
	}
	if results[0].ROI != 495 {
		t.Errorf("ROI = %f, want 495 — a poisoned 0 base falls back to the 100c own-base ROI of 400",
			results[0].ROI)
	}
}

func TestBuildCompareResults_PrefersAVariantWithRealDataOverNoBase(t *testing.T) {
	// The 20/20 row is genuinely priced but currently sells below its base — a real
	// league-start shape. Its negative ROI must still beat the NO_BASE row's 0,
	// which means "unknown", or the comparison shows the variant we know nothing about.
	transfigure := []TransfigureResult{
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20", GemColor: "BLUE",
			BasePrice: 300, TransfiguredPrice: 100, ROI: -200, ROIPct: -66, Confidence: "OK"},
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "1", GemColor: "BLUE",
			TransfiguredPrice: 12, Confidence: ConfidenceNoBase},
	}

	results := BuildCompareResults([]string{"Spark of Nova"}, transfigure, nil, nil, nil, "", nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Variant != "20/20" {
		t.Errorf("variant = %q, want 20/20 — the only variant with a known cost basis", results[0].Variant)
	}
}

func TestBuildCompareResults_NoBaseGemWithAColorBaseIsNoLongerLabelledNoBase(t *testing.T) {
	// The lab cost basis is the cheapest base of that colour/variant, not this
	// gem's own base — so once one is found the ROI is real and the row must stop
	// claiming its cost basis is unknown, or the UI hides valid numbers.
	transfigure := []TransfigureResult{
		{TransfiguredName: "Spark of Nova", BaseName: "Spark", Variant: "20/20", GemColor: "BLUE",
			TransfiguredPrice: 200, Confidence: ConfidenceNoBase},
		{TransfiguredName: "Arc of Surging", BaseName: "Arc", Variant: "20/20", GemColor: "BLUE",
			BasePrice: 20, TransfiguredPrice: 90, ROI: 70, ROIPct: 350, Confidence: "OK"},
	}

	results := BuildCompareResults([]string{"Spark of Nova"}, transfigure, nil, nil, nil, "20/20", nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Confidence == ConfidenceNoBase {
		t.Errorf("confidence = %s with BasePrice %.0f and ROI %.0f — a substituted colour base is a known cost basis",
			results[0].Confidence, results[0].BasePrice, results[0].ROI)
	}
	if results[0].BasePrice != 20 || results[0].ROI != 180 {
		t.Errorf("BasePrice/ROI = %.0f/%.0f, want 20/180 (cheapest BLUE 20/20 base)",
			results[0].BasePrice, results[0].ROI)
	}
}

func TestBuildDedicationCompareResults_ReportsTheRequestedVariant(t *testing.T) {
	gemPrices := []GemPrice{
		{Name: "Vaal Arc", Variant: "21/20c", Chaos: 60, Listings: 12, IsCorrupted: true, GemColor: "BLUE"},
	}

	results := BuildDedicationCompareResults([]string{"Vaal Arc"}, gemPrices, nil, nil, "21/20c")

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Variant != "21/20c" {
		t.Errorf("Variant = %q, want %q", results[0].Variant, "21/20c")
	}
	if results[0].TransfiguredPrice != 60 {
		t.Errorf("TransfiguredPrice = %.0f, want 60", results[0].TransfiguredPrice)
	}
}

// A gem that does not trade at the requested variant must not come back priced
// from a different one. The label is the caller's, so the price has to be the
// only thing proving which market answered.
func TestBuildDedicationCompareResults_DoesNotPriceAGemFromAnotherVariant(t *testing.T) {
	gemPrices := []GemPrice{
		{Name: "Vaal Arc", Variant: "21/23c", Chaos: 4000, Listings: 12, IsCorrupted: true, GemColor: "BLUE"},
	}

	results := BuildDedicationCompareResults([]string{"Vaal Arc"}, gemPrices, nil, nil, "21/20c")

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].TransfiguredPrice == 4000 {
		t.Error("the 21/23c price answered a 21/20c comparison")
	}
}

// The font never hands out a Vaal gem, so pricing one against the pool's input
// cost describes a trade nobody can make — and the ranking would otherwise call
// the most expensive such gem the BEST play.
func TestBuildDedicationCompareResults_GivesNoROIToAGemTheFontCannotProduce(t *testing.T) {
	gemPrices := []GemPrice{
		{Name: "Vaal Arc", Variant: "21/20c", Chaos: 4000, Listings: 12, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Arc", Variant: "21/20c", Chaos: 300, Listings: 12, IsCorrupted: true, GemColor: "BLUE"},
	}
	dedication := []DedicationResult{
		{Color: "BLUE", GemType: "skill", Mode: "safe", InputCost: 100},
	}

	results := BuildDedicationCompareResults([]string{"Vaal Arc", "Arc"}, gemPrices, dedication, nil, "21/20c")

	byName := make(map[string]CompareResult, len(results))
	for _, r := range results {
		byName[r.TransfiguredName] = r
	}

	vaal, ok := byName["Vaal Arc"]
	if !ok {
		t.Fatal("the requested Vaal Arc row is missing — it should be returned, just not scored")
	}
	if vaal.ROI != 0 || vaal.ROIPct != 0 {
		t.Errorf("Vaal Arc ROI/ROIPct = %.0f/%.0f, want 0/0 — the font cannot produce it",
			vaal.ROI, vaal.ROIPct)
	}
	if vaal.Recommendation != "AVOID" {
		t.Errorf("Vaal Arc Recommendation = %q, want AVOID", vaal.Recommendation)
	}
	if byName["Arc"].Recommendation != "BEST" {
		t.Errorf("Arc Recommendation = %q, want BEST — it is the only real outcome here",
			byName["Arc"].Recommendation)
	}
}

type fakeCollectiveSource struct {
	page CollectivePage
}

func (f fakeCollectiveSource) CollectivePage(context.Context, league.Scope, CollectiveQuery) (CollectivePage, error) {
	return f.page, nil
}

func (f fakeCollectiveSource) CollectiveBasePrices(context.Context, league.Scope, []string) (map[string]float64, error) {
	return nil, nil
}

func (f fakeCollectiveSource) SparklineData(context.Context, league.Scope, []string, string, int) (map[string][]SparklinePoint, error) {
	return nil, nil
}

// gem_signals has no column for the trade note; the tick-computed signals in
// the cache are its only source, keyed by gem and market.
func TestQueryCollective_TradeConfidenceNoteComesFromTheCachedSignalOfTheSameMarket(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := NewCache(scope)
	cache.For(scope).SetGemSignals([]GemSignal{
		{Name: "Spark of Nova", Variant: "20/20", TradeConfidenceNote: "MONOPOLY: downgraded from SAFE"},
		{Name: "Spark of Nova", Variant: "1/20", TradeConfidenceNote: "other market"},
	})
	cache.For(scope).SetSparklinesByName(nil, nil, time.Now())
	src := fakeCollectiveSource{page: CollectivePage{Rows: []CollectiveResult{
		{TransfiguredName: "Spark of Nova", Variant: "20/20"},
		{TransfiguredName: "Cleave of Rage", Variant: "20/20"},
	}, Total: 2}}

	got, err := QueryCollective(context.Background(), src, cache, scope, CollectiveQuery{Page: 1, PageSize: 10, Sort: CollectiveSortPrice})
	if err != nil {
		t.Fatalf("QueryCollective: %v", err)
	}
	if got.Results[0].TradeConfidenceNote != "MONOPOLY: downgraded from SAFE" || got.Results[1].TradeConfidenceNote != "" {
		t.Errorf("notes = (%q, %q), want the 20/20 note on Spark of Nova and none on Cleave of Rage",
			got.Results[0].TradeConfidenceNote, got.Results[1].TradeConfidenceNote)
	}
}

func TestCollectiveQuery_ValidateRejectsOutOfRangeValues(t *testing.T) {
	ok := CollectiveQuery{Page: 1, PageSize: 10, Sort: CollectiveSortPrice}
	if err := ok.Validate(); err != nil {
		t.Fatalf("valid query rejected: %v", err)
	}
	for name, q := range map[string]CollectiveQuery{
		"page 0":            {Page: 0, PageSize: 10, Sort: CollectiveSortPrice},
		"pageSize 0":        {Page: 1, PageSize: 0, Sort: CollectiveSortPrice},
		"pageSize 101":      {Page: 1, PageSize: 101, Sort: CollectiveSortPrice},
		"unknown sort":      {Page: 1, PageSize: 10, Sort: "pct"},
		"legacy sort paged": {Page: 1, PageSize: 10, Sort: CollectiveSortLegacyWeightedChaos},
		"negative budget":   {Page: 1, PageSize: 10, Sort: CollectiveSortPrice, Budget: -1},
		"unknown colour":    {Page: 1, PageSize: 10, Sort: CollectiveSortPrice, Color: "PURPLE"},
		"unknown variant":   {Page: 1, PageSize: 10, Sort: CollectiveSortPrice, Variant: "21/23c"},
	} {
		if err := q.Validate(); err == nil {
			t.Errorf("%s: Validate() = nil, want an error", name)
		}
	}
}
