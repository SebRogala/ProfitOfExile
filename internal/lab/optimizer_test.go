package lab

import (
	"testing"
	"time"
)

// sweepMarketContext returns a MarketContext with custom velocity stats and
// properly initialized temporal slices (neutral 1.0 biases) for sweep tests.
func sweepMarketContext(velMean, velSigma, listVelMean, listVelSigma float64) MarketContext {
	mc := testMarketContext()
	mc.VelocityMean = velMean
	mc.VelocitySigma = velSigma
	mc.ListingVelMean = listVelMean
	mc.ListingVelSigma = listVelSigma
	// Set neutral biases (1.0) for predictable confidence scoring.
	for i := range mc.HourlyBias {
		mc.HourlyBias[i] = 1.0
	}
	for i := range mc.WeekdayBias {
		mc.WeekdayBias[i] = 1.0
	}
	return mc
}

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
		VelocityMean:     5,
		VelocitySigma:    10,
		ListingVelMean:   2,
		ListingVelSigma:  4,
		PricePercentiles: map[string]float64{"P50": 100},
	}

	sc := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	cfg := sc.ToSignalConfig(mc)

	// PreHERDPriceVelPct = (VelocityMean + HERDPriceMult * VelocitySigma) / P50 * 100
	//                    = (5 + 2.0 * 10) / 100 * 100 = 25%
	if got := cfg.PreHERDPriceVelPct; !approxEqual(got, 25.0, 0.01) {
		t.Errorf("PreHERDPriceVelPct: got %.2f, want 25.0", got)
	}

	// PreHERDListingVelPct = (ListingVelMean + HERDListingMult * ListingVelSigma) * 100 / 50
	//                      = (2 + 1.5 * 4) * 100 / 50 = 16%
	if got := cfg.PreHERDListingVelPct; !approxEqual(got, 16.0, 0.01) {
		t.Errorf("PreHERDListingVelPct: got %.2f, want 16.0", got)
	}

	// StablePriceVelPct = StablePriceMult * VelocitySigma / P50 * 100
	//                   = 0.5 * 10 / 100 * 100 = 5%
	if got := cfg.StablePriceVelPct; !approxEqual(got, 5.0, 0.01) {
		t.Errorf("StablePriceVelPct: got %.2f, want 5.0", got)
	}

	// BrewingMinPVel stays absolute = BrewingPriceMult * VelocitySigma = 1.0 * 10 = 10
	if got := cfg.BrewingMinPVel; !approxEqual(got, 10.0, 0.01) {
		t.Errorf("BrewingMinPVel: got %.2f, want 10.0", got)
	}

	// DumpPriceVelPct = -(DumpPriceMult * VelocitySigma) / P50 * 100
	//                 = -(2.0 * 10) / 100 * 100 = -20%
	if got := cfg.DumpPriceVelPct; !approxEqual(got, -20.0, 0.01) {
		t.Errorf("DumpPriceVelPct: got %.2f, want -20.0", got)
	}
}

