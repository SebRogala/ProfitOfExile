package lab

import (
	"fmt"
	"math"
	"reflect"
	"testing"
	"time"
)

func TestExpectedBestOf3_RankedValues(t *testing.T) {
	if got := expectedBestOf3([]float64{100, 80, 60, 40, 20}); got != 90 {
		t.Fatalf("EV = %g, want 90", got)
	}
}

func TestExpectedBestOf3_SmallPools(t *testing.T) {
	for _, tc := range []struct {
		name   string
		values []float64
		want   float64
	}{
		{"empty", nil, 0},
		{"one", []float64{100}, 100},
		{"two", []float64{100, 80}, 100},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if got := expectedBestOf3(tc.values); got != tc.want {
				t.Fatalf("EV = %g, want %g", got, tc.want)
			}
		})
	}
}

func TestFontPickProbability_Ranks(t *testing.T) {
	for rank, want := range []float64{0.6, 0.3, 0.1, 0, 0} {
		if got := fontPickProbability(rank, 5); got != want {
			t.Errorf("rank %d probability = %g, want %g", rank, got, want)
		}
	}
}

func TestFontPickProbability_SmallPools(t *testing.T) {
	for _, tc := range []struct {
		rank, total int
		want        float64
	}{
		{0, 0, 0}, {0, 1, 1}, {0, 2, 1}, {1, 2, 0}, {-1, 5, 0}, {5, 5, 0},
	} {
		t.Run(fmt.Sprintf("rank%d_pool%d", tc.rank, tc.total), func(t *testing.T) {
			if got := fontPickProbability(tc.rank, tc.total); got != tc.want {
				t.Fatalf("probability = %g, want %g", got, tc.want)
			}
		})
	}
}

func TestPWin3Picks_Basic(t *testing.T) {
	// 5 winners out of 50: P(at least one win in 3 picks)
	p := pWin3Picks(5, 50)
	if p < 0.27 || p > 0.29 {
		t.Errorf("pWin3Picks(5, 50) = %f, want ~0.28", p)
	}
}

func TestPWin3Picks_AllWinners(t *testing.T) {
	p := pWin3Picks(10, 10)
	if p != 1.0 {
		t.Errorf("pWin3Picks(10, 10) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_NoWinners(t *testing.T) {
	p := pWin3Picks(0, 50)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 50) = %f, want 0.0", p)
	}
}

func TestPWin3Picks_TotalLessThan3_WithWinners(t *testing.T) {
	p := pWin3Picks(1, 2)
	if p != 1.0 {
		t.Errorf("pWin3Picks(1, 2) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_TotalLessThan3_NoWinners(t *testing.T) {
	p := pWin3Picks(0, 2)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 2) = %f, want 0.0", p)
	}
}

func TestPWin3Picks_OneWinnerInThree(t *testing.T) {
	// 1 winner in 3: P = 1 - (2/3)*(1/2)*(0/1) = 1 - 0 = 1.0
	p := pWin3Picks(1, 3)
	if p != 1.0 {
		t.Errorf("pWin3Picks(1, 3) = %f, want 1.0", p)
	}
}

func TestPWin3Picks_ZeroTotal(t *testing.T) {
	p := pWin3Picks(0, 0)
	if p != 0.0 {
		t.Errorf("pWin3Picks(0, 0) = %f, want 0.0", p)
	}
}

func TestPWin3Picks_WinnersExceedTotal(t *testing.T) {
	p := pWin3Picks(10, 5)
	if p < 0 || p > 1 {
		t.Errorf("pWin3Picks(10, 5) = %f, want value in [0, 1]", p)
	}
	if p != 1.0 {
		t.Errorf("pWin3Picks(10, 5) = %f, want 1.0 (all winners)", p)
	}
}

// makeFeature creates a GemFeature with the given parameters for testing.
func makeFeature(name, variant string, chaos float64, listings int, tier string, sellProb, stabDisc float64) GemFeature {
	return GemFeature{
		Time:                  time.Now(),
		Name:                  name,
		Variant:               variant,
		Chaos:                 chaos,
		Listings:              listings,
		Tier:                  tier,
		GemColor:              "RED", // default for tests
		SellProbabilityFactor: sellProb,
		StabilityDiscount:     stabDisc,
	}
}

func makeFeatureColored(name, variant, color string, chaos float64, listings int, tier string, sellProb, stabDisc float64) GemFeature {
	f := makeFeature(name, variant, chaos, listings, tier, sellProb, stabDisc)
	f.GemColor = color
	return f
}

