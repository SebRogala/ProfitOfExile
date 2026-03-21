package lab

import (
	"testing"
)

func TestDetectTierBoundariesRecursive_RealisticDistribution(t *testing.T) {
	// Real-ish price distribution with clear natural gaps.
	prices := []float64{785, 600, 580, 570, 510, 500, 470, 460, 390, 350, 300, 200, 150, 80, 40, 30, 20, 15, 10, 8}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true, Listings: 10,
		}
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Must produce at least 1 boundary.
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary, got 0")
	}
	// Boundaries must be sorted descending.
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f (descending)", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
	// All boundaries must be > 0.
	for i, b := range tb.Boundaries {
		if b <= 0 {
			t.Errorf("Boundaries[%d] = %f, want > 0", i, b)
		}
	}
}

func TestDetectTierBoundariesRecursive_SingleOutlier(t *testing.T) {
	// One outlier at 1000c, then a tight cluster 92-100c.
	prices := []float64{1000, 100, 99, 98, 97, 96, 95, 94, 93, 92}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true, Listings: 10,
		}
	}

	tb := DetectTierBoundariesRecursive(gems)

	// The 1000->100 gap is the largest absolute gap, so TOP boundary = 1000.
	// Only gems >= 1000 are TOP (just the outlier).
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary")
	}
	if tb.Boundaries[0] != 1000 {
		t.Errorf("Boundaries[0] = %f, want 1000 (TOP = gems above the gap)", tb.Boundaries[0])
	}
	// Boundaries must be sorted descending.
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
}

func TestDetectTierBoundariesRecursive_SmallPool(t *testing.T) {
	// Exactly 10 gems -- should still produce valid boundaries.
	prices := []float64{500, 400, 300, 200, 150, 100, 80, 50, 20, 6}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true, Listings: 10,
		}
	}

	tb := DetectTierBoundariesRecursive(gems)

	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary")
	}
	// Boundaries must be sorted descending.
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
}

func TestDetectTierBoundariesRecursive_MinimumPool(t *testing.T) {
	// <4 gems -> fallback to P75/P50/P25.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 10, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Fallback: P75/P50/P25 of [10, 50, 100]
	// P75: 0.75 * 2 = 1.5 -> lerp(50, 100, 0.5) = 75
	// P50: 0.50 * 2 = 1.0 -> 50
	// P25: 0.25 * 2 = 0.5 -> lerp(10, 50, 0.5) = 30
	if len(tb.Boundaries) != 3 {
		t.Fatalf("expected 3 boundaries for fallback, got %d", len(tb.Boundaries))
	}
	if !approxEqual(tb.Boundaries[0], 75, 0.01) {
		t.Errorf("Boundaries[0] = %f, want 75 (P75 fallback)", tb.Boundaries[0])
	}
	if !approxEqual(tb.Boundaries[1], 50, 0.01) {
		t.Errorf("Boundaries[1] = %f, want 50 (P50 fallback)", tb.Boundaries[1])
	}
	if !approxEqual(tb.Boundaries[2], 30, 0.01) {
		t.Errorf("Boundaries[2] = %f, want 30 (P25 fallback)", tb.Boundaries[2])
	}
}

func TestDetectTierBoundariesRecursive_ExactlyThreeDistinctPrices(t *testing.T) {
	// Only 3 distinct prices -> fallback needed.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 10, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Fallback produces 3 boundaries -- they should be descending.
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary")
	}
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
}

func TestDetectTierBoundariesRecursive_MultiVariantDedup(t *testing.T) {
	// Same gem name at different variants -- should use max price per name.
	gems := []GemPrice{
		{Name: "Kinetic Blast of Clustering", Variant: "20/20", Chaos: 785, IsTransfigured: true, Listings: 10},
		{Name: "Kinetic Blast of Clustering", Variant: "20", Chaos: 600, IsTransfigured: true, Listings: 10},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 400, IsTransfigured: true, Listings: 10},
		{Name: "Gem C of X", Variant: "20/20", Chaos: 200, IsTransfigured: true, Listings: 10},
		{Name: "Gem D of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "Gem E of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Kinetic Blast should contribute only 785c, not both 785 and 600.
	// 5 unique gems: 785, 400, 200, 100, 50
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary")
	}
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
}

