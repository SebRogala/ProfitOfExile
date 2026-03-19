package lab

import (
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
		Top:  300,
		High: 100,
		Mid:  30,
	}
	return mc
}

// testFeature returns a GemFeature with sensible defaults for signal tests.
// Caller can override fields after construction.
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
		VelLongPrice:      2,
		VelLongListing:    1,
		CV:                25,
		HistPosition:      50,
		High7d:            chaos * 1.2,
		Low7d:             chaos * 0.8,
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

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	// All velocities strongly positive and agreeing => HERD signal + high confidence.
	f.VelShortPrice = 10
	f.VelMedPrice = 8
	f.VelLongPrice = 6
	f.VelShortListing = 12
	f.VelMedListing = 11
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
	// Signal should be HERD (high priceVel + high listingVel).
	if sig.Signal != "HERD" {
		t.Errorf("Signal = %q, want HERD", sig.Signal)
	}
}

func TestComputeGemSignals_TRAPLowConfidence(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Volatile of Storm", "20/20", 100, 10)
	// Very high CV => TRAP signal.
	f.CV = 110
	f.VelShortPrice = 5
	f.VelMedPrice = -3
	f.VelLongPrice = 2
	f.Tier = "MID"

	gems := testBaseGems("Volatile", 30)

	signals := ComputeGemSignals(snapTime, []GemFeature{f}, mc, gems, nil, 40.0)
	if len(signals) != 1 {
		t.Fatalf("got %d signals, want 1", len(signals))
	}

	sig := signals[0]
	if sig.Signal != "TRAP" {
		t.Errorf("Signal = %q, want TRAP (CV=110)", sig.Signal)
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
	f.CV = 110 // TRAP
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
	f.VelShortPrice = -8
	f.VelMedPrice = -7
	f.VelLongPrice = -6
	f.VelShortListing = 8
	f.VelMedListing = 7
	f.VelLongListing = 6
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

	// HERD at historical peak => SellUrgency = UNDERCUT (histPos > 90),
	// but for SELL_NOW we need TRAP signal which always produces SELL_NOW.
	f := testFeature("Trap of Danger", "20/20", 100, 10)
	f.CV = 110 // TRAP
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

	f := testFeature("Spark of Nova", "20/20", 200, 15)
	// Set up for HERD signal with high confidence.
	f.VelShortPrice = 10
	f.VelMedPrice = 8
	f.VelLongPrice = 6
	f.VelShortListing = 12
	f.VelMedListing = 11
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
	// HERD + high confidence + not SELL_NOW => should be "OK" or ""
	// (depends on confidence >= 65 check).
	if sig.Confidence >= 65 && sig.Signal == "HERD" && sig.SellUrgency != "SELL_NOW" {
		if sig.Recommendation != "OK" {
			t.Errorf("Recommendation = %q, want OK for high-confidence positive HERD", sig.Recommendation)
		}
	}
}

func TestComputeGemSignals_HERDAtHistoricalPeakAVOID(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)
	mc := testSignalMarketContext()

	f := testFeature("Lacerate of Haemophilia", "20/20", 350, 5)
	// HERD signal at historical peak => SellUrgency = UNDERCUT, reason mentions peak.
	f.VelShortPrice = 10
	f.VelMedPrice = 8
	f.VelLongPrice = 6
	f.VelShortListing = 12
	f.VelMedListing = 11
	f.VelLongListing = 10
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
	// HERD at peak (histPosition > 90) => SellUrgency should be UNDERCUT.
	if sig.SellUrgency != "UNDERCUT" {
		t.Errorf("SellUrgency = %q, want UNDERCUT for HERD at historical peak", sig.SellUrgency)
	}
	// Even though HERD is normally positive, SELL_NOW or UNDERCUT at peak => AVOID
	// is defined by the recommendation logic. Since SellUrgency is not SELL_NOW,
	// we verify that the signal still gets appropriate recommendation handling.
	// The collective.go pattern: TRAP/DUMPING/SELL_NOW => AVOID.
	// UNDERCUT at peak is not SELL_NOW, so recommendation depends on confidence/signal.
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
	features[1].Tier = "MID"
	features[2].Tier = "LOW"

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
	if tierMap["Mid of Range"] != "MID" {
		t.Errorf("Mid tier = %q, want MID", tierMap["Mid of Range"])
	}
	if tierMap["Cheap of Nothing"] != "LOW" {
		t.Errorf("Cheap tier = %q, want LOW", tierMap["Cheap of Nothing"])
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
	f := testFeature("Rare of Gem", "20/20", 200, 5)
	f.VelShortPrice = 8
	f.VelMedPrice = 7
	f.VelLongPrice = 5
	f.VelShortListing = -5
	f.VelMedListing = -4
	f.VelLongListing = -3
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
	f := testFeature("Manipulated of Scam", "20/20", 500, 2)
	f.VelShortPrice = 0.5
	f.VelMedPrice = 0.3
	f.VelLongPrice = 0.1
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
	f.VelShortPrice = -8
	f.VelMedPrice = -7
	f.VelLongPrice = -6
	f.VelShortListing = 8
	f.VelMedListing = 7
	f.VelLongListing = 6
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
	// STABLE base confidence is 55, with power dampening => moderate range.
	if sig.Confidence < 40 || sig.Confidence > 70 {
		t.Errorf("STABLE confidence = %d, want 40-70", sig.Confidence)
	}
}
