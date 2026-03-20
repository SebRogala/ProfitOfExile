package lab

import (
	"testing"
)

func TestDetectTierBoundaries_RealisticDistribution(t *testing.T) {
	// Real-ish price distribution with clear natural gaps.
	// Prices: 785, 600, 580, 570, 510, 500, 470, 460, 390, 350, 300, 200, 150, 80, 40, 30, 20, 15, 10, 8
	// Major gaps: 785→600 (23.6%), 460→390 (15.2%), 40→30 (25%)
	prices := []float64{785, 600, 580, 570, 510, 500, 470, 460, 390, 350, 300, 200, 150, 80, 40, 30, 20, 15, 10, 8}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true,
		}
	}

	tb := DetectTierBoundaries(gems)

	// Verify ordering invariant: Top > High > Mid > 0
	if tb.Top <= 0 {
		t.Errorf("Top = %f, want > 0", tb.Top)
	}
	if tb.High <= 0 {
		t.Errorf("High = %f, want > 0", tb.High)
	}
	if tb.Mid <= 0 {
		t.Errorf("Mid = %f, want > 0", tb.Mid)
	}
	if tb.Top <= tb.High {
		t.Errorf("Top (%f) should be > High (%f)", tb.Top, tb.High)
	}
	if tb.High <= tb.Mid {
		t.Errorf("High (%f) should be > Mid (%f)", tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_SingleOutlier(t *testing.T) {
	// One outlier at 1000c, then a tight cluster 92-100c.
	prices := []float64{1000, 100, 99, 98, 97, 96, 95, 94, 93, 92}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true,
		}
	}

	tb := DetectTierBoundaries(gems)

	// The 1000→100 gap (90%) should be the largest, so TOP boundary = 100.
	if tb.Top > 200 {
		t.Errorf("Top = %f, want <= 200 (1000c is the only TOP gem)", tb.Top)
	}
	// Invariant
	if tb.Top <= tb.High {
		t.Errorf("Top (%f) should be > High (%f)", tb.Top, tb.High)
	}
	if tb.High <= tb.Mid {
		t.Errorf("High (%f) should be > Mid (%f)", tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_SmallPool(t *testing.T) {
	// Exactly 10 gems — should still produce valid 3 boundaries.
	// All prices > 5 (the analysis pipeline floor).
	prices := []float64{500, 400, 300, 200, 150, 100, 80, 50, 20, 6}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true,
		}
	}

	tb := DetectTierBoundaries(gems)

	if tb.Top <= tb.High {
		t.Errorf("Top (%f) should be > High (%f)", tb.Top, tb.High)
	}
	if tb.High <= tb.Mid {
		t.Errorf("High (%f) should be > Mid (%f)", tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_MinimumPool(t *testing.T) {
	// <4 gems → fallback to P75/P50/P25.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 50, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 10, IsTransfigured: true},
	}

	tb := DetectTierBoundaries(gems)

	// Fallback: P75/P50/P25 of [10, 50, 100]
	// P75: 0.75 * 2 = 1.5 → lerp(50, 100, 0.5) = 75
	// P50: 0.50 * 2 = 1.0 → 50
	// P25: 0.25 * 2 = 0.5 → lerp(10, 50, 0.5) = 30
	if !approxEqual(tb.Top, 75, 0.01) {
		t.Errorf("Top = %f, want 75 (P75 fallback)", tb.Top)
	}
	if !approxEqual(tb.High, 50, 0.01) {
		t.Errorf("High = %f, want 50 (P50 fallback)", tb.High)
	}
	if !approxEqual(tb.Mid, 30, 0.01) {
		t.Errorf("Mid = %f, want 30 (P25 fallback)", tb.Mid)
	}
}

func TestDetectTierBoundaries_ExactlyThreeDistinctPrices(t *testing.T) {
	// Only 3 distinct prices → only 2 gaps → fallback needed.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 10, IsTransfigured: true},
	}

	tb := DetectTierBoundaries(gems)

	// Fallback triggers (< 4 distinct prices).
	if tb.Top <= tb.High {
		t.Errorf("Top (%f) should be > High (%f)", tb.Top, tb.High)
	}
	if tb.High <= tb.Mid {
		t.Errorf("High (%f) should be > Mid (%f)", tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_MultiVariantDedup(t *testing.T) {
	// Same gem name at different variants — should use max price per name.
	gems := []GemPrice{
		{Name: "Kinetic Blast of Clustering", Variant: "20/20", Chaos: 785, IsTransfigured: true},
		{Name: "Kinetic Blast of Clustering", Variant: "20", Chaos: 600, IsTransfigured: true},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 400, IsTransfigured: true},
		{Name: "Gem C of X", Variant: "20/20", Chaos: 200, IsTransfigured: true},
		{Name: "Gem D of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "Gem E of X", Variant: "20/20", Chaos: 50, IsTransfigured: true},
	}

	tb := DetectTierBoundaries(gems)

	// Kinetic Blast should contribute only 785c, not both 785 and 600.
	// 5 unique gems: 785, 400, 200, 100, 50
	if tb.Top <= tb.High {
		t.Errorf("Top (%f) should be > High (%f)", tb.Top, tb.High)
	}
	if tb.High <= tb.Mid {
		t.Errorf("High (%f) should be > Mid (%f)", tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_FiltersCorrectly(t *testing.T) {
	// Verify corrupted, non-transfigured, Trarthus, zero-price, and sub-5c gems are excluded.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true},
		// These should all be excluded:
		{Name: "Corrupted of X", Variant: "20/20", Chaos: 9999, IsTransfigured: true, IsCorrupted: true},
		{Name: "BaseGem", Variant: "20/20", Chaos: 9999, IsTransfigured: false},
		{Name: "Wave of Trarthus", Variant: "20/20", Chaos: 9999, IsTransfigured: true},
		{Name: "Zero of X", Variant: "20/20", Chaos: 0, IsTransfigured: true},
		{Name: "Negative of X", Variant: "20/20", Chaos: -10, IsTransfigured: true},
		// Sub-5c gems excluded to match analysis pipeline floor (Chaos > 5):
		{Name: "Cheap of X", Variant: "20/20", Chaos: 3, IsTransfigured: true},
		{Name: "AtFloor of X", Variant: "20/20", Chaos: 5, IsTransfigured: true},
	}

	tb := DetectTierBoundaries(gems)

	// Only 4 qualifying gems: 500, 300, 100, 50.
	// If the excluded gems were included, boundaries would be way off.
	if tb.Top > 500 {
		t.Errorf("Top = %f, should not exceed 500 (filtered correctly)", tb.Top)
	}
	if tb.Top <= 0 {
		t.Errorf("Top = %f, want > 0", tb.Top)
	}
}

func TestDetectTierBoundaries_EmptyInput(t *testing.T) {
	tb := DetectTierBoundaries(nil)
	if tb.Top != 0 || tb.High != 0 || tb.Mid != 0 {
		t.Errorf("empty input should return zero boundaries, got Top=%f High=%f Mid=%f", tb.Top, tb.High, tb.Mid)
	}
}

func TestDetectTierBoundaries_AllSamePrice(t *testing.T) {
	// All gems at the same price → 1 unique price → fallback.
	gems := make([]GemPrice, 10)
	for i := range gems {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          100,
			IsTransfigured: true,
		}
	}

	tb := DetectTierBoundaries(gems)

	// All percentiles of a single value = 100.
	if !approxEqual(tb.Top, 100, 0.01) {
		t.Errorf("Top = %f, want 100", tb.Top)
	}
}

func TestClassifyTier_FourTiers(t *testing.T) {
	tb := TierBoundaries{Top: 500, High: 200, Mid: 50}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{500, "TOP"},   // exactly at Top → TOP (>= semantics)
		{499.99, "HIGH"},
		{200, "HIGH"},  // exactly at High → HIGH
		{199.99, "MID"},
		{50, "MID"},    // exactly at Mid → MID
		{49.99, "LOW"},
		{0, "LOW"},
	}

	for _, tt := range tests {
		got := classifyTier(tt.price, tb)
		if got != tt.want {
			t.Errorf("classifyTier(%v, {500, 200, 50}) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestClassifyTier_Exported(t *testing.T) {
	tb := TierBoundaries{Top: 100, High: 50, Mid: 20}
	got := ClassifyTier(150, tb)
	if got != "TOP" {
		t.Errorf("ClassifyTier(150, ...) = %s, want TOP", got)
	}
}

func TestDetectTierBoundaries_FourExactGems(t *testing.T) {
	// Exactly 4 gems — minimum for gap detection (3 gaps).
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 1000, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 500, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "D of X", Variant: "20/20", Chaos: 10, IsTransfigured: true},
	}

	tb := DetectTierBoundaries(gems)

	// 3 gaps: 1000→500 (50%), 500→100 (80%), 100→10 (90%)
	// Largest gaps sorted by relGap desc: 100→10 (90%), 500→100 (80%), 1000→500 (50%)
	// Sorted by index asc: index 0 (1000→500), index 1 (500→100), index 2 (100→10)
	// Top = prices[1] = 500, High = prices[2] = 100, Mid = prices[3] = 10
	if !approxEqual(tb.Top, 500, 0.01) {
		t.Errorf("Top = %f, want 500", tb.Top)
	}
	if !approxEqual(tb.High, 100, 0.01) {
		t.Errorf("High = %f, want 100", tb.High)
	}
	if !approxEqual(tb.Mid, 10, 0.01) {
		t.Errorf("Mid = %f, want 10", tb.Mid)
	}
}

func TestDetectTierBoundaries_CheapGemsExcluded(t *testing.T) {
	// Verify that sub-5c gems (which the analysis pipeline skips) don't affect
	// tier boundaries. Previously Chaos > 0 was used, causing dozens of 1-5c
	// transfigured gems to create a large artificial gap that skewed boundaries.
	gemsWithCheap := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true},
		{Name: "E of X", Variant: "20/20", Chaos: 30, IsTransfigured: true},
		// Cheap transfigured gems that should be excluded:
		{Name: "F of X", Variant: "20/20", Chaos: 4, IsTransfigured: true},
		{Name: "G of X", Variant: "20/20", Chaos: 3, IsTransfigured: true},
		{Name: "H of X", Variant: "20/20", Chaos: 2, IsTransfigured: true},
		{Name: "I of X", Variant: "20/20", Chaos: 1, IsTransfigured: true},
	}

	gemsWithout := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true},
		{Name: "E of X", Variant: "20/20", Chaos: 30, IsTransfigured: true},
	}

	tbWith := DetectTierBoundaries(gemsWithCheap)
	tbWithout := DetectTierBoundaries(gemsWithout)

	// Boundaries should be identical — cheap gems are excluded.
	if !approxEqual(tbWith.Top, tbWithout.Top, 0.01) {
		t.Errorf("Top with cheap gems = %f, without = %f — cheap gems should not affect boundaries", tbWith.Top, tbWithout.Top)
	}
	if !approxEqual(tbWith.High, tbWithout.High, 0.01) {
		t.Errorf("High with cheap gems = %f, without = %f — cheap gems should not affect boundaries", tbWith.High, tbWithout.High)
	}
	if !approxEqual(tbWith.Mid, tbWithout.Mid, 0.01) {
		t.Errorf("Mid with cheap gems = %f, without = %f — cheap gems should not affect boundaries", tbWith.Mid, tbWithout.Mid)
	}
}
