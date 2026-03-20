package lab

import (
	"math"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// sellProbabilityFactor tests
// ---------------------------------------------------------------------------

func TestSellProbabilityFactor_SigmoidCurve(t *testing.T) {
	// Test pure sigmoid at key listing counts with medianListings=20.
	// Use low7d=60 with currentPrice=100 to avoid thin-market adjustments:
	// low7d=60 is between 0.5*100=50 and 0.7*100=70, so no boost or penalty.
	// For listings >= 10, thin-market adjustments don't apply regardless.
	medianListings := 20.0
	tests := []struct {
		listings int
		wantMin  float64
		wantMax  float64
	}{
		{2, 0.30, 0.45},   // very few listings: near floor (no thin-market adj with low7d=60)
		{10, 0.40, 0.60},  // below midpoint
		{20, 0.60, 0.70},  // at sigmoid midpoint
		{50, 0.85, 0.98},  // well above midpoint
		{100, 0.95, 1.01}, // near ceiling
	}

	for _, tt := range tests {
		got := sellProbabilityFactor(tt.listings, 60, 100, medianListings)
		if got < tt.wantMin || got > tt.wantMax {
			t.Errorf("sellProbabilityFactor(listings=%d, median=%v) = %f, want [%f, %f]",
				tt.listings, medianListings, got, tt.wantMin, tt.wantMax)
		}
	}
}

func TestSellProbabilityFactor_ThinMarketStablePrice(t *testing.T) {
	// Thin listings (< 10) with stable historical price (low7d > 0.7 * current)
	// should boost the factor (genuine rarity).
	currentPrice := 100.0
	stableLow7d := 80.0 // 80 > 0.7 * 100 = 70
	median := 30.0

	boosted := sellProbabilityFactor(5, stableLow7d, currentPrice, median)
	normal := sellProbabilityFactor(5, 50, currentPrice, median) // low7d=50, between 50 and 70

	if boosted <= normal {
		t.Errorf("stable thin market should boost: boosted=%f, normal=%f", boosted, normal)
	}
	// Boosted should be capped at 1.0.
	if boosted > 1.0 {
		t.Errorf("boosted should be <= 1.0, got %f", boosted)
	}
}

func TestSellProbabilityFactor_ThinMarketSpikePrice(t *testing.T) {
	// Thin listings (< 10) with recent spike (low7d < 0.5 * current)
	// should penalize the factor (manipulation risk).
	currentPrice := 200.0
	spikeLow7d := 80.0 // 80 < 0.5 * 200 = 100
	median := 30.0

	penalized := sellProbabilityFactor(5, spikeLow7d, currentPrice, median)
	normal := sellProbabilityFactor(5, 120, currentPrice, median) // low7d=120, in the middle

	if penalized >= normal {
		t.Errorf("spike thin market should penalize: penalized=%f, normal=%f", penalized, normal)
	}
}

func TestSellProbabilityFactor_NotThinMarketNoAdjustment(t *testing.T) {
	// With >= 10 listings, thin-market adjustments should not apply.
	currentPrice := 100.0
	stableLow7d := 90.0 // stable, but listings >= 10
	median := 30.0

	val10 := sellProbabilityFactor(10, stableLow7d, currentPrice, median)
	val15 := sellProbabilityFactor(15, stableLow7d, currentPrice, median)

	// These should be pure sigmoid values, no rarity boost.
	if val10 < 0.3 || val10 > 1.0 {
		t.Errorf("sellProbabilityFactor(10) = %f, out of range", val10)
	}
	if val15 < val10 {
		t.Errorf("more listings should give higher factor: 15=%f < 10=%f", val15, val10)
	}
}

func TestSellProbabilityFactor_ZeroPrice(t *testing.T) {
	// Zero price should not trigger thin-market adjustment (guarded by currentPrice > 0).
	got := sellProbabilityFactor(5, 0, 0, 30)
	if got < 0.3 || got > 1.0 {
		t.Errorf("sellProbabilityFactor with zero price = %f, want [0.3, 1.0]", got)
	}
}

func TestSellProbabilityFactor_FloorEnforced(t *testing.T) {
	// With 1 listing and spike penalty (*0.5), the result
	// should be at least 0.3 (floor enforced).
	currentPrice := 200.0
	spikeLow7d := 50.0 // 50 < 0.5 * 200 = 100 → penalty

	got := sellProbabilityFactor(1, spikeLow7d, currentPrice, 30)
	if got < 0.3 {
		t.Errorf("sellProbabilityFactor(1 listing, spike) = %f, want >= 0.3 (floor)", got)
	}
}

// ---------------------------------------------------------------------------
// stabilityDiscount tests
// ---------------------------------------------------------------------------

func TestStabilityDiscount_KeyPoints(t *testing.T) {
	tests := []struct {
		cv   float64
		want float64
	}{
		{0, 1.0},     // zero CV: full discount (no discount)
		{10, 0.95},   // low CV
		{30, 0.85},   // moderate CV
		{50, 0.75},   // medium CV
		{100, 0.5},   // high CV: floor
		{200, 0.5},   // very high CV: clamped at floor
		{300, 0.5},   // extreme CV: clamped at floor
	}

	for _, tt := range tests {
		got := stabilityDiscount(tt.cv)
		if math.Abs(got-tt.want) > 0.001 {
			t.Errorf("stabilityDiscount(%f) = %f, want %f", tt.cv, got, tt.want)
		}
	}
}

func TestStabilityDiscount_Clamping(t *testing.T) {
	// Negative CV should still return <= 1.0.
	got := stabilityDiscount(-10)
	if got > 1.0 {
		t.Errorf("stabilityDiscount(-10) = %f, want <= 1.0", got)
	}
	if got < 0.5 {
		t.Errorf("stabilityDiscount(-10) = %f, want >= 0.5", got)
	}

	// Very high CV should be clamped at 0.5.
	got = stabilityDiscount(500)
	if got != 0.5 {
		t.Errorf("stabilityDiscount(500) = %f, want 0.5", got)
	}
}

// ---------------------------------------------------------------------------
// quickSellUndercutFactor tests
// ---------------------------------------------------------------------------

func TestQuickSellUndercutFactor_ListingBrackets(t *testing.T) {
	tests := []struct {
		listings int
		tier     string
		wantBase float64
	}{
		{50, "MID", 0.05},  // >= 30: 5%
		{30, "MID", 0.05},  // exactly 30: 5%
		{20, "MID", 0.10},  // >= 10: 10%
		{10, "MID", 0.10},  // exactly 10: 10%
		{7, "MID", 0.15},   // >= 5: 15%
		{5, "MID", 0.15},   // exactly 5: 15%
		{3, "MID", 0.25},   // < 5: 25%
		{0, "MID", 0.25},   // zero: 25%
	}

	for _, tt := range tests {
		got := quickSellUndercutFactor(tt.listings, tt.tier)
		if math.Abs(got-tt.wantBase) > 0.001 {
			t.Errorf("quickSellUndercutFactor(listings=%d, tier=%s) = %f, want %f",
				tt.listings, tt.tier, got, tt.wantBase)
		}
	}
}

func TestQuickSellUndercutFactor_TierModifier(t *testing.T) {
	// TOP tier adds 0.05, HIGH adds 0.02, MID/LOW add 0.
	midBase := quickSellUndercutFactor(20, "MID")
	highBase := quickSellUndercutFactor(20, "HIGH")
	topBase := quickSellUndercutFactor(20, "TOP")
	lowBase := quickSellUndercutFactor(20, "LOW")

	if math.Abs(highBase-midBase-0.02) > 0.001 {
		t.Errorf("HIGH should add 0.02: HIGH=%f, MID=%f", highBase, midBase)
	}
	if math.Abs(topBase-midBase-0.05) > 0.001 {
		t.Errorf("TOP should add 0.05: TOP=%f, MID=%f", topBase, midBase)
	}
	if math.Abs(lowBase-midBase) > 0.001 {
		t.Errorf("LOW should be same as MID: LOW=%f, MID=%f", lowBase, midBase)
	}
}

// ---------------------------------------------------------------------------
// classifySellConfidence tests
// ---------------------------------------------------------------------------

func TestClassifySellConfidence_SAFE(t *testing.T) {
	// SAFE requires sellProb >= 0.8 AND stabilityDisc >= 0.85.
	got := classifySellConfidence(0.8, 0.85)
	if got != "SAFE" {
		t.Errorf("classifySellConfidence(0.8, 0.85) = %q, want SAFE", got)
	}

	got = classifySellConfidence(0.9, 0.95)
	if got != "SAFE" {
		t.Errorf("classifySellConfidence(0.9, 0.95) = %q, want SAFE", got)
	}
}

func TestClassifySellConfidence_FAIR(t *testing.T) {
	// FAIR: not SAFE and not RISKY.
	tests := []struct {
		sellProb     float64
		stabilityDsc float64
	}{
		{0.5, 0.8},  // sellProb >= 0.5 but < 0.8, stabilityDisc < 0.85
		{0.8, 0.7},  // sellProb >= 0.8 but stabilityDisc < 0.85
		{0.6, 0.75}, // both moderate
		{0.4, 0.8},  // sellProb < 0.5 but stabilityDisc >= 0.7 → not RISKY
		{0.5, 0.6},  // sellProb >= 0.5, so not RISKY
	}

	for _, tt := range tests {
		got := classifySellConfidence(tt.sellProb, tt.stabilityDsc)
		if got != "FAIR" {
			t.Errorf("classifySellConfidence(%f, %f) = %q, want FAIR",
				tt.sellProb, tt.stabilityDsc, got)
		}
	}
}

func TestClassifySellConfidence_RISKY(t *testing.T) {
	// RISKY: sellProb < 0.5 AND stabilityDisc < 0.7.
	got := classifySellConfidence(0.4, 0.6)
	if got != "RISKY" {
		t.Errorf("classifySellConfidence(0.4, 0.6) = %q, want RISKY", got)
	}

	got = classifySellConfidence(0.3, 0.5)
	if got != "RISKY" {
		t.Errorf("classifySellConfidence(0.3, 0.5) = %q, want RISKY", got)
	}
}

// ---------------------------------------------------------------------------
// Integration: ComputeGemFeatures → ComputeGemSignals produces non-zero risk fields
// ---------------------------------------------------------------------------

func TestIntegration_RiskAdjustedValueNonZero(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	t0 := snapTime.Add(-90 * time.Minute)

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 25, IsTransfigured: true, GemColor: "BLUE"},
	}

	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 180, Listings: 20},
				{Time: t0.Add(30 * time.Minute), Chaos: 185, Listings: 22},
				{Time: t0.Add(60 * time.Minute), Chaos: 190, Listings: 24},
				{Time: t0.Add(90 * time.Minute), Chaos: 200, Listings: 25},
			},
		},
	}

	mc := testSignalMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}

	f := features[0]

	// Verify SellProbabilityFactor is computed.
	if f.SellProbabilityFactor <= 0 || f.SellProbabilityFactor > 1.0 {
		t.Errorf("SellProbabilityFactor = %f, want (0, 1.0]", f.SellProbabilityFactor)
	}
	// Verify StabilityDiscount is computed.
	if f.StabilityDiscount < 0.5 || f.StabilityDiscount > 1.0 {
		t.Errorf("StabilityDiscount = %f, want [0.5, 1.0]", f.StabilityDiscount)
	}

	// Now compute signals.
	baseGems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 50, IsTransfigured: false, GemColor: "BLUE"},
	}
	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: t0, Chaos: 5, Listings: 55},
			{Time: t0.Add(90 * time.Minute), Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, features, mc, baseGems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]

	// RiskAdjustedValue should be positive (price * sellProb * stabilityDisc).
	if sig.RiskAdjustedValue <= 0 {
		t.Errorf("RiskAdjustedValue = %f, want > 0", sig.RiskAdjustedValue)
	}
	// RiskAdjustedValue should be <= price (factors are <= 1.0).
	if sig.RiskAdjustedValue > f.Chaos {
		t.Errorf("RiskAdjustedValue = %f > Chaos = %f, should be <=", sig.RiskAdjustedValue, f.Chaos)
	}

	// QuickSellPrice should be positive and less than current price.
	if sig.QuickSellPrice <= 0 {
		t.Errorf("QuickSellPrice = %f, want > 0", sig.QuickSellPrice)
	}
	if sig.QuickSellPrice >= f.Chaos {
		t.Errorf("QuickSellPrice = %f >= Chaos = %f, should be less", sig.QuickSellPrice, f.Chaos)
	}

	// SellConfidence should be one of the valid values.
	validConf := map[string]bool{"SAFE": true, "FAIR": true, "RISKY": true}
	if !validConf[sig.SellConfidence] {
		t.Errorf("SellConfidence = %q, want SAFE/FAIR/RISKY", sig.SellConfidence)
	}
}

