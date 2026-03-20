package lab

import (
	"math"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// ValidateSellability tests
// ---------------------------------------------------------------------------

func TestValidateSellability_EmptyEvals(t *testing.T) {
	mc := sweepMarketContext(0, 10, 0, 5)

	report := ValidateSellability(nil, mc)

	if report.TotalEvals != 0 {
		t.Errorf("TotalEvals: got %d, want 0", report.TotalEvals)
	}
	if len(report.PerSignalCapture) != 0 {
		t.Errorf("PerSignalCapture: expected empty, got %d entries", len(report.PerSignalCapture))
	}
	if len(report.FloorHoldRate) != 0 {
		t.Errorf("FloorHoldRate: expected empty, got %d entries", len(report.FloorHoldRate))
	}
	if len(report.ConfidenceCalibration) != 0 {
		t.Errorf("ConfidenceCalibration: expected empty, got %d entries", len(report.ConfidenceCalibration))
	}
	if len(report.PerTierCapture) != 0 {
		t.Errorf("PerTierCapture: expected empty, got %d entries", len(report.PerTierCapture))
	}
}

func TestValidateSellability_ValueCapture(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// Create eval points with known SellProbabilityFactor and StabilityDiscount.
	// STABLE signal: low velocity.
	// RiskAdjustedValue = Chaos * SellProbabilityFactor * StabilityDiscount
	//
	// For feature with Chaos=100, SellProb=0.8, StabDisc=0.9:
	//   RAV = 100 * 0.8 * 0.9 = 72
	// If future price = 100 (0% change): capture = 100/72 = 1.389
	// If future price = 80 (-20% change): capture = 80/72 = 1.111
	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120, HistPosition: 50,
			},
			FuturePct: 0.0, // future price = 100
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120, HistPosition: 50,
			},
			FuturePct: -20.0, // future price = 80
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	if report.TotalEvals != 2 {
		t.Fatalf("TotalEvals: got %d, want 2", report.TotalEvals)
	}

	// Both should be STABLE signal with these low velocities.
	vc, ok := report.PerSignalCapture["STABLE"]
	if !ok {
		// Check what signals were actually generated.
		for sig := range report.PerSignalCapture {
			t.Logf("Signal found: %s", sig)
		}
		t.Fatal("expected STABLE signal in PerSignalCapture")
	}
	if vc.Count != 2 {
		t.Errorf("STABLE count: got %d, want 2", vc.Count)
	}

	// RAV = 72. Captures: 100/72 = 1.39, 80/72 = 1.11. Avg = 1.25.
	expectedAvg := (100.0/72.0 + 80.0/72.0) / 2.0
	if !approxEqual(vc.AvgCapture, math.Round(expectedAvg*100)/100, 0.01) {
		t.Errorf("STABLE AvgCapture: got %.2f, want %.2f", vc.AvgCapture, expectedAvg)
	}
}

func TestValidateSellability_FloorHoldRate(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "HIGH", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 0.0, // future price = 100, above Low7d=80
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "HIGH", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -25.0, // future price = 75, below Low7d=80
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: -0.1, VelMedListing: 0.2, CV: 10, Listings: 30,
				Tier: "HIGH", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -10.0, // future price = 90, above Low7d=80
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	fh, ok := report.FloorHoldRate["HIGH"]
	if !ok {
		t.Fatal("expected HIGH tier in FloorHoldRate")
	}
	if fh.Count != 3 {
		t.Errorf("FloorHoldRate[HIGH].Count: got %d, want 3", fh.Count)
	}
	if fh.Held != 2 {
		t.Errorf("FloorHoldRate[HIGH].Held: got %d, want 2", fh.Held)
	}
	// 2/3 = 66.67%
	if !approxEqual(fh.HeldRate, 66.67, 0.1) {
		t.Errorf("FloorHoldRate[HIGH].HeldRate: got %.2f, want ~66.67", fh.HeldRate)
	}
}

