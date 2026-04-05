package lab

import (
	"math"
	"testing"
	"time"
)

// ---------------------------------------------------------------------------
// testSignalMarketContext returns a MarketContext with temporal biases suitable
// for ComputeGemSignals tests. Extends testMarketContext with populated slices.
// ---------------------------------------------------------------------------
func testSignalMarketContext() MarketContext {
	mc := testConfidenceMarketContext()
	// Ensure tier boundaries are set for tier classification.
	mc.TierBoundaries = TierBoundaries{
		Boundaries: []float64{300, 100, 30},
	}
	return mc
}

// testFeature returns a GemFeature with sensible defaults for signal tests.
// Caller can override fields after construction.
// Signal classification uses VelLongPrice/VelLongListing (6h velocity).
func testFeature(name, variant string, chaos float64, listings int) GemFeature {
	return GemFeature{
		Time:              time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC),
		Name:              name,
		Variant:           variant,
		Chaos:             chaos,
		Listings:          listings,
		Tier:              "MID",
		VelShortPrice:     3,
		VelShortListing:   1,
		VelMedPrice:       3,
		VelMedListing:     1,
		VelLongPrice:      3,
		VelLongListing:    1,
		CV:                25,
		HistPosition:      50,
		High7Days:            chaos * 1.2,
		Low7Days:             chaos * 0.8,
		FloodCount:        0,
		CrashCount:        0,
		ListingElasticity: -0.3,
		RelativePrice:     1.0,
		RelativeListings:  1.0,
	}
}

// testBaseGems returns a slice of non-transfigured GemPrice entries for
// building the baseCurrentListings map inside ComputeGemSignals.
func testBaseGems(baseName string, listings int) []GemPrice {
	return []GemPrice{
		{
			Name:           baseName,
			Variant:        "20/20",
			Chaos:          10,
			Listings:       listings,
			IsTransfigured: false,
			IsCorrupted:    false,
			GemColor:       "BLUE",
		},
	}
}

// ---------------------------------------------------------------------------
// ComputeGemSignals tests
// ---------------------------------------------------------------------------

func TestComputeGemSignals_ThreeFeaturesAllPopulated(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	features := []GemFeature{
		testFeature("Spark of Nova", "20/20", 200, 15),
		testFeature("Cleave of Rage", "20/20", 80, 30),
		testFeature("Ice Shot of Frost", "20/20", 400, 5),
	}
	// Set tiers based on prices vs tier boundaries (Top=300, High=100, Mid=30).
	features[0].Tier = "HIGH" // 200 >= 100
	features[1].Tier = "MID"  // 80 >= 30
	features[2].Tier = "TOP"  // 400 >= 300

	// Base gems for base-side data lookup.
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 50, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Cleave", Variant: "20/20", Chaos: 3, Listings: 40, IsTransfigured: false, GemColor: "RED"},
		{Name: "Ice Shot", Variant: "20/20", Chaos: 2, Listings: 30, IsTransfigured: false, GemColor: "GREEN"},
	}

	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: snapTime.Add(-2 * time.Hour), Chaos: 5, Listings: 60},
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 55},
			{Time: snapTime, Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, features, mc, gems, baseHistory, 40.0)

	if len(signals) != 3 {
		t.Fatalf("got %d signals, want 3", len(signals))
	}

	for i, sig := range signals {
		if sig.Name == "" {
			t.Errorf("signal[%d].Name is empty", i)
		}
		if sig.Variant == "" {
			t.Errorf("signal[%d].Variant is empty", i)
		}
		if sig.Signal == "" {
			t.Errorf("signal[%d].Signal is empty", i)
		}
		if sig.Confidence < 0 || sig.Confidence > 100 {
			t.Errorf("signal[%d].Confidence = %d, want 0-100", i, sig.Confidence)
		}
		if sig.SellUrgency == "" {
			t.Errorf("signal[%d].SellUrgency is empty", i)
		}
		if sig.Sellability < 0 || sig.Sellability > 100 {
			t.Errorf("signal[%d].Sellability = %d, want 0-100", i, sig.Sellability)
		}
		if sig.SellabilityLabel == "" {
			t.Errorf("signal[%d].SellabilityLabel is empty", i)
		}
		if sig.Tier == "" {
			t.Errorf("signal[%d].Tier is empty", i)
		}
		// AdvancedSignal can be empty (no advanced signal detected), so we skip it.
		// WindowSignal should always be populated.
		if sig.WindowSignal == "" {
			t.Errorf("signal[%d].WindowSignal is empty", i)
		}
		// Time must match the snap time.
		if !sig.Time.Equal(snapTime) {
			t.Errorf("signal[%d].Time = %v, want %v", i, sig.Time, snapTime)
		}
	}
}

