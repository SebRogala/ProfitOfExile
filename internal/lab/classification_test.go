package lab

import (
	"fmt"
	"math"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/price/tier"
)

// solidSample is a poe.ninja sample above Lab's thin threshold, so a fixture
// graded on it is gated only by its listings.
func solidSample() *int {
	n := labThinSampleCount + 1
	return &n
}

func tierClampGems() []GemPrice {
	var gems []GemPrice
	for i := 0; i < 9; i++ {
		gems = append(gems, GemPrice{Name: fmt.Sprintf("Clamp %d of Test", i), Variant: "20/20", Chaos: 100 * math.Pow(0.7, float64(i)), Listings: 20, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"})
	}
	return append(gems, GemPrice{Name: "Entrant of Test", Variant: "20/20", Chaos: 100, Listings: 20, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"})
}

// keep:critical — silent: Lab omits previous HIGH and disables the stored step bound.
func TestKeepCritical_FontClassificationClampsHighAgainstThePreviousSnapshot(t *testing.T) {
	gems := tierClampGems()
	previous := map[string]tier.Boundaries{"20/20": {Names: []string{tier.High}, Boundaries: []float64{38.8234}}}
	inputs := deriveTierInputs(gems, nil, nil, time.Time{}, false, previous, tierInputAsOf)
	got := ComputeGemClassification(inputs)
	b := got.Boundaries["20/20"]
	if len(b.Boundaries) == 0 || len(b.Names) == 0 || b.Names[0] != tier.High {
		t.Fatalf("boundaries = %+v, want named HIGH", b)
	}
	if math.Abs(b.Boundaries[0]-48.52925) > 0.0001 {
		t.Errorf("HIGH = %.6f, want 48.52925 (previous 38.8234 plus 25%%)", b.Boundaries[0])
	}
}

// keep:critical — silent: a rejected spike is admitted into tier-based EV.
func TestKeepCritical_FontClassificationMarksAnOverBandRejectedGemChaoticWhateverItsMembership(t *testing.T) {
	for _, prev := range []string{tier.Excluded, tier.Floor} {
		t.Run(prev, func(t *testing.T) {
			gems := tierClampGems()
			g := GemPrice{Name: "Rejected of Test", Variant: "20/20", Chaos: 6, Listings: 1, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"}
			gems = append(gems, g)
			point := evidencePoint{Name: g.Name, Variant: g.Variant, Time: tierInputAsOf.Add(-30 * time.Minute), Chaos: 2.5, Listings: 9, SampleCount: solidSample()}
			held := deriveEvidenceHolds(gems, []evidencePoint{point}, tierInputAsOf)
			if !held[len(held)-1].holdRejected {
				t.Fatal("fixture must reject current 6c against last SOLID 2.5c")
			}
			inputs := deriveTierInputs(held, nil, GemClassificationMap{{Name: g.Name, Variant: g.Variant}: {Tier: prev}}, tierInputAsOf.Add(-30*time.Minute), true, nil, tierInputAsOf)
			got, ok := ComputeGemClassification(inputs).Gems[GemClassificationKey{Name: g.Name, Variant: g.Variant}]
			if !ok || got.Tier != tier.Chaotic || !got.LowConfidence {
				t.Errorf("rejected member = %+v, present=%v; want CHAOTIC/LowConfidence=true for Prev %s", got, ok, prev)
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
		{Name: "KB of Clustering", Variant: "20/20", Chaos: 1300, Listings: 260, IsTransfigured: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Cyclone of Tumult", Variant: "20/20", Chaos: 1200, Listings: 135, IsTransfigured: true, GemColor: "GREEN", SampleCount: solidSample()},
		// Low confidence: thin market spike (4500c with 3 listings — CASCADE-like)
		{Name: "Lightning Strike", Variant: "20/20", Chaos: 4500, Listings: 3, IsTransfigured: true, GemColor: "GREEN", SampleCount: solidSample()},
		// Normal gems — tightly spaced to keep avg gap low
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 400, Listings: 74, IsTransfigured: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "AG of Smiting", Variant: "20/20", Chaos: 380, Listings: 68, IsTransfigured: true, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Lacerate", Variant: "20/20", Chaos: 350, Listings: 80, IsTransfigured: true, GemColor: "GREEN", SampleCount: solidSample()},
		{Name: "Ground Slam", Variant: "20/20", Chaos: 300, Listings: 90, IsTransfigured: true, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Frostbolt", Variant: "20/20", Chaos: 250, Listings: 85, IsTransfigured: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Fireball", Variant: "20/20", Chaos: 200, Listings: 70, IsTransfigured: true, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Ice Shot", Variant: "20/20", Chaos: 150, Listings: 60, IsTransfigured: true, GemColor: "GREEN", SampleCount: solidSample()},
		{Name: "Cleave", Variant: "20/20", Chaos: 100, Listings: 90, IsTransfigured: true, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Some Floor", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: true, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Cheap Gem", Variant: "20/20", Chaos: 30, Listings: 120, IsTransfigured: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Another", Variant: "20/20", Chaos: 60, Listings: 80, IsTransfigured: true, GemColor: "GREEN", SampleCount: solidSample()},
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
		{Name: "Expensive Gem", Variant: "21/23c", Chaos: 1500, Listings: 40, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "High Gem", Variant: "21/23c", Chaos: 800, Listings: 50, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid Gem 1", Variant: "21/23c", Chaos: 300, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid Gem 2", Variant: "21/23c", Chaos: 250, Listings: 55, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid Gem 3", Variant: "21/23c", Chaos: 200, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Low Gem 1", Variant: "21/23c", Chaos: 100, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Low Gem 2", Variant: "21/23c", Chaos: 50, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Floor Gem 1", Variant: "21/23c", Chaos: 20, Listings: 100, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Floor Gem 2", Variant: "21/23c", Chaos: 10, Listings: 120, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
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
		{Name: "Top 1", Variant: "21/23c", Chaos: 3000, Listings: 40, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Top 2", Variant: "21/23c", Chaos: 2500, Listings: 35, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "High 1", Variant: "21/23c", Chaos: 800, Listings: 60, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "High 2", Variant: "21/23c", Chaos: 700, Listings: 55, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid 1", Variant: "21/23c", Chaos: 300, Listings: 70, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid 2", Variant: "21/23c", Chaos: 250, Listings: 65, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Mid 3", Variant: "21/23c", Chaos: 200, Listings: 80, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Low 1", Variant: "21/23c", Chaos: 100, Listings: 90, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Low 2", Variant: "21/23c", Chaos: 80, Listings: 85, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Floor 1", Variant: "21/23c", Chaos: 20, Listings: 100, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Floor 2", Variant: "21/23c", Chaos: 15, Listings: 110, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
		{Name: "Floor 3", Variant: "21/23c", Chaos: 10, Listings: 120, IsCorrupted: true, IsTransfigured: false, GemColor: "RED", SampleCount: solidSample()},
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
		{Name: "Arc", Variant: "21/23c", Chaos: 5000, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Spark", Variant: "21/23c", Chaos: 4000, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Ball Lightning", Variant: "21/23c", Chaos: 3000, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Firestorm", Variant: "21/23c", Chaos: 2000, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Arc", Variant: "21/20c", Chaos: 200, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Spark", Variant: "21/20c", Chaos: 120, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Ball Lightning", Variant: "21/20c", Chaos: 60, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Firestorm", Variant: "21/20c", Chaos: 30, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
		{Name: "Freezing Pulse", Variant: "21/20c", Chaos: 12, Listings: 30, IsCorrupted: true, GemColor: "BLUE", SampleCount: solidSample()},
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

// covens2020Gems is shaped like the Covens BLUE 20/20 market: 11 transfigured
// gems, 10 with 1 listing and 1 with 2, poe.ninja samples of 1-2.
func covens2020Gems() []GemPrice {
	prices := []float64{527, 264, 170, 88, 64, 45, 33, 24, 17, 12, 8}
	gems := make([]GemPrice, len(prices))
	for i, price := range prices {
		listings, sample := 1, 1+i%2
		if i == 2 {
			listings = 2
		}
		gems[i] = GemPrice{Name: fmt.Sprintf("Covens Gem %d of Test", i), Variant: "20/20", Chaos: price,
			Listings: listings, SampleCount: &sample, IsTransfigured: true, GemColor: "BLUE"}
	}
	return gems
}

// keep:happy — AC 2: every row of a Covens-shaped 20/20 market is flagged.
func TestKeepHappy_ComputeGemClassificationFlagsEveryCovens2020Row(t *testing.T) {
	gems := covens2020Gems()

	cls := ComputeGemClassification(gems)

	for _, g := range gems {
		got, ok := cls.Gems[GemClassificationKey{g.Name, g.Variant}]
		if !ok {
			t.Errorf("%s (%.0fc) missing from the classification", g.Name, g.Chaos)
			continue
		}
		if !got.LowConfidence {
			t.Errorf("%s (%.0fc, %d listings) LowConfidence = false, want true", g.Name, g.Chaos, g.Listings)
		}
	}
	if b, ok := cls.Boundaries["20/20"]; ok {
		t.Errorf("Boundaries[20/20] = %+v, want no 20/20 boundary", b)
	}
}

// keep:happy — AC 3: in a deep market the relative 0.4 × median rule flags the
// same gems as today. Green before the gate by design (characterisation).
func TestKeepHappy_ComputeGemClassificationDeepMarketFlagsSameGemsAsToday(t *testing.T) {
	sample := 50
	depths := []int{30, 35, 38, 40, 40, 40, 42, 45, 50, 60, 10} // median 40
	var gems []GemPrice
	var todayItems []tier.Item
	for i, depth := range depths {
		g := GemPrice{Name: fmt.Sprintf("Deep Gem %d of Test", i), Variant: "20/20", Chaos: float64(400 - i*30),
			Listings: depth, SampleCount: &sample, IsTransfigured: true, GemColor: "BLUE"}
		gems = append(gems, g)
		todayItems = append(todayItems, tier.Item{Group: g.Variant, Key: g.Name, Price: g.Chaos, Depth: g.Listings})
	}
	thin := gems[len(gems)-1].Name

	want := tier.LowConfidence(todayItems, labPolicy)
	if !reflect.DeepEqual(want, map[tier.Key]bool{{Group: "20/20", Key: thin}: true}) {
		t.Fatalf("fixture: today's relative rule flags %v, want only %s", want, thin)
	}

	got := make(map[tier.Key]bool)
	for key, c := range ComputeGemClassification(gems).Gems {
		if c.LowConfidence {
			got[tier.Key{Group: key.Variant, Key: key.Name}] = true
		}
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("flagged = %v, want %v (today's relative-rule set)", got, want)
	}
}

// keep:critical — silent: one variant's deep sample would certify another
// market's ask.
func TestKeepCritical_TierItemsGradesEachVariantOnItsOwnSample(t *testing.T) {
	deep, thin := 40, 2
	gems := []GemPrice{
		{Name: "Ice Crash of Cadence", Variant: "1/20", Chaos: 100, Listings: 30, SampleCount: &deep, IsTransfigured: true, GemColor: "RED"},
		{Name: "Ice Crash of Cadence", Variant: "20/20", Chaos: 100, Listings: 30, SampleCount: &thin, IsTransfigured: true, GemColor: "RED"},
	}

	gated := make(map[string]bool)
	for _, it := range tierItems(gems, isFontOutcome) {
		gated[it.Group] = it.Gated
	}

	want := map[string]bool{"1/20": false, "20/20": true}
	if !reflect.DeepEqual(gated, want) {
		t.Errorf("Gated by variant = %v, want %v (each variant graded on its own sample)", gated, want)
	}
}

// keep:critical — silent: a held thin key disappears from the confident EV pool.
func TestKeepCritical_HeldShallowKeyContributesCappedPriceToBoundaries(t *testing.T) {
	gems := heldSnapshot()
	prices := []float64{300, 280, 150, 120, 90, 70, 50, 40, 30}
	for i := range gems {
		if i < len(prices) {
			gems[i].Chaos = prices[i]
		}
		gems[i].Listings = 40
	}
	thin := &gems[len(gems)-1]
	thin.Chaos, thin.Listings = 120, 1
	held := deriveEvidenceHolds(gems, []evidencePoint{heldPoint(holdAsOf.Add(-time.Hour), 100, 9, 9)}, holdAsOf)
	got := ComputeGemClassification(held)
	key := GemClassificationKey{heldName, "20/20"}
	if c := got.Gems[key]; c.LowConfidence || c.Tier == "" {
		t.Errorf("held key = %+v, want confident classification", c)
	}
	valued := append([]GemPrice(nil), gems...)
	valued[len(valued)-1].Chaos = 100
	valued[len(valued)-1].Listings = 40
	valued[len(valued)-1].SampleCount = solidSample()
	want := ComputeGemClassification(valued)
	without := ComputeGemClassification(gems[:len(gems)-1])
	if reflect.DeepEqual(want.Boundaries, without.Boundaries) {
		t.Fatal("fixture capped key does not change boundaries")
	}
	if !reflect.DeepEqual(got.Boundaries, want.Boundaries) || got.Gems[key] != want.Gems[key] {
		t.Errorf("held boundaries/class = %+v/%+v, want capped confident reference %+v/%+v", got.Boundaries, got.Gems[key], want.Boundaries, want.Gems[key])
	}
	control := append([]GemPrice(nil), gems...)
	control[len(control)-1].SampleCount = solidSample()
	if c := ComputeGemClassification(control).Gems[key]; !c.LowConfidence {
		t.Errorf("unheld same-depth key = %+v, want relative-depth flag", c)
	}
}

// keep:critical — silent: an inflated held price reaches TOP and ranking.
func TestKeepCritical_HeldAnimateWeaponUsesLastSolidPriceForTop(t *testing.T) {
	gems := heldSnapshot()
	for i := range gems {
		gems[i].Chaos = float64(140 - i*10)
	}
	g := &gems[len(gems)-1]
	g.Name, g.Chaos, g.Listings = "Animate Weapon of Ranged Arms", 291.5, 4
	point := heldPoint(holdAsOf.Add(-time.Hour), 146.3, 9, 9)
	point.Name = g.Name
	held := deriveEvidenceHolds(gems, []evidencePoint{point}, holdAsOf)
	if price := held[len(held)-1].Chaos; price != 146.3 {
		t.Errorf("Animate Weapon valuation = %g, want 146.3", price)
	}
	reference := append([]GemPrice(nil), gems...)
	reference[len(reference)-1].Chaos = 146.3
	reference[len(reference)-1].SampleCount = solidSample()
	want := ComputeGemClassification(reference)
	got := ComputeGemClassification(held)
	key := GemClassificationKey{g.Name, g.Variant}
	if got.Gems[key] != want.Gems[key] || !reflect.DeepEqual(got.Boundaries, want.Boundaries) {
		t.Errorf("Animate Weapon tier/boundaries = %+v/%+v, want capped reference %+v/%+v", got.Gems[key], got.Boundaries, want.Gems[key], want.Boundaries)
	}
	top := tier.Classify(tierItems(held, isFontOutcome), labPolicy).TopBoundary["20/20"]
	if top > 146.3 {
		t.Errorf("TOP boundary = %g, exceeds last SOLID 146.3", top)
	}
}