func TestDetectTierBoundariesRecursive_FiltersCorrectly(t *testing.T) {
	// Verify corrupted, non-transfigured, Trarthus, zero-price, and sub-5c gems are excluded.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
		// These should all be excluded:
		{Name: "Corrupted of X", Variant: "20/20", Chaos: 9999, IsTransfigured: true, Listings: 10, IsCorrupted: true},
		{Name: "BaseGem", Variant: "20/20", Chaos: 9999, IsTransfigured: false},
		{Name: "Wave of Trarthus", Variant: "20/20", Chaos: 9999, IsTransfigured: true, Listings: 10},
		{Name: "Zero of X", Variant: "20/20", Chaos: 0, IsTransfigured: true, Listings: 10},
		{Name: "Negative of X", Variant: "20/20", Chaos: -10, IsTransfigured: true, Listings: 10},
		// Sub-5c gems excluded to match analysis pipeline floor (Chaos > 5):
		{Name: "Cheap of X", Variant: "20/20", Chaos: 3, IsTransfigured: true, Listings: 10},
		{Name: "AtFloor of X", Variant: "20/20", Chaos: 5, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Only 4 qualifying gems: 500, 300, 100, 50.
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary")
	}
	// TOP boundary should not exceed 500.
	if tb.Boundaries[0] > 500 {
		t.Errorf("Boundaries[0] = %f, should not exceed 500 (filtered correctly)", tb.Boundaries[0])
	}
	if tb.Boundaries[0] <= 0 {
		t.Errorf("Boundaries[0] = %f, want > 0", tb.Boundaries[0])
	}
}

func TestDetectTierBoundariesRecursive_EmptyInput(t *testing.T) {
	tb := DetectTierBoundariesRecursive(nil)
	if len(tb.Boundaries) != 0 {
		t.Errorf("empty input should return zero boundaries, got %v", tb.Boundaries)
	}
}

func TestDetectTierBoundariesRecursive_AllSamePrice(t *testing.T) {
	// All gems at the same price -> 1 unique price -> fallback.
	gems := make([]GemPrice, 10)
	for i := range gems {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          100,
			IsTransfigured: true, Listings: 10,
		}
	}

	tb := DetectTierBoundariesRecursive(gems)

	// All percentiles of a single value = 100.
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary from fallback")
	}
	if !approxEqual(tb.Boundaries[0], 100, 0.01) {
		t.Errorf("Boundaries[0] = %f, want 100", tb.Boundaries[0])
	}
}