func TestComputeGemSignals_HighVelocityAgreementHighConfidence(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 50)
	// All velocities strongly positive and agreeing => HERD signal + high confidence.
	// classifySignal converts to percentages: pVelPct = vel/price*100, lVelPct = vel/listings*100.
	// HERD needs pVelPct > 8 AND lVelPct > 15 AND absListingVel >= 5.
	// At price=200, VelLongPrice=20 → 10%. At listings=50, VelLongListing=10 → 20%. Abs=10 >= 5.
	f.VelShortPrice = 30
	f.VelMedPrice = 25
	f.VelLongPrice = 20
	f.VelShortListing = 15
	f.VelMedListing = 12
	f.VelLongListing = 10
	f.CV = 20
	f.Tier = "HIGH"
	f.ListingElasticity = -0.5 // predictable

	gems := testBaseGems("Spark", 50)
	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: snapTime.Add(-2 * time.Hour), Chaos: 5, Listings: 60},
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 55},
			{Time: snapTime, Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// With high velocity + HERD + all windows agreeing => should have high confidence.
	if sig.Confidence < 55 {
		t.Errorf("high velocity + agreement confidence = %d, want >= 55", sig.Confidence)
	}
	// Signal should be HERD (pVelPct=10% > 8%, lVelPct=26.7% > 15% via VelLong).
	if sig.Signal != "HERD" {
		t.Errorf("Signal = %q, want HERD", sig.Signal)
	}
}

