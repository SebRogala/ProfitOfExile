package lab

import (
	"testing"
	"time"
)

func TestExtractBaseName(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"Spark of Nova", "Spark"},
		{"Rain of Arrows of Saturation", "Rain of Arrows"},
		{"Vaal Spark of Nova", "Vaal Spark"},
		{"Holy Relic of Conviction", "Holy Relic"},
		{"Lacerate of Butchering", "Lacerate"},
		{"Elemental Hit of the Spectrum", "Elemental Hit"},
		// Edge: no " of " at all
		{"Cyclone", "Cyclone"},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got := extractBaseName(tt.input)
			if got != tt.want {
				t.Errorf("extractBaseName(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}

func TestAnalyzeTransfigure_BasicROI(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 30, IsTransfigured: true, GemColor: "BLUE"},
	}

	results := AnalyzeTransfigure(now, gems)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if r.BaseName != "Spark" {
		t.Errorf("BaseName = %q, want %q", r.BaseName, "Spark")
	}
	if r.TransfiguredName != "Spark of Nova" {
		t.Errorf("TransfiguredName = %q, want %q", r.TransfiguredName, "Spark of Nova")
	}
	if r.ROI != 150 {
		t.Errorf("ROI = %f, want 150", r.ROI)
	}
	if r.ROIPct != 300 {
		t.Errorf("ROIPct = %f, want 300", r.ROIPct)
	}
	if r.Confidence != "OK" {
		t.Errorf("Confidence = %q, want %q", r.Confidence, "OK")
	}
}

func TestAnalyzeTransfigure_LowConfidence(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Lacerate", Variant: "20/20", Chaos: 20, Listings: 3, IsTransfigured: false},
		{Name: "Lacerate of Butchering", Variant: "20/20", Chaos: 500, Listings: 2, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Confidence != "LOW" {
		t.Errorf("Confidence = %q, want %q", results[0].Confidence, "LOW")
	}
}

func TestAnalyzeTransfigure_ExcludesCorrupted(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 10, IsTransfigured: true, IsCorrupted: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (corrupted should be excluded)", len(results))
	}
}

func TestAnalyzeTransfigure_ExcludesTrarthus(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Wave of Conviction", Variant: "1", Chaos: 1, Listings: 100, IsTransfigured: false},
		{Name: "Wave of Conviction of Trarthus", Variant: "1", Chaos: 400, Listings: 50, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (Trarthus should be excluded)", len(results))
	}
}

func TestAnalyzeTransfigure_MultipleVariants(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "1/20", Chaos: 5, Listings: 50, IsTransfigured: false},
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "1/20", Chaos: 100, Listings: 20, IsTransfigured: true},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 30, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2", len(results))
	}

	// Check each variant matched correctly
	variantROIs := make(map[string]float64)
	for _, r := range results {
		variantROIs[r.Variant] = r.ROI
	}
	if variantROIs["1/20"] != 95 {
		t.Errorf("1/20 ROI = %f, want 95", variantROIs["1/20"])
	}
	if variantROIs["20/20"] != 150 {
		t.Errorf("20/20 ROI = %f, want 150", variantROIs["20/20"])
	}
}

func TestAnalyzeTransfigure_RainOfArrows(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Rain of Arrows", Variant: "20/20", Chaos: 10, Listings: 50, IsTransfigured: false},
		{Name: "Rain of Arrows of Saturation", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].BaseName != "Rain of Arrows" {
		t.Errorf("BaseName = %q, want %q", results[0].BaseName, "Rain of Arrows")
	}
}

func TestAnalyzeTransfigure_VaalGem(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Vaal Spark", Variant: "20/20", Chaos: 30, Listings: 40, IsTransfigured: false},
		{Name: "Vaal Spark of Nova", Variant: "20/20", Chaos: 150, Listings: 15, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].BaseName != "Vaal Spark" {
		t.Errorf("BaseName = %q, want %q", results[0].BaseName, "Vaal Spark")
	}
}

func TestAnalyzeTransfigure_ZeroPriceBase(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "1", Chaos: 0, Listings: 50, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "1", Chaos: 100, Listings: 20, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].ROI != 100 {
		t.Errorf("ROI = %f, want 100", results[0].ROI)
	}
	if results[0].ROIPct != 0 {
		t.Errorf("ROIPct = %f, want 0 (zero base price)", results[0].ROIPct)
	}
}

func TestAnalyzeTransfigure_NegativeROI(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 300, Listings: 50, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].ROI != -200 {
		t.Errorf("ROI = %f, want -200", results[0].ROI)
	}
	if results[0].ROIPct > 0 {
		t.Errorf("ROIPct = %f, want negative", results[0].ROIPct)
	}
}

func TestAnalyzeTransfigure_EmptyInput(t *testing.T) {
	now := time.Now()

	results := AnalyzeTransfigure(now, nil)
	if len(results) != 0 {
		t.Errorf("nil input: got %d results, want 0", len(results))
	}

	results = AnalyzeTransfigure(now, []GemPrice{})
	if len(results) != 0 {
		t.Errorf("empty input: got %d results, want 0", len(results))
	}
}