func TestToSignalConfig_PreservesDefaults(t *testing.T) {
	mc := MarketContext{
		VelocityMean:     5,
		VelocitySigma:    10,
		ListingVelMean:   2,
		ListingVelSigma:  4,
		PricePercentiles: map[string]float64{"P50": 100},
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
	if cfg.HERDPriceVelPct != defaults.HERDPriceVelPct {
		t.Errorf("HERDPriceVelPct: got %.2f, want default %.2f", cfg.HERDPriceVelPct, defaults.HERDPriceVelPct)
	}
	if cfg.HERDListingVelPct != defaults.HERDListingVelPct {
		t.Errorf("HERDListingVelPct: got %.2f, want default %.2f", cfg.HERDListingVelPct, defaults.HERDListingVelPct)
	}
	if cfg.StableListingVelPct != defaults.StableListingVelPct {
		t.Errorf("StableListingVelPct: got %.2f, want default %.2f", cfg.StableListingVelPct, defaults.StableListingVelPct)
	}
	if cfg.DumpListingVelPct != defaults.DumpListingVelPct {
		t.Errorf("DumpListingVelPct: got %.2f, want default %.2f", cfg.DumpListingVelPct, defaults.DumpListingVelPct)
	}
	if cfg.RecoveryMaxListings != defaults.RecoveryMaxListings {
		t.Errorf("RecoveryMaxListings: got %d, want default %d", cfg.RecoveryMaxListings, defaults.RecoveryMaxListings)
	}
	if cfg.OpenMinPVel != defaults.OpenMinPVel {
		t.Errorf("OpenMinPVel: got %.2f, want default %.2f", cfg.OpenMinPVel, defaults.OpenMinPVel)
	}
	if cfg.DrainPct != defaults.DrainPct {
		t.Errorf("DrainPct: got %.4f, want default %.4f", cfg.DrainPct, defaults.DrainPct)
	}
	if cfg.ThinPoolFloor != defaults.ThinPoolFloor {
		t.Errorf("ThinPoolFloor: got %.2f, want default %.2f", cfg.ThinPoolFloor, defaults.ThinPoolFloor)
	}
	if cfg.NormalFloor != defaults.NormalFloor {
		t.Errorf("NormalFloor: got %.2f, want default %.2f", cfg.NormalFloor, defaults.NormalFloor)
	}
	if cfg.BreakoutMaxPrice != defaults.BreakoutMaxPrice {
		t.Errorf("BreakoutMaxPrice: got %.2f, want default %.2f", cfg.BreakoutMaxPrice, defaults.BreakoutMaxPrice)
	}
	if cfg.BreakoutMaxList != defaults.BreakoutMaxList {
		t.Errorf("BreakoutMaxList: got %d, want default %d", cfg.BreakoutMaxList, defaults.BreakoutMaxList)
	}
	if cfg.BreakoutMinLVel != defaults.BreakoutMinLVel {
		t.Errorf("BreakoutMinLVel: got %.2f, want default %.2f", cfg.BreakoutMinLVel, defaults.BreakoutMinLVel)
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

func TestPredictedDirection(t *testing.T) {
	tests := []struct {
		signal string
		want   string
	}{
		{"HERD", "UP"},
		{"RECOVERY", "UP"},
		{"DUMPING", "DOWN"},
		{"TRAP", "DOWN"},
		{"STABLE", "FLAT"},
		{"UNCERTAIN", "FLAT"},
		{"UNKNOWN", "FLAT"},
		{"", "FLAT"},
	}

	for _, tt := range tests {
		got := predictedDirection(tt.signal)
		if got != tt.want {
			t.Errorf("predictedDirection(%q) = %q, want %q", tt.signal, got, tt.want)
		}
	}
}

func TestDirectionFromChange(t *testing.T) {
	tests := []struct {
		pctChange float64
		want      string
	}{
		{5.0, "UP"},
		{2.01, "UP"},
		{2.0, "FLAT"},
		{0.0, "FLAT"},
		{-2.0, "FLAT"},
		{-2.01, "DOWN"},
		{-10.0, "DOWN"},
	}

	for _, tt := range tests {
		got := directionFromChange(tt.pctChange)
		if got != tt.want {
			t.Errorf("directionFromChange(%.2f) = %q, want %q", tt.pctChange, got, tt.want)
		}
	}
}

func TestTierWeight(t *testing.T) {
	tests := []struct {
		tier string
		want float64
	}{
		{"TOP", 2.0},
		{"HIGH", 1.5},
		{"MID", 1.0},
		{"LOW", 0.5},
		{"", 1.0},
		{"UNKNOWN", 1.0},
	}

	for _, tt := range tests {
		got := tierWeight(tt.tier)
		if got != tt.want {
			t.Errorf("tierWeight(%q) = %.1f, want %.1f", tt.tier, got, tt.want)
		}
	}
}

func TestClassifyTemporalPhase(t *testing.T) {
	tests := []struct {
		name string
		t    time.Time
		want string
	}{
		{
			name: "Saturday is weekend",
			t:    time.Date(2026, 3, 14, 16, 0, 0, 0, time.UTC), // Saturday
			want: "weekend",
		},
		{
			name: "Sunday is weekend",
			t:    time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC), // Sunday
			want: "weekend",
		},
		{
			name: "Monday 16:00 UTC is weekday-peak",
			t:    time.Date(2026, 3, 16, 16, 0, 0, 0, time.UTC), // Monday
			want: "weekday-peak",
		},
		{
			name: "Wednesday 14:00 UTC is weekday-peak (boundary)",
			t:    time.Date(2026, 3, 18, 14, 0, 0, 0, time.UTC), // Wednesday
			want: "weekday-peak",
		},
		{
			name: "Thursday 21:59 UTC is weekday-peak (end boundary)",
			t:    time.Date(2026, 3, 19, 21, 59, 0, 0, time.UTC), // Thursday
			want: "weekday-peak",
		},
		{
			name: "Friday 22:00 UTC is weekday-offpeak (past peak)",
			t:    time.Date(2026, 3, 20, 22, 0, 0, 0, time.UTC), // Friday
			want: "weekday-offpeak",
		},
		{
			name: "Tuesday 08:00 UTC is weekday-offpeak",
			t:    time.Date(2026, 3, 17, 8, 0, 0, 0, time.UTC), // Tuesday
			want: "weekday-offpeak",
		},
		{
			name: "Monday 00:00 UTC is weekday-offpeak",
			t:    time.Date(2026, 3, 16, 0, 0, 0, 0, time.UTC), // Monday
			want: "weekday-offpeak",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifyTemporalPhase(tt.t)
			if got != tt.want {
				t.Errorf("classifyTemporalPhase(%v) = %q, want %q", tt.t, got, tt.want)
			}
		})
	}
}

func TestSweepV2_SingleCombo(t *testing.T) {
	// Wednesday 16:00 UTC = weekday-peak
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// 5 eval points with known outcomes.
	// Use STABLE-triggering velocities (low abs values) so predictions are FLAT.
	// FuturePct near 0 = FLAT = correct, large positive/negative = wrong.
	sigma := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	evals := []EvalPoint{
		// STABLE signal (low vel), FLAT actual → correct
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.5, VelMedListing: 0.5, CV: 0.1, Listings: 10, Tier: "TOP"}, FuturePct: 0.5, SnapTime: t0},
		// STABLE signal, FLAT actual → correct
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.3, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "HIGH"}, FuturePct: -1.0, SnapTime: t0},
		// STABLE signal, UP actual → wrong
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.2, VelMedListing: -0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 5.0, SnapTime: t0},
		// STABLE signal, DOWN actual → wrong
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.4, VelMedListing: 0.3, CV: 0.1, Listings: 10, Tier: "LOW"}, FuturePct: -5.0, SnapTime: t0},
		// STABLE signal, FLAT actual → correct
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 1.5, SnapTime: t0},
	}

	results := SweepV2(evals, mc, []SigmaConfig{sigma})

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}
	r := results[0]

	if r.TotalEvals != 5 {
		t.Errorf("TotalEvals: got %d, want 5", r.TotalEvals)
	}

	// Overall: 3 correct out of 5 = 60%
	if !approxEqual(r.OverallAcc, 60.0, 0.1) {
		t.Errorf("OverallAcc: got %.2f, want 60.0", r.OverallAcc)
	}

	// TOP: 1/1 = 100%
	if !approxEqual(r.TopAcc, 100.0, 0.1) {
		t.Errorf("TopAcc: got %.2f, want 100.0", r.TopAcc)
	}

	// HIGH: 1/1 = 100%
	if !approxEqual(r.HighAcc, 100.0, 0.1) {
		t.Errorf("HighAcc: got %.2f, want 100.0", r.HighAcc)
	}

	// MID: 1/2 = 50%
	if !approxEqual(r.MidAcc, 50.0, 0.1) {
		t.Errorf("MidAcc: got %.2f, want 50.0", r.MidAcc)
	}

	// LOW: 0/1 = 0%
	if !approxEqual(r.LowAcc, 0.0, 0.1) {
		t.Errorf("LowAcc: got %.2f, want 0.0", r.LowAcc)
	}

	// WeightedScore should be > 0 (some correct with non-zero confidence).
	if r.WeightedScore <= 0 {
		t.Errorf("WeightedScore should be > 0, got %.2f", r.WeightedScore)
	}

	// All timestamps are weekday-peak.
	if acc, ok := r.TemporalAcc["weekday-peak"]; !ok {
		t.Error("missing temporal phase weekday-peak")
	} else if !approxEqual(acc, 60.0, 0.1) {
		t.Errorf("weekday-peak accuracy: got %.2f, want 60.0", acc)
	}

	// Confidence bands should be populated.
	if len(r.ConfBands) == 0 {
		t.Error("expected at least one confidence band")
	}
}