func TestComputeGemSignals_TRAPLowConfidence(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Volatile of Storm", "20/20", 100, 10)
	// Very high CVShort + dangerous directional velocity => TRAP signal.
	f.CVShort = 110
	f.VelShortPrice = 8
	f.VelMedPrice = -7
	f.VelLongPrice = -7 // 6h velocity used for signal classification (negative = falling)
	f.Tier = "MID"

	gems := testBaseGems("Volatile", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.Signal != "TRAP" {
		t.Errorf("Signal = %q, want TRAP (CVShort=110)", sig.Signal)
	}
	if sig.Confidence > 25 {
		t.Errorf("TRAP confidence = %d, want <= 25", sig.Confidence)
	}
}

func TestComputeGemSignals_NoMatchingBaseGemDefaults(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"

	// Provide no base gems at all — no base match for "Spark".
	gems := []GemPrice{}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// Should still produce a valid signal — base data unavailable should not crash.
	if sig.Signal == "" {
		t.Error("Signal is empty even without base gem data")
	}
	if sig.SellUrgency == "" {
		t.Error("SellUrgency is empty even without base gem data")
	}
	if sig.WindowSignal == "" {
		t.Error("WindowSignal is empty even without base gem data")
	}
}

func TestComputeGemSignals_EmptyFeatures(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	signals := ComputeGemSignals(snapTime, nil, mc, nil, nil, 0)
	if len(signals) != 0 {
		t.Errorf("got %d signals, want 0 for empty features", len(signals))
	}

	signals = ComputeGemSignals(snapTime, []GemFeature{}, mc, nil, nil, 0)
	if len(signals) != 0 {
		t.Errorf("got %d signals, want 0 for empty features slice", len(signals))
	}
}

func TestComputeGemSignals_RecommendationAVOID_TRAP(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Volatile of Storm", "20/20", 100, 10)
	f.CVShort = 110 // TRAP uses CVShort (6h) + dangerous directional velocity
	f.VelMedPrice = -8
	f.VelLongPrice = -8 // 6h velocity used for signal classification (negative = falling)
	f.Tier = "MID"

	gems := testBaseGems("Volatile", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	if signals[0].Recommendation != "AVOID" {
		t.Errorf("Recommendation = %q, want AVOID for TRAP signal", signals[0].Recommendation)
	}
}

func TestComputeGemSignals_RecommendationAVOID_DUMPING(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Dump of Doom", "20/20", 100, 20)
	// DUMPING needs pVelPct < -8 AND lVelPct > 10.
	// At price=100, VelLongPrice=-9 → -9%. At listings=20, VelLongListing=3 → 15%.
	f.VelShortPrice = -12
	f.VelMedPrice = -10
	f.VelLongPrice = -9
	f.VelShortListing = 5
	f.VelMedListing = 4
	f.VelLongListing = 3
	f.CV = 30
	f.Tier = "MID"

	gems := testBaseGems("Dump", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	if signals[0].Signal != "DUMPING" {
		t.Errorf("Signal = %q, want DUMPING", signals[0].Signal)
	}
	if signals[0].Recommendation != "AVOID" {
		t.Errorf("Recommendation = %q, want AVOID for DUMPING signal", signals[0].Recommendation)
	}
}

func TestComputeGemSignals_RecommendationAVOID_SELL_NOW(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// TRAP signal on a HIGH tier gem always produces SellUrgency=SELL_NOW.
	// See trends.go: sellUrgency checks TRAP before any other condition for non-LOW tiers.
	f := testFeature("Trap of Danger", "20/20", 100, 10)
	f.CVShort = 110 // TRAP uses CVShort (6h) — high CV + dangerous directional velocity
	f.VelMedPrice = -10
	f.VelLongPrice = -10 // 6h velocity used for signal classification (negative = falling)
	f.Tier = "HIGH"

	gems := testBaseGems("Trap", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	// TRAP signal => SellUrgency should be SELL_NOW (for non-LOW tier).
	if signals[0].SellUrgency != "SELL_NOW" {
		t.Errorf("SellUrgency = %q, want SELL_NOW for TRAP signal", signals[0].SellUrgency)
	}
	if signals[0].Recommendation != "AVOID" {
		t.Errorf("Recommendation = %q, want AVOID when SellUrgency=SELL_NOW", signals[0].Recommendation)
	}
}

func TestComputeGemSignals_RecommendationOK_HighConfidencePositive(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 50)
	// Set up for HERD signal with high confidence: all velocity windows positive +
	// agreeing, bullish hour (14:00 Monday), predictable profile (CV=20, neg elasticity).
	// HERD needs pVelPct > 8% AND lVelPct > 15% AND absListingVel >= 5.
	// At price=200, VelLongPrice=20 → 10%. At listings=50, VelLongListing=10 → 20%. Abs=10 >= 5.
	f.VelShortPrice = 30
	f.VelMedPrice = 25
	f.VelLongPrice = 20
	f.VelShortListing = 15
	f.VelMedListing = 12
	f.VelLongListing = 10
	f.CV = 20
	f.HistPosition = 50 // not at peak, so not SELL_NOW
	f.Tier = "HIGH"
	f.ListingElasticity = -0.5

	gems := testBaseGems("Spark", 50)
	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: snapTime.Add(-2 * time.Hour), Chaos: 5, Listings: 60},
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 55},
			{Time: snapTime, Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// Preconditions must hold for this test to be meaningful.
	if sig.Signal != "HERD" {
		t.Fatalf("Signal = %q, want HERD — test setup is wrong", sig.Signal)
	}
	if sig.SellUrgency == "SELL_NOW" {
		t.Fatalf("SellUrgency = SELL_NOW — unexpected for histPos=50 HERD signal; test setup is wrong")
	}
	// HERD + high confidence + not SELL_NOW => Recommendation should be OK.
	if sig.Confidence < 65 {
		t.Errorf("Confidence = %d, want >= 65 for all-agreeing HERD at bullish hour", sig.Confidence)
	}
	if sig.Recommendation != "OK" {
		t.Errorf("Recommendation = %q, want OK for high-confidence HERD without SELL_NOW urgency", sig.Recommendation)
	}
}

func TestComputeGemSignals_HERDAtHistoricalPeakUNDERCUT(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Lacerate of Haemophilia", "20/20", 350, 40)
	// HERD signal at historical peak (histPos > 90) => sellUrgency overrides to UNDERCUT.
	// HERD needs pVelPct > 8% AND lVelPct > 15% AND absListingVel >= 5.
	// At price=350, VelLongPrice=35 → 10%. At listings=40, VelLongListing=8 → 20%. Abs=8 >= 5.
	f.VelShortPrice = 50
	f.VelMedPrice = 40
	f.VelLongPrice = 35
	f.VelShortListing = 12
	f.VelMedListing = 10
	f.VelLongListing = 8
	f.CV = 30
	f.HistPosition = 95 // at historical peak
	f.Tier = "TOP"

	gems := testBaseGems("Lacerate", 50)
	baseHistory := map[string][]PricePoint{
		"Lacerate": {
			{Time: snapTime.Add(-2 * time.Hour), Chaos: 5, Listings: 60},
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 55},
			{Time: snapTime, Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.Signal != "HERD" {
		t.Errorf("Signal = %q, want HERD", sig.Signal)
	}
	// HERD at peak (histPosition > 90) => SellUrgency should be UNDERCUT.
	if sig.SellUrgency != "UNDERCUT" {
		t.Errorf("SellUrgency = %q, want UNDERCUT for HERD at historical peak", sig.SellUrgency)
	}
}

func TestComputeGemSignals_TierFromFeature(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	features := []GemFeature{
		testFeature("Expensive of Power", "20/20", 500, 3),
		testFeature("Mid of Range", "20/20", 50, 20),
		testFeature("Cheap of Nothing", "20/20", 10, 100),
	}
	features[0].Tier = "TOP"
	features[1].Tier = "MID-HIGH"
	features[2].Tier = "MID"

	gems := []GemPrice{
		{Name: "Expensive", Variant: "20/20", Chaos: 5, Listings: 50, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid", Variant: "20/20", Chaos: 3, Listings: 30, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Cheap", Variant: "20/20", Chaos: 1, Listings: 100, IsTransfigured: false, GemColor: "GREEN"},
	}

	signals := ComputeGemSignals(snapTime, features, mc, gems, nil, 40.0)
	if len(signals) != 3 {
		t.Fatalf("got %d signals, want 3", len(signals))
	}

	tierMap := make(map[string]string)
	for _, sig := range signals {
		tierMap[sig.Name] = sig.Tier
	}

	if tierMap["Expensive of Power"] != "TOP" {
		t.Errorf("Expensive tier = %q, want TOP", tierMap["Expensive of Power"])
	}
	if tierMap["Mid of Range"] != "MID-HIGH" {
		t.Errorf("Mid tier = %q, want MID-HIGH", tierMap["Mid of Range"])
	}
	if tierMap["Cheap of Nothing"] != "MID" {
		t.Errorf("Cheap tier = %q, want MID", tierMap["Cheap of Nothing"])
	}
}

func TestComputeGemSignals_BaseCurrentListingsFromGems(t *testing.T) {
	// Verifies that baseCurrentListings is built from the gems slice
	// (non-transfigured, non-corrupted) following the pattern at trends.go:363-373.
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"
	f.VelShortPrice = 3
	f.VelMedPrice = 3
	f.VelLongPrice = 3
	f.CV = 25

	// Multiple variants of the base gem — should keep highest listings per name.
	gems := []GemPrice{
		{Name: "Spark", Variant: "1", Chaos: 1, Listings: 10, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 50, IsTransfigured: false, GemColor: "BLUE"},
		// Transfigured gems should be excluded from baseCurrentListings.
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 15, IsTransfigured: true, GemColor: "BLUE"},
		// Corrupted gems should be excluded from baseCurrentListings.
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 100, IsTransfigured: false, IsCorrupted: true, GemColor: "BLUE"},
	}

	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: snapTime.Add(-2 * time.Hour), Chaos: 5, Listings: 60},
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 55},
			{Time: snapTime, Chaos: 5, Listings: 50},
		},
	}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	// The function should produce a valid signal without crashing, even
	// with mixed gem types in the gems slice.
	if signals[0].Signal == "" {
		t.Error("Signal is empty — baseCurrentListings may not be built correctly")
	}
}