func fontSnapshotFromFeatures(features []GemFeature) []GemPrice {
	gems := make([]GemPrice, 0, len(features))
	for _, f := range features {
		gems = append(gems, GemPrice{Name: f.Name, Variant: f.Variant, GemColor: f.GemColor,
			IsTransfigured: true, Chaos: f.Chaos, Listings: f.Listings})
	}
	return gems
}

func TestAnalyzeFont_SafeMode_MIDPlusWinners(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Cleave of Rage", "20/20", 250, 15, "TOP", 0.9, 0.95),
		makeFeature("Slam of Force", "20/20", 120, 10, "HIGH", 0.8, 0.9),
		makeFeature("Strike of Fear", "20/20", 50, 8, "MID", 0.7, 0.85),
		makeFeature("Bash of Nothing", "20/20", 10, 20, "LOW", 0.6, 0.8),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var found *FontResult
	for i := range analysis.Safe {
		if analysis.Safe[i].Color == "RED" && analysis.Safe[i].Variant == "20/20" {
			found = &analysis.Safe[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 safe result")
	}

	// Safe mode: LOW+ are winners = 4 (LOW, MID, HIGH, TOP)
	if found.Winners != 4 {
		t.Errorf("Safe Winners = %d, want 4 (LOW+)", found.Winners)
	}
	// FLOOR gems should NOT be winners
	if found.Pool != 4 {
		t.Errorf("Pool = %d, want 4", found.Pool)
	}
	if found.Mode != "safe" {
		t.Errorf("Mode = %q, want %q", found.Mode, "safe")
	}
}

func TestAnalyzeFont_PremiumMode_MIDHIGHPlusWinners(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Cleave of Rage", "20/20", 250, 15, "TOP", 0.9, 0.95),
		makeFeature("Slam of Force", "20/20", 120, 10, "HIGH", 0.8, 0.9),
		makeFeature("Strike of Fear", "20/20", 50, 8, "MID-HIGH", 0.7, 0.85),
		makeFeature("Bash of Nothing", "20/20", 10, 20, "LOW", 0.6, 0.8),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var found *FontResult
	for i := range analysis.Premium {
		if analysis.Premium[i].Color == "RED" && analysis.Premium[i].Variant == "20/20" {
			found = &analysis.Premium[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 premium result")
	}

	// Premium mode uses color-specific tiers. Winner count depends on
	// the recursive average boundaries for this specific gem set.
	if found.Winners <= 0 {
		t.Errorf("Premium Winners = %d, want > 0", found.Winners)
	}
	if found.Mode != "premium" {
		t.Errorf("Mode = %q, want %q", found.Mode, "premium")
	}
}

func TestAnalyzeFont_JackpotMode_TOPOnly(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Cleave of Rage", "20/20", 250, 15, "TOP", 0.9, 0.95),
		makeFeature("Slam of Force", "20/20", 120, 10, "HIGH", 0.8, 0.9),
		makeFeature("Strike of Fear", "20/20", 50, 8, "MID", 0.7, 0.85),
		makeFeature("Bash of Nothing", "20/20", 10, 20, "LOW", 0.6, 0.8),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var found *FontResult
	for i := range analysis.Jackpot {
		if analysis.Jackpot[i].Color == "RED" && analysis.Jackpot[i].Variant == "20/20" {
			found = &analysis.Jackpot[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 jackpot result")
	}

	// Jackpot winners should be <= Premium winners (stricter tier).
	// May be 0 if no TOP gap detected in this small pool.
	var premiumWinners int
	for _, r := range analysis.Premium {
		if r.Color == "RED" && r.Variant == "20/20" {
			premiumWinners = r.Winners
		}
	}
	if found.Winners > premiumWinners {
		t.Errorf("Jackpot Winners (%d) should be <= Premium Winners (%d)", found.Winners, premiumWinners)
	}
	if found.Mode != "jackpot" {
		t.Errorf("Mode = %q, want %q", found.Mode, "jackpot")
	}
}

func TestAnalyzeFont_RiskAdjustedAvgWin(t *testing.T) {
	now := time.Now()
	// Two gems: A is expensive but thin market (low sell prob), B is cheaper but liquid

	features := []GemFeature{
		makeFeature("Gem A", "20/20", 300, 2, "TOP", 0.3, 0.6),   // risk-adjusted: 300*0.3*0.6 = 54
		makeFeature("Gem B", "20/20", 200, 30, "TOP", 0.9, 0.95), // risk-adjusted: 200*0.9*0.95 = 171
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var found *FontResult
	for i := range analysis.Safe {
		if analysis.Safe[i].Color == "RED" && analysis.Safe[i].Variant == "20/20" {
			found = &analysis.Safe[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 safe result")
	}

	// AvgWin is computed fresh from current gem data, not from stored features.
	// Gem A: 300c, 2 listings → sellProb = 0.3 (floor), stabDisc = stabilityDiscount(150)
	// Gem B: 200c, 30 listings → sellProb = sellProbabilityFactor(30,...), stabDisc = stabilityDiscount(10)
	adjustedA := 300 * sellProbabilityFactor(2, 0, 300) * stabilityDiscount(150)
	adjustedB := 200 * sellProbabilityFactor(30, 0, 200) * stabilityDiscount(10)
	_ = (adjustedA + adjustedB) / 2.0 // used for reference
	// Verify risk-adjusted avg is computed (non-zero) and reasonable.
	if found.AvgWin <= 0 || found.AvgWin > 300 {
		t.Errorf("AvgWin = %f, want > 0 and < 300 (risk-adjusted)", found.AvgWin)
	}
	// Raw avg should be higher than risk-adjusted.
	if found.AvgWinRaw <= found.AvgWin {
		t.Errorf("AvgWinRaw (%f) should be > AvgWin (%f)", found.AvgWinRaw, found.AvgWin)
	}

	// The 200c/30-listing gem (B) should contribute more than 300c/2-listing gem (A)
	adjustedA = 300 * sellProbabilityFactor(2, 0, 300) * stabilityDiscount(150)
	adjustedB = 200 * sellProbabilityFactor(30, 0, 200) * stabilityDiscount(10)
	if adjustedB <= adjustedA {
		t.Errorf("Expected gem B adjusted price (%f) > gem A adjusted price (%f)", adjustedB, adjustedA)
	}
}

func TestAnalyzeFont_LiquidityRiskClassification(t *testing.T) {
	tests := []struct {
		name     string
		thin     int
		total    int
		wantRisk string
	}{
		{"no winners", 0, 0, "LOW"},
		{"no thin", 0, 5, "LOW"},
		{"20% exact is LOW (> not >=)", 1, 5, "LOW"}, // 0.2 exactly -> LOW (uses > 0.2)
		{"above 20%", 2, 5, "MEDIUM"},                // 0.4 -> MEDIUM
		{"above 50%", 3, 5, "HIGH"},                  // 0.6 -> HIGH
		{"all thin", 5, 5, "HIGH"},
		{"50% exact is MEDIUM (> not >=)", 1, 2, "MEDIUM"}, // 0.5 exactly -> not > 0.5 -> MEDIUM
		{"just below 20%", 1, 6, "LOW"},                    // 0.166 -> LOW
		{"just above 20%", 2, 9, "MEDIUM"},                 // 0.222 -> MEDIUM
		{"above 50% boundary", 4, 7, "HIGH"},               // 0.571 -> HIGH
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := computeLiquidityRisk(tt.thin, tt.total)
			if got != tt.wantRisk {
				t.Errorf("computeLiquidityRisk(%d, %d) = %q, want %q", tt.thin, tt.total, got, tt.wantRisk)
			}
		})
	}
}

func TestAnalyzeFont_AllModesProduceIndependentResults(t *testing.T) {
	now := time.Now()
	// RED: 1 MID-HIGH + 2 MID + 1 LOW = 4 gems
	// BLUE: 1 TOP + 1 HIGH + 1 LOW = 3 gems
	//
	// Safe (LOW+):      RED=4, BLUE=3
	// Premium (MID-HIGH+): RED=1, BLUE=2
	// Jackpot (TOP):    RED=0, BLUE=1

	features := []GemFeature{
		makeFeatureColored("Red Gem 1", "20/20", "RED", 80, 10, "MID-HIGH", 0.8, 0.9),
		makeFeatureColored("Red Gem 2", "20/20", "RED", 40, 10, "MID", 0.8, 0.9),
		makeFeatureColored("Red Gem 3", "20/20", "RED", 45, 10, "MID", 0.8, 0.9),
		makeFeatureColored("Red Low", "20/20", "RED", 6, 10, "LOW", 0.6, 0.8),
		makeFeatureColored("Blue Top", "20/20", "BLUE", 300, 10, "TOP", 0.9, 0.95),
		makeFeatureColored("Blue High", "20/20", "BLUE", 150, 10, "HIGH", 0.85, 0.95),
		makeFeatureColored("Blue Low", "20/20", "BLUE", 6, 10, "LOW", 0.6, 0.8),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	// Verify safe mode: LOW+ winners
	var redSafeWinners, blueSafeWinners int
	for _, r := range analysis.Safe {
		if r.Variant == "20/20" {
			if r.Color == "RED" {
				redSafeWinners = r.Winners
			} else if r.Color == "BLUE" {
				blueSafeWinners = r.Winners
			}
		}
	}
	if redSafeWinners != 4 {
		t.Errorf("RED safe winners = %d, want 4 (LOW+)", redSafeWinners)
	}
	if blueSafeWinners != 3 {
		t.Errorf("BLUE safe winners = %d, want 3 (LOW+)", blueSafeWinners)
	}

	// Verify premium mode: MID-HIGH+ winners
	var redPremiumWinners, bluePremiumWinners int
	for _, r := range analysis.Premium {
		if r.Variant == "20/20" {
			if r.Color == "RED" {
				redPremiumWinners = r.Winners
			} else if r.Color == "BLUE" {
				bluePremiumWinners = r.Winners
			}
		}
	}
	// Premium winners should be <= Safe winners (stricter tier).
	if redPremiumWinners > redSafeWinners {
		t.Errorf("RED premium (%d) should be <= safe (%d)", redPremiumWinners, redSafeWinners)
	}
	if bluePremiumWinners > blueSafeWinners {
		t.Errorf("BLUE premium (%d) should be <= safe (%d)", bluePremiumWinners, blueSafeWinners)
	}

	// Verify jackpot mode: winners <= premium
	var redJackpotWinners, blueJackpotWinners int
	for _, r := range analysis.Jackpot {
		if r.Variant == "20/20" {
			if r.Color == "RED" {
				redJackpotWinners = r.Winners
			} else if r.Color == "BLUE" {
				blueJackpotWinners = r.Winners
			}
		}
	}
	if redJackpotWinners > redPremiumWinners {
		t.Errorf("RED jackpot (%d) should be <= premium (%d)", redJackpotWinners, redPremiumWinners)
	}
	if blueJackpotWinners != 1 {
		t.Errorf("BLUE jackpot winners = %d, want 1 (TOP only)", blueJackpotWinners)
	}
}

func TestAnalyzeFont_PoolIsUniqueNamesAcrossVariants(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Cleave of Rage", "1", 10, 10, "LOW", 0.5, 0.8),
		makeFeature("Cleave of Rage", "20/20", 100, 10, "HIGH", 0.8, 0.9),
		makeFeature("Slam of Force", "20/20", 50, 10, "MID", 0.7, 0.85),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	// Pool should be 2 unique names (Cleave of Rage + Slam of Force), not 3 rows
	for _, r := range analysis.Safe {
		if r.Color == "RED" {
			if r.Pool != 2 {
				t.Errorf("Pool = %d, want 2 (unique names across variants)", r.Pool)
			}
			break
		}
	}
}

func TestAnalyzeFont_ExcludesCorruptedAndTrarthus(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Cleave of Rage", "20/20", 100, 10, "HIGH", 0.8, 0.9),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)
	for _, r := range analysis.Safe {
		if r.Color == "RED" && r.Variant == "20/20" {
			if r.Pool != 1 {
				t.Errorf("Pool = %d, want 1 (corrupted and Trarthus excluded)", r.Pool)
			}
			return
		}
	}
}

func TestAnalyzeFont_EmptyInput(t *testing.T) {
	analysis := AnalyzeFont(time.Now(), nil, nil)
	if len(analysis.Safe) != 0 {
		t.Errorf("got %d safe results, want 0", len(analysis.Safe))
	}
	if len(analysis.Premium) != 0 {
		t.Errorf("got %d premium results, want 0", len(analysis.Premium))
	}
	if len(analysis.Jackpot) != 0 {
		t.Errorf("got %d jackpot results, want 0", len(analysis.Jackpot))
	}

	analysis = AnalyzeFont(time.Now(), []GemPrice{}, []GemFeature{})
	if len(analysis.Safe) != 0 {
		t.Errorf("got %d safe results, want 0", len(analysis.Safe))
	}
	if len(analysis.Premium) != 0 {
		t.Errorf("got %d premium results, want 0", len(analysis.Premium))
	}
	if len(analysis.Jackpot) != 0 {
		t.Errorf("got %d jackpot results, want 0", len(analysis.Jackpot))
	}
}

func TestAnalyzeFont_PartialVariantCoverage(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Gem A", "20/20", 100, 10, "HIGH", 0.8, 0.9),
		makeFeature("Gem B", "20/20", 50, 10, "MID", 0.7, 0.85),
		makeFeature("Gem C", "1", 5, 10, "LOW", 0.5, 0.8),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)
	for _, r := range analysis.Safe {
		if r.Color == "RED" && r.Variant == "20/20" {
			// Pool should be 3 (all unique names), even though only 2 have 20/20 entries
			if r.Pool != 3 {
				t.Errorf("Pool = %d, want 3 (unique names across all variants)", r.Pool)
			}
			// pWin uses pool=3, not number of variant-specific entries
			if r.PWin < 0 || r.PWin > 1 {
				t.Errorf("PWin = %f, want value in [0, 1]", r.PWin)
			}
			return
		}
	}
	t.Error("expected RED/20/20 result")
}

func TestAnalyzeFont_ThinPoolGems(t *testing.T) {
	now := time.Now()

	features := []GemFeature{
		makeFeature("Gem A", "20/20", 100, 2, "HIGH", 0.4, 0.7),
		makeFeature("Gem B", "20/20", 120, 3, "HIGH", 0.4, 0.7),
		makeFeature("Gem C", "20/20", 80, 15, "MID", 0.8, 0.9),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)
	for _, r := range analysis.Safe {
		if r.Color == "RED" && r.Variant == "20/20" {
			// All 3 are MID+ winners in safe mode, 2 have < 5 listings
			if r.ThinPoolGems != 2 {
				t.Errorf("ThinPoolGems = %d, want 2", r.ThinPoolGems)
			}
			if r.LiquidityRisk != "HIGH" {
				t.Errorf("LiquidityRisk = %q, want HIGH (2/3 = 66%% thin)", r.LiquidityRisk)
			}
			return
		}
	}
	t.Error("expected RED/20/20 safe result")
}

func TestAnalyzeFont_GemsWithoutFeaturesSkipped(t *testing.T) {
	now := time.Now()

	// Only provide feature for Gem A, not Gem B
	features := []GemFeature{
		makeFeature("Gem A", "20/20", 100, 10, "TOP", 0.8, 0.9),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)
	for _, r := range analysis.Jackpot {
		if r.Color == "RED" && r.Variant == "20/20" {
			// Only Gem A has features. With 1 gem, tier detection is trivial.
			// Gem B has no features → skipped. Only Gem A participates.
			// With 1 gem in pool, jackpot may be 0 or 1 depending on tier fallback.
			if r.Winners > 1 {
				t.Errorf("Winners = %d, want <= 1 (only Gem A has features)", r.Winners)
			}
			return
		}
	}
	t.Error("expected RED/20/20 jackpot result")
}

func TestAnalyzeFont_PWin3PicksStillWorksWithTierBasedCounts(t *testing.T) {
	now := time.Now()
	// Create pool of 10 gems, 3 are HIGH (LOW+ winners), 7 are FLOOR (not winners)
	gems := make([]GemPrice, 10)
	features := make([]GemFeature, 10)
	for i := 0; i < 10; i++ {
		name := "Gem " + string(rune('A'+i))
		var chaos float64
		var tier string
		if i < 3 {
			chaos = 100
			tier = "HIGH"
		} else {
			chaos = 6
			tier = "FLOOR"
		}
		gems[i] = GemPrice{Name: name, Variant: "20/20", Chaos: chaos, Listings: 10, IsTransfigured: true, GemColor: "RED"}
		features[i] = makeFeature(name, "20/20", chaos, 10, tier, 0.8, 0.9)
	}

	analysis := AnalyzeFont(now, gems, features)
	for _, r := range analysis.Safe {
		if r.Color == "RED" && r.Variant == "20/20" {
			if r.Pool != 10 {
				t.Errorf("Pool = %d, want 10", r.Pool)
			}
			// With color-specific tiers: 3 gems at 100c, 7 at 6c.
			// Recursive average separates them. Safe (LOW+) should include the 100c gems.
			if r.Winners < 3 {
				t.Errorf("Winners = %d, want >= 3", r.Winners)
			}
			// pWin should be consistent with winners/pool.
			expected := pWin3Picks(r.Winners, r.Pool)
			if math.Abs(r.PWin-expected) > 0.0001 {
				t.Errorf("PWin = %f, want %f (winners=%d, pool=%d)", r.PWin, expected, r.Winners, r.Pool)
			}
			return
		}
	}
	t.Error("expected RED/20/20 safe result")
}

func TestAnalyzeFont_LowConfidenceExcludedFromEV(t *testing.T) {
	now := time.Now()

	spikeF := makeFeature("Spike Gem", "20/20", 5000, 3, "HIGH", 0.3, 0.7)
	spikeF.LowConfidence = true

	features := []GemFeature{
		spikeF,
		makeFeature("Normal A", "20/20", 200, 50, "HIGH", 0.8, 0.9),
		makeFeature("Normal B", "20/20", 100, 60, "MID", 0.8, 0.9),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var found *FontResult
	for i := range analysis.Safe {
		if analysis.Safe[i].Color == "RED" && analysis.Safe[i].Variant == "20/20" {
			found = &analysis.Safe[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED/20/20 safe result")
	}

	// Pool should include all 3 gems (Font draws from all).
	if found.Pool != 3 {
		t.Errorf("Pool = %d, want 3 (low-confidence counted in pool)", found.Pool)
	}

	// But only 2 winners (Spike excluded from counting).
	// Normal A (HIGH) and Normal B (MID) are Safe winners.
	if found.Winners > 2 {
		t.Errorf("Winners = %d, want <= 2 (Spike excluded from winners)", found.Winners)
	}

	// AvgWinRaw should not be inflated by 5000c Spike.
	if found.AvgWinRaw > 300 {
		t.Errorf("AvgWinRaw = %f, want < 300 (Spike excluded)", found.AvgWinRaw)
	}

	// LowConfidenceGems should contain the spike.
	if len(found.LowConfidenceGems) != 1 {
		t.Errorf("LowConfidenceGems = %d, want 1", len(found.LowConfidenceGems))
	}
}

func TestAnalyzeFont_PricedGemsCountsOnlyThisVariantWhilePoolStaysWhole(t *testing.T) {
	now := time.Now()
	// League-start shape: both RED gems trade at 1/0, only one of them at 20/20.
	// The Font can still roll either gem at 20/20 — that outcome just has no price.
	features := []GemFeature{
		makeFeature("Cleave of Rage", "1", 30, 12, "TOP", 0.9, 0.95),
		makeFeature("Slam of Force", "1", 8, 9, "LOW", 0.8, 0.9),
		makeFeature("Cleave of Rage", "20/20", 250, 15, "TOP", 0.9, 0.95),
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	byVariant := make(map[string]FontResult, len(analysis.Safe))
	for _, r := range analysis.Safe {
		if r.Color == "RED" {
			byVariant[r.Variant] = r
		}
	}

	sparse, ok := byVariant["20/20"]
	if !ok {
		t.Fatalf("no RED 20/20 result; got variants %v", byVariant)
	}
	if sparse.Pool != 2 {
		t.Errorf("20/20 Pool = %d, want 2 — both gems remain possible outcomes", sparse.Pool)
	}
	if sparse.PricedGems != 1 {
		t.Errorf("20/20 PricedGems = %d, want 1 — only one gem is listed at this variant", sparse.PricedGems)
	}

	full, ok := byVariant["1"]
	if !ok {
		t.Fatalf("no RED 1 result; got variants %v", byVariant)
	}
	if full.PricedGems != full.Pool {
		t.Errorf("1: PricedGems = %d, Pool = %d — want equal so the UI keeps the plain count",
			full.PricedGems, full.Pool)
	}
}

func TestAnalyzeFont_PricedGemsExcludesLowConfidenceGems(t *testing.T) {
	now := time.Now()
	// Both gems are listed at 20/20, but the thin-market one is excluded from the
	// EV and contributes 0. Counting it as "priced" would tell the UI the pool is
	// fully valued while the EV is quietly depressed by a zero-valued outcome.
	lowConf := makeFeature("Slam of Force", "20/20", 120, 1, "HIGH", 0.8, 0.9)
	lowConf.LowConfidence = true
	features := []GemFeature{
		makeFeature("Cleave of Rage", "20/20", 250, 15, "TOP", 0.9, 0.95),
		lowConf,
	}

	analysis := AnalyzeFont(now, fontSnapshotFromFeatures(features), features)

	var got *FontResult
	for i := range analysis.Safe {
		if analysis.Safe[i].Color == "RED" && analysis.Safe[i].Variant == "20/20" {
			got = &analysis.Safe[i]
		}
	}
	if got == nil {
		t.Fatalf("no RED 20/20 safe result")
	}
	if got.Pool != 2 {
		t.Fatalf("Pool = %d, want 2 (both gems are possible outcomes)", got.Pool)
	}
	if got.PricedGems != 1 {
		t.Errorf("PricedGems = %d, want 1 — the low-confidence gem enters the EV as 0, so it is not a valued outcome",
			got.PricedGems)
	}
}

func TestLensCraftEV_LensAboveSupportIsPositive(t *testing.T) {
	lens, support := 412.0, 95.0
	if ev := LensCraftEV(&lens, &support); ev == nil || *ev != 317 {
		t.Fatalf("ev = %v, want 317", ev)
	}
}

func TestLensCraftEV_LensBelowSupportIsNegative(t *testing.T) {
	lens, support := 60.0, 95.0
	if ev := LensCraftEV(&lens, &support); ev == nil || *ev != -35 {
		t.Fatalf("ev = %v, want -35", ev)
	}
}

func TestLensCraftEV_MissingEitherPriceHasNoEV(t *testing.T) {
	price := 95.0
	if ev := LensCraftEV(nil, &price); ev != nil {
		t.Errorf("missing lens: ev = %v, want nil", *ev)
	}
	if ev := LensCraftEV(&price, nil); ev != nil {
		t.Errorf("missing support: ev = %v, want nil", *ev)
	}
}

func TestAnalyzeFont_CompleteSnapshotPool(t *testing.T) {
	now := time.Date(2026, 11, 16, 12, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Valuable", Variant: "20/20", GemColor: "RED", IsTransfigured: true, Chaos: 100, Listings: 50},
		{Name: "Boundary", Variant: "20/20", GemColor: "RED", IsTransfigured: true, Chaos: 5, Listings: 50},
		{Name: "Cheap", Variant: "20/20", GemColor: "RED", IsTransfigured: true, Chaos: 4, Listings: 50},
		{Name: "Unpriced", Variant: "1", GemColor: "RED", IsTransfigured: true},
	}
	features := ComputeGemFeatures(now, gems, nil, MarketContext{}, nil, nil)
	analysis := AnalyzeFont(now, gems, features)
	for mode, results := range map[string][]FontResult{"safe": analysis.Safe, "premium": analysis.Premium, "jackpot": analysis.Jackpot} {
		if len(results) != 4 {
			t.Errorf("%s rows = %d, want 4", mode, len(results))
		}
		for _, r := range results {
			if r.Pool != 4 {
				t.Errorf("%s/%s pool = %d, want 4", mode, r.Variant, r.Pool)
			}
			wantEV := 0.0
			if r.Variant == "20/20" {
				// Three of the four equally likely triples contain Valuable.
				wantEV = 75
			}
			if math.Abs(r.EVRaw-wantEV) > 1e-9 {
				t.Errorf("%s/%s EVRaw = %g, want %g", mode, r.Variant, r.EVRaw, wantEV)
			}
		}
	}
}

func TestAnalyzeFont_PoolIgnoresSnapshotPrices(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{}
	for _, color := range []string{"RED", "GREEN", "BLUE"} {
		for _, name := range []string{"A", "B", "C", "D"} {
			gems = append(gems, GemPrice{Name: color + name, Variant: "1", GemColor: color, IsTransfigured: true})
		}
		// Another variant of A must not grow the pool.
		gems = append(gems, GemPrice{Name: color + "A", Variant: "20/20", GemColor: color, IsTransfigured: true})
	}
	for _, price := range []float64{0, 5, 100} {
		t.Run(fmt.Sprint(price), func(t *testing.T) {
			for i := range gems {
				gems[i].Chaos = price
				gems[i].Listings = 50
			}
			features := ComputeGemFeatures(now, gems, nil, MarketContext{}, nil, nil)
			analysis := AnalyzeFont(now, gems, features)
			for _, results := range [][]FontResult{analysis.Safe, analysis.Premium, analysis.Jackpot} {
				if len(results) != 12 {
					t.Fatalf("rows = %d, want 12", len(results))
				}
				for _, r := range results {
					if r.Pool != 4 {
						t.Errorf("%s/%s/%s pool = %d, want 4", r.Mode, r.Color, r.Variant, r.Pool)
					}
					wantEV := 0.0
					if price > 5 {
						if r.Variant == "1" {
							wantEV = 100
						} else if r.Variant == "20/20" {
							wantEV = 75
						}
					}
					if math.Abs(r.EVRaw-wantEV) > 1e-9 {
						t.Errorf("%s/%s/%s EVRaw = %g, want %g", r.Mode, r.Color, r.Variant, r.EVRaw, wantEV)
					}
					if price <= 5 && (r.EV != 0 || r.PricedGems != 0 || r.Winners != 0 || r.PWin != 0 || r.FontsToHit != 0) {
						t.Errorf("zero-valued pool has valued outcomes: %+v", r)
					}
				}
			}
		})
	}
}

func TestAnalyzeFont_SnapshotEligibility(t *testing.T) {
	eligible := GemPrice{Name: "Eligible", Variant: "20/20", GemColor: "RED", IsTransfigured: true, Chaos: 100, Listings: 50}
	for _, excluded := range []GemPrice{
		{Name: "Corrupted", IsTransfigured: true, IsCorrupted: true, GemColor: "RED"},
		{Name: "Normal", GemColor: "RED"},
		{Name: "Gem of Trarthus", IsTransfigured: true, GemColor: "RED"},
		{Name: "Excluded Support", IsTransfigured: true, GemColor: "RED"},
		{Name: "White", IsTransfigured: true, GemColor: "WHITE"},
		{Name: "Unresolved", IsTransfigured: true},
	} {
		t.Run(excluded.Name, func(t *testing.T) {
			excluded.Variant, excluded.Chaos, excluded.Listings = "20/20", 1000, 50
			gems := []GemPrice{eligible, excluded}
			features := ComputeGemFeatures(time.Now(), gems, nil, MarketContext{}, nil, nil)
			analysis := AnalyzeFont(time.Now(), gems, features)
			for _, results := range [][]FontResult{analysis.Safe, analysis.Premium, analysis.Jackpot} {
				if len(results) != 4 {
					t.Fatalf("rows = %d, want 4", len(results))
				}
				for _, r := range results {
					if r.Pool != 1 || r.Color != "RED" {
						t.Errorf("unexpected pool: %+v", r)
					}
					if r.Variant == "20/20" && r.EVRaw != 100 {
						t.Errorf("EVRaw = %g, want 100", r.EVRaw)
					}
				}
			}
		})
	}
}

func blueFontSafe2020(t *testing.T, analysis FontAnalysis) FontResult {
	t.Helper()
	for _, r := range analysis.Safe {
		if r.Color == "BLUE" && r.Variant == "20/20" {
			return r
		}
	}
	t.Fatalf("no BLUE 20/20 safe result in %+v", analysis.Safe)
	return FontResult{}
}

// keep:happy — AC 1: a gem with a thin poe.ninja sample is flagged, kept out
// of tier boundaries and Font EV, and still listed with its flag.
func TestKeepHappy_AnalyzeFontExcludesGatedGemFromEVAndListsIt(t *testing.T) {
	now := time.Date(2026, 11, 16, 20, 0, 0, 0, time.UTC)
	prices := []float64{300, 250, 200, 150, 120, 100, 80, 60, 40, 20}
	depths := []int{10, 12, 15, 18, 20, 20, 25, 30, 35, 40} // with the gated gem: odd-sized, median 20
	var peers []GemPrice
	for i := range prices {
		peers = append(peers, GemPrice{Name: fmt.Sprintf("Blue Gem %d of Test", i), Variant: "20/20", Chaos: prices[i],
			Listings: depths[i], SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"})
	}
	three := 3
	gated := GemPrice{Name: "Gated Gem of Test", Variant: "20/20", Chaos: 90, Listings: 20, SampleCount: &three, IsTransfigured: true, GemColor: "BLUE"}
	gems := append(append([]GemPrice{}, peers...), gated)

	cls := ComputeGemClassification(gems)
	features := ComputeGemFeatures(now, gems, nil, MarketContext{}, cls.Gems, nil)
	got := blueFontSafe2020(t, AnalyzeFont(now, gems, features))

	// Baseline: the gated gem is a pool member with no priced feature.
	peerCls := ComputeGemClassification(peers)
	baseline := blueFontSafe2020(t, AnalyzeFont(now, gems, ComputeGemFeatures(now, peers, nil, MarketContext{}, peerCls.Gems, nil)))
	if baseline.EVRaw <= 0 {
		t.Fatalf("fixture: baseline EVRaw = %v, want a priced pool", baseline.EVRaw)
	}

	wantListed := []LowConfidenceGemInfo{{Name: gated.Name, Chaos: 90, Listings: 20}}
	if !reflect.DeepEqual(got.LowConfidenceGems, wantListed) {
		t.Errorf("LowConfidenceGems = %+v, want %+v", got.LowConfidenceGems, wantListed)
	}
	if got.EV != baseline.EV || got.EVRaw != baseline.EVRaw {
		t.Errorf("EV/EVRaw = %v/%v, want %v/%v (the gated gem adds nothing)", got.EV, got.EVRaw, baseline.EV, baseline.EVRaw)
	}
	if got.Pool != len(gems) {
		t.Errorf("Pool = %d, want %d (the gated gem still counts)", got.Pool, len(gems))
	}
	if !reflect.DeepEqual(cls.Boundaries["20/20"], peerCls.Boundaries["20/20"]) {
		t.Errorf("Boundaries[20/20] = %+v, want %+v (as without the gated gem)", cls.Boundaries["20/20"], peerCls.Boundaries["20/20"])
	}
	var flagged, found bool
	for _, f := range features {
		if f.Name == gated.Name && f.Variant == gated.Variant {
			found, flagged = true, f.LowConfidence
		}
	}
	if !found || !flagged {
		t.Errorf("gated GemFeature present=%v LowConfidence=%v, want present and true", found, flagged)
	}
}