func TestSweepV2_SweetSpot(t *testing.T) {
	// Design eval points so high-confidence ones are correct (accuracy >= 80%)
	// and low-confidence ones are wrong, establishing a sweet spot.
	// Tuesday 16:00 UTC = weekday-peak
	t0 := time.Date(2026, 3, 17, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	sigma := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	// STABLE signal with high cross-window agreement (all windows aligned)
	// should yield higher confidence. Mix correct and incorrect to create
	// a boundary.
	var evals []EvalPoint

	// 10 STABLE+high-agreement points (should get higher confidence): 9 correct, 1 wrong
	for i := 0; i < 9; i++ {
		evals = append(evals, EvalPoint{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05,
				Listings: 15, Tier: "HIGH",
				VelShortPrice: 0.1, VelLongPrice: 0.1,
			},
			FuturePct: 0.5, // FLAT = correct for STABLE
			SnapTime:  t0,
		})
	}
	evals = append(evals, EvalPoint{
		Feature: GemFeature{
			Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05,
			Listings: 15, Tier: "HIGH",
			VelShortPrice: 0.1, VelLongPrice: 0.1,
		},
		FuturePct: 5.0, // UP = wrong for STABLE
		SnapTime:  t0,
	})

	results := SweepV2(evals, mc, []SigmaConfig{sigma})
	r := results[0]

	// With all same-confidence evals at 90% accuracy, sweet spot should be found.
	// (All evals have the same features/confidence, so they land in the same band.)
	if r.SweetSpot == -1 {
		t.Error("expected SweetSpot to be found (90% accuracy), got -1")
	}
}