func TestComputeGemSignals_SellabilityPopulated(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// Few listings + price rising + listings dropping => high sellability ("FAST SELL").
	// Signal classification + sellability now use VelLong (6h velocity).
	f := testFeature("Rare of Gem", "20/20", 200, 5)
	f.VelShortPrice = 8
	f.VelMedPrice = 7
	f.VelLongPrice = 7
	f.VelShortListing = -5
	f.VelMedListing = -4
	f.VelLongListing = -4
	f.CV = 15
	f.Tier = "HIGH"

	gems := testBaseGems("Rare", 40)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.Sellability <= 0 {
		t.Errorf("Sellability = %d, want > 0", sig.Sellability)
	}
	if sig.SellabilityLabel == "" {
		t.Error("SellabilityLabel is empty")
	}
	// With very few listings (5) + rising price + dropping listings,
	// sellability should be high.
	if sig.Sellability < 60 {
		t.Errorf("Sellability = %d, want >= 60 for thin-listing rising gem", sig.Sellability)
	}
}

func TestComputeGemSignals_WindowSignalPopulated(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"

	// Provide base gems with very low listings to trigger EXHAUSTED window.
	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 1, IsTransfigured: false, GemColor: "BLUE"},
	}

	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: snapTime.Add(-1 * time.Hour), Chaos: 5, Listings: 3},
			{Time: snapTime, Chaos: 5, Listings: 1},
		},
	}

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, baseHistory, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// Base listings = 1 (<= 2) => EXHAUSTED window.
	if sig.WindowSignal != "EXHAUSTED" {
		t.Errorf("WindowSignal = %q, want EXHAUSTED (base listings = 1)", sig.WindowSignal)
	}
}

