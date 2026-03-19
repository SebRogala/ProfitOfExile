package lab

import (
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// testConfidenceMarketContext returns a MarketContext with populated temporal
// slices suitable for confidence scoring tests.
// ---------------------------------------------------------------------------
func testConfidenceMarketContext() MarketContext {
	mc := testMarketContext()
	// Set up temporal biases: bullish at hour 14, bearish at hour 3.
	for i := 0; i < 24; i++ {
		mc.HourlyBias[i] = 1.0
		mc.HourlyVolatility[i] = 0.02
		mc.HourlyActivity[i] = 0.3
	}
	mc.HourlyBias[14] = 1.10 // bullish hour
	mc.HourlyBias[3] = 0.90  // bearish hour
	mc.HourlyVolatility[3] = 0.08

	for i := 0; i < 7; i++ {
		mc.WeekdayBias[i] = 1.0
		mc.WeekdayVolatility[i] = 0.02
		mc.WeekdayActivity[i] = 0.3
	}
	mc.WeekdayBias[1] = 1.05 // Monday bullish
	mc.WeekdayBias[0] = 0.95 // Sunday bearish
	return mc
}

// ---------------------------------------------------------------------------
// computeConfidence tests
// ---------------------------------------------------------------------------

func TestComputeConfidence_HERDAllAgree(t *testing.T) {
	// HERD signal + all windows agree (short/med/long all positive) +
	// bullish hour + no flood history => high confidence (~80-95).
	f := GemFeature{
		VelShortPrice: 5,
		VelMedPrice:   4,
		VelLongPrice:  3,
		FloodCount:    0,
		CrashCount:    0,
		CV:            30,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: -0.5,
	}
	mc := testConfidenceMarketContext()
	// Monday 14:00 UTC => bullish hour + Monday weekday.
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC) // Monday

	confidence, _ := computeConfidence("HERD", f, mc, snapTime)
	if confidence < 80 || confidence > 95 {
		t.Errorf("HERD+allAgree+bullish confidence = %d, want 80-95", confidence)
	}
}

