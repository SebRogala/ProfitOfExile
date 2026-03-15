package lab

import (
	"math"
	"testing"
	"time"
)

func TestCoefficientOfVariation_Empty(t *testing.T) {
	cv := coefficientOfVariation(nil)
	if cv != 0 {
		t.Errorf("CV(nil) = %f, want 0", cv)
	}
}

func TestCoefficientOfVariation_SingleValue(t *testing.T) {
	cv := coefficientOfVariation([]float64{42})
	if cv != 0 {
		t.Errorf("CV([42]) = %f, want 0", cv)
	}
}

func TestCoefficientOfVariation_IdenticalValues(t *testing.T) {
	cv := coefficientOfVariation([]float64{10, 10, 10, 10})
	if cv != 0 {
		t.Errorf("CV([10,10,10,10]) = %f, want 0", cv)
	}
}

func TestCoefficientOfVariation_KnownValues(t *testing.T) {
	// Values: 10, 20, 30. Mean = 20.
	// Variance = ((10-20)^2 + (20-20)^2 + (30-20)^2) / 3 = 200/3 ≈ 66.67
	// Stdev = sqrt(66.67) ≈ 8.165
	// CV = 8.165 / 20 * 100 ≈ 40.82
	cv := coefficientOfVariation([]float64{10, 20, 30})
	if math.Abs(cv-40.82) > 0.1 {
		t.Errorf("CV([10,20,30]) = %f, want ~40.82", cv)
	}
}

func TestCoefficientOfVariation_ZeroMean(t *testing.T) {
	cv := coefficientOfVariation([]float64{-5, 5})
	if cv != 0 {
		t.Errorf("CV([-5,5]) = %f, want 0 (zero mean)", cv)
	}
}

func TestVelocity_SinglePoint(t *testing.T) {
	points := []PricePoint{
		{Time: time.Now(), Chaos: 100, Listings: 10},
	}
	v := velocity(points, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocity(1 point) = %f, want 0", v)
	}
}

func TestVelocity_TwoPoints(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(time.Hour), Chaos: 110, Listings: 15},
	}
	v := velocity(points, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-10) > 0.01 {
		t.Errorf("price velocity = %f, want 10", v)
	}
	vl := velocity(points, func(p PricePoint) float64 { return float64(p.Listings) })
	if math.Abs(vl-5) > 0.01 {
		t.Errorf("listing velocity = %f, want 5", vl)
	}
}

func TestVelocity_UsesLast4Points(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},                           // skipped (>4 points)
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 8},     // first used
		{Time: t0.Add(60 * time.Minute), Chaos: 70, Listings: 10},    //
		{Time: t0.Add(90 * time.Minute), Chaos: 75, Listings: 12},    //
		{Time: t0.Add(120 * time.Minute), Chaos: 80, Listings: 14},   // last used
	}
	// Last 4 points: 30min to 120min. Delta = 80-60 = 20 over 1.5h = 13.33/h
	v := velocity(points, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-13.33) > 0.1 {
		t.Errorf("price velocity = %f, want ~13.33", v)
	}
}

func TestVelocity_SameTimestamp(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0, Chaos: 110, Listings: 15},
	}
	v := velocity(points, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocity(same time) = %f, want 0", v)
	}
}

func TestClassifySignal_TRAP(t *testing.T) {
	s := classifySignal(0, 0, 150)
	if s != "TRAP" {
		t.Errorf("signal = %s, want TRAP", s)
	}
}

func TestClassifySignal_DUMPING(t *testing.T) {
	s := classifySignal(-10, 10, 50)
	if s != "DUMPING" {
		t.Errorf("signal = %s, want DUMPING", s)
	}
}

func TestClassifySignal_HERD(t *testing.T) {
	s := classifySignal(10, 15, 30)
	if s != "HERD" {
		t.Errorf("signal = %s, want HERD", s)
	}
}

func TestClassifySignal_RECOVERY(t *testing.T) {
	s := classifySignal(-10, -10, 50)
	if s != "RECOVERY" {
		t.Errorf("signal = %s, want RECOVERY", s)
	}
}

func TestClassifySignal_STABLE(t *testing.T) {
	s := classifySignal(0.5, 1.0, 15)
	if s != "STABLE" {
		t.Errorf("signal = %s, want STABLE", s)
	}
}

func TestClassifySignal_RISING(t *testing.T) {
	s := classifySignal(3, 0, 30)
	if s != "RISING" {
		t.Errorf("signal = %s, want RISING", s)
	}
}

func TestClassifySignal_FALLING(t *testing.T) {
	s := classifySignal(-3, 0, 30)
	if s != "FALLING" {
		t.Errorf("signal = %s, want FALLING", s)
	}
}