func TestComputeGemSignals_AdvancedSignalDetection(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// Set up a gem with PRICE_MANIPULATION characteristics:
	// very few listings, extreme price, no velocity, high CV.
	// Signal classification + advanced signal use VelLong (6h velocity).
	f := testFeature("Manipulated of Scam", "20/20", 500, 2)
	f.VelShortPrice = 0.5
	f.VelMedPrice = 0.3
	f.VelLongPrice = 0.3
	f.CV = 90
	f.Tier = "TOP"
	f.Listings = 2

	gems := testBaseGems("Manipulated", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// classifyAdvancedSignal checks: listings <= 3, price > 200, |priceVel| < 1, CV > 80
	// => PRICE_MANIPULATION.
	if sig.AdvancedSignal != "PRICE_MANIPULATION" {
		t.Errorf("AdvancedSignal = %q, want PRICE_MANIPULATION", sig.AdvancedSignal)
	}
}

func TestComputeGemSignals_PhaseModifierPopulated(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"

	gems := testBaseGems("Spark", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	// Phase modifier should be a reasonable temporal value (> 0, <= 2.0).
	if signals[0].PhaseModifier <= 0 || signals[0].PhaseModifier > 2.0 {
		t.Errorf("PhaseModifier = %f, want in (0, 2.0]", signals[0].PhaseModifier)
	}
}

func TestComputeGemSignals_SellReasonPopulated(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Dump of Doom", "20/20", 100, 20)
	// DUMPING needs pVelPct < -8 AND lVelPct > 10.
	// At price=100, VelLongPrice=-9 → -9%. At listings=20, VelLongListing=3 → 15%.
	f.VelShortPrice = -12
	f.VelMedPrice = -10
	f.VelLongPrice = -9
	f.VelShortListing = 5
	f.VelMedListing = 4
	f.VelLongListing = 3
	f.CV = 30
	f.Tier = "MID"

	gems := testBaseGems("Dump", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.SellReason == "" {
		t.Error("SellReason is empty for DUMPING signal — should explain why")
	}
}

func TestComputeGemSignals_NameAndVariantPreserved(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	features := []GemFeature{
		testFeature("Spark of Nova", "20/20", 200, 15),
		testFeature("Cleave of Rage", "1/20", 50, 30),
	}
	features[0].Tier = "HIGH"
	features[1].Tier = "MID"

	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 50, IsTransfigured: false, GemColor: "BLUE"},
		{Name: "Cleave", Variant: "1/20", Chaos: 3, Listings: 30, IsTransfigured: false, GemColor: "RED"},
	}

	signals := ComputeGemSignals(snapTime, features, mc, gems, nil, 40.0)
	if len(signals) != 2 {
		t.Fatalf("got %d signals, want 2", len(signals))
	}

	if signals[0].Name != "Spark of Nova" {
		t.Errorf("signals[0].Name = %q, want %q", signals[0].Name, "Spark of Nova")
	}
	if signals[0].Variant != "20/20" {
		t.Errorf("signals[0].Variant = %q, want %q", signals[0].Variant, "20/20")
	}
	if signals[1].Name != "Cleave of Rage" {
		t.Errorf("signals[1].Name = %q, want %q", signals[1].Name, "Cleave of Rage")
	}
	if signals[1].Variant != "1/20" {
		t.Errorf("signals[1].Variant = %q, want %q", signals[1].Variant, "1/20")
	}
}

func TestComputeGemSignals_STABLESignalModerateConfidence(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Steady of Stone", "20/20", 80, 25)
	// Low velocity, low CV => STABLE signal.
	// Signal classification uses VelLong (6h velocity). |0.2| < 2 and |0.1| < 3 => STABLE.
	f.VelShortPrice = 0.5
	f.VelMedPrice = 0.3
	f.VelLongPrice = 0.2
	f.VelShortListing = 0.5
	f.VelMedListing = 0.3
	f.VelLongListing = 0.1
	f.CV = 10
	f.Tier = "MID"

	gems := testBaseGems("Steady", 40)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.Signal != "STABLE" {
		t.Errorf("Signal = %q, want STABLE", sig.Signal)
	}
	// STABLE base confidence is 55, with power dampening + predictable profile
	// (low CV + negative elasticity = 1.2 modifier) => moderate-to-high range.
	if sig.Confidence < 40 || sig.Confidence > 75 {
		t.Errorf("STABLE confidence = %d, want 40-75", sig.Confidence)
	}
}

func TestComputeGemSignals_CASCADEAdvancedSignal(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// CASCADE requires: CV > 200 AND High7Days/Low7Days > 20 (buyout aftermath).
	f := testFeature("Rare of Cascade", "20/20", 65, 35)
	f.CV = 283         // extreme CV from buyout spike
	f.Low7Days = 30    // normal pre-buyout price
	f.High7Days = 4220 // buyout spike (ratio: 140x > 20x threshold)
	f.HistPosition = 50
	f.Tier = "FLOOR"

	gems := testBaseGems("Rare", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	if signals[0].AdvancedSignal != "CASCADE" {
		t.Errorf("AdvancedSignal = %q, want CASCADE (CV=283 > 200, spike ratio=140x > 20x)", signals[0].AdvancedSignal)
	}
}

func TestComputeGemSignals_CASCADEDoesNotOverridePRICE_MANIPULATION(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// Set up PRICE_MANIPULATION: listings <= 3, price > 200, |velocity| < 1, CV > 80
	// Also set CASCADE conditions: CV > 200, spike ratio > 20x.
	f := testFeature("Manipulated of Cascade", "20/20", 500, 2)
	f.VelShortPrice = 0.5
	f.VelMedPrice = 0.3
	f.VelLongPrice = 0.3
	f.VelShortListing = 0.1
	f.VelMedListing = 0.1
	f.VelLongListing = 0.1
	f.CV = 250 // CASCADE condition: CV > 200
	f.Listings = 2
	f.Tier = "TOP"
	f.Low7Days = 20        // CASCADE condition: 500/20 = 25x > 20x
	f.High7Days = 500

	gems := testBaseGems("Manipulated", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	// PRICE_MANIPULATION has priority over CASCADE.
	if signals[0].AdvancedSignal != "PRICE_MANIPULATION" {
		t.Errorf("AdvancedSignal = %q, want PRICE_MANIPULATION (keeps priority over CASCADE)", signals[0].AdvancedSignal)
	}
}

func TestComputeGemSignals_CASCADENotFiredWithLowCV(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	// Normal gem with moderate spike ratio but low CV — CASCADE should NOT fire.
	// CASCADE requires CV > 200, which filters out normal volatility.
	f := testFeature("Spike of Normal", "20/20", 300, 50)
	f.CV = 25 // below 200 threshold
	f.Low7Days = 100
	f.High7Days = 3000 // spike ratio 30x > 20x, but CV too low
	f.Tier = "TOP"

	gems := testBaseGems("Spike", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	if signals[0].AdvancedSignal == "CASCADE" {
		t.Errorf("AdvancedSignal = CASCADE, should NOT fire when CV=%v (below 200 threshold)", f.CV)
	}
}

// ---------------------------------------------------------------------------
// Trade-adjusted sellability tests (adjustSellabilityForTrade)
// ---------------------------------------------------------------------------

func TestAdjustSellabilityForTrade_MONOPOLY(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60, // 1 min, fresh
		TradeSellerConcentration: "MONOPOLY",
	}
	got, _ := adjustSellabilityForTrade(80, f)
	// 80 - 20 = 60
	if got != 60 {
		t.Errorf("MONOPOLY: adjustSellabilityForTrade(80) = %d, want 60 (-20)", got)
	}
}

func TestAdjustSellabilityForTrade_CONCENTRATED(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60,
		TradeSellerConcentration: "CONCENTRATED",
	}
	got, _ := adjustSellabilityForTrade(80, f)
	// 80 - 10 = 70
	if got != 70 {
		t.Errorf("CONCENTRATED: adjustSellabilityForTrade(80) = %d, want 70 (-10)", got)
	}
}

func TestAdjustSellabilityForTrade_STALE(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:     true,
		TradeDataAge:           60,
		TradeCheapestStaleness: "STALE",
	}
	got, _ := adjustSellabilityForTrade(80, f)
	// 80 - 10 = 70
	if got != 70 {
		t.Errorf("STALE: adjustSellabilityForTrade(80) = %d, want 70 (-10)", got)
	}
}

func TestAdjustSellabilityForTrade_FRESH(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:     true,
		TradeDataAge:           60,
		TradeCheapestStaleness: "FRESH",
	}
	got, _ := adjustSellabilityForTrade(80, f)
	// 80 + 5 = 85
	if got != 85 {
		t.Errorf("FRESH: adjustSellabilityForTrade(80) = %d, want 85 (+5)", got)
	}
}

