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

func TestComputeCleanTierBoundaries_REDLikePool(t *testing.T) {
	// RED 20/20 pool without TOPs: 25 gems ranging 267c to 29c
	gems := []GemPrice{
		{Name: "G1", Variant: "20/20", Chaos: 267, Listings: 30, IsTransfigured: true},
		{Name: "G2", Variant: "20/20", Chaos: 250, Listings: 50, IsTransfigured: true},
		{Name: "G3", Variant: "20/20", Chaos: 229, Listings: 70, IsTransfigured: true},
		{Name: "G4", Variant: "20/20", Chaos: 200, Listings: 40, IsTransfigured: true},
		{Name: "G5", Variant: "20/20", Chaos: 175, Listings: 30, IsTransfigured: true},
		{Name: "G6", Variant: "20/20", Chaos: 137, Listings: 60, IsTransfigured: true},
		{Name: "G7", Variant: "20/20", Chaos: 120, Listings: 80, IsTransfigured: true},
		{Name: "G8", Variant: "20/20", Chaos: 115, Listings: 100, IsTransfigured: true},
		{Name: "G9", Variant: "20/20", Chaos: 105, Listings: 50, IsTransfigured: true},
		{Name: "G10", Variant: "20/20", Chaos: 95, Listings: 40, IsTransfigured: true},
		{Name: "G11", Variant: "20/20", Chaos: 89, Listings: 60, IsTransfigured: true},
		{Name: "G12", Variant: "20/20", Chaos: 75, Listings: 80, IsTransfigured: true},
		{Name: "G13", Variant: "20/20", Chaos: 74, Listings: 70, IsTransfigured: true},
		{Name: "G14", Variant: "20/20", Chaos: 73, Listings: 60, IsTransfigured: true},
		{Name: "G15", Variant: "20/20", Chaos: 66, Listings: 90, IsTransfigured: true},
		{Name: "G16", Variant: "20/20", Chaos: 65, Listings: 100, IsTransfigured: true},
		{Name: "G17", Variant: "20/20", Chaos: 63, Listings: 80, IsTransfigured: true},
		{Name: "G18", Variant: "20/20", Chaos: 54, Listings: 70, IsTransfigured: true},
		{Name: "G19", Variant: "20/20", Chaos: 50, Listings: 60, IsTransfigured: true},
		{Name: "G20", Variant: "20/20", Chaos: 48, Listings: 50, IsTransfigured: true},
		{Name: "G21", Variant: "20/20", Chaos: 42, Listings: 40, IsTransfigured: true},
		{Name: "G22", Variant: "20/20", Chaos: 40, Listings: 30, IsTransfigured: true},
		{Name: "G23", Variant: "20/20", Chaos: 35, Listings: 90, IsTransfigured: true},
		{Name: "G24", Variant: "20/20", Chaos: 31, Listings: 100, IsTransfigured: true},
		{Name: "G25", Variant: "20/20", Chaos: 29, Listings: 80, IsTransfigured: true},
	}

	lowConf := map[string]bool{}
	tops := map[string]bool{}

	boundaries := computeCleanTierBoundaries(gems, lowConf, tops)

	vb, ok := boundaries["20/20"]
	if !ok {
		t.Fatal("expected boundaries for 20/20")
	}
	if len(vb.Boundaries) < 2 {
		t.Errorf("expected >= 2 boundaries, got %d", len(vb.Boundaries))
	}

	// Key assertion: FLOOR should be < 50% of pool.
	floorCount := 0
	for _, g := range gems {
		tier := classifyTier(g.Chaos, vb)
		if tier == "FLOOR" {
			floorCount++
		}
	}
	if floorCount > 12 {
		t.Errorf("too many FLOOR gems: %d/25 (want <= 12, was 66%% before refactor)", floorCount)
	}

	// No gem should get "TOP" from these boundaries.
	for _, g := range gems {
		tier := classifyTier(g.Chaos, vb)
		if tier == "TOP" {
			t.Errorf("gem %s at %fc classified as TOP — should not happen with DetectTierBoundariesNoTop", g.Name, g.Chaos)
		}
	}
}

func TestComputeCleanTierBoundaries_ExcludesLowConfAndTops(t *testing.T) {
	gems := []GemPrice{
		{Name: "Top Gem", Variant: "20/20", Chaos: 1300, Listings: 50, IsTransfigured: true},
		{Name: "Spike", Variant: "20/20", Chaos: 5000, Listings: 3, IsTransfigured: true},
		{Name: "Normal A", Variant: "20/20", Chaos: 300, Listings: 60, IsTransfigured: true},
		{Name: "Normal B", Variant: "20/20", Chaos: 200, Listings: 70, IsTransfigured: true},
		{Name: "Normal C", Variant: "20/20", Chaos: 100, Listings: 80, IsTransfigured: true},
		{Name: "Normal D", Variant: "20/20", Chaos: 50, Listings: 90, IsTransfigured: true},
	}

	lowConf := map[string]bool{"Spike|20/20": true}
	tops := map[string]bool{"Top Gem|20/20": true}

	boundaries := computeCleanTierBoundaries(gems, lowConf, tops)

	vb := boundaries["20/20"]
	// The pool should only have Normal A-D (4 gems).
	// Boundaries computed from [300, 200, 100, 50] only.
	if len(vb.Boundaries) == 0 {
		t.Error("expected at least some boundaries from 4-gem pool")
	}
}

