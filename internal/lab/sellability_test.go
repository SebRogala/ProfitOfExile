package lab

import (
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
	if len(report.PerVariant) != 0 {
		t.Errorf("PerVariant: expected empty, got %d entries", len(report.PerVariant))
	}
}

func TestValidateSellability_ValueCapture(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// Create eval points with known SellProbabilityFactor and StabilityDiscount.
	// STABLE signal: low velocity.
	// RiskAdjustedValue is computed on-the-fly from feature data:
	//   sellProb = sellProbabilityFactor(listings=30, low7d=80, chaos=100)
	//   stabDisc = stabilityDiscount(cv=10)
	//   RAV = chaos * sellProb * stabDisc
	chaos := 100.0
	listings := 30
	low7d := 80.0
	cv := 10.0
	expectedSellProb := sellProbabilityFactor(listings, low7d, chaos)
	expectedStabDisc := stabilityDiscount(cv)
	expectedRAV := chaos * expectedSellProb * expectedStabDisc

	evals := []EvalPoint{
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: cv, Listings: listings,
				Tier: "MID", Chaos: chaos,
				Low7d: low7d, High7d: 120, HistPosition: 50,
			},
			FuturePct: 0.0, // future price = chaos
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: cv, Listings: listings,
				Tier: "MID", Chaos: chaos,
				Low7d: low7d, High7d: 120, HistPosition: 50,
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
		for sig := range report.PerSignalCapture {
			t.Logf("Signal found: %s", sig)
		}
		t.Fatal("expected STABLE signal in PerSignalCapture")
	}
	if vc.Count != 2 {
		t.Errorf("STABLE count: got %d, want 2", vc.Count)
	}

	// Captures: chaos/RAV and 80/RAV. Avg = (chaos + 80) / (2 * RAV).
	expectedAvg := (chaos/expectedRAV + 80.0/expectedRAV) / 2.0
	if !approxEqual(vc.AvgCapture, expectedAvg, 0.02) {
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

	// With 30 listings and CV=10, on-the-fly computation:
	// sellProb = sellProbabilityFactor(30, 80, 100) — linear: ~0.78
	// stabDisc = stabilityDiscount(10) = 0.95
	// With new thresholds: SAFE requires sellProb >= 0.8 AND stabDisc >= 0.85.
	// sellProb ~0.65 < 0.8, so these will be FAIR (not SAFE).
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

	// On-the-fly computation with default median=30 produces FAIR for all three.
	cal, ok := report.ConfidenceCalibration["FAIR"]
	if !ok {
		for conf := range report.ConfidenceCalibration {
			t.Logf("Confidence found: %s", conf)
		}
		t.Fatal("expected FAIR in ConfidenceCalibration")
	}
	if cal.Count != 3 {
		t.Errorf("FAIR count: got %d, want 3", cal.Count)
	}
	if cal.PriceHeld != 2 {
		t.Errorf("FAIR PriceHeld: got %d, want 2", cal.PriceHeld)
	}
	// 2/3 = 66.67%
	if !approxEqual(cal.HeldRate, 66.67, 0.1) {
		t.Errorf("FAIR HeldRate: got %.2f, want ~66.67", cal.HeldRate)
	}
	// avg change: (-5 + -15 + 5) / 3 = -5
	if !approxEqual(cal.AvgChange, -5.0, 0.1) {
		t.Errorf("FAIR AvgChange: got %.2f, want -5.0", cal.AvgChange)
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

	// TOP: RAV computed on-the-fly from feature data.
	topSellProb := sellProbabilityFactor(30, 180, 200)
	topStabDisc := stabilityDiscount(10)
	expectedTopRAV := 200.0 * topSellProb * topStabDisc
	expectedTopCapture := 200.0 / expectedTopRAV
	if !approxEqual(topVC.AvgCapture, expectedTopCapture, 0.02) {
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
	// DUMPING: priceVel < -5 && listingVel > 5 (uses VelLong for classification)
	// STABLE: |priceVel| < 2 && |listingVel| < 3
	evals := []EvalPoint{
		// STABLE
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1,
				VelLongPrice: 0.1, VelLongListing: 0.1,
				CV: 10, Listings: 30,
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
				Time: t0, VelMedPrice: -10, VelMedListing: 10,
				VelLongPrice: -10, VelLongListing: 10,
				CV: 10, Listings: 30,
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

func TestValidateSellability_ConfCalibrationMultipleLevels(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		// FAIR: on-the-fly sellProb ~0.65 (30 listings, median=30), stabDisc=0.95
		// sellProb < 0.8 → not SAFE, but sellProb >= 0.5 → not RISKY
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
		// RISKY: very few listings (2) + high CV (120) + price spike → low sellProb, low stabDisc
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 120, Listings: 2,
				Tier: "MID", Chaos: 100,
				Low7d: 30, High7d: 120,
			},
			FuturePct: -20.0,
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	if _, ok := report.ConfidenceCalibration["FAIR"]; !ok {
		t.Error("expected FAIR in ConfidenceCalibration")
	}
	if _, ok := report.ConfidenceCalibration["RISKY"]; !ok {
		t.Error("expected RISKY in ConfidenceCalibration")
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

// ---------------------------------------------------------------------------
// Per-variant breakdown tests
// ---------------------------------------------------------------------------

func TestValidateSellability_PerVariantBreakdown(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		// Variant "20/20" — 2 evals
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100, Variant: "20/20",
				Low7d: 80, High7d: 120,
			},
			FuturePct: -5.0, // future 95, held (>= 90)
			SnapTime:  t0,
		},
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100, Variant: "20/20",
				Low7d: 80, High7d: 120,
			},
			FuturePct: -15.0, // future 85, NOT held (< 90)
			SnapTime:  t0,
		},
		// Variant "1/20" — 1 eval
		{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 10, Listings: 30,
				Tier: "MID", Chaos: 100, Variant: "1/20",
				Low7d: 80, High7d: 120,
			},
			FuturePct: 5.0, // future 105, held (>= 90)
			SnapTime:  t0,
		},
	}

	report := ValidateSellability(evals, mc)

	// Should have 2 variants.
	if len(report.PerVariant) != 2 {
		t.Fatalf("expected 2 variants, got %d", len(report.PerVariant))
	}

	// Check "20/20" variant.
	vr2020, ok := report.PerVariant["20/20"]
	if !ok {
		t.Fatal("expected variant 20/20 in PerVariant")
	}
	if vr2020.TotalEvals != 2 {
		t.Errorf("20/20 TotalEvals: got %d, want 2", vr2020.TotalEvals)
	}

	// "20/20" confidence calibration: both should classify as FAIR (same as global test).
	// First held (95 >= 90), second not held (85 < 90).
	if len(vr2020.ConfidenceCalibration) == 0 {
		t.Fatal("expected non-empty ConfidenceCalibration for 20/20")
	}
	// Find the confidence level for 20/20 — should be FAIR with 2 entries, 1 held.
	totalConf := 0
	totalHeld := 0
	for _, cal := range vr2020.ConfidenceCalibration {
		totalConf += cal.Count
		totalHeld += cal.PriceHeld
	}
	if totalConf != 2 {
		t.Errorf("20/20 total conf calibration count: got %d, want 2", totalConf)
	}
	if totalHeld != 1 {
		t.Errorf("20/20 total conf calibration held: got %d, want 1", totalHeld)
	}

	// Check "1/20" variant.
	vr120, ok := report.PerVariant["1/20"]
	if !ok {
		t.Fatal("expected variant 1/20 in PerVariant")
	}
	if vr120.TotalEvals != 1 {
		t.Errorf("1/20 TotalEvals: got %d, want 1", vr120.TotalEvals)
	}

	// "1/20" should have 1 entry, held.
	totalConf = 0
	totalHeld = 0
	for _, cal := range vr120.ConfidenceCalibration {
		totalConf += cal.Count
		totalHeld += cal.PriceHeld
	}
	if totalConf != 1 {
		t.Errorf("1/20 total conf calibration count: got %d, want 1", totalConf)
	}
	if totalHeld != 1 {
		t.Errorf("1/20 total conf calibration held: got %d, want 1", totalHeld)
	}

	// Per-variant signal capture should exist.
	if len(vr2020.PerSignalCapture) == 0 {
		t.Error("expected non-empty PerSignalCapture for 20/20")
	}
	if len(vr120.PerSignalCapture) == 0 {
		t.Error("expected non-empty PerSignalCapture for 1/20")
	}

	// Per-variant floor hold should exist.
	if len(vr2020.FloorHoldRate) == 0 {
		t.Error("expected non-empty FloorHoldRate for 20/20")
	}
	if len(vr120.FloorHoldRate) == 0 {
		t.Error("expected non-empty FloorHoldRate for 1/20")
	}
}