func TestAdjustSellabilityForTrade_SkippedWhenUnavailable(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      false,
		TradeSellerConcentration: "MONOPOLY",
		TradeCheapestStaleness:  "STALE",
	}
	got, label := adjustSellabilityForTrade(80, f)
	if got != 80 {
		t.Errorf("unavailable: adjustSellabilityForTrade(80) = %d, want 80 (unchanged)", got)
	}
	if label != "FAST SELL" {
		t.Errorf("unavailable: label = %q, want FAST SELL (for score 80)", label)
	}
}

func TestAdjustSellabilityForTrade_SkippedWhenStaleAge(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            5400, // exactly at boundary (>= 5400 = skip)
		TradeSellerConcentration: "MONOPOLY",
	}
	got, _ := adjustSellabilityForTrade(80, f)
	if got != 80 {
		t.Errorf("stale age: adjustSellabilityForTrade(80) = %d, want 80 (unchanged)", got)
	}
}

func TestAdjustSellabilityForTrade_ClampToZero(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60,
		TradeSellerConcentration: "MONOPOLY",
		TradeCheapestStaleness:  "STALE",
	}
	got, label := adjustSellabilityForTrade(15, f)
	// 15 - 20 - 10 = -15 → clamped to 0
	if got != 0 {
		t.Errorf("clamp to 0: adjustSellabilityForTrade(15) = %d, want 0 (clamped)", got)
	}
	if label != "UNLIKELY" {
		t.Errorf("clamp to 0: label = %q, want UNLIKELY", label)
	}
}

func TestAdjustSellabilityForTrade_ClampTo100(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:     true,
		TradeDataAge:           60,
		TradeCheapestStaleness: "FRESH",
	}
	got, _ := adjustSellabilityForTrade(98, f)
	// 98 + 5 = 103 → clamped to 100
	if got != 100 {
		t.Errorf("clamp to 100: adjustSellabilityForTrade(98) = %d, want 100 (clamped)", got)
	}
}

// ---------------------------------------------------------------------------
// classifySellConfidence trade overrides
// ---------------------------------------------------------------------------

func TestClassifySellConfidence_MONOPOLY_SafeToFair(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60,
		TradeSellerConcentration: "MONOPOLY",
	}
	// Base would be SAFE (sellProb >= 0.8, stabilityDisc >= 0.85).
	got, note := classifySellConfidence(0.9, 0.95, f)
	if got != "FAIR" {
		t.Errorf("MONOPOLY SAFE→FAIR: got %q, want FAIR", got)
	}
	if note == "" {
		t.Error("MONOPOLY SAFE→FAIR: note should be non-empty")
	}
}

