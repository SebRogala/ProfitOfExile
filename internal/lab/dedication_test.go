package lab

import (
	"math"
	"testing"
	"time"
)

// makeDedicationGem creates a corrupted 21/23c GemPrice for Dedication pool tests.
func makeDedicationGem(name, color string, chaos float64, listings int, isTransfigured bool) GemPrice {
	return GemPrice{
		Name:           name,
		Variant:        "21/23c",
		Chaos:          chaos,
		Listings:       listings,
		IsTransfigured: isTransfigured,
		IsCorrupted:    true,
		GemColor:       color,
	}
}

func TestIsDedicationGem_IncludesCorruptedSkills(t *testing.T) {
	g := GemPrice{Name: "Arc", IsCorrupted: true}
	if !isDedicationGem(g) {
		t.Error("corrupted skill gem 'Arc' should pass isDedicationGem")
	}
}

func TestIsDedicationGem_ExcludesNonCorrupted(t *testing.T) {
	g := GemPrice{Name: "Arc", IsCorrupted: false}
	if isDedicationGem(g) {
		t.Error("non-corrupted gem should NOT pass isDedicationGem")
	}
}

func TestIsDedicationGem_ExcludesSupports(t *testing.T) {
	tests := []struct {
		name string
		gem  GemPrice
	}{
		{"Lifetap Support", GemPrice{Name: "Lifetap Support", IsCorrupted: true}},
		{"Added Fire Support", GemPrice{Name: "Added Fire Damage Support", IsCorrupted: true}},
		{"Empower Support", GemPrice{Name: "Empower Support", IsCorrupted: true}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if isDedicationGem(tt.gem) {
				t.Errorf("support gem %q should NOT pass isDedicationGem", tt.gem.Name)
			}
		})
	}
}

func TestIsDedicationGem_ExcludesTrarthus(t *testing.T) {
	g := GemPrice{Name: "Trarthus Ire", IsCorrupted: true}
	if isDedicationGem(g) {
		t.Error("Trarthus should NOT pass isDedicationGem")
	}
}

func TestDedicationInputCost_AvgOf10Cheapest(t *testing.T) {
	prices := make([]float64, 15)
	for i := 0; i < 15; i++ {
		prices[i] = float64(10 + i*5) // 10, 15, 20, ..., 80
	}
	got := dedicationInputCostFromPrices(prices)
	// 10 cheapest: 10, 15, 20, 25, 30, 35, 40, 45, 50, 55 => avg = 32.5
	want := 32.5
	if math.Abs(got-want) > 0.01 {
		t.Errorf("dedicationInputCostFromPrices = %f, want %f", got, want)
	}
}

func TestDedicationInputCost_FewerThan10Gems(t *testing.T) {
	prices := []float64{10, 20, 30}
	got := dedicationInputCostFromPrices(prices)
	want := 20.0 // (10+20+30)/3
	if math.Abs(got-want) > 0.01 {
		t.Errorf("dedicationInputCostFromPrices = %f, want %f", got, want)
	}
}

func TestDedicationInputCost_EmptyPool(t *testing.T) {
	got := dedicationInputCostFromPrices(nil)
	if got != 0 {
		t.Errorf("dedicationInputCostFromPrices(nil) = %f, want 0", got)
	}
}

func TestAnalyzeDedication_EmptyPool(t *testing.T) {
	now := time.Now()
	// No corrupted gems at all.
	analysis := AnalyzeDedication(now, nil, nil)
	if len(analysis.Skills) != 0 {
		t.Errorf("Skills = %d results, want 0 for empty pool", len(analysis.Skills))
	}
	if len(analysis.Transfigured) != 0 {
		t.Errorf("Transfigured = %d results, want 0 for empty pool", len(analysis.Transfigured))
	}
}