func TestSweepV2_NoSweetSpot(t *testing.T) {
	// All predictions wrong → no sweet spot.
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	sigma := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	// STABLE signals with large actual moves → all wrong.
	evals := []EvalPoint{
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05, Listings: 10, Tier: "MID"}, FuturePct: 10.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 0.05, Listings: 10, Tier: "MID"}, FuturePct: -8.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.1, VelMedListing: 0.2, CV: 0.05, Listings: 10, Tier: "MID"}, FuturePct: 7.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.3, VelMedListing: -0.1, CV: 0.05, Listings: 10, Tier: "MID"}, FuturePct: -6.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.2, VelMedListing: 0.3, CV: 0.05, Listings: 10, Tier: "MID"}, FuturePct: 9.0, SnapTime: t0},
	}

	results := SweepV2(evals, mc, []SigmaConfig{sigma})
	r := results[0]

	if r.SweetSpot != -1 {
		t.Errorf("expected SweetSpot=-1 (no band >= 80%%), got %d", r.SweetSpot)
	}

	// Overall accuracy should be 0%.
	if !approxEqual(r.OverallAcc, 0.0, 0.1) {
		t.Errorf("OverallAcc: got %.2f, want 0.0", r.OverallAcc)
	}
}

func TestSweepV2_SortedByWeightedScore(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// Two configs with different accuracy profiles.
	// Config 1: tight stable threshold → more STABLE signals
	// Config 2: loose stable threshold → fewer STABLE signals
	grid := []SigmaConfig{
		{HERDPriceMult: 2.0, HERDListingMult: 1.5, StablePriceMult: 0.3, BrewingPriceMult: 1.0, DumpPriceMult: 2.0},
		{HERDPriceMult: 2.0, HERDListingMult: 1.5, StablePriceMult: 1.0, BrewingPriceMult: 1.0, DumpPriceMult: 2.0},
	}

	evals := []EvalPoint{
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.5, VelMedListing: 0.5, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.5, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.3, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -1.0, SnapTime: t0},
	}

	results := SweepV2(evals, mc, grid)

	if len(results) != 2 {
		t.Fatalf("expected 2 results, got %d", len(results))
	}

	// Results must be sorted by WeightedScore descending.
	if results[0].WeightedScore < results[1].WeightedScore {
		t.Errorf("results not sorted: [0].WeightedScore=%.2f < [1].WeightedScore=%.2f",
			results[0].WeightedScore, results[1].WeightedScore)
	}
}

