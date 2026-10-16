package lab

import "testing"

func TestGemPriceThinSample(t *testing.T) {
	zero, two, three, five, six := 0, 2, 3, 5, 6
	empty, populated := true, false
	tests := []struct {
		name string
		gem  GemPrice
		want bool
	}{
		{"count_two_with_populated_sparkline_is_thin", GemPrice{Name: "Ice Crash of Cadence", Variant: "20/20", SampleCount: &two, SparklineEmpty: &populated}, true},
		{"zero_count_is_thin", GemPrice{SampleCount: &zero}, true},
		{"count_five_is_thin", GemPrice{SampleCount: &five}, true},
		{"count_six_is_not_thin", GemPrice{SampleCount: &six}, false},
		{"count_three_with_empty_sparkline_is_thin", GemPrice{SampleCount: &three, SparklineEmpty: &empty}, true},
		{"unknown_count_is_not_thin", GemPrice{}, false},
		{"unknown_count_with_empty_sparkline_is_not_thin", GemPrice{SparklineEmpty: &empty}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.gem.ThinSample(); got != tt.want {
				t.Errorf("ThinSample() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestGemPriceThinSample_UsesEachVariantsSample(t *testing.T) {
	thinCount, deepCount := 2, 40
	tests := []struct {
		gem  GemPrice
		want bool
	}{
		{GemPrice{Name: "Ice Crash of Cadence", Variant: "20/20", SampleCount: &thinCount}, true},
		{GemPrice{Name: "Ice Crash of Cadence", Variant: "1", SampleCount: &deepCount}, false},
	}
	for _, tt := range tests {
		t.Run(tt.gem.Variant, func(t *testing.T) {
			if got := tt.gem.ThinSample(); got != tt.want {
				t.Errorf("%s variant %s: ThinSample() = %v, want %v", tt.gem.Name, tt.gem.Variant, got, tt.want)
			}
		})
	}
}

func TestComputeGemClassification_LeavesOutCorruptedUntransfiguredAndLabCutoffGems(t *testing.T) {
	gems := []GemPrice{
		{Name: "Normal", Variant: "20/20", Chaos: 100, Listings: 50, IsTransfigured: true},
		{Name: "Corrupted", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: true, IsCorrupted: true},
		{Name: "Not Trans", Variant: "20/20", Chaos: 100, Listings: 3, IsTransfigured: false},
		{Name: "At Cutoff", Variant: "20/20", Chaos: 5, Listings: 3, IsTransfigured: true},
	}

	cls := ComputeGemClassification(gems)

	for _, name := range []string{"Corrupted", "Not Trans", "At Cutoff"} {
		if got, ok := cls.Gems[GemClassificationKey{name, "20/20"}]; ok {
			t.Errorf("%s classified as %+v, want it left out of the Font pool", name, got)
		}
	}
	if _, ok := cls.Gems[GemClassificationKey{"Normal", "20/20"}]; !ok {
		t.Error("Normal (uncorrupted transfigured, 100c) missing from the Font pool")
	}
}

func TestComputeGemClassification_Integration(t *testing.T) {
	gems := []GemPrice{
		// TOP: clear gap above the rest (1300, 1200 → gap ~800 → next at 400)
		{Name: "KB of Clustering", Variant: "20/20", Chaos: 1300, Listings: 260, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cyclone of Tumult", Variant: "20/20", Chaos: 1200, Listings: 135, IsTransfigured: true, GemColor: "GREEN"},
		// Low confidence: thin market spike (4500c with 3 listings — CASCADE-like)
		{Name: "Lightning Strike", Variant: "20/20", Chaos: 4500, Listings: 3, IsTransfigured: true, GemColor: "GREEN"},
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

	// Lightning Strike should be low confidence (3 listings vs median ~82)
	// and CHAOTIC tier (price 3204c would be TOP, but low conf caps at CHAOTIC).
	ls := cls.Gems[GemClassificationKey{"Lightning Strike", "20/20"}]
	if !ls.LowConfidence {
		t.Errorf("Lightning Strike: LowConfidence = %v, want true", ls.LowConfidence)
	}
	if ls.Tier != "CHAOTIC" {
		t.Errorf("Lightning Strike tier = %s, want CHAOTIC (low-conf gem with TOP-level price)", ls.Tier)
	}
	// CHAOTIC gems must NOT qualify as font EV winners in any mode.
	if isSafeTierWinner(ls.Tier) {
		t.Error("CHAOTIC tier should not be a Safe winner")
	}
	if isPremiumTierWinner(ls.Tier) {
		t.Error("CHAOTIC tier should not be a Premium winner")
	}
	if isJackpotTierWinner(ls.Tier) {
		t.Error("CHAOTIC tier should not be a Jackpot winner")
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
		if !isFontOutcome(g) || g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		if _, ok := cls.Gems[key]; !ok {
			t.Errorf("gem %s missing from classification", g.Name)
		}
	}
}

// --- Dedication Classification Tests ---

func TestComputeDedicationClassification_SkillPool(t *testing.T) {
	// Build a pool of corrupted non-transfigured 21/23c skill gems.
	gems := []GemPrice{
		{Name: "Expensive Gem", Variant: "21/23c", Chaos: 1500, Listings: 40, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "High Gem", Variant: "21/23c", Chaos: 800, Listings: 50, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid Gem 1", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid Gem 2", Variant: "21/23c", Chaos: 250, Listings: 55, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid Gem 3", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Low Gem 1", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Low Gem 2", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Floor Gem 1", Variant: "21/23c", Chaos: 20, Listings: 100, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Floor Gem 2", Variant: "21/23c", Chaos: 10, Listings: 120, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/23c")

	// All gems that pass isDedicationOutcome + are not transfigured + chaos > 5 should be classified.
	for _, g := range gems {
		if g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		if _, ok := cls.Gems[key]; !ok {
			t.Errorf("gem %s missing from Dedication classification", g.Name)
		}
	}

	// Boundaries should exist for "21/23c" variant.
	if _, ok := cls.Boundaries["21/23c"]; !ok {
		t.Error("expected Boundaries for 21/23c variant")
	}
}

func TestComputeDedicationClassification_TransfiguredPool(t *testing.T) {
	gems := []GemPrice{
		{Name: "Trans Expensive", Variant: "21/23c", Chaos: 1200, Listings: 35, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Trans High", Variant: "21/23c", Chaos: 600, Listings: 45, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Trans Mid", Variant: "21/23c", Chaos: 200, Listings: 55, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Trans Low 1", Variant: "21/23c", Chaos: 80, Listings: 70, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Trans Low 2", Variant: "21/23c", Chaos: 50, Listings: 85, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Trans Floor", Variant: "21/23c", Chaos: 10, Listings: 100, IsCorrupted: true, IsTransfigured: true, GemColor: "BLUE"},
	}

	cls := ComputeDedicationClassification(gems, true, "21/23c")

	for _, g := range gems {
		if g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		gc, ok := cls.Gems[key]
		if !ok {
			t.Errorf("gem %s missing from Dedication transfigured classification", g.Name)
			continue
		}
		// Every classified gem should have a non-empty tier.
		if gc.Tier == "" {
			t.Errorf("gem %s has empty tier", g.Name)
		}
	}
}

func TestComputeDedicationClassification_ExcludesNonCorrupted(t *testing.T) {
	gems := []GemPrice{
		// Non-corrupted gem should be excluded by isDedicationOutcome.
		{Name: "Non Corrupted", Variant: "21/23c", Chaos: 500, Listings: 50, IsCorrupted: false, IsTransfigured: false, GemColor: "RED"},
		// Valid dedication gem.
		{Name: "Valid Gem 1", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Valid Gem 2", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Valid Gem 3", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Valid Gem 4", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/23c")

	// Non-corrupted gem should NOT be in classification.
	key := GemClassificationKey{"Non Corrupted", "21/23c"}
	if _, ok := cls.Gems[key]; ok {
		t.Error("non-corrupted gem should not be in Dedication classification")
	}
}

func TestComputeDedicationClassification_ExcludesSupports(t *testing.T) {
	gems := []GemPrice{
		{Name: "Lifetap Support", Variant: "21/23c", Chaos: 500, Listings: 50, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Empower Support", Variant: "21/23c", Chaos: 800, Listings: 40, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Arc", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Fireball", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Cleave", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Slam", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/23c")

	// Support gems should NOT be in classification.
	for _, name := range []string{"Lifetap Support", "Empower Support"} {
		key := GemClassificationKey{name, "21/23c"}
		if _, ok := cls.Gems[key]; ok {
			t.Errorf("support gem %q should not be in Dedication classification", name)
		}
	}

	// Skill gems should be in classification.
	for _, name := range []string{"Arc", "Fireball", "Cleave", "Slam"} {
		key := GemClassificationKey{name, "21/23c"}
		if _, ok := cls.Gems[key]; !ok {
			t.Errorf("skill gem %q should be in Dedication classification", name)
		}
	}
}

func TestComputeDedicationClassification_ExcludesTrarthus(t *testing.T) {
	gems := []GemPrice{
		{Name: "Trarthus Ire", Variant: "21/23c", Chaos: 5000, Listings: 30, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Arc", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Fireball", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Cleave", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Slam", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/23c")

	key := GemClassificationKey{"Trarthus Ire", "21/23c"}
	if _, ok := cls.Gems[key]; ok {
		t.Error("Trarthus should not be in Dedication classification")
	}
}

func TestComputeDedicationClassification_PoolIsolation(t *testing.T) {
	// Both skill and transfigured gems present. Classification for skills
	// should NOT include transfigured gems and vice versa.
	gems := []GemPrice{
		// Skills
		{Name: "Arc", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Fireball", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Cleave", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Slam", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		// Transfigured
		{Name: "Arc of Surging", Variant: "21/23c", Chaos: 800, Listings: 35, IsCorrupted: true, IsTransfigured: true, GemColor: "RED"},
		{Name: "Fireball of Volatility", Variant: "21/23c", Chaos: 600, Listings: 45, IsCorrupted: true, IsTransfigured: true, GemColor: "RED"},
		{Name: "Cleave of Rage", Variant: "21/23c", Chaos: 400, Listings: 55, IsCorrupted: true, IsTransfigured: true, GemColor: "RED"},
		{Name: "Slam of Magnitude", Variant: "21/23c", Chaos: 250, Listings: 65, IsCorrupted: true, IsTransfigured: true, GemColor: "RED"},
	}

	skillCls := ComputeDedicationClassification(gems, false, "21/23c")
	transCls := ComputeDedicationClassification(gems, true, "21/23c")

	// Skills classification should contain only non-transfigured gems.
	for _, name := range []string{"Arc of Surging", "Fireball of Volatility", "Cleave of Rage", "Slam of Magnitude"} {
		key := GemClassificationKey{name, "21/23c"}
		if _, ok := skillCls.Gems[key]; ok {
			t.Errorf("transfigured gem %q should not be in skills classification", name)
		}
	}

	// Transfigured classification should contain only transfigured gems.
	for _, name := range []string{"Arc", "Fireball", "Cleave"} {
		key := GemClassificationKey{name, "21/23c"}
		if _, ok := transCls.Gems[key]; ok {
			t.Errorf("non-transfigured gem %q should not be in transfigured classification", name)
		}
	}
}

func TestComputeDedicationClassification_TierDistribution(t *testing.T) {
	// Create a pool with clear tier separation.
	gems := []GemPrice{
		{Name: "Top 1", Variant: "21/23c", Chaos: 3000, Listings: 40, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Top 2", Variant: "21/23c", Chaos: 2500, Listings: 35, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "High 1", Variant: "21/23c", Chaos: 800, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "High 2", Variant: "21/23c", Chaos: 700, Listings: 55, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid 1", Variant: "21/23c", Chaos: 300, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid 2", Variant: "21/23c", Chaos: 250, Listings: 65, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Mid 3", Variant: "21/23c", Chaos: 200, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Low 1", Variant: "21/23c", Chaos: 100, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Low 2", Variant: "21/23c", Chaos: 80, Listings: 85, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Floor 1", Variant: "21/23c", Chaos: 20, Listings: 100, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Floor 2", Variant: "21/23c", Chaos: 15, Listings: 110, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
		{Name: "Floor 3", Variant: "21/23c", Chaos: 10, Listings: 120, IsCorrupted: true, IsTransfigured: false, GemColor: "RED"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/23c")

	// Verify valid tiers are assigned.
	validTiers := map[string]bool{
		"TOP": true, "HIGH": true, "MID-HIGH": true,
		"MID": true, "LOW": true, "FLOOR": true, "CHAOTIC": true,
	}
	tierCounts := make(map[string]int)
	for _, g := range gems {
		if g.Chaos <= 5 {
			continue
		}
		key := GemClassificationKey{g.Name, g.Variant}
		gc, ok := cls.Gems[key]
		if !ok {
			continue
		}
		if !validTiers[gc.Tier] {
			t.Errorf("gem %s has invalid tier %q", g.Name, gc.Tier)
		}
		tierCounts[gc.Tier]++
	}

	// With 12 gems and clear price gaps (3000/2500 then 800/700 then 300/250/200 then 100/80 then 20/15/10),
	// we should have at least 3 distinct tiers.
	if len(tierCounts) < 3 {
		t.Errorf("expected at least 3 distinct tiers, got %d: %v", len(tierCounts), tierCounts)
	}
}

// Tiers are relative to the pool they are computed over. 21/20c is the cheaper,
// deeper market, so classifying it against 21/23c prices would push its whole
// pool into the bottom tiers.
//
// Every 21/20c gem is priced above the classifier's 5c floor and the pool is
// large enough for real boundaries: a degenerate pool tiers everything TOP by
// fallback, which would pass whether or not the variants were kept apart.
func TestComputeDedicationClassification_ClassifiesEachVariantAgainstItsOwnPool(t *testing.T) {
	gems := []GemPrice{
		{Name: "Arc", Variant: "21/23c", Chaos: 5000, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Spark", Variant: "21/23c", Chaos: 4000, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Ball Lightning", Variant: "21/23c", Chaos: 3000, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Firestorm", Variant: "21/23c", Chaos: 2000, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Arc", Variant: "21/20c", Chaos: 200, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Spark", Variant: "21/20c", Chaos: 120, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Ball Lightning", Variant: "21/20c", Chaos: 60, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Firestorm", Variant: "21/20c", Chaos: 30, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Freezing Pulse", Variant: "21/20c", Chaos: 12, Listings: 30, IsCorrupted: true, GemColor: "BLUE"},
	}

	cls := ComputeDedicationClassification(gems, false, "21/20c")

	if _, ok := cls.Gems[GemClassificationKey{"Arc", "21/23c"}]; ok {
		t.Error("21/23c gems were classified into the 21/20c pool")
	}
	arc, ok := cls.Gems[GemClassificationKey{"Arc", "21/20c"}]
	if !ok {
		t.Fatal("no classification for Arc at 21/20c")
	}
	cheapest, ok := cls.Gems[GemClassificationKey{"Freezing Pulse", "21/20c"}]
	if !ok {
		t.Fatal("no classification for Freezing Pulse at 21/20c")
	}

	// The load-bearing assertion: the 21/20c pool is spread across real tiers on
	// its own price range. Against the 21/23c prices every one of these gems is
	// bottom-tier, so a merged pool cannot produce a spread — and a degenerate
	// pool tiers everything identically via the fallback.
	rank := map[string]int{"TOP": 0, "HIGH": 1, "MID-HIGH": 2, "MID": 3, "LOW": 4, "FLOOR": 5}
	arcRank, cheapRank := rank[arc.Tier], rank[cheapest.Tier]
	if arcRank >= cheapRank {
		t.Errorf("tiers did not separate within the 21/20c pool: Arc (200c) = %q, Freezing Pulse (12c) = %q",
			arc.Tier, cheapest.Tier)
	}
	if arc.LowConfidence {
		t.Error("Arc 21/20c came back low-confidence — it is the top of a fully priced pool")
	}
}