func TestComputeConfidence_RISINGConflicting(t *testing.T) {
	// RISING signal + conflicting windows (short up, long down) +
	// bearish hour => low confidence (~25-40).
	f := GemFeature{
		VelShortPrice: 3,
		VelMedPrice:   -1,
		VelLongPrice:  -2,
		FloodCount:    0,
		CrashCount:    0,
		CV:            40,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	// Sunday 03:00 UTC => bearish hour + Sunday weekday.
	snapTime := time.Date(2026, 3, 15, 3, 0, 0, 0, time.UTC) // Sunday

	confidence, _ := computeConfidence("RISING", f, mc, snapTime)
	if confidence < 25 || confidence > 40 {
		t.Errorf("RISING+conflicting+bearish confidence = %d, want 25-40", confidence)
	}
}

func TestComputeConfidence_STABLENeutral(t *testing.T) {
	// STABLE signal + neutral everything => moderate confidence (~50-60).
	f := GemFeature{
		VelShortPrice: 0,
		VelMedPrice:   0,
		VelLongPrice:  0,
		FloodCount:    0,
		CrashCount:    0,
		CV:            20,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         30,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	// Tuesday 12:00 UTC => neutral.
	snapTime := time.Date(2026, 3, 17, 12, 0, 0, 0, time.UTC)

	confidence, _ := computeConfidence("STABLE", f, mc, snapTime)
	if confidence < 50 || confidence > 60 {
		t.Errorf("STABLE+neutral confidence = %d, want 50-60", confidence)
	}
}

func TestComputeConfidence_TRAPAlwaysLow(t *testing.T) {
	// TRAP signal => always low confidence regardless of other factors (~10-20).
	f := GemFeature{
		VelShortPrice: 5,
		VelMedPrice:   5,
		VelLongPrice:  5,
		FloodCount:    0,
		CrashCount:    0,
		CV:            110,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC) // bullish

	confidence, _ := computeConfidence("TRAP", f, mc, snapTime)
	if confidence < 10 || confidence > 20 {
		t.Errorf("TRAP confidence = %d, want 10-20", confidence)
	}
}

func TestComputeConfidence_FloodCrashDampens(t *testing.T) {
	// Gem with FloodCount>2 and CrashCount>2 => profile modifier dampens.
	fStable := GemFeature{
		VelShortPrice: 3,
		VelMedPrice:   3,
		VelLongPrice:  3,
		FloodCount:    0,
		CrashCount:    0,
		CV:            30,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	fUnstable := GemFeature{
		VelShortPrice: 3,
		VelMedPrice:   3,
		VelLongPrice:  3,
		FloodCount:    3,
		CrashCount:    3,
		CV:            30,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	confStable, _ := computeConfidence("RISING", fStable, mc, snapTime)
	confUnstable, _ := computeConfidence("RISING", fUnstable, mc, snapTime)

	if confUnstable >= confStable {
		t.Errorf("unstable gem confidence (%d) should be < stable gem confidence (%d)", confUnstable, confStable)
	}
}

func TestComputeConfidence_ZeroHistory(t *testing.T) {
	// Zero-history gem (all velocities 0) => low confidence.
	f := GemFeature{
		VelShortPrice: 0,
		VelMedPrice:   0,
		VelLongPrice:  0,
		FloodCount:    0,
		CrashCount:    0,
		CV:            0,
		RelativePrice:    0,
		RelativeListings: 0,
		Listings:         0,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC)

	confidence, _ := computeConfidence("RISING", f, mc, snapTime)
	// With all zeros, the signal has no data backing it.
	if confidence > 40 {
		t.Errorf("zero-history confidence = %d, want <= 40", confidence)
	}
}

func TestComputeConfidence_TemporalDifference(t *testing.T) {
	// Same signal at bullish hour vs bearish hour => different confidence values.
	f := GemFeature{
		VelShortPrice: 3,
		VelMedPrice:   3,
		VelLongPrice:  3,
		FloodCount:    0,
		CrashCount:    0,
		CV:            30,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	bullishTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC) // Monday 14:00
	bearishTime := time.Date(2026, 3, 15, 3, 0, 0, 0, time.UTC)  // Sunday 03:00

	confBullish, _ := computeConfidence("RISING", f, mc, bullishTime)
	confBearish, _ := computeConfidence("RISING", f, mc, bearishTime)

	if confBullish <= confBearish {
		t.Errorf("bullish confidence (%d) should be > bearish confidence (%d)", confBullish, confBearish)
	}
}

func TestComputeConfidence_PhaseModifier(t *testing.T) {
	// Verify that phaseModifier is returned and is a reasonable value.
	f := GemFeature{
		VelShortPrice: 5,
		VelMedPrice:   4,
		VelLongPrice:  3,
		FloodCount:    0,
		CrashCount:    0,
		CV:            30,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: -0.5,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	_, phaseModifier := computeConfidence("HERD", f, mc, snapTime)
	if phaseModifier < 0 || phaseModifier > 2.0 {
		t.Errorf("phaseModifier = %f, want 0-2.0", phaseModifier)
	}
}

func TestComputeConfidence_DUMPINGHighBase(t *testing.T) {
	// DUMPING has high base_signal_strength (1.3), should produce decent confidence
	// with agreeing windows.
	f := GemFeature{
		VelShortPrice: -5,
		VelMedPrice:   -4,
		VelLongPrice:  -3,
		FloodCount:    0,
		CrashCount:    0,
		CV:            40,
		RelativePrice:    1.0,
		RelativeListings: 1.0,
		Listings:         20,
		ListingElasticity: 0,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	confidence, _ := computeConfidence("DUMPING", f, mc, snapTime)
	if confidence < 60 {
		t.Errorf("DUMPING+allAgreeDown confidence = %d, want >= 60", confidence)
	}
}

func TestComputeConfidence_Clamped0to100(t *testing.T) {
	// Regardless of extreme inputs, confidence must be 0-100.
	f := GemFeature{
		VelShortPrice: 100,
		VelMedPrice:   100,
		VelLongPrice:  100,
		FloodCount:    0,
		CrashCount:    0,
		CV:            5,
		RelativePrice:    2.0,
		RelativeListings: 2.0,
		Listings:         50,
		ListingElasticity: -1.0,
	}
	mc := testConfidenceMarketContext()
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	confidence, _ := computeConfidence("HERD", f, mc, snapTime)
	if confidence < 0 || confidence > 100 {
		t.Errorf("confidence = %d, want 0-100", confidence)
	}
}

// ---------------------------------------------------------------------------
// windowAgreement tests
// ---------------------------------------------------------------------------

func TestWindowAgreement_AllPositive(t *testing.T) {
	got := windowAgreement(3.0, 2.0, 1.0)
	if got != 1.4 {
		t.Errorf("windowAgreement(3,2,1) = %f, want 1.4 (all agree)", got)
	}
}

func TestWindowAgreement_AllNegative(t *testing.T) {
	got := windowAgreement(-3.0, -2.0, -1.0)
	if got != 1.4 {
		t.Errorf("windowAgreement(-3,-2,-1) = %f, want 1.4 (all agree)", got)
	}
}

func TestWindowAgreement_TwoAgree(t *testing.T) {
	got := windowAgreement(3.0, 2.0, -1.0)
	if got != 1.0 {
		t.Errorf("windowAgreement(3,2,-1) = %f, want 1.0 (two agree)", got)
	}
}

func TestWindowAgreement_Conflicting(t *testing.T) {
	got := windowAgreement(3.0, -2.0, -1.0)
	if got != 0.6 {
		t.Errorf("windowAgreement(3,-2,-1) = %f, want 0.6 (conflicting)", got)
	}
}

func TestWindowAgreement_AllZero(t *testing.T) {
	// Zero velocities should not crash. Neutral or all-agree.
	got := windowAgreement(0, 0, 0)
	// All zero = all same sign (non-negative) or treated as agreeing.
	if got < 0.6 || got > 1.4 {
		t.Errorf("windowAgreement(0,0,0) = %f, want in [0.6, 1.4]", got)
	}
}

func TestWindowAgreement_MixedWithZero(t *testing.T) {
	// One positive, two zero — only one non-zero window, insufficient for agreement.
	got := windowAgreement(3.0, 0, 0)
	if got != 1.0 {
		t.Errorf("windowAgreement(3,0,0) = %f, want 1.0 (only one non-zero)", got)
	}
}

func TestWindowAgreement_MedLongAgreeShortZero(t *testing.T) {
	// Short is zero (no data) but med+long agree: neutral, not conflicting.
	got := windowAgreement(0, 5.0, 4.0)
	if got != 1.0 {
		t.Errorf("windowAgreement(0,5,4) = %f, want 1.0 (med+long agree, short absent)", got)
	}
}

func TestWindowAgreement_MedLongAgreeNegativeShortZero(t *testing.T) {
	// Short is zero but med+long both negative: neutral, not conflicting.
	got := windowAgreement(0, -5.0, -4.0)
	if got != 1.0 {
		t.Errorf("windowAgreement(0,-5,-4) = %f, want 1.0 (med+long agree negative, short absent)", got)
	}
}

func TestWindowAgreement_MedLongConflictShortZero(t *testing.T) {
	// Short is zero, med and long conflict each other: only 2 windows, they disagree.
	// Treated as 0.6 because the 2 available windows conflict.
	got := windowAgreement(0, 5.0, -3.0)
	if got != 0.6 {
		t.Errorf("windowAgreement(0,5,-3) = %f, want 0.6 (med+long conflict, short absent)", got)
	}
}

func TestWindowAgreement_ShortPresentConflictsWithMed(t *testing.T) {
	// Short is present and disagrees with medium: conflicting near-term data.
	// Even though med+long agree, short-term contradiction is a warning.
	got := windowAgreement(3.0, -2.0, -1.0)
	if got != 0.6 {
		t.Errorf("windowAgreement(3,-2,-1) = %f, want 0.6 (short vs med conflict)", got)
	}
}

// ---------------------------------------------------------------------------
// profileModifier tests
// ---------------------------------------------------------------------------

func TestProfileModifier_StableGem(t *testing.T) {
	f := GemFeature{
		FloodCount:        0,
		CrashCount:        0,
		CV:                15,
		ListingElasticity: -0.5,
	}
	got := profileModifier(f)
	// Low CV + negative elasticity => 1.2 (predictable).
	if got != 1.2 {
		t.Errorf("profileModifier(stable) = %f, want 1.2", got)
	}
}

func TestProfileModifier_VolatileGem(t *testing.T) {
	f := GemFeature{
		FloodCount:        0,
		CrashCount:        0,
		CV:                90,
		ListingElasticity: 0,
	}
	got := profileModifier(f)
	// High CV => 0.8.
	if got != 0.8 {
		t.Errorf("profileModifier(volatile) = %f, want 0.8", got)
	}
}

func TestProfileModifier_FloodProneGem(t *testing.T) {
	f := GemFeature{
		FloodCount:        3,
		CrashCount:        3,
		CV:                50,
		ListingElasticity: 0,
	}
	got := profileModifier(f)
	// FloodCount>2 or CrashCount>2 => 0.7 (unstable).
	if got != 0.7 {
		t.Errorf("profileModifier(flood-prone) = %f, want 0.7", got)
	}
}

func TestProfileModifier_NeutralGem(t *testing.T) {
	f := GemFeature{
		FloodCount:        0,
		CrashCount:        0,
		CV:                50,
		ListingElasticity: 0.5,
	}
	got := profileModifier(f)
	// Not predictable, not volatile, not flood-prone => 1.0.
	if got != 1.0 {
		t.Errorf("profileModifier(neutral) = %f, want 1.0", got)
	}
}

// ---------------------------------------------------------------------------
// marketModifier tests
// ---------------------------------------------------------------------------

func TestMarketModifier_ThinListingsHighPrice(t *testing.T) {
	f := GemFeature{
		RelativePrice:    3.0, // outlier high price
		RelativeListings: 0.1, // very thin listings
	}
	got := marketModifier(f)
	// Outlier thin-listing high-price => 0.7 (manipulation risk).
	if got != 0.7 {
		t.Errorf("marketModifier(thin+expensive) = %f, want 0.7", got)
	}
}

func TestMarketModifier_Normal(t *testing.T) {
	f := GemFeature{
		RelativePrice:    1.0,
		RelativeListings: 1.0,
	}
	got := marketModifier(f)
	// Normal => 1.0.
	if got != 1.0 {
		t.Errorf("marketModifier(normal) = %f, want 1.0", got)
	}
}

func TestMarketModifier_ThickListings(t *testing.T) {
	f := GemFeature{
		RelativePrice:    0.5,
		RelativeListings: 3.0,
	}
	got := marketModifier(f)
	// Thick listings, low price => normal or slight boost.
	if got < 1.0 || got > 1.2 {
		t.Errorf("marketModifier(thick+cheap) = %f, want 1.0-1.2", got)
	}
}

// ---------------------------------------------------------------------------
// safeIndex tests
// ---------------------------------------------------------------------------

func TestSafeIndex_InBounds(t *testing.T) {
	s := []float64{0.9, 1.0, 1.1}
	got := safeIndex(s, 1, 1.0)
	if got != 1.0 {
		t.Errorf("safeIndex(s, 1, 1.0) = %f, want 1.0", got)
	}
}

func TestSafeIndex_OutOfBounds(t *testing.T) {
	s := []float64{0.9, 1.0, 1.1}
	got := safeIndex(s, 5, 1.0)
	if got != 1.0 {
		t.Errorf("safeIndex(s, 5, 1.0) = %f, want 1.0 (default)", got)
	}
}

func TestSafeIndex_NegativeIndex(t *testing.T) {
	s := []float64{0.9, 1.0, 1.1}
	got := safeIndex(s, -1, 1.0)
	if got != 1.0 {
		t.Errorf("safeIndex(s, -1, 1.0) = %f, want 1.0 (default)", got)
	}
}

func TestSafeIndex_EmptySlice(t *testing.T) {
	got := safeIndex(nil, 0, 1.0)
	if got != 1.0 {
		t.Errorf("safeIndex(nil, 0, 1.0) = %f, want 1.0 (default)", got)
	}
}

func TestSafeIndex_LastElement(t *testing.T) {
	s := []float64{0.9, 1.0, 1.1}
	got := safeIndex(s, 2, 1.0)
	if got != 1.1 {
		t.Errorf("safeIndex(s, 2, 1.0) = %f, want 1.1", got)
	}
}

// ---------------------------------------------------------------------------
// clampFloat64 tests
// ---------------------------------------------------------------------------

func TestClampFloat64_BelowMin(t *testing.T) {
	got := clampFloat64(0.3, 0.5, 1.5)
	if got != 0.5 {
		t.Errorf("clampFloat64(0.3, 0.5, 1.5) = %f, want 0.5", got)
	}
}

func TestClampFloat64_AboveMax(t *testing.T) {
	got := clampFloat64(2.0, 0.5, 1.5)
	if got != 1.5 {
		t.Errorf("clampFloat64(2.0, 0.5, 1.5) = %f, want 1.5", got)
	}
}

func TestClampFloat64_InRange(t *testing.T) {
	got := clampFloat64(1.0, 0.5, 1.5)
	if got != 1.0 {
		t.Errorf("clampFloat64(1.0, 0.5, 1.5) = %f, want 1.0", got)
	}
}

func TestClampFloat64_AtMin(t *testing.T) {
	got := clampFloat64(0.5, 0.5, 1.5)
	if got != 0.5 {
		t.Errorf("clampFloat64(0.5, 0.5, 1.5) = %f, want 0.5", got)
	}
}

func TestClampFloat64_AtMax(t *testing.T) {
	got := clampFloat64(1.5, 0.5, 1.5)
	if got != 1.5 {
		t.Errorf("clampFloat64(1.5, 0.5, 1.5) = %f, want 1.5", got)
	}
}

// ---------------------------------------------------------------------------
// clampInt tests
// ---------------------------------------------------------------------------

func TestClampInt_BelowMin(t *testing.T) {
	got := clampInt(-5, 0, 100)
	if got != 0 {
		t.Errorf("clampInt(-5, 0, 100) = %d, want 0", got)
	}
}

func TestClampInt_AboveMax(t *testing.T) {
	got := clampInt(150, 0, 100)
	if got != 100 {
		t.Errorf("clampInt(150, 0, 100) = %d, want 100", got)
	}
}

func TestClampInt_InRange(t *testing.T) {
	got := clampInt(50, 0, 100)
	if got != 50 {
		t.Errorf("clampInt(50, 0, 100) = %d, want 50", got)
	}
}

func TestClampInt_AtMin(t *testing.T) {
	got := clampInt(0, 0, 100)
	if got != 0 {
		t.Errorf("clampInt(0, 0, 100) = %d, want 0", got)
	}
}

func TestClampInt_AtMax(t *testing.T) {
	got := clampInt(100, 0, 100)
	if got != 100 {
		t.Errorf("clampInt(100, 0, 100) = %d, want 100", got)
	}
}