func TestSweepV2_EmptyEvals(t *testing.T) {
	mc := sweepMarketContext(0, 10, 0, 5)
	sigma := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	results := SweepV2(nil, mc, []SigmaConfig{sigma})

	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}
	r := results[0]

	if r.TotalEvals != 0 {
		t.Errorf("TotalEvals: got %d, want 0", r.TotalEvals)
	}
	if r.WeightedScore != 0 {
		t.Errorf("WeightedScore: got %.2f, want 0", r.WeightedScore)
	}
	if r.OverallAcc != 0 {
		t.Errorf("OverallAcc: got %.2f, want 0", r.OverallAcc)
	}
	if r.SweetSpot != -1 {
		t.Errorf("SweetSpot: got %d, want -1", r.SweetSpot)
	}
}

func TestSweepV2_EmptyTierDefaultsToLOW(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)
	sigma := SigmaConfig{
		HERDPriceMult:    2.0,
		HERDListingMult:  1.5,
		StablePriceMult:  0.5,
		BrewingPriceMult: 1.0,
		DumpPriceMult:    2.0,
	}

	// Feature with empty tier should be counted as LOW.
	evals := []EvalPoint{
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05, Listings: 10, Tier: ""}, FuturePct: 0.5, SnapTime: t0},
	}

	results := SweepV2(evals, mc, []SigmaConfig{sigma})
	r := results[0]

	// LOW tier should have 1 entry (the empty-tier feature).
	if !approxEqual(r.LowAcc, 100.0, 0.1) {
		t.Errorf("LowAcc: got %.2f, want 100.0 (empty tier defaults to LOW)", r.LowAcc)
	}
	// TOP/HIGH/MID should be 0.
	if r.TopAcc != 0 {
		t.Errorf("TopAcc: got %.2f, want 0", r.TopAcc)
	}
	if r.HighAcc != 0 {
		t.Errorf("HighAcc: got %.2f, want 0", r.HighAcc)
	}
	if r.MidAcc != 0 {
		t.Errorf("MidAcc: got %.2f, want 0", r.MidAcc)
	}
}

// --- ValidateDefaults tests ---