func TestComputeGemClassification_Integration(t *testing.T) {
	gems := []GemPrice{
		// TOP: clear gap above the rest (1300, 1200 → gap ~800 → next at 400)
		{Name: "KB of Clustering", Variant: "20/20", Chaos: 1300, Listings: 260, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cyclone of Tumult", Variant: "20/20", Chaos: 1200, Listings: 135, IsTransfigured: true, GemColor: "GREEN"},
		// Low confidence: thin market spike
		{Name: "Lightning Strike", Variant: "20/20", Chaos: 3204, Listings: 3, IsTransfigured: true, GemColor: "GREEN"},
		// Normal gems — tightly spaced to keep avg gap low
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 400, Listings: 74, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "AG of Smiting", Variant: "20/20", Chaos: 380, Listings: 68, IsTransfigured: true, GemColor: "RED"},
		{Name: "Lacerate", Variant: "20/20", Chaos: 350, Listings: 80, IsTransfigured: true, GemColor: "GREEN"},
		{Name: "Ground Slam", Variant: "20/20", Chaos: 300, Listings: 90, IsTransfigured: true, GemColor: "RED"},
		{Name: "Frostbolt", Variant: "20/20", Chaos: 250, Listings: 85, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Fireball", Variant: "20/20", Chaos: 200, Listings: 70, IsTransfigured: true, GemColor: "RED"},
		{Name: "Ice Shot", Variant: "20/20", Chaos: 150, Listings: 60, IsTransfigured: true, GemColor: "GREEN"},
		{Name: "Cleave", Variant: "20/20", Chaos: 100, Listings: 90, IsTransfigured: true, GemColor: "RED"},
		{Name: "Some Floor", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: true, GemColor: "RED"},
		{Name: "Cheap Gem", Variant: "20/20", Chaos: 30, Listings: 120, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Another", Variant: "20/20", Chaos: 60, Listings: 80, IsTransfigured: true, GemColor: "GREEN"},
	}

	cls := ComputeGemClassification(gems)

	// Lightning Strike should be low confidence (3 listings vs median ~82).
	ls := cls.Gems[GemClassificationKey{"Lightning Strike", "20/20"}]
	if !ls.LowConfidence {
		t.Errorf("Lightning Strike: LowConfidence = %v, want true", ls.LowConfidence)
	}

	// KB and Cyclone should be TOP (gap of ~800 vs avg gap ~100).
	kb := cls.Gems[GemClassificationKey{"KB of Clustering", "20/20"}]
	if kb.Tier != "TOP" {
		t.Errorf("KB tier = %s, want TOP", kb.Tier)
	}
	cyclone := cls.Gems[GemClassificationKey{"Cyclone of Tumult", "20/20"}]
	if cyclone.Tier != "TOP" {
		t.Errorf("Cyclone tier = %s, want TOP", cyclone.Tier)
	}
	if kb.LowConfidence {
		t.Error("KB should not be low confidence")
	}

	// Normal gems should NOT be TOP.
	spark := cls.Gems[GemClassificationKey{"Spark of Nova", "20/20"}]
	if spark.Tier == "TOP" {
		t.Error("Spark should not be TOP")
	}
	if spark.Tier == "" {
		t.Error("Spark should have a tier assigned")
	}

	// Cheap Gem (30c) should be in a low tier — not TOP or HIGH.
	cheap := cls.Gems[GemClassificationKey{"Cheap Gem", "20/20"}]
	if cheap.Tier == "TOP" || cheap.Tier == "HIGH" {
		t.Errorf("Cheap Gem (30c) tier = %s, want a low tier (MID/LOW/FLOOR)", cheap.Tier)
	}
	if cheap.Tier == "" {
		t.Error("Cheap Gem should have a tier assigned")
	}

	// Boundaries should exist for 20/20.
	if _, ok := cls.Boundaries["20/20"]; !ok {
		t.Error("expected Boundaries for 20/20 variant")
	}

	// All analyzable gems should have a classification.
	for _, g := range gems {
		if !isAnalyzableGem(g) || g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		if _, ok := cls.Gems[key]; !ok {
			t.Errorf("gem %s missing from classification", g.Name)
		}
	}
}