func TestIntegration_NoHistoryProducesValidRiskFields(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "New of Gem", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "GREEN"},
	}

	mc := testSignalMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}

	f := features[0]
	if f.SellProbabilityFactor <= 0 {
		t.Errorf("SellProbabilityFactor = %f, want > 0", f.SellProbabilityFactor)
	}
	if f.StabilityDiscount != 1.0 {
		t.Errorf("StabilityDiscount = %f, want 1.0 (CV=0 → no discount)", f.StabilityDiscount)
	}

	baseGems := []GemPrice{
		{Name: "New", Variant: "20/20", Chaos: 5, Listings: 30, IsTransfigured: false, GemColor: "GREEN"},
	}

	signals := ComputeGemSignals(snapTime, features, mc, baseGems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.RiskAdjustedValue <= 0 {
		t.Errorf("RiskAdjustedValue = %f, want > 0", sig.RiskAdjustedValue)
	}
	if sig.QuickSellPrice <= 0 {
		t.Errorf("QuickSellPrice = %f, want > 0", sig.QuickSellPrice)
	}
	validConf := map[string]bool{"SAFE": true, "FAIR": true, "RISKY": true}
	if !validConf[sig.SellConfidence] {
		t.Errorf("SellConfidence = %q, want SAFE/FAIR/RISKY", sig.SellConfidence)
	}
}