func TestValidateDefaults_PerSignalAccuracy(t *testing.T) {
	// Wednesday 16:00 UTC = weekday-peak
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// classifySignal uses VelLongPrice/VelLongListing and converts to pct via Chaos/Listings.
	// STABLE fires when |pVelPct| < 3 AND |lVelPct| < 5.
	// Using Chaos=100 so absolute vel = percentage for convenience.

	evals := []EvalPoint{
		// STABLE signal (low vel), FLAT actual -> correct
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: 0.5, VelLongListing: 0.3, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.5, SnapTime: t0},
		// STABLE signal, FLAT actual -> correct
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: -0.3, VelLongListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -1.0, SnapTime: t0},
		// STABLE signal, UP actual -> wrong
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: 0.2, VelLongListing: -0.05, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 5.0, SnapTime: t0},
	}

	report := ValidateDefaults(evals, mc)

	if report.TotalEvals != 3 {
		t.Errorf("TotalEvals: got %d, want 3", report.TotalEvals)
	}

	// All should classify as STABLE.
	sa, ok := report.PerSignal["STABLE"]
	if !ok {
		t.Fatal("expected STABLE signal in PerSignal")
	}
	if sa.Count != 3 {
		t.Errorf("STABLE count: got %d, want 3", sa.Count)
	}
	if sa.Correct != 2 {
		t.Errorf("STABLE correct: got %d, want 2", sa.Correct)
	}
	if !approxEqual(sa.Accuracy, 66.67, 0.1) {
		t.Errorf("STABLE accuracy: got %.2f, want ~66.67", sa.Accuracy)
	}
	if sa.Predicted != "FLAT" {
		t.Errorf("STABLE predicted: got %q, want FLAT", sa.Predicted)
	}
	if sa.AvgConfidence <= 0 {
		t.Error("STABLE AvgConfidence should be > 0")
	}
}

func TestValidateDefaults_ConfusionMatrix(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// STABLE predicts FLAT. Create outcomes: 3 FLAT, 1 UP, 1 DOWN.
	evals := []EvalPoint{
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.5, SnapTime: t0},   // FLAT
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.2, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 1.0, SnapTime: t0},   // FLAT
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -0.5, SnapTime: t0}, // FLAT
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.3, VelMedListing: -0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 5.0, SnapTime: t0},  // UP
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.4, VelMedListing: 0.3, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -5.0, SnapTime: t0}, // DOWN
	}

	report := ValidateDefaults(evals, mc)

	// All signals are STABLE => predicted FLAT.
	flatRow, ok := report.ConfusionMatrix["FLAT"]
	if !ok {
		t.Fatal("expected FLAT row in confusion matrix")
	}
	if flatRow["FLAT"] != 3 {
		t.Errorf("ConfusionMatrix[FLAT][FLAT]: got %d, want 3", flatRow["FLAT"])
	}
	if flatRow["UP"] != 1 {
		t.Errorf("ConfusionMatrix[FLAT][UP]: got %d, want 1", flatRow["UP"])
	}
	if flatRow["DOWN"] != 1 {
		t.Errorf("ConfusionMatrix[FLAT][DOWN]: got %d, want 1", flatRow["DOWN"])
	}
}

