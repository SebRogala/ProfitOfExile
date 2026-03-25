package lab

import "testing"

func TestDetectLowConfidence_BasicThreshold(t *testing.T) {
	gems := []GemPrice{
		{Name: "Normal Gem", Variant: "20/20", Chaos: 500, Listings: 60, IsTransfigured: true},
		{Name: "Thin Gem", Variant: "20/20", Chaos: 3000, Listings: 3, IsTransfigured: true},
		{Name: "Another Normal", Variant: "20/20", Chaos: 200, Listings: 50, IsTransfigured: true},
		{Name: "Moderate Gem", Variant: "20/20", Chaos: 100, Listings: 40, IsTransfigured: true},
	}

	lc := detectLowConfidence(gems)

	if lc["Normal Gem|20/20"] {
		t.Error("Normal Gem should not be low confidence")
	}
	if !lc["Thin Gem|20/20"] {
		t.Error("Thin Gem (3 listings vs median ~45) should be low confidence")
	}
}

func TestDetectLowConfidence_PerVariant(t *testing.T) {
	gems := []GemPrice{
		// Variant "20/20": median ~50 listings
		{Name: "Gem A", Variant: "20/20", Chaos: 100, Listings: 50, IsTransfigured: true},
		{Name: "Gem B", Variant: "20/20", Chaos: 100, Listings: 60, IsTransfigured: true},
		{Name: "Gem C", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true}, // 10/50 = 0.2 < 0.4
		// Variant "1": median ~10 listings (different market)
		{Name: "Gem D", Variant: "1", Chaos: 10, Listings: 10, IsTransfigured: true},
		{Name: "Gem E", Variant: "1", Chaos: 10, Listings: 12, IsTransfigured: true},
		{Name: "Gem F", Variant: "1", Chaos: 10, Listings: 3, IsTransfigured: true}, // 3/10 = 0.3 < 0.4
	}

	lc := detectLowConfidence(gems)

	if !lc["Gem C|20/20"] {
		t.Error("Gem C (10 listings, variant median 50) should be low confidence")
	}
	if !lc["Gem F|1"] {
		t.Error("Gem F (3 listings, variant median 10) should be low confidence")
	}
	if lc["Gem A|20/20"] {
		t.Error("Gem A should not be low confidence")
	}
	if lc["Gem D|1"] {
		t.Error("Gem D (at median) should not be low confidence")
	}
}

func TestDetectLowConfidence_SkipsNonAnalyzable(t *testing.T) {
	gems := []GemPrice{
		{Name: "Normal", Variant: "20/20", Chaos: 100, Listings: 50, IsTransfigured: true},
		{Name: "Corrupted", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: true, IsCorrupted: true},
		{Name: "Not Trans", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: false},
		{Name: "Too Cheap", Variant: "20/20", Chaos: 3, Listings: 3, IsTransfigured: true},
	}

	lc := detectLowConfidence(gems)

	// Non-analyzable gems should not appear in results at all.
	if _, exists := lc["Corrupted|20/20"]; exists {
		t.Error("Corrupted gem should not be in results")
	}
	if _, exists := lc["Not Trans|20/20"]; exists {
		t.Error("Non-transfigured gem should not be in results")
	}
}