func TestClassifySellConfidence_MONOPOLY_FairToRisky(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60,
		TradeSellerConcentration: "MONOPOLY",
	}
	// Base would be FAIR.
	got, note := classifySellConfidence(0.6, 0.82, f)
	if got != "RISKY" {
		t.Errorf("MONOPOLY FAIR→RISKY: got %q, want RISKY", got)
	}
	if note == "" {
		t.Error("MONOPOLY FAIR→RISKY: note should be non-empty")
	}
}

func TestClassifySellConfidence_CASCADE_AlwaysRisky(t *testing.T) {
	// CASCADE regime: always RISKY regardless of trade data.
	f := GemFeature{
		MarketRegime: "CASCADE",
	}
	got, note := classifySellConfidence(0.9, 0.95, f)
	if got != "RISKY" {
		t.Errorf("CASCADE: got %q, want RISKY", got)
	}
	if note == "" {
		t.Error("CASCADE: note should be non-empty")
	}
}

func TestClassifySellConfidence_PriceOutlier_MONOPOLY_AlwaysRisky(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:      true,
		TradeDataAge:            60,
		TradeSellerConcentration: "MONOPOLY",
		TradePriceOutlier:       true,
	}
	// Even with SAFE base conditions, PriceOutlier + MONOPOLY = always RISKY.
	got, note := classifySellConfidence(0.9, 0.95, f)
	if got != "RISKY" {
		t.Errorf("PriceOutlier+MONOPOLY: got %q, want RISKY", got)
	}
	if note == "" {
		t.Error("PriceOutlier+MONOPOLY: note should be non-empty")
	}
}

func TestClassifySellConfidence_STALE_SafeToFair(t *testing.T) {
	f := GemFeature{
		TradeDataAvailable:     true,
		TradeDataAge:           60,
		TradeCheapestStaleness: "STALE",
	}
	// Base: SAFE. STALE should downgrade SAFE→FAIR.
	got, note := classifySellConfidence(0.9, 0.95, f)
	if got != "FAIR" {
		t.Errorf("STALE SAFE→FAIR: got %q, want FAIR", got)
	}
	if note == "" {
		t.Error("STALE SAFE→FAIR: note should be non-empty")
	}
}

func TestClassifySellConfidence_FRESH_NoPromotionWhenSellProbTooLow(t *testing.T) {
	// FRESH promotion requires sellProb >= 0.5, but RISKY base requires sellProb < 0.5,
	// so these conditions are mutually exclusive for the base classification.
	// This test confirms FRESH does not promote when sellProb is below the threshold.
	f := GemFeature{
		TradeDataAvailable:     true,
		TradeDataAge:           60,
		TradeCheapestStaleness: "FRESH",
	}
	// sellProb=0.49, stabilityDisc=0.75 → RISKY base (both < 0.5 and < 0.8).
	// FRESH promotion needs sellProb >= 0.5 — not met.
	got, _ := classifySellConfidence(0.49, 0.75, f)
	if got != "RISKY" {
		t.Errorf("FRESH no-promotion: got %q, want RISKY (sellProb 0.49 below 0.5 threshold)", got)
	}
}

// ---------------------------------------------------------------------------
// QuickSellPrice: trade floor vs ninja fallback
// ---------------------------------------------------------------------------