func TestValidateDefaults_SweetSpot(t *testing.T) {
	t0 := time.Date(2026, 3, 17, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// 10 STABLE points with high agreement: 9 correct, 1 wrong -> 90% accuracy.
	// All land in the same confidence band, so sweet spot should be found.
	var evals []EvalPoint
	for i := 0; i < 9; i++ {
		evals = append(evals, EvalPoint{
			Feature: GemFeature{
				Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05,
				Listings: 15, Tier: "HIGH",
				VelShortPrice: 0.1, VelLongPrice: 0.1,
			},
			FuturePct: 0.5,
			SnapTime:  t0,
		})
	}
	evals = append(evals, EvalPoint{
		Feature: GemFeature{
			Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.05,
			Listings: 15, Tier: "HIGH",
			VelShortPrice: 0.1, VelLongPrice: 0.1,
		},
		FuturePct: 5.0, // UP = wrong for STABLE
		SnapTime:  t0,
	})

	report := ValidateDefaults(evals, mc)

	if report.SweetSpot == -1 {
		t.Error("expected SweetSpot to be found (90% accuracy), got -1")
	}
}

func TestValidateDefaults_EmptyEvals(t *testing.T) {
	mc := sweepMarketContext(0, 10, 0, 5)

	report := ValidateDefaults(nil, mc)

	if report.TotalEvals != 0 {
		t.Errorf("TotalEvals: got %d, want 0", report.TotalEvals)
	}
	if report.OverallAcc != 0 {
		t.Errorf("OverallAcc: got %.2f, want 0", report.OverallAcc)
	}
	if report.SweetSpot != -1 {
		t.Errorf("SweetSpot: got %d, want -1", report.SweetSpot)
	}
	if len(report.PerSignal) != 0 {
		t.Errorf("PerSignal: expected empty, got %d entries", len(report.PerSignal))
	}
	if len(report.ConfBands) != 0 {
		t.Errorf("ConfBands: expected empty, got %d entries", len(report.ConfBands))
	}
}

func TestValidateDefaults_PerTier(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	evals := []EvalPoint{
		// TOP: STABLE -> FLAT, actual FLAT -> correct
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "TOP"}, FuturePct: 0.5, SnapTime: t0},
		// HIGH: STABLE -> FLAT, actual UP -> wrong
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.2, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "HIGH"}, FuturePct: 5.0, SnapTime: t0},
		// MID: STABLE -> FLAT, actual FLAT -> correct
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.1, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 1.0, SnapTime: t0},
		// LOW: STABLE -> FLAT, actual DOWN -> wrong
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.3, VelMedListing: -0.1, CV: 0.1, Listings: 10, Tier: "LOW"}, FuturePct: -5.0, SnapTime: t0},
	}

	report := ValidateDefaults(evals, mc)

	if !approxEqual(report.PerTier["TOP"], 100.0, 0.1) {
		t.Errorf("PerTier[TOP]: got %.2f, want 100.0", report.PerTier["TOP"])
	}
	if !approxEqual(report.PerTier["HIGH"], 0.0, 0.1) {
		t.Errorf("PerTier[HIGH]: got %.2f, want 0.0", report.PerTier["HIGH"])
	}
	if !approxEqual(report.PerTier["MID"], 100.0, 0.1) {
		t.Errorf("PerTier[MID]: got %.2f, want 100.0", report.PerTier["MID"])
	}
	if !approxEqual(report.PerTier["LOW"], 0.0, 0.1) {
		t.Errorf("PerTier[LOW]: got %.2f, want 0.0", report.PerTier["LOW"])
	}
}