func TestDetectTops_GapDetection(t *testing.T) {
	gems := []GemPrice{
		// Clear gap: 1300, 1200 → gap 800 → 400 (well above 3x avg gap)
		{Name: "Mega Gem", Variant: "20/20", Chaos: 1300, Listings: 50, IsTransfigured: true},
		{Name: "Big Gem", Variant: "20/20", Chaos: 1200, Listings: 40, IsTransfigured: true},
		{Name: "High Gem", Variant: "20/20", Chaos: 400, Listings: 30, IsTransfigured: true},
		{Name: "Mid Gem", Variant: "20/20", Chaos: 350, Listings: 60, IsTransfigured: true},
		{Name: "Low Gem", Variant: "20/20", Chaos: 300, Listings: 80, IsTransfigured: true},
		{Name: "Floor A", Variant: "20/20", Chaos: 100, Listings: 100, IsTransfigured: true},
		{Name: "Floor B", Variant: "20/20", Chaos: 80, Listings: 100, IsTransfigured: true},
		{Name: "Floor C", Variant: "20/20", Chaos: 50, Listings: 120, IsTransfigured: true},
	}
	lowConf := map[string]bool{} // none low confidence

	tops := detectTops(gems, lowConf)

	if !tops["Mega Gem|20/20"] {
		t.Error("Mega Gem should be TOP")
	}
	if !tops["Big Gem|20/20"] {
		t.Error("Big Gem should be TOP")
	}
	if tops["High Gem|20/20"] {
		t.Error("High Gem should NOT be TOP")
	}
	if tops["Mid Gem|20/20"] {
		t.Error("Mid Gem should NOT be TOP")
	}
}

func TestDetectTops_ExcludesLowConfidence(t *testing.T) {
	gems := []GemPrice{
		{Name: "Spike Gem", Variant: "20/20", Chaos: 5000, Listings: 3, IsTransfigured: true},
		{Name: "Normal A", Variant: "20/20", Chaos: 500, Listings: 50, IsTransfigured: true},
		{Name: "Normal B", Variant: "20/20", Chaos: 400, Listings: 60, IsTransfigured: true},
		{Name: "Normal C", Variant: "20/20", Chaos: 300, Listings: 70, IsTransfigured: true},
		{Name: "Normal D", Variant: "20/20", Chaos: 200, Listings: 80, IsTransfigured: true},
		{Name: "Normal E", Variant: "20/20", Chaos: 100, Listings: 90, IsTransfigured: true},
	}
	lowConf := map[string]bool{
		"Spike Gem|20/20": true, // flagged as low confidence
	}

	tops := detectTops(gems, lowConf)

	// Spike Gem is excluded from pool, so it shouldn't be TOP.
	if tops["Spike Gem|20/20"] {
		t.Error("Low-confidence Spike Gem should not be TOP")
	}
}

func TestDetectTops_PerVariant(t *testing.T) {
	gems := []GemPrice{
		// Variant 20/20: clear TOP at 1000 (gap of 800 vs next at 200)
		{Name: "Top 2020", Variant: "20/20", Chaos: 1000, Listings: 50, IsTransfigured: true},
		{Name: "Mid 2020", Variant: "20/20", Chaos: 200, Listings: 60, IsTransfigured: true},
		{Name: "Low 2020 A", Variant: "20/20", Chaos: 150, Listings: 70, IsTransfigured: true},
		{Name: "Low 2020 B", Variant: "20/20", Chaos: 100, Listings: 70, IsTransfigured: true},
		{Name: "Low 2020 C", Variant: "20/20", Chaos: 50, Listings: 70, IsTransfigured: true},
		// Variant 1: evenly spaced, no significant gap
		{Name: "Gem 1A", Variant: "1", Chaos: 200, Listings: 100, IsTransfigured: true},
		{Name: "Gem 1B", Variant: "1", Chaos: 180, Listings: 100, IsTransfigured: true},
		{Name: "Gem 1C", Variant: "1", Chaos: 150, Listings: 100, IsTransfigured: true},
		{Name: "Gem 1D", Variant: "1", Chaos: 120, Listings: 100, IsTransfigured: true},
		{Name: "Gem 1E", Variant: "1", Chaos: 100, Listings: 100, IsTransfigured: true},
	}

	tops := detectTops(gems, map[string]bool{})

	if !tops["Top 2020|20/20"] {
		t.Error("Top 2020 should be TOP")
	}
	// Variant 1 has no significant gap — no TOPs expected
	if tops["Gem 1A|1"] {
		t.Error("Gem 1A in variant 1 should not be TOP (no significant gap)")
	}
}