func TestClassifySignal_TRAPOverridesDUMPING(t *testing.T) {
	// CV > 100 should override any velocity-based signal.
	s := classifySignal(-10, 10, 200)
	if s != "TRAP" {
		t.Errorf("signal = %s, want TRAP (CV overrides DUMPING)", s)
	}
}

func TestClassifySignal_Boundaries(t *testing.T) {
	tests := []struct {
		name                        string
		priceVel, listingVel, cv    float64
		want                        string
	}{
		// CV boundary: exactly 100 is NOT TRAP (uses > 100)
		{"cv=100 not TRAP", 0, 0, 100, "STABLE"},
		{"cv=100.01 is TRAP", 0, 0, 100.01, "TRAP"},
		// DUMPING boundary: priceVel must be < -5 (not <=)
		{"priceVel=-5 not DUMPING", -5, 10, 50, "FALLING"},
		{"priceVel=-5.01 is DUMPING", -5.01, 10, 50, "DUMPING"},
		// HERD boundary: listingVel must be > 10
		{"listingVel=10 not HERD", 10, 10, 30, "RISING"},
		{"listingVel=10.01 is HERD", 10, 10.01, 30, "HERD"},
		// STABLE boundary: |priceVel| must be < 2 and |listingVel| < 3
		{"priceVel=2 not STABLE", 2, 0, 30, "RISING"},
		{"priceVel=1.99 is STABLE", 1.99, 0, 30, "STABLE"},
		{"listingVel=3 not STABLE", 0, 3, 30, "FALLING"},
		{"listingVel=2.99 is STABLE", 0, 2.99, 30, "STABLE"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifySignal(tt.priceVel, tt.listingVel, tt.cv)
			if got != tt.want {
				t.Errorf("classifySignal(%v, %v, %v) = %s, want %s",
					tt.priceVel, tt.listingVel, tt.cv, got, tt.want)
			}
		})
	}
}

func TestHistoricalPosition(t *testing.T) {
	// Prices range 50-150, current at 100 → midpoint = 50%
	prices := []float64{50, 75, 100, 125, 150}
	high, low, pos := historicalPosition(100, prices)
	if high != 150 {
		t.Errorf("high = %f, want 150", high)
	}
	if low != 50 {
		t.Errorf("low = %f, want 50", low)
	}
	if math.Abs(pos-50) > 0.01 {
		t.Errorf("position = %f, want 50", pos)
	}
}

func TestHistoricalPosition_AtHigh(t *testing.T) {
	prices := []float64{50, 75, 100}
	_, _, pos := historicalPosition(100, prices)
	if math.Abs(pos-100) > 0.01 {
		t.Errorf("position = %f, want 100", pos)
	}
}

func TestHistoricalPosition_AtLow(t *testing.T) {
	prices := []float64{50, 75, 100}
	_, _, pos := historicalPosition(50, prices)
	if math.Abs(pos-0) > 0.01 {
		t.Errorf("position = %f, want 0", pos)
	}
}

func TestHistoricalPosition_AboveHistoricalHigh(t *testing.T) {
	prices := []float64{50, 75, 100}
	high, _, pos := historicalPosition(200, prices)
	if high != 200 {
		t.Errorf("high = %f, want 200 (expanded range)", high)
	}
	if math.Abs(pos-100) > 0.01 {
		t.Errorf("position = %f, want 100", pos)
	}
}

func TestHistoricalPosition_BelowHistoricalLow(t *testing.T) {
	prices := []float64{50, 75, 100}
	_, low, pos := historicalPosition(10, prices)
	if low != 10 {
		t.Errorf("low = %f, want 10 (expanded range)", low)
	}
	if math.Abs(pos-0) > 0.01 {
		t.Errorf("position = %f, want 0", pos)
	}
}

func TestHistoricalPosition_EmptyPrices(t *testing.T) {
	high, low, pos := historicalPosition(42, nil)
	if high != 42 {
		t.Errorf("high = %f, want 42", high)
	}
	if low != 42 {
		t.Errorf("low = %f, want 42", low)
	}
	if pos != 50 {
		t.Errorf("position = %f, want 50", pos)
	}
}

func TestHistoricalPosition_IdenticalPrices(t *testing.T) {
	prices := []float64{50, 50, 50}
	_, _, pos := historicalPosition(50, prices)
	if pos != 50 {
		t.Errorf("position = %f, want 50 (flat range)", pos)
	}
}