func TestValidateDefaults_PerPhase(t *testing.T) {
	mc := sweepMarketContext(0, 10, 0, 5)

	// Saturday = weekend, Wednesday 16:00 = weekday-peak, Monday 08:00 = weekday-offpeak
	weekend := time.Date(2026, 3, 14, 16, 0, 0, 0, time.UTC)   // Saturday
	peak := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)      // Wednesday
	offpeak := time.Date(2026, 3, 16, 8, 0, 0, 0, time.UTC)    // Monday

	evals := []EvalPoint{
		// Weekend: correct
		{Feature: GemFeature{Time: weekend, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.5, SnapTime: weekend},
		// Weekday-peak: wrong
		{Feature: GemFeature{Time: peak, VelMedPrice: 0.2, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 5.0, SnapTime: peak},
		// Weekday-offpeak: correct
		{Feature: GemFeature{Time: offpeak, VelMedPrice: -0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 1.0, SnapTime: offpeak},
	}

	report := ValidateDefaults(evals, mc)

	if !approxEqual(report.PerPhase["weekend"], 100.0, 0.1) {
		t.Errorf("PerPhase[weekend]: got %.2f, want 100.0", report.PerPhase["weekend"])
	}
	if !approxEqual(report.PerPhase["weekday-peak"], 0.0, 0.1) {
		t.Errorf("PerPhase[weekday-peak]: got %.2f, want 0.0", report.PerPhase["weekday-peak"])
	}
	if !approxEqual(report.PerPhase["weekday-offpeak"], 100.0, 0.1) {
		t.Errorf("PerPhase[weekday-offpeak]: got %.2f, want 100.0", report.PerPhase["weekday-offpeak"])
	}
}

func TestValidateDefaults_OverallAccuracy(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// 5 eval points, 3 correct (STABLE->FLAT), 2 wrong
	evals := []EvalPoint{
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.5, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.2, VelMedListing: 0.2, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -1.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.1, VelMedListing: 0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 0.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: 0.3, VelMedListing: -0.1, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: 8.0, SnapTime: t0},
		{Feature: GemFeature{Time: t0, VelMedPrice: -0.2, VelMedListing: 0.3, CV: 0.1, Listings: 10, Tier: "MID"}, FuturePct: -8.0, SnapTime: t0},
	}

	report := ValidateDefaults(evals, mc)

	if !approxEqual(report.OverallAcc, 60.0, 0.1) {
		t.Errorf("OverallAcc: got %.2f, want 60.0", report.OverallAcc)
	}
}

func TestValidateDefaults_MultipleSignalTypes(t *testing.T) {
	t0 := time.Date(2026, 3, 18, 16, 0, 0, 0, time.UTC)
	mc := sweepMarketContext(0, 10, 0, 5)

	// classifySignal converts to pct via Chaos/Listings. Absolute listing velocity
	// must also pass floor checks (>=3 for DUMPING, >=5 for HERD/DEMAND).
	// Using Chaos=100 and Listings=50 for realistic absolute values.

	evals := []EvalPoint{
		// STABLE: pVelPct=0.1%, lVelPct=0.2% → STABLE, predicts FLAT, actual FLAT -> correct
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: 0.1, VelLongListing: 0.1, CV: 0.1, Listings: 50, Tier: "MID"}, FuturePct: 0.5, SnapTime: t0},
		// DUMPING: pVelPct=-10% < -8, lVelPct=12% > 10, absVel=6 >= 3 -> predicts DOWN, actual DOWN -> correct
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: -10, VelLongListing: 6, CV: 0.1, Listings: 50, Tier: "MID"}, FuturePct: -8.0, SnapTime: t0},
		// UNCERTAIN: pVelPct=4% (>3 so not STABLE), lVelPct=2% → not HERD/DUMPING/DEMAND → UNCERTAIN, predicts FLAT, actual FLAT -> correct
		{Feature: GemFeature{Time: t0, Chaos: 100, VelLongPrice: 4, VelLongListing: 1, CV: 0.1, Listings: 50, Tier: "MID"}, FuturePct: 1.0, SnapTime: t0},
	}

	report := ValidateDefaults(evals, mc)

	if len(report.PerSignal) < 2 {
		t.Errorf("expected at least 2 signal types, got %d", len(report.PerSignal))
	}

	// Check STABLE exists.
	if _, ok := report.PerSignal["STABLE"]; !ok {
		t.Error("expected STABLE signal in PerSignal")
	}

	// Check DUMPING exists.
	if sa, ok := report.PerSignal["DUMPING"]; ok {
		if sa.Predicted != "DOWN" {
			t.Errorf("DUMPING predicted: got %q, want DOWN", sa.Predicted)
		}
	} else {
		t.Error("expected DUMPING signal in PerSignal")
	}

	// Check UNCERTAIN exists and predicts FLAT.
	if sa, ok := report.PerSignal["UNCERTAIN"]; ok {
		if sa.Predicted != "FLAT" {
			t.Errorf("UNCERTAIN predicted: got %q, want FLAT", sa.Predicted)
		}
	} else {
		t.Error("expected UNCERTAIN signal in PerSignal")
	}

	// Overall: all 3 correct = 100%.
	if !approxEqual(report.OverallAcc, 100.0, 0.1) {
		t.Errorf("OverallAcc: got %.2f, want 100.0", report.OverallAcc)
	}
}