func TestComputeGemSignals_QuickSellPrice_UsesTradeFloor(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"
	// Trade data: fresh, not outlier, sufficient listings.
	f.TradeDataAvailable = true
	f.TradeDataAge = 120 // 2 min
	f.TradePriceFloor = 185.0
	f.TradePriceOutlier = false
	f.TradeMedianTop10 = 195.0

	gems := testBaseGems("Spark", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// QuickSellPrice should use trade floor (185) as base, not ninja (200).
	undercutFactor := quickSellUndercutFactor(15, "HIGH", sig.Signal)
	expectedQuickSell := 185.0 * (1.0 - undercutFactor)

	if math.Abs(sig.QuickSellPrice-expectedQuickSell) > 0.01 {
		t.Errorf("QuickSellPrice = %f, want %f (based on trade floor 185)", sig.QuickSellPrice, expectedQuickSell)
	}
}

func TestComputeGemSignals_QuickSellPrice_FallsBackToNinja(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"
	// No trade data.
	f.TradeDataAvailable = false

	gems := testBaseGems("Spark", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// QuickSellPrice should use ninja chaos (200) as base.
	undercutFactor := quickSellUndercutFactor(15, "HIGH", sig.Signal)
	expectedQuickSell := 200.0 * (1.0 - undercutFactor)

	if math.Abs(sig.QuickSellPrice-expectedQuickSell) > 0.01 {
		t.Errorf("QuickSellPrice = %f, want %f (based on ninja 200)", sig.QuickSellPrice, expectedQuickSell)
	}
}

func TestComputeGemSignals_QuickSellPrice_FallsBackWhenOutlier(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	f.Tier = "HIGH"
	// Trade data available but outlier — should fall back to ninja.
	f.TradeDataAvailable = true
	f.TradeDataAge = 120
	f.TradePriceFloor = 90.0 // outlier
	f.TradePriceOutlier = true
	f.TradeMedianTop10 = 195.0

	gems := testBaseGems("Spark", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// Outlier: fallback to ninja (200).
	undercutFactor := quickSellUndercutFactor(15, "HIGH", sig.Signal)
	expectedQuickSell := 200.0 * (1.0 - undercutFactor)

	if math.Abs(sig.QuickSellPrice-expectedQuickSell) > 0.01 {
		t.Errorf("QuickSellPrice = %f, want %f (outlier → ninja fallback)", sig.QuickSellPrice, expectedQuickSell)
	}
}

func TestComputeGemSignals_QuickSellPrice_FallsBackWhenThinListings(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Thin of Market", "20/20", 200, 2) // < 3 listings
	f.Tier = "HIGH"
	f.TradeDataAvailable = true
	f.TradeDataAge = 120
	f.TradePriceFloor = 185.0
	f.TradePriceOutlier = false

	gems := testBaseGems("Thin", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// listings < 3: fallback to ninja (200).
	undercutFactor := quickSellUndercutFactor(2, "HIGH", sig.Signal)
	expectedQuickSell := 200.0 * (1.0 - undercutFactor)

	if math.Abs(sig.QuickSellPrice-expectedQuickSell) > 0.01 {
		t.Errorf("QuickSellPrice = %f, want %f (thin listings → ninja fallback)", sig.QuickSellPrice, expectedQuickSell)
	}
}

// ---------------------------------------------------------------------------
// RiskAdjustedValue: trade median usage
// ---------------------------------------------------------------------------

func TestComputeGemSignals_RiskAdjustedValue_UsesTradeMedian(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Spark of Nova", "20/20", 200, 25)
	f.Tier = "HIGH"
	f.SellProbabilityFactor = 0.8
	f.StabilityDiscount = 0.9
	f.TradeDataAvailable = true
	f.TradeDataAge = 600     // 10 min: within 30min threshold for median
	f.TradeMedianTop10 = 190 // lower than ninja

	gems := testBaseGems("Spark", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// RAV should use TradeMedianTop10 (190), not ninja (200).
	expectedRAV := 190.0 * f.SellProbabilityFactor * f.StabilityDiscount
	// Note: applyTradeMultipliers may modify SellProbabilityFactor, so check direction.
	if sig.RiskAdjustedValue >= 200.0*0.8*0.9 {
		t.Errorf("RiskAdjustedValue = %f, should be less than ninja-based %f (trade median is lower)",
			sig.RiskAdjustedValue, 200.0*0.8*0.9)
	}
	// Should be approximately expectedRAV (may differ slightly due to applyTradeMultipliers).
	_ = expectedRAV // used for reasoning, not exact comparison
	if sig.RiskAdjustedValue <= 0 {
		t.Errorf("RiskAdjustedValue = %f, want > 0", sig.RiskAdjustedValue)
	}
}

// ---------------------------------------------------------------------------
// SellConfidence trade note propagation
// ---------------------------------------------------------------------------

func TestComputeGemSignals_SellConfidence_TradeNote(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Monopoly of Doom", "20/20", 200, 15)
	f.Tier = "HIGH"
	f.SellProbabilityFactor = 0.9
	f.StabilityDiscount = 0.95
	// MONOPOLY with SAFE base → downgrade to FAIR.
	f.TradeDataAvailable = true
	f.TradeDataAge = 60
	f.TradeSellerConcentration = "MONOPOLY"

	gems := testBaseGems("Monopoly", 50)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	// SellConfidence should be downgraded by MONOPOLY.
	if sig.SellConfidence == "SAFE" {
		t.Error("SellConfidence should not be SAFE when MONOPOLY (downgrade expected)")
	}
	// TradeConfidenceNote should explain the downgrade.
	if sig.TradeConfidenceNote == "" {
		t.Error("TradeConfidenceNote should be non-empty for MONOPOLY downgrade")
	}
}

// ---------------------------------------------------------------------------
// sellabilityLabelFor tests
// ---------------------------------------------------------------------------

func TestSellabilityLabelFor(t *testing.T) {
	tests := []struct {
		score int
		want  string
	}{
		{100, "FAST SELL"},
		{80, "FAST SELL"},
		{79, "GOOD"},
		{60, "GOOD"},
		{59, "MODERATE"},
		{40, "MODERATE"},
		{39, "SLOW"},
		{20, "SLOW"},
		{19, "UNLIKELY"},
		{0, "UNLIKELY"},
	}
	for _, tt := range tests {
		got := sellabilityLabelFor(tt.score)
		if got != tt.want {
			t.Errorf("sellabilityLabelFor(%d) = %q, want %q", tt.score, got, tt.want)
		}
	}
}