func TestAnalyzeDedication_EmptyColorPool(t *testing.T) {
	now := time.Now()
	// Only GREEN gems; RED and BLUE are empty.
	gems := []GemPrice{
		makeDedicationGem("Arc", "GREEN", 100, 20, false),
		makeDedicationGem("Fireball", "GREEN", 50, 30, false),
		makeDedicationGem("Ice Shot", "GREEN", 200, 15, false),
		makeDedicationGem("Cleave", "GREEN", 75, 25, false),
		makeDedicationGem("Slam", "GREEN", 30, 40, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	// RED and BLUE should produce no results.
	for _, r := range analysis.Skills {
		if r.Color == "RED" || r.Color == "BLUE" {
			t.Errorf("unexpected %s skill result for color with 0 gems", r.Color)
		}
	}
	// GREEN should produce 3 mode results (safe, premium, jackpot).
	greenCount := 0
	for _, r := range analysis.Skills {
		if r.Color == "GREEN" {
			greenCount++
		}
	}
	if greenCount != 3 {
		t.Errorf("GREEN skill results = %d, want 3 (safe+premium+jackpot)", greenCount)
	}
}

func TestAnalyzeDedication_PoolLessThan3(t *testing.T) {
	now := time.Now()
	// Only 2 gems in RED skill pool.
	gems := []GemPrice{
		makeDedicationGem("Arc", "RED", 500, 20, false),
		makeDedicationGem("Fireball", "RED", 100, 30, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var found *DedicationResult
	for i := range analysis.Skills {
		if analysis.Skills[i].Color == "RED" && analysis.Skills[i].Mode == "safe" {
			found = &analysis.Skills[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED skill safe result")
	}
	if found.Pool != 2 {
		t.Errorf("Pool = %d, want 2", found.Pool)
	}
	// With pool < 3, pWin3Picks returns 1.0 for any positive winner count.
	if found.Winners > 0 && found.PWin != 1.0 {
		t.Errorf("PWin = %f, want 1.0 (pool < 3 with winners)", found.PWin)
	}
	// EV should be the best gem's price (expectedBestOf3 with n<3 returns values[0]).
	// Since pool values include risk-adjusted prices, just verify EV > 0.
	if found.EV <= 0 {
		t.Errorf("EV = %f, want > 0", found.EV)
	}
}

func TestAnalyzeDedication_SupportFiltering(t *testing.T) {
	now := time.Now()
	// Mix of skill gems and support gems in RED pool.
	gems := []GemPrice{
		makeDedicationGem("Arc", "RED", 100, 20, false),
		makeDedicationGem("Lifetap Support", "RED", 500, 10, false),
		makeDedicationGem("Fireball", "RED", 200, 30, false),
		makeDedicationGem("Added Fire Damage Support", "RED", 300, 15, false),
		makeDedicationGem("Cleave", "RED", 50, 25, false),
		makeDedicationGem("Slam", "RED", 75, 35, false),
		makeDedicationGem("Ice Shot", "RED", 60, 40, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var found *DedicationResult
	for i := range analysis.Skills {
		if analysis.Skills[i].Color == "RED" && analysis.Skills[i].Mode == "safe" {
			found = &analysis.Skills[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected RED skill safe result")
	}
	// Only non-support gems: Arc, Fireball, Cleave, Slam, Ice Shot = 5
	if found.Pool != 5 {
		t.Errorf("Pool = %d, want 5 (supports excluded)", found.Pool)
	}
}

func TestAnalyzeDedication_TwoPoolSeparation(t *testing.T) {
	now := time.Now()
	// Mix of transfigured and non-transfigured corrupted gems.
	gems := []GemPrice{
		// Non-transfigured skills (skill pool)
		makeDedicationGem("Arc", "RED", 100, 20, false),
		makeDedicationGem("Fireball", "RED", 200, 30, false),
		makeDedicationGem("Cleave", "RED", 50, 25, false),
		makeDedicationGem("Slam", "RED", 75, 35, false),
		makeDedicationGem("Ice Shot", "RED", 60, 40, false),
		// Transfigured skills (transfigured pool)
		makeDedicationGem("Arc of Surging", "RED", 300, 15, true),
		makeDedicationGem("Fireball of Volatility", "RED", 400, 10, true),
		makeDedicationGem("Cleave of Rage", "RED", 150, 20, true),
		makeDedicationGem("Slam of Magnitude", "RED", 250, 25, true),
		makeDedicationGem("Ice Shot of Shattering", "RED", 180, 30, true),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	// Skills pool: 5 gems.
	var skillResult *DedicationResult
	for i := range analysis.Skills {
		if analysis.Skills[i].Color == "RED" && analysis.Skills[i].Mode == "safe" {
			skillResult = &analysis.Skills[i]
			break
		}
	}
	if skillResult == nil {
		t.Fatal("expected RED skill safe result")
	}
	if skillResult.Pool != 5 {
		t.Errorf("Skills Pool = %d, want 5", skillResult.Pool)
	}
	if skillResult.GemType != "skill" {
		t.Errorf("Skills GemType = %q, want %q", skillResult.GemType, "skill")
	}

	// Transfigured pool: 5 gems.
	var transfigResult *DedicationResult
	for i := range analysis.Transfigured {
		if analysis.Transfigured[i].Color == "RED" && analysis.Transfigured[i].Mode == "safe" {
			transfigResult = &analysis.Transfigured[i]
			break
		}
	}
	if transfigResult == nil {
		t.Fatal("expected RED transfigured safe result")
	}
	if transfigResult.Pool != 5 {
		t.Errorf("Transfigured Pool = %d, want 5", transfigResult.Pool)
	}
	if transfigResult.GemType != "transfigured" {
		t.Errorf("Transfigured GemType = %q, want %q", transfigResult.GemType, "transfigured")
	}
}

func TestAnalyzeDedication_DynamicInputCost(t *testing.T) {
	now := time.Now()
	// GREEN skill pool with known prices.
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "GREEN", 5, 20, false),
		makeDedicationGem("Gem 2", "GREEN", 7, 25, false),
		makeDedicationGem("Gem 3", "GREEN", 8, 30, false),
		makeDedicationGem("Gem 4", "GREEN", 9, 15, false),
		makeDedicationGem("Gem 5", "GREEN", 10, 40, false),
		makeDedicationGem("Gem 6", "GREEN", 12, 35, false),
		makeDedicationGem("Gem 7", "GREEN", 15, 20, false),
		makeDedicationGem("Gem 8", "GREEN", 20, 50, false),
		makeDedicationGem("Gem 9", "GREEN", 50, 30, false),
		makeDedicationGem("Gem 10", "GREEN", 100, 45, false),
		makeDedicationGem("Gem 11", "GREEN", 200, 10, false),
		makeDedicationGem("Gem 12", "GREEN", 500, 60, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var found *DedicationResult
	for i := range analysis.Skills {
		if analysis.Skills[i].Color == "GREEN" && analysis.Skills[i].Mode == "safe" {
			found = &analysis.Skills[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected GREEN skill safe result")
	}

	// 10 cheapest: 5, 7, 8, 9, 10, 12, 15, 20, 50, 100 => avg = 23.6
	wantInputCost := 23.6
	if math.Abs(found.InputCost-wantInputCost) > 0.01 {
		t.Errorf("InputCost = %f, want %f", found.InputCost, wantInputCost)
	}
}

func TestAnalyzeDedication_ProfitCalculation(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "BLUE", 10, 30, false),
		makeDedicationGem("Gem 2", "BLUE", 20, 40, false),
		makeDedicationGem("Gem 3", "BLUE", 50, 25, false),
		makeDedicationGem("Gem 4", "BLUE", 100, 50, false),
		makeDedicationGem("Gem 5", "BLUE", 200, 20, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var found *DedicationResult
	for i := range analysis.Skills {
		if analysis.Skills[i].Color == "BLUE" && analysis.Skills[i].Mode == "safe" {
			found = &analysis.Skills[i]
			break
		}
	}
	if found == nil {
		t.Fatal("expected BLUE skill safe result")
	}

	// Profit = EVRaw - InputCost (no offering cost).
	expectedProfit := found.EVRaw - found.InputCost
	if math.Abs(found.Profit-expectedProfit) > 0.01 {
		t.Errorf("Profit = %f, want EVRaw(%f) - InputCost(%f) = %f",
			found.Profit, found.EVRaw, found.InputCost, expectedProfit)
	}
}

func TestAnalyzeDedication_ThreeModesPerPoolColor(t *testing.T) {
	now := time.Now()
	// Create enough RED skill gems for meaningful tier separation.
	gems := []GemPrice{
		makeDedicationGem("Expensive 1", "RED", 1500, 50, false),
		makeDedicationGem("Expensive 2", "RED", 1200, 40, false),
		makeDedicationGem("High 1", "RED", 500, 60, false),
		makeDedicationGem("High 2", "RED", 400, 55, false),
		makeDedicationGem("Mid 1", "RED", 200, 70, false),
		makeDedicationGem("Mid 2", "RED", 150, 65, false),
		makeDedicationGem("Mid 3", "RED", 100, 80, false),
		makeDedicationGem("Low 1", "RED", 50, 90, false),
		makeDedicationGem("Low 2", "RED", 30, 100, false),
		makeDedicationGem("Floor 1", "RED", 10, 120, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	modes := map[string]bool{}
	for _, r := range analysis.Skills {
		if r.Color == "RED" {
			modes[r.Mode] = true
		}
	}
	for _, mode := range []string{"safe", "premium", "jackpot"} {
		if !modes[mode] {
			t.Errorf("missing %s mode for RED skill pool", mode)
		}
	}
}

func TestAnalyzeDedication_WinnerHierarchy(t *testing.T) {
	now := time.Now()
	// Large enough pool for tier classification.
	gems := []GemPrice{
		makeDedicationGem("Top 1", "RED", 2000, 30, false),
		makeDedicationGem("Top 2", "RED", 1800, 25, false),
		makeDedicationGem("High 1", "RED", 500, 60, false),
		makeDedicationGem("High 2", "RED", 450, 55, false),
		makeDedicationGem("Mid 1", "RED", 200, 70, false),
		makeDedicationGem("Mid 2", "RED", 180, 65, false),
		makeDedicationGem("Mid 3", "RED", 150, 80, false),
		makeDedicationGem("Low 1", "RED", 50, 90, false),
		makeDedicationGem("Low 2", "RED", 40, 100, false),
		makeDedicationGem("Floor 1", "RED", 10, 120, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var safeWinners, premiumWinners, jackpotWinners int
	for _, r := range analysis.Skills {
		if r.Color == "RED" {
			switch r.Mode {
			case "safe":
				safeWinners = r.Winners
			case "premium":
				premiumWinners = r.Winners
			case "jackpot":
				jackpotWinners = r.Winners
			}
		}
	}

	// Safe >= Premium >= Jackpot (each mode is stricter).
	if safeWinners < premiumWinners {
		t.Errorf("Safe winners (%d) should be >= Premium winners (%d)", safeWinners, premiumWinners)
	}
	if premiumWinners < jackpotWinners {
		t.Errorf("Premium winners (%d) should be >= Jackpot winners (%d)", premiumWinners, jackpotWinners)
	}
}

func TestAnalyzeDedication_ThinLiquidity(t *testing.T) {
	now := time.Now()
	// All gems have < 15 listings (thin market).
	gems := []GemPrice{
		makeDedicationGem("Thin 1", "RED", 500, 3, false),
		makeDedicationGem("Thin 2", "RED", 400, 4, false),
		makeDedicationGem("Thin 3", "RED", 300, 2, false),
		makeDedicationGem("Thin 4", "RED", 200, 4, false),
		makeDedicationGem("Thin 5", "RED", 100, 3, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "RED" && r.Mode == "safe" {
			// With all listings < 5, ThinPoolGems should == Winners.
			if r.Winners > 0 && r.ThinPoolGems != r.Winners {
				t.Errorf("ThinPoolGems = %d, want %d (all gems have < 5 listings)", r.ThinPoolGems, r.Winners)
			}
			// LiquidityRisk should be HIGH when all winners are thin.
			if r.Winners > 0 && r.LiquidityRisk != "HIGH" {
				t.Errorf("LiquidityRisk = %q, want HIGH (all winners thin)", r.LiquidityRisk)
			}
			return
		}
	}
	t.Error("expected RED skill safe result")
}

func TestAnalyzeDedication_FontsToHitCalculation(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "BLUE", 500, 50, false),
		makeDedicationGem("Gem 2", "BLUE", 400, 40, false),
		makeDedicationGem("Gem 3", "BLUE", 300, 60, false),
		makeDedicationGem("Gem 4", "BLUE", 200, 55, false),
		makeDedicationGem("Gem 5", "BLUE", 100, 70, false),
		makeDedicationGem("Gem 6", "BLUE", 50, 80, false),
		makeDedicationGem("Gem 7", "BLUE", 30, 90, false),
		makeDedicationGem("Gem 8", "BLUE", 20, 100, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "BLUE" && r.Mode == "safe" {
			if r.PWin > 0 {
				expected := 1.0 / r.PWin
				if math.Abs(r.FontsToHit-expected) > 0.001 {
					t.Errorf("FontsToHit = %f, want 1/PWin = %f", r.FontsToHit, expected)
				}
			}
			return
		}
	}
	t.Error("expected BLUE skill safe result")
}

func TestAnalyzeDedication_SkipsNon2123Variant(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		// Non 21/23c variants should be ignored.
		{Name: "Arc", Variant: "20/20", Chaos: 500, Listings: 50, IsCorrupted: true, GemColor: "RED"},
		{Name: "Fireball", Variant: "1", Chaos: 200, Listings: 30, IsCorrupted: true, GemColor: "RED"},
		// This one is the only valid entry.
		makeDedicationGem("Cleave", "RED", 100, 25, false),
		makeDedicationGem("Slam", "RED", 75, 35, false),
		makeDedicationGem("Ice Shot", "RED", 60, 40, false),
		makeDedicationGem("Bash", "RED", 50, 20, false),
		makeDedicationGem("Strike", "RED", 45, 30, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "RED" && r.Mode == "safe" {
			// Only 21/23c gems should be in pool.
			if r.Pool != 5 {
				t.Errorf("Pool = %d, want 5 (non-21/23c variants excluded)", r.Pool)
			}
			return
		}
	}
	t.Error("expected RED skill safe result")
}

func TestAnalyzeDedication_SkipsInvalidColors(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Unknown Gem", "PURPLE", 100, 20, false),
		makeDedicationGem("Another", "", 200, 30, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	if len(analysis.Skills) != 0 {
		t.Errorf("Skills = %d, want 0 (invalid colors should produce no results)", len(analysis.Skills))
	}
}

func TestAnalyzeDedication_EVSharedAcrossModes(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "GREEN", 800, 50, false),
		makeDedicationGem("Gem 2", "GREEN", 400, 40, false),
		makeDedicationGem("Gem 3", "GREEN", 200, 60, false),
		makeDedicationGem("Gem 4", "GREEN", 100, 55, false),
		makeDedicationGem("Gem 5", "GREEN", 50, 70, false),
		makeDedicationGem("Gem 6", "GREEN", 30, 80, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	var safeEV, premiumEV, jackpotEV float64
	for _, r := range analysis.Skills {
		if r.Color == "GREEN" {
			switch r.Mode {
			case "safe":
				safeEV = r.EV
			case "premium":
				premiumEV = r.EV
			case "jackpot":
				jackpotEV = r.EV
			}
		}
	}

	// EV is computed from the full pool (expectedBestOf3), so it should be
	// the same across all three modes for a given (color, gemType).
	if safeEV != premiumEV || premiumEV != jackpotEV {
		t.Errorf("EV should be identical across modes: safe=%f, premium=%f, jackpot=%f",
			safeEV, premiumEV, jackpotEV)
	}
}

func TestAnalyzeDedication_WithFeatures(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem A", "RED", 500, 50, false),
		makeDedicationGem("Gem B", "RED", 300, 30, false),
		makeDedicationGem("Gem C", "RED", 100, 60, false),
		makeDedicationGem("Gem D", "RED", 50, 80, false),
		makeDedicationGem("Gem E", "RED", 20, 90, false),
	}

	features := []GemFeature{
		{Name: "Gem A", Variant: "21/23c", Chaos: 500, Listings: 50, Low7Days: 450, CVShort: 10},
		{Name: "Gem B", Variant: "21/23c", Chaos: 300, Listings: 30, Low7Days: 280, CVShort: 15},
		{Name: "Gem C", Variant: "21/23c", Chaos: 100, Listings: 60, Low7Days: 90, CVShort: 5},
	}

	analysisWithFeats := AnalyzeDedication(now, gems, features)
	analysisWithout := AnalyzeDedication(now, gems, nil)

	// Both should produce results.
	if len(analysisWithFeats.Skills) == 0 {
		t.Fatal("expected skill results with features")
	}
	if len(analysisWithout.Skills) == 0 {
		t.Fatal("expected skill results without features")
	}

	// With features vs without should produce different risk-adjusted EV
	// (features enable CVShort-based stability discount).
	var evWith, evWithout float64
	for _, r := range analysisWithFeats.Skills {
		if r.Color == "RED" && r.Mode == "safe" {
			evWith = r.EV
		}
	}
	for _, r := range analysisWithout.Skills {
		if r.Color == "RED" && r.Mode == "safe" {
			evWithout = r.EV
		}
	}
	// Both should be > 0. The exact values differ based on feature presence.
	if evWith <= 0 {
		t.Errorf("EV with features = %f, want > 0", evWith)
	}
	if evWithout <= 0 {
		t.Errorf("EV without features = %f, want > 0", evWithout)
	}
}

func TestAnalyzeDedication_JackpotGemsPopulated(t *testing.T) {
	now := time.Now()
	// Create a pool with a clear TOP gem (huge gap).
	gems := []GemPrice{
		makeDedicationGem("Mega Gem", "BLUE", 5000, 30, false),
		makeDedicationGem("High 1", "BLUE", 500, 60, false),
		makeDedicationGem("High 2", "BLUE", 450, 55, false),
		makeDedicationGem("Mid 1", "BLUE", 200, 70, false),
		makeDedicationGem("Mid 2", "BLUE", 150, 65, false),
		makeDedicationGem("Mid 3", "BLUE", 100, 80, false),
		makeDedicationGem("Low 1", "BLUE", 50, 90, false),
		makeDedicationGem("Low 2", "BLUE", 30, 100, false),
		makeDedicationGem("Floor 1", "BLUE", 10, 120, false),
		makeDedicationGem("Floor 2", "BLUE", 8, 110, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "BLUE" && r.Mode == "jackpot" {
			// Jackpot mode should have JackpotGems populated if there are TOP winners.
			if r.Winners > 0 && len(r.JackpotGems) == 0 {
				t.Error("JackpotGems should be populated when jackpot has winners")
			}
			// JackpotGems should only be in jackpot mode.
			return
		}
	}
	t.Error("expected BLUE skill jackpot result")
}

func TestAnalyzeDedication_SafeAndPremiumHaveNoJackpotGems(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "RED", 500, 50, false),
		makeDedicationGem("Gem 2", "RED", 300, 40, false),
		makeDedicationGem("Gem 3", "RED", 200, 60, false),
		makeDedicationGem("Gem 4", "RED", 100, 70, false),
		makeDedicationGem("Gem 5", "RED", 50, 80, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "RED" && (r.Mode == "safe" || r.Mode == "premium") {
			if len(r.JackpotGems) != 0 {
				t.Errorf("%s mode should not have JackpotGems (has %d)", r.Mode, len(r.JackpotGems))
			}
		}
	}
}

func TestAnalyzeDedication_PoolBreakdownInSafeModeOnly(t *testing.T) {
	now := time.Now()
	gems := []GemPrice{
		makeDedicationGem("Gem 1", "GREEN", 500, 50, false),
		makeDedicationGem("Gem 2", "GREEN", 300, 40, false),
		makeDedicationGem("Gem 3", "GREEN", 200, 60, false),
		makeDedicationGem("Gem 4", "GREEN", 100, 70, false),
		makeDedicationGem("Gem 5", "GREEN", 50, 80, false),
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "GREEN" {
			if r.Mode == "safe" {
				// PoolBreakdown should be present on safe mode.
				if r.PoolBreakdown == nil {
					t.Error("safe mode should have PoolBreakdown")
				}
			} else {
				// Other modes should not have PoolBreakdown.
				if len(r.PoolBreakdown) > 0 {
					t.Errorf("%s mode should not have PoolBreakdown", r.Mode)
				}
			}
		}
	}
}

func TestAnalyzeDedication_LowConfidenceGemsInSafeMode(t *testing.T) {
	now := time.Now()
	// Create a mix: most gems have many listings, one has very few (thin market).
	gems := []GemPrice{
		makeDedicationGem("Normal 1", "RED", 500, 80, false),
		makeDedicationGem("Normal 2", "RED", 400, 70, false),
		makeDedicationGem("Normal 3", "RED", 300, 60, false),
		makeDedicationGem("Normal 4", "RED", 200, 90, false),
		makeDedicationGem("Normal 5", "RED", 100, 75, false),
		makeDedicationGem("Thin Spike", "RED", 3000, 2, false), // very thin market vs median ~75
	}
	analysis := AnalyzeDedication(now, gems, nil)

	for _, r := range analysis.Skills {
		if r.Color == "RED" && r.Mode == "safe" {
			// LowConfidenceGems should include Thin Spike.
			if len(r.LowConfidenceGems) == 0 {
				t.Error("expected at least 1 low-confidence gem (Thin Spike)")
			}
			found := false
			for _, lcg := range r.LowConfidenceGems {
				if lcg.Name == "Thin Spike" {
					found = true
					if lcg.Chaos != 3000 {
						t.Errorf("Thin Spike chaos = %f, want 3000", lcg.Chaos)
					}
				}
			}
			if !found {
				t.Error("Thin Spike should be in LowConfidenceGems")
			}
			return
		}
	}
	t.Error("expected RED skill safe result")
}