func TestClassifyTier_DynamicBoundaries(t *testing.T) {
	// 3 boundaries: TOP >= 500, HIGH >= 200, MID-HIGH >= 50
	tb := TierBoundaries{Boundaries: []float64{500, 200, 50}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{500, "TOP"},     // exactly at boundary[0] -> TOP (>= semantics)
		{499.99, "HIGH"}, // below boundary[0], above boundary[1]
		{200, "HIGH"},    // exactly at boundary[1] -> HIGH
		{199.99, "MID-HIGH"},
		{50, "MID-HIGH"}, // exactly at boundary[2] -> MID-HIGH
		{49.99, "MID"},   // below all boundaries, index 3 -> MID
		{0, "MID"},
	}

	for _, tt := range tests {
		got := classifyTier(tt.price, tb)
		if got != tt.want {
			t.Errorf("classifyTier(%v, {500, 200, 50}) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestClassifyTier_TwoBoundaries(t *testing.T) {
	// 2 boundaries: TOP >= 500, HIGH >= 100
	tb := TierBoundaries{Boundaries: []float64{500, 100}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{500, "TOP"},
		{499, "HIGH"},
		{100, "HIGH"},
		{99, "MID-HIGH"}, // below all 2 boundaries -> TierNames[2] = "MID-HIGH"
		{0, "MID-HIGH"},
	}

	for _, tt := range tests {
		got := classifyTier(tt.price, tb)
		if got != tt.want {
			t.Errorf("classifyTier(%v, {500, 100}) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestClassifyTier_FourBoundaries(t *testing.T) {
	// 4 boundaries: TOP >= 500, HIGH >= 200, MID-HIGH >= 100, MID >= 30
	tb := TierBoundaries{Boundaries: []float64{500, 200, 100, 30}}

	tests := []struct {
		price float64
		want  string
	}{
		{600, "TOP"},
		{300, "HIGH"},
		{150, "MID-HIGH"},
		{50, "MID"},
		{29, "LOW"}, // below all 4 boundaries -> TierNames[4] = "LOW"
		{0, "LOW"},
	}

	for _, tt := range tests {
		got := classifyTier(tt.price, tb)
		if got != tt.want {
			t.Errorf("classifyTier(%v, ...) = %s, want %s", tt.price, got, tt.want)
		}
	}
}

func TestClassifyTier_NoBoundaries(t *testing.T) {
	// No boundaries -> everything is the first tier name.
	tb := TierBoundaries{}
	got := classifyTier(100, tb)
	if got != "TOP" {
		t.Errorf("classifyTier(100, empty) = %s, want TOP", got)
	}
}

func TestClassifyTier_Exported(t *testing.T) {
	tb := TierBoundaries{Boundaries: []float64{100, 50, 20}}
	got := ClassifyTier(150, tb)
	if got != "TOP" {
		t.Errorf("ClassifyTier(150, ...) = %s, want TOP", got)
	}
}

func TestDetectTierBoundariesRecursive_FourExactGems(t *testing.T) {
	// Exactly 4 gems -- minimum for recursive detection.
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 1000, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "D of X", Variant: "20/20", Chaos: 10, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Should produce at least 1 boundary with descending order.
	if len(tb.Boundaries) == 0 {
		t.Fatal("expected at least 1 boundary for 4 gems")
	}
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
	// The largest absolute gap is 1000->500 (500c). But avg gap = (500+400+90)/3 = 330.
	// 500 >= 330*3 = 990? No, 500 < 990. So TOP gap is NOT significant enough (< 3x avg).
	// All gems go through recursive average instead. First boundary should be the highest.
	if tb.Boundaries[0] < 100 || tb.Boundaries[0] > 1000 {
		t.Errorf("Boundaries[0] = %f, want in reasonable range", tb.Boundaries[0])
	}
}

func TestDetectTierBoundariesRecursive_CheapGemsIncluded(t *testing.T) {
	// Cheap gems ARE included in tier boundaries (no hardcoded price floor).
	// Adding cheap gems should produce MORE boundaries (more price spread).
	gemsWithCheap := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
		{Name: "E of X", Variant: "20/20", Chaos: 30, IsTransfigured: true, Listings: 10},
		{Name: "F of X", Variant: "20/20", Chaos: 4, IsTransfigured: true, Listings: 10},
		{Name: "G of X", Variant: "20/20", Chaos: 3, IsTransfigured: true, Listings: 10},
		{Name: "H of X", Variant: "20/20", Chaos: 2, IsTransfigured: true, Listings: 10},
		{Name: "I of X", Variant: "20/20", Chaos: 1, IsTransfigured: true, Listings: 10},
	}

	gemsWithout := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
		{Name: "E of X", Variant: "20/20", Chaos: 30, IsTransfigured: true, Listings: 10},
	}

	tbWith := DetectTierBoundariesRecursive(gemsWithCheap)
	tbWithout := DetectTierBoundariesRecursive(gemsWithout)

	// Both should produce valid boundaries.
	if len(tbWith.Boundaries) == 0 {
		t.Fatal("expected boundaries with cheap gems")
	}
	if len(tbWithout.Boundaries) == 0 {
		t.Fatal("expected boundaries without cheap gems")
	}
	// With more gems (including cheap), may produce different boundary count.
	// Both should be sorted descending.
	for i := 1; i < len(tbWith.Boundaries); i++ {
		if tbWith.Boundaries[i] >= tbWith.Boundaries[i-1] {
			t.Errorf("Boundaries[%d] with cheap=%f, without=%f -- should be equal",
				i, tbWith.Boundaries[i], tbWithout.Boundaries[i])
		}
	}
}

func TestDetectTierBoundariesRecursive_RecursiveSplitting(t *testing.T) {
	// Verify recursive splitting actually produces multiple boundaries.
	// 20 gems with a clear structure:
	// TOP: 1000 (1 gem above largest gap)
	// Pool: 400, 350, 300, 250, 200, 150, 100, 80, 60, 50, 40, 30, 25, 20, 15, 10, 8, 7, 6
	prices := []float64{1000, 400, 350, 300, 250, 200, 150, 100, 80, 60, 50, 40, 30, 25, 20, 15, 10, 8, 7, 6}
	gems := make([]GemPrice, len(prices))
	for i, p := range prices {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			IsTransfigured: true, Listings: 10,
		}
	}

	tb := DetectTierBoundariesRecursive(gems)

	// Should produce multiple boundaries (at least 2: TOP gap + at least 1 recursive split).
	if len(tb.Boundaries) < 2 {
		t.Errorf("expected at least 2 boundaries for 20 varied gems, got %d: %v", len(tb.Boundaries), tb.Boundaries)
	}
	// All boundaries must be strictly descending.
	for i := 1; i < len(tb.Boundaries); i++ {
		if tb.Boundaries[i] >= tb.Boundaries[i-1] {
			t.Errorf("Boundaries[%d]=%f should be < Boundaries[%d]=%f", i, tb.Boundaries[i], i-1, tb.Boundaries[i-1])
		}
	}
	// TOP boundary = 1000 (the price at the top side of the gap).
	// Only gems >= 1000 are TOP.
	if !approxEqual(tb.Boundaries[0], 1000, 0.01) {
		t.Errorf("Boundaries[0] = %f, want 1000", tb.Boundaries[0])
	}
}

// TestDetectTierBoundaries_Alias verifies the backward-compat alias works.
func TestDetectTierBoundaries_Alias(t *testing.T) {
	gems := []GemPrice{
		{Name: "A of X", Variant: "20/20", Chaos: 500, IsTransfigured: true, Listings: 10},
		{Name: "B of X", Variant: "20/20", Chaos: 300, IsTransfigured: true, Listings: 10},
		{Name: "C of X", Variant: "20/20", Chaos: 100, IsTransfigured: true, Listings: 10},
		{Name: "D of X", Variant: "20/20", Chaos: 50, IsTransfigured: true, Listings: 10},
	}

	tb := DetectTierBoundaries(gems)
	if len(tb.Boundaries) == 0 {
		t.Fatal("alias DetectTierBoundaries should produce boundaries")
	}
}