func TestAnalyzeTrends_BasicSignals(t *testing.T) {
	now := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	current := []GemPrice{
		// Stable gem — small price changes
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
		// Dumping gem — price crashing, listings rising
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 80, Listings: 30, IsTransfigured: true, GemColor: "RED"},
		// Should be excluded: corrupted
		{Name: "Corrupted Gem", Variant: "20/20", Chaos: 500, Listings: 5, IsTransfigured: true, IsCorrupted: true, GemColor: "RED"},
		// Should be excluded: not transfigured
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: false, GemColor: "BLUE"},
		// Should be excluded: Trarthus
		{Name: "Wave of Trarthus", Variant: "20/20", Chaos: 200, Listings: 5, IsTransfigured: true, GemColor: "RED"},
		// Should be excluded: chaos <= 5
		{Name: "Cheap Gem of Nothing", Variant: "20/20", Chaos: 3, Listings: 50, IsTransfigured: true, GemColor: "GREEN"},
	}

	t0 := now.Add(-90 * time.Minute)
	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 99, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 10},
				{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 11},
				{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 10},
			},
		},
		{
			Name: "Cleave of Rage", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: t0, Chaos: 120, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 110, Listings: 15},
				{Time: t0.Add(60 * time.Minute), Chaos: 95, Listings: 22},
				{Time: t0.Add(90 * time.Minute), Chaos: 80, Listings: 30},
			},
		},
	}

	results := AnalyzeTrends(now, current, history)

	if len(results) != 2 {
		t.Fatalf("got %d results, want 2 (filtered out corrupted, non-trans, Trarthus, cheap)", len(results))
	}

	// Find each result by name.
	resultMap := make(map[string]TrendResult)
	for _, r := range results {
		resultMap[r.Name] = r
	}

	// Spark of Nova: stable — tiny velocity
	spark, ok := resultMap["Spark of Nova"]
	if !ok {
		t.Fatal("missing Spark of Nova result")
	}
	if spark.Signal != "STABLE" {
		t.Errorf("Spark signal = %s, want STABLE", spark.Signal)
	}

	// Cleave of Rage: dumping — price drops fast, listings rise
	// priceVel = (80-120)/1.5h = -26.67, listingVel = (30-10)/1.5h = 13.33
	cleave, ok := resultMap["Cleave of Rage"]
	if !ok {
		t.Fatal("missing Cleave of Rage result")
	}
	if cleave.Signal != "DUMPING" {
		t.Errorf("Cleave signal = %s, want DUMPING", cleave.Signal)
	}
	if cleave.PriceVelocity > -20 {
		t.Errorf("Cleave price velocity = %f, want < -20", cleave.PriceVelocity)
	}
	if cleave.ListingVelocity < 10 {
		t.Errorf("Cleave listing velocity = %f, want > 10", cleave.ListingVelocity)
	}
}

func TestAnalyzeTrends_NoHistory(t *testing.T) {
	now := time.Now()
	current := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	results := AnalyzeTrends(now, current, nil)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	r := results[0]
	if r.PriceVelocity != 0 {
		t.Errorf("price velocity = %f, want 0 (no history)", r.PriceVelocity)
	}
	if r.ListingVelocity != 0 {
		t.Errorf("listing velocity = %f, want 0 (no history)", r.ListingVelocity)
	}
	if r.CV != 0 {
		t.Errorf("CV = %f, want 0 (no history)", r.CV)
	}
	if r.Signal != "STABLE" {
		t.Errorf("signal = %s, want STABLE (no history defaults)", r.Signal)
	}
	if r.HistPosition != 50 {
		t.Errorf("hist position = %f, want 50 (default midpoint)", r.HistPosition)
	}
}

func TestAnalyzeTrends_ExcludesInvalidVariants(t *testing.T) {
	now := time.Now()
	current := []GemPrice{
		{Name: "Spark of Nova", Variant: "5/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	results := AnalyzeTrends(now, current, nil)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0 (invalid variant excluded)", len(results))
	}
}

func TestSanitizeFloat(t *testing.T) {
	if sanitizeFloat(math.NaN()) != 0 {
		t.Error("NaN should be sanitized to 0")
	}
	if sanitizeFloat(math.Inf(1)) != 0 {
		t.Error("+Inf should be sanitized to 0")
	}
	if sanitizeFloat(math.Inf(-1)) != 0 {
		t.Error("-Inf should be sanitized to 0")
	}
	if sanitizeFloat(42.5) != 42.5 {
		t.Error("normal float should pass through")
	}
}

func TestCoefficientOfVariation_NegativeMean(t *testing.T) {
	// Mean is negative but non-zero — should use |mean| and return valid CV
	cv := coefficientOfVariation([]float64{-10, -20, -30})
	if cv < 0 || math.IsNaN(cv) || math.IsInf(cv, 0) {
		t.Errorf("CV with negative mean = %f, want valid non-negative value", cv)
	}
	// Same spread as [10,20,30] → CV should be ~40.82
	if math.Abs(cv-40.82) > 0.1 {
		t.Errorf("CV([-10,-20,-30]) = %f, want ~40.82", cv)
	}
}

func TestAnalyzeTrends_EmptyInput(t *testing.T) {
	results := AnalyzeTrends(time.Now(), nil, nil)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0", len(results))
	}
}