func TestValidateSellability_ConfidenceCalibration(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// GREEN: sellProb >= 0.7 AND stabilityDisc >= 0.8
	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -5.0, // future price = 95, >= 0.9*100=90: held
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -15.0, // future price = 85, < 0.9*100=90: NOT held
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 5.0, // future price = 105, >= 90: held
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	// All three should be GREEN (sellProb=0.8 >= 0.7, stabDisc=0.9 >= 0.8).
	cal, ok := report.ConfidenceCalibration["GREEN"]
	if !ok {
		for conf := range report.ConfidenceCalibration {
			t.Logf("Confidence found: %s", conf)
		}
		t.Fatal("expected GREEN in ConfidenceCalibration")
	}
	if cal.Count != 3 {
		t.Errorf("GREEN count: got %d, want 3", cal.Count)
	}
	if cal.PriceHeld != 2 {
		t.Errorf("GREEN PriceHeld: got %d, want 2", cal.PriceHeld)
	}
	// 2/3 = 66.67%
	if !approxEqual(cal.HeldRate, 66.67, 0.1) {
		t.Errorf("GREEN HeldRate: got %.2f, want ~66.67", cal.HeldRate)
	}
	// avg change: (-5 + -15 + 5) / 3 = -5
	if !approxEqual(cal.AvgChange, -5.0, 0.1) {
		t.Errorf("GREEN AvgChange: got %.2f, want -5.0", cal.AvgChange)
	}
}

func TestValidateSellability_PerTierCapture(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "TOP", Chaos: 200,
				SellProbabilityFactor: 0.9, StabilityDiscount: 0.95,
				Low7d: 180, High7d: 220,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "LOW", Chaos: 10,
				SellProbabilityFactor: 0.5, StabilityDiscount: 0.8,
				Low7d: 8, High7d: 12,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	// TOP tier should have 1 entry.
	topVC, ok := report.PerTierCapture["TOP"]
	if !ok {
		t.Fatal("expected TOP in PerTierCapture")
	}
	if topVC.Count != 1 {
		t.Errorf("TOP count: got %d, want 1", topVC.Count)
	}

	// LOW tier should have 1 entry.
	lowVC, ok := report.PerTierCapture["LOW"]
	if !ok {
		t.Fatal("expected LOW in PerTierCapture")
	}
	if lowVC.Count != 1 {
		t.Errorf("LOW count: got %d, want 1", lowVC.Count)
	}

	// TOP: RAV = 200*0.9*0.95 = 171, future=200, capture=200/171 = 1.17
	expectedTopCapture := 200.0 / (200.0 * 0.9 * 0.95)
	if !approxEqual(topVC.AvgCapture, math.Round(expectedTopCapture*100)/100, 0.01) {
		t.Errorf("TOP AvgCapture: got %.2f, want %.2f", topVC.AvgCapture, expectedTopCapture)
	}
}

func TestValidateSellability_SkipsZeroRAV(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 0, // zero chaos => RAV = 0
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 0, High7d: 0,
			},
			FuturePct: 10.0,
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	// Only 1 eval should be scored (the zero-RAV one is skipped).
	if report.TotalEvals != 1 {
		t.Errorf("TotalEvals: got %d, want 1 (zero-RAV should be skipped)", report.TotalEvals)
	}
}

func TestValidateSellability_EmptyTierDefaultsToLOW(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "", Chaos: 100, // empty tier
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	// Empty tier should be counted as LOW.
	if _, ok := report.PerTierCapture["LOW"]; !ok {
		t.Error("expected empty tier to be counted as LOW in PerTierCapture")
	}
	if _, ok := report.FloorHoldRate["LOW"]; !ok {
		t.Error("expected empty tier to be counted as LOW in FloorHoldRate")
	}
}

