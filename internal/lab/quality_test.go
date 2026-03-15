package lab

import (
	"math"
	"testing"
	"time"
)

func TestAnalyzeQuality_BasicROI(t *testing.T) {
	now := time.Now()
	gcpPrice := 4.0
	gems := []GemPrice{
		{Name: "Spark", Variant: "20", Chaos: 10, Listings: 50, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 100, Listings: 30, GemColor: "BLUE"},
	}

	results := AnalyzeQuality(now, gems, gcpPrice)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if r.Name != "Spark" {
		t.Errorf("Name = %q, want %q", r.Name, "Spark")
	}
	if r.Level != 20 {
		t.Errorf("Level = %d, want 20", r.Level)
	}

	// ROI at +15 quality: sell = 100 - (20-15)*4 = 100-20 = 80; ROI = 80-10 = 70
	if math.Abs(r.ROI15-70) > 0.01 {
		t.Errorf("ROI15 = %f, want 70", r.ROI15)
	}
	// ROI at +10 quality: sell = 100 - (20-10)*4 = 100-40 = 60; ROI = 60-10 = 50
	if math.Abs(r.ROI10-50) > 0.01 {
		t.Errorf("ROI10 = %f, want 50", r.ROI10)
	}
	// ROI at +6: sell = 100 - 14*4 = 44; ROI = 44-10 = 34
	if math.Abs(r.ROI6-34) > 0.01 {
		t.Errorf("ROI6 = %f, want 34", r.ROI6)
	}
	// ROI at +4: sell = 100 - 16*4 = 36; ROI = 36-10 = 26
	if math.Abs(r.ROI4-26) > 0.01 {
		t.Errorf("ROI4 = %f, want 26", r.ROI4)
	}
	// AvgROI = (26+34+50+70)/4 = 45
	if math.Abs(r.AvgROI-45) > 0.01 {
		t.Errorf("AvgROI = %f, want 45", r.AvgROI)
	}
}

func TestAnalyzeQuality_Level1Pair(t *testing.T) {
	now := time.Now()
	gcpPrice := 4.0
	gems := []GemPrice{
		{Name: "Spark", Variant: "1", Chaos: 1, Listings: 50, GemColor: "BLUE"},
		{Name: "Spark", Variant: "1/20", Chaos: 30, Listings: 30, GemColor: "BLUE"},
	}

	results := AnalyzeQuality(now, gems, gcpPrice)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Level != 1 {
		t.Errorf("Level = %d, want 1", results[0].Level)
	}
}

func TestAnalyzeQuality_FilterNegativeROI15(t *testing.T) {
	now := time.Now()
	gcpPrice := 4.0
	gems := []GemPrice{
		// ROI15 = (10 - (20-15)*4) - 8 = (10-20) - 8 = -18 → filtered out
		{Name: "BadGem", Variant: "20", Chaos: 8, Listings: 50, GemColor: "RED"},
		{Name: "BadGem", Variant: "20/20", Chaos: 10, Listings: 30, GemColor: "RED"},
	}

	results := AnalyzeQuality(now, gems, gcpPrice)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (roi15 <= 0 should be filtered)", len(results))
	}
}

func TestAnalyzeQuality_ExcludesCorrupted(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20", Chaos: 10, Listings: 50, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 100, Listings: 30, GemColor: "BLUE"},
	}

	results := AnalyzeQuality(now, gems, 4.0)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (corrupted base excluded)", len(results))
	}
}

func TestAnalyzeQuality_Confidence(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20", Chaos: 10, Listings: 3, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 100, Listings: 30, GemColor: "BLUE"},
	}

	results := AnalyzeQuality(now, gems, 4.0)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	if results[0].Confidence != "LOW" {
		t.Errorf("Confidence = %q, want %q", results[0].Confidence, "LOW")
	}
}

func TestAnalyzeQuality_BothPairs(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "1", Chaos: 1, Listings: 50, GemColor: "BLUE"},
		{Name: "Spark", Variant: "1/20", Chaos: 30, Listings: 30, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20", Chaos: 10, Listings: 50, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 100, Listings: 30, GemColor: "BLUE"},
	}

	results := AnalyzeQuality(now, gems, 4.0)
	if len(results) != 2 {
		t.Fatalf("got %d results, want 2 (both level pairs)", len(results))
	}

	levels := map[int]bool{}
	for _, r := range results {
		levels[r.Level] = true
	}
	if !levels[1] || !levels[20] {
		t.Errorf("expected both level 1 and 20, got levels %v", levels)
	}
}

func TestAnalyzeQuality_EmptyInput(t *testing.T) {
	results := AnalyzeQuality(time.Now(), nil, 4.0)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0", len(results))
	}
}

func TestAnalyzeQuality_GCPPriceAffectsROI(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		{Name: "Spark", Variant: "20", Chaos: 10, Listings: 50, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 100, Listings: 30, GemColor: "BLUE"},
	}

	// With cheap GCP (1c), ROI should be higher
	r1 := AnalyzeQuality(now, gems, 1.0)
	// With expensive GCP (10c), ROI should be lower
	r2 := AnalyzeQuality(now, gems, 10.0)

	if len(r1) != 1 || len(r2) != 1 {
		t.Fatalf("expected 1 result each, got %d and %d", len(r1), len(r2))
	}
	if r1[0].AvgROI <= r2[0].AvgROI {
		t.Errorf("cheaper GCP should yield higher ROI: cheap=%f, expensive=%f", r1[0].AvgROI, r2[0].AvgROI)
	}
}
