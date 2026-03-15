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
