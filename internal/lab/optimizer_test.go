package lab

import (
	"testing"
	"time"
)

func TestBuildEvalPoints_ExactMatch(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0, Name: "Ice Nova", Variant: "1", Chaos: 200},
	}

	prices := []SnapshotPrice{
		// Baseline prices at feature time
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0, Name: "Ice Nova", Variant: "1", Chaos: 200},
		// Future prices at exactly t0 + 2h
		{Time: t0.Add(horizon), Name: "Spark", Variant: "1", Chaos: 110},
		{Time: t0.Add(horizon), Name: "Ice Nova", Variant: "1", Chaos: 180},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 0 {
		t.Errorf("expected 0 dropped, got %d", dropped)
	}
	if len(points) != 2 {
		t.Fatalf("expected 2 eval points, got %d", len(points))
	}

	// Spark: (110-100)/100 * 100 = 10%
	if got := points[0].FuturePct; !approxEqual(got, 10.0, 0.01) {
		t.Errorf("Spark futurePct: got %.2f, want 10.0", got)
	}
	// Ice Nova: (180-200)/200 * 100 = -10%
	if got := points[1].FuturePct; !approxEqual(got, -10.0, 0.01) {
		t.Errorf("Ice Nova futurePct: got %.2f, want -10.0", got)
	}
}

func TestBuildEvalPoints_NoFuturePrice(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
	}

	// No future prices at all
	prices := []SnapshotPrice{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 1 {
		t.Errorf("expected 1 dropped, got %d", dropped)
	}
	if len(points) != 0 {
		t.Errorf("expected 0 eval points, got %d", len(points))
	}
}

func TestBuildEvalPoints_EdgeOfRange(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
	}

	// Price just beyond the 30min tolerance window (31 minutes late)
	prices := []SnapshotPrice{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0.Add(horizon).Add(31 * time.Minute), Name: "Spark", Variant: "1", Chaos: 120},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 1 {
		t.Errorf("expected 1 dropped (out of tolerance), got %d", dropped)
	}
	if len(points) != 0 {
		t.Errorf("expected 0 eval points, got %d", len(points))
	}
}

func TestBuildEvalPoints_WithinTolerance(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
	}

	// Price 20 minutes late (within 30min tolerance)
	prices := []SnapshotPrice{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0.Add(horizon).Add(20 * time.Minute), Name: "Spark", Variant: "1", Chaos: 115},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 0 {
		t.Errorf("expected 0 dropped, got %d", dropped)
	}
	if len(points) != 1 {
		t.Fatalf("expected 1 eval point, got %d", len(points))
	}
	// (115-100)/100 * 100 = 15%
	if got := points[0].FuturePct; !approxEqual(got, 15.0, 0.01) {
		t.Errorf("futurePct: got %.2f, want 15.0", got)
	}
}

func TestBuildEvalPoints_PicksClosestWithinWindow(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
	}

	// Two prices in window — one 25min early, one 5min late. Should pick the closer one (5min late).
	prices := []SnapshotPrice{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0.Add(horizon).Add(-25 * time.Minute), Name: "Spark", Variant: "1", Chaos: 90},
		{Time: t0.Add(horizon).Add(5 * time.Minute), Name: "Spark", Variant: "1", Chaos: 130},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 0 {
		t.Errorf("expected 0 dropped, got %d", dropped)
	}
	if len(points) != 1 {
		t.Fatalf("expected 1 eval point, got %d", len(points))
	}
	// Should use the 130 chaos (5min late, closer to target)
	// (130-100)/100 * 100 = 30%
	if got := points[0].FuturePct; !approxEqual(got, 30.0, 0.01) {
		t.Errorf("futurePct: got %.2f, want 30.0", got)
	}
}

func TestBuildEvalPoints_MultipleGemsIndependent(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0, Name: "Ice Nova", Variant: "1", Chaos: 50},
		{Time: t0, Name: "Arc", Variant: "1", Chaos: 200},
	}

	prices := []SnapshotPrice{
		// Baselines
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0, Name: "Ice Nova", Variant: "1", Chaos: 50},
		{Time: t0, Name: "Arc", Variant: "1", Chaos: 200},
		// Futures — Spark has future, Ice Nova does not, Arc has future
		{Time: t0.Add(horizon), Name: "Spark", Variant: "1", Chaos: 120},
		{Time: t0.Add(horizon), Name: "Arc", Variant: "1", Chaos: 160},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 1 {
		t.Errorf("expected 1 dropped (Ice Nova), got %d", dropped)
	}
	if len(points) != 2 {
		t.Fatalf("expected 2 eval points, got %d", len(points))
	}
}