func TestAnalyzeTransfigure_UnrecognizedVariantExcluded(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "21/23", Chaos: 50, Listings: 50, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "21/23", Chaos: 200, Listings: 20, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (unrecognized variant should be excluded)", len(results))
	}
}

func TestAnalyzeTransfigure_GemColorFromTransfigured(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false, GemColor: "RED"},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 30, IsTransfigured: true, GemColor: "BLUE"},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].GemColor != "BLUE" {
		t.Errorf("GemColor = %q, want %q (should use transfigured gem's color)", results[0].GemColor, "BLUE")
	}
}

func TestAnalyzeTransfigure_MultipleTransfiguredPerBase(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 30, IsTransfigured: true},
		{Name: "Spark of Unpredictability", Variant: "20/20", Chaos: 80, Listings: 25, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2 (two transfigured variants of one base)", len(results))
	}
}

func TestAnalyzeTransfigure_UnpricedBaseKeepsGemWithUnknownROI(t *testing.T) {
	now := time.Now()
	// League-start shape: poe.ninja lists the transfigured gem before its base.
	gems := []GemPrice{
		{Name: "Tornado of Elemental Turbulence", Variant: "1", Chaos: 20, Listings: 1, IsTransfigured: true, GemColor: "GREEN"},
	}

	results := AnalyzeTransfigure(now, gems)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1 (gem must survive a missing base)", len(results))
	}
	r := results[0]
	if r.Confidence != ConfidenceNoBase {
		t.Errorf("Confidence = %q, want %q", r.Confidence, ConfidenceNoBase)
	}
	if r.TransfiguredPrice != 20 {
		t.Errorf("TransfiguredPrice = %f, want 20 (the known half of the pair)", r.TransfiguredPrice)
	}
	if r.BasePrice != 0 || r.ROI != 0 || r.ROIPct != 0 {
		t.Errorf("BasePrice/ROI/ROIPct = %f/%f/%f, want 0/0/0 — unknown, not a computed profit",
			r.BasePrice, r.ROI, r.ROIPct)
	}
	if r.BaseName != "Tornado" {
		t.Errorf("BaseName = %q, want %q (still names the base we could not price)", r.BaseName, "Tornado")
	}
	if r.GemColor != "GREEN" {
		t.Errorf("GemColor = %q, want GREEN", r.GemColor)
	}
}

func TestAnalyzeTransfigure_MissingBaseVariantMarkedNoBaseWhileOtherVariantComputes(t *testing.T) {
	now := time.Now()
	// Base priced only at 20/20; the transfigured gem trades at both 20/20 and 1.
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false},
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 30, IsTransfigured: true},
		{Name: "Spark of Nova", Variant: "1", Chaos: 12, Listings: 4, IsTransfigured: true},
	}

	results := AnalyzeTransfigure(now, gems)

	byVariant := make(map[string]TransfigureResult, len(results))
	for _, r := range results {
		byVariant[r.Variant] = r
	}
	if len(byVariant) != 2 {
		t.Fatalf("got variants %v, want both 1 and 20/20", byVariant)
	}
	if got := byVariant["20/20"]; got.ROI != 150 || got.Confidence == ConfidenceNoBase {
		t.Errorf("20/20: ROI = %f, Confidence = %q — want ROI 150 with a real confidence", got.ROI, got.Confidence)
	}
	if got := byVariant["1"]; got.Confidence != ConfidenceNoBase || got.TransfiguredPrice != 12 {
		t.Errorf("1: Confidence = %q, TransfiguredPrice = %f — want %q at 12c",
			got.Confidence, got.TransfiguredPrice, ConfidenceNoBase)
	}
}

func TestFilterGemDictionary_TransfiguredRequiresAKnownBaseGem(t *testing.T) {
	// "Herald of Ash" contains " of " but has no base gem "Herald" — it is a base
	// gem itself. "Rain of Arrows of Saturation" has base "Rain of Arrows", which
	// is in the list, so it is transfigured.
	all := []string{
		"Herald of Ash",
		"Rain of Arrows",
		"Rain of Arrows of Saturation",
		"Spark",
		"Spark of Nova",
	}

	got := FilterGemDictionary(all, true)

	want := map[string]bool{"Rain of Arrows of Saturation": true, "Spark of Nova": true}
	if len(got) != len(want) {
		t.Fatalf("transfigured = %v, want exactly %v", got, want)
	}
	for _, n := range got {
		if !want[n] {
			t.Errorf("%q classified transfigured — it has no known base gem", n)
		}
	}
}

func TestFilterGemDictionary_SkillsExcludeSupportGems(t *testing.T) {
	all := []string{"Bloodlust Support", "Herald of Ash", "Spark", "Spark of Nova"}

	got := FilterGemDictionary(all, false)

	for _, n := range got {
		if n == "Bloodlust Support" {
			t.Fatalf("support gem in the skill dictionary: %v — supports never drop from a Font", got)
		}
		if n == "Spark of Nova" {
			t.Fatalf("transfigured gem in the skill dictionary: %v", got)
		}
	}
	if len(got) != 2 {
		t.Errorf("skills = %v, want Herald of Ash and Spark", got)
	}
}