func TestValidateSellability_MultipleSignalTypes(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// DefaultSignalConfig: DumpPriceVel=-5, DumpListingVel=5
	// DUMPING: priceVel < -5 && listingVel > 5
	// STABLE: |priceVel| < 2 && |listingVel| < 3
	evals := []EvalPoint{
		// STABLE
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
		// DUMPING: priceVel=-10 < -5, listingVel=10 > 5
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: -10, VelMedListing: 10, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -15.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	if len(report.PerSignalCapture) < 2 {
		t.Errorf("expected at least 2 signal types, got %d", len(report.PerSignalCapture))
	}

	if _, ok := report.PerSignalCapture["STABLE"]; !ok {
		t.Error("expected STABLE in PerSignalCapture")
	}
	if _, ok := report.PerSignalCapture["DUMPING"]; !ok {
		t.Error("expected DUMPING in PerSignalCapture")
	}
}

func TestValidateSellability_ConfCalibrationMultipleColors(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		// GREEN: sellProb=0.8 >= 0.7, stabDisc=0.9 >= 0.8
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.8, StabilityDiscount: 0.9,
				Low7d: 80, High7d: 120,
			},
			FuturePct: 0.0,
			SnapTime:  t0,
		},
		// RED: sellProb=0.35 < 0.5, stabDisc=0.55 < 0.7
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100,
				SellProbabilityFactor: 0.35, StabilityDiscount: 0.55,
				Low7d: 80, High7d: 120,
			},
			FuturePct: -20.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	if _, ok := report.ConfidenceCalibration["GREEN"]; !ok {
		t.Error("expected GREEN in ConfidenceCalibration")
	}
	if _, ok := report.ConfidenceCalibration["RED"]; !ok {
		t.Error("expected RED in ConfidenceCalibration")
	}
}

// ---------------------------------------------------------------------------
// computeValueCapture tests
// ---------------------------------------------------------------------------

func TestComputeValueCapture_Empty(t *testing.T) {
	vc := computeValueCapture(nil)

	if vc.Count != 0 {
		t.Errorf("Count: got %d, want 0", vc.Count)
	}
	if vc.AvgCapture != 0 {
		t.Errorf("AvgCapture: got %.2f, want 0", vc.AvgCapture)
	}
}

func TestComputeValueCapture_SingleValue(t *testing.T) {
	vc := computeValueCapture([]float64{1.5})

	if vc.Count != 1 {
		t.Errorf("Count: got %d, want 1", vc.Count)
	}
	if !approxEqual(vc.AvgCapture, 1.5, 0.01) {
		t.Errorf("AvgCapture: got %.2f, want 1.5", vc.AvgCapture)
	}
	if !approxEqual(vc.MedianCapture, 1.5, 0.01) {
		t.Errorf("MedianCapture: got %.2f, want 1.5", vc.MedianCapture)
	}
	if !approxEqual(vc.P25Capture, 1.5, 0.01) {
		t.Errorf("P25Capture: got %.2f, want 1.5", vc.P25Capture)
	}
	if !approxEqual(vc.P75Capture, 1.5, 0.01) {
		t.Errorf("P75Capture: got %.2f, want 1.5", vc.P75Capture)
	}
}

func TestComputeValueCapture_KnownDistribution(t *testing.T) {
	// 5 values: 0.5, 0.8, 1.0, 1.2, 1.5
	// Mean = (0.5+0.8+1.0+1.2+1.5)/5 = 1.0
	// Median (p50) = 1.0
	// P25 = interpolation at index 1.0 = 0.8
	// P75 = interpolation at index 3.0 = 1.2
	ratios := []float64{1.0, 0.5, 1.5, 0.8, 1.2}

	vc := computeValueCapture(ratios)

	if vc.Count != 5 {
		t.Errorf("Count: got %d, want 5", vc.Count)
	}
	if !approxEqual(vc.AvgCapture, 1.0, 0.01) {
		t.Errorf("AvgCapture: got %.2f, want 1.0", vc.AvgCapture)
	}
	if !approxEqual(vc.MedianCapture, 1.0, 0.01) {
		t.Errorf("MedianCapture: got %.2f, want 1.0", vc.MedianCapture)
	}
	if !approxEqual(vc.P25Capture, 0.8, 0.01) {
		t.Errorf("P25Capture: got %.2f, want 0.8", vc.P25Capture)
	}
	if !approxEqual(vc.P75Capture, 1.2, 0.01) {
		t.Errorf("P75Capture: got %.2f, want 1.2", vc.P75Capture)
	}
}