func TestBuildEvalPoints_UsesSnapshotBaseline(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	// Feature Chaos differs from snapshot price at same time.
	// Should use snapshot price (100) as baseline, not feature Chaos (95).
	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 95},
	}

	prices := []SnapshotPrice{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 100},
		{Time: t0.Add(horizon), Name: "Spark", Variant: "1", Chaos: 120},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 0 {
		t.Errorf("expected 0 dropped, got %d", dropped)
	}
	if len(points) != 1 {
		t.Fatalf("expected 1 eval point, got %d", len(points))
	}
	// (120-100)/100 * 100 = 20% (based on snapshot price, not feature Chaos)
	if got := points[0].FuturePct; !approxEqual(got, 20.0, 0.01) {
		t.Errorf("futurePct: got %.2f, want 20.0 (should use snapshot baseline)", got)
	}
}

func TestBuildEvalPoints_ZeroBaselineDropped(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	// Feature with zero chaos and no snapshot baseline
	features := []GemFeature{
		{Time: t0, Name: "Spark", Variant: "1", Chaos: 0},
	}

	prices := []SnapshotPrice{
		{Time: t0.Add(horizon), Name: "Spark", Variant: "1", Chaos: 120},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 1 {
		t.Errorf("expected 1 dropped (zero baseline), got %d", dropped)
	}
	if len(points) != 0 {
		t.Errorf("expected 0 eval points, got %d", len(points))
	}
}

func TestBuildEvalPoints_EmptyInputs(t *testing.T) {
	horizon := 2 * time.Hour

	points, dropped := BuildEvalPoints(nil, nil, horizon)

	if dropped != 0 {
		t.Errorf("expected 0 dropped, got %d", dropped)
	}
	if len(points) != 0 {
		t.Errorf("expected 0 eval points, got %d", len(points))
	}
}

func TestBuildEvalPoints_DroppedCountDiagnostics(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	horizon := 2 * time.Hour

	// 5 features, only 2 have matching future prices
	features := []GemFeature{
		{Time: t0, Name: "A", Variant: "1", Chaos: 100},
		{Time: t0, Name: "B", Variant: "1", Chaos: 100},
		{Time: t0, Name: "C", Variant: "1", Chaos: 100},
		{Time: t0, Name: "D", Variant: "1", Chaos: 100},
		{Time: t0, Name: "E", Variant: "1", Chaos: 100},
	}

	prices := []SnapshotPrice{
		{Time: t0, Name: "A", Variant: "1", Chaos: 100},
		{Time: t0, Name: "B", Variant: "1", Chaos: 100},
		{Time: t0.Add(horizon), Name: "A", Variant: "1", Chaos: 120},
		{Time: t0.Add(horizon), Name: "B", Variant: "1", Chaos: 80},
	}

	points, dropped := BuildEvalPoints(features, prices, horizon)

	if dropped != 3 {
		t.Errorf("expected 3 dropped, got %d", dropped)
	}
	if len(points) != 2 {
		t.Errorf("expected 2 eval points, got %d", len(points))
	}
}

func TestToSignalConfig(t *testing.T) {
	mc := MarketContext{
		VelocityMean:    5,
		VelocitySigma:   10,
		ListingVelMean:  2,
		ListingVelSigma: 4,
	}

	sc := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	cfg := sc.ToSignalConfig(mc)

	// PreHERDPriceVel = VelocityMean + HERDPriceMult * VelocitySigma = 5 + 2.0 * 10 = 25
	if got := cfg.PreHERDPriceVel; !approxEqual(got, 25.0, 0.01) {
		t.Errorf("PreHERDPriceVel: got %.2f, want 25.0", got)
	}

	// PreHERDListingVel = ListingVelMean + HERDListingMult * ListingVelSigma = 2 + 1.5 * 4 = 8
	if got := cfg.PreHERDListingVel; !approxEqual(got, 8.0, 0.01) {
		t.Errorf("PreHERDListingVel: got %.2f, want 8.0", got)
	}

	// StablePriceVel = StablePriceMult * VelocitySigma = 0.5 * 10 = 5
	if got := cfg.StablePriceVel; !approxEqual(got, 5.0, 0.01) {
		t.Errorf("StablePriceVel: got %.2f, want 5.0", got)
	}

	// BrewingMinPVel = BrewingPriceMult * VelocitySigma = 1.0 * 10 = 10
	if got := cfg.BrewingMinPVel; !approxEqual(got, 10.0, 0.01) {
		t.Errorf("BrewingMinPVel: got %.2f, want 10.0", got)
	}

	// DumpPriceVel = -(DumpPriceMult * VelocitySigma) = -(2.0 * 10) = -20
	if got := cfg.DumpPriceVel; !approxEqual(got, -20.0, 0.01) {
		t.Errorf("DumpPriceVel: got %.2f, want -20.0", got)
	}
}

func TestToSignalConfig_PreservesDefaults(t *testing.T) {
	mc := MarketContext{
		VelocityMean:    5,
		VelocitySigma:   10,
		ListingVelMean:  2,
		ListingVelSigma: 4,
	}

	sc := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	cfg := sc.ToSignalConfig(mc)
	defaults := DefaultSignalConfig()

	// Non-swept fields must retain default values.
	if cfg.HERDPriceVel != defaults.HERDPriceVel {
		t.Errorf("HERDPriceVel: got %.2f, want default %.2f", cfg.HERDPriceVel, defaults.HERDPriceVel)
	}
	if cfg.HERDListingVel != defaults.HERDListingVel {
		t.Errorf("HERDListingVel: got %.2f, want default %.2f", cfg.HERDListingVel, defaults.HERDListingVel)
	}
	if cfg.StableListingVel != defaults.StableListingVel {
		t.Errorf("StableListingVel: got %.2f, want default %.2f", cfg.StableListingVel, defaults.StableListingVel)
	}
	if cfg.RecoveryMaxList != defaults.RecoveryMaxList {
		t.Errorf("RecoveryMaxList: got %d, want default %d", cfg.RecoveryMaxList, defaults.RecoveryMaxList)
	}
	if cfg.TrapCV != defaults.TrapCV {
		t.Errorf("TrapCV: got %.2f, want default %.2f", cfg.TrapCV, defaults.TrapCV)
	}
}

func TestGenerateSigmaGrid(t *testing.T) {
	grid := GenerateSigmaGrid()

	// Expected size: 5 * 4 * 4 * 4 = 320 combos.
	if got := len(grid); got != 320 {
		t.Fatalf("grid size: got %d, want 320", got)
	}

	// Verify all values are within expected ranges.
	for i, sc := range grid {
		if sc.HERDPriceMult < 1.5 || sc.HERDPriceMult > 3.5 {
			t.Errorf("grid[%d].HERDPriceMult=%.1f out of range [1.5, 3.5]", i, sc.HERDPriceMult)
		}
		if sc.HERDListingMult < 1.0 || sc.HERDListingMult > 2.5 {
			t.Errorf("grid[%d].HERDListingMult=%.1f out of range [1.0, 2.5]", i, sc.HERDListingMult)
		}
		if sc.StablePriceMult < 0.3 || sc.StablePriceMult > 1.0 {
			t.Errorf("grid[%d].StablePriceMult=%.1f out of range [0.3, 1.0]", i, sc.StablePriceMult)
		}
		if sc.BrewingPriceMult < 0.5 || sc.BrewingPriceMult > 2.0 {
			t.Errorf("grid[%d].BrewingPriceMult=%.1f out of range [0.5, 2.0]", i, sc.BrewingPriceMult)
		}
		if sc.DumpPriceMult != 2.0 {
			t.Errorf("grid[%d].DumpPriceMult=%.1f, want fixed 2.0", i, sc.DumpPriceMult)
		}
	}
}

func TestGenerateSigmaGrid_NoDuplicates(t *testing.T) {
	grid := GenerateSigmaGrid()

	seen := make(map[SigmaConfig]bool, len(grid))
	for _, sc := range grid {
		if seen[sc] {
			t.Errorf("duplicate SigmaConfig: %+v", sc)
		}
		seen[sc] = true
	}
}

