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
	s := classifySignal(0, 0, 150, 50)
	if s != "TRAP" {
		t.Errorf("signal = %s, want TRAP", s)
	}
}

func TestClassifySignal_DUMPING(t *testing.T) {
	s := classifySignal(-10, 10, 50, 50)
	if s != "DUMPING" {
		t.Errorf("signal = %s, want DUMPING", s)
	}
}

func TestClassifySignal_HERD(t *testing.T) {
	s := classifySignal(10, 15, 30, 50)
	if s != "HERD" {
		t.Errorf("signal = %s, want HERD", s)
	}
}

func TestClassifySignal_RECOVERY(t *testing.T) {
	// New RECOVERY: price drifting down slowly (0 to -5), listings thin (<20) and dropping (<-3)
	s := classifySignal(-2, -4, 50, 15)
	if s != "RECOVERY" {
		t.Errorf("signal = %s, want RECOVERY", s)
	}
}

func TestClassifySignal_OldRECOVERY_NowFALLING(t *testing.T) {
	// Old RECOVERY conditions (priceVel < -5 && listingVel < -5) should no longer fire RECOVERY.
	// priceVel=-10 is outside the new range (must be > -5), so this becomes FALLING.
	s := classifySignal(-10, -10, 50, 50)
	if s != "FALLING" {
		t.Errorf("signal = %s, want FALLING (old RECOVERY conditions no longer match)", s)
	}
}

func TestClassifySignal_STABLE(t *testing.T) {
	s := classifySignal(0.5, 1.0, 15, 50)
	if s != "STABLE" {
		t.Errorf("signal = %s, want STABLE", s)
	}
}

func TestClassifySignal_RISING(t *testing.T) {
	s := classifySignal(3, 0, 30, 50)
	if s != "RISING" {
		t.Errorf("signal = %s, want RISING", s)
	}
}

func TestClassifySignal_FALLING(t *testing.T) {
	s := classifySignal(-3, 0, 30, 50)
	if s != "FALLING" {
		t.Errorf("signal = %s, want FALLING", s)
	}
}

func TestClassifySignal_TRAPOverridesDUMPING(t *testing.T) {
	// CV > 100 should override any velocity-based signal.
	s := classifySignal(-10, 10, 200, 50)
	if s != "TRAP" {
		t.Errorf("signal = %s, want TRAP (CV overrides DUMPING)", s)
	}
}

func TestClassifySignal_PreHERD_HighVelocity(t *testing.T) {
	// Extreme price movement with moderate listing growth → HERD (pre-signal)
	s := classifySignal(50, 5, 30, 50)
	if s != "HERD" {
		t.Errorf("signal = %s, want HERD (pre-HERD high velocity)", s)
	}
}

func TestClassifySignal_Boundaries(t *testing.T) {
	tests := []struct {
		name                        string
		priceVel, listingVel, cv    float64
		listings                    int
		want                        string
	}{
		// CV boundary: exactly 100 is NOT TRAP (uses > 100)
		{"cv=100 not TRAP", 0, 0, 100, 50, "STABLE"},
		{"cv=100.01 is TRAP", 0, 0, 100.01, 50, "TRAP"},
		// DUMPING boundary: priceVel must be < -5 (not <=)
		{"priceVel=-5 not DUMPING", -5, 10, 50, 50, "FALLING"},
		{"priceVel=-5.01 is DUMPING", -5.01, 10, 50, 50, "DUMPING"},
		// HERD boundary: listingVel must be > 10
		{"listingVel=10 not HERD", 10, 10, 30, 50, "RISING"},
		{"listingVel=10.01 is HERD", 10, 10.01, 30, 50, "HERD"},
		// STABLE boundary: |priceVel| must be < 2 and |listingVel| < 3
		{"priceVel=2 not STABLE", 2, 0, 30, 50, "RISING"},
		{"priceVel=1.99 is STABLE", 1.99, 0, 30, 50, "STABLE"},
		{"listingVel=3 not STABLE", 0, 3, 30, 50, "FALLING"},
		{"listingVel=2.99 is STABLE", 0, 2.99, 30, 50, "STABLE"},
		// Pre-HERD boundary: priceVel must be > 30 and listingVel > 3
		{"preHERD: priceVel=30 not HERD", 30, 5, 30, 50, "RISING"},
		{"preHERD: priceVel=30.01 listVel=3.01 is HERD", 30.01, 3.01, 30, 50, "HERD"},
		{"preHERD: priceVel=50 listVel=3 not HERD", 50, 3, 30, 50, "RISING"},
		{"preHERD: priceVel=50 listVel=3.01 is HERD", 50, 3.01, 30, 50, "HERD"},
		// RECOVERY boundaries: priceVel in (-5, 0), listingVel < -3, listings < 20
		{"RECOVERY: priceVel=-1 listVel=-4 lst=10", -1, -4, 30, 10, "RECOVERY"},
		{"RECOVERY: priceVel=0 not RECOVERY (must be <0)", 0, -4, 30, 10, "FALLING"},
		{"RECOVERY: priceVel=-5 not RECOVERY (must be >-5)", -5, -4, 30, 10, "FALLING"},
		{"RECOVERY: listVel=-3 not RECOVERY (must be <-3)", -1, -3, 30, 10, "FALLING"},
		{"RECOVERY: lst=20 not RECOVERY (must be <20)", -1, -4, 30, 20, "FALLING"},
		{"RECOVERY: lst=19 is RECOVERY", -1, -4, 30, 19, "RECOVERY"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifySignal(tt.priceVel, tt.listingVel, tt.cv, tt.listings)
			if got != tt.want {
				t.Errorf("classifySignal(%v, %v, %v, %d) = %s, want %s",
					tt.priceVel, tt.listingVel, tt.cv, tt.listings, got, tt.want)
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

	results := AnalyzeTrends(now, current, history, nil, 0)

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

	results := AnalyzeTrends(now, current, nil, nil, 0)

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

	results := AnalyzeTrends(now, current, nil, nil, 0)
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
	results := AnalyzeTrends(time.Now(), nil, nil, nil, 0)
	if len(results) != 0 {
		t.Errorf("got %d results, want 0", len(results))
	}
}

func TestComputeRelativeLiquidity(t *testing.T) {
	tests := []struct {
		name     string
		gem, avg float64
		want     float64
	}{
		{"average gem", 100, 100, 1.0},
		{"low gem", 30, 100, 0.3},
		{"high gem", 200, 100, 2.0},
		{"zero avg defaults to 1.0", 50, 0, 1.0},
		{"negative avg defaults to 1.0", 50, -10, 1.0},
		{"zero listings", 0, 100, 0.0},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := computeRelativeLiquidity(tt.gem, tt.avg)
			if math.Abs(got-tt.want) > 0.001 {
				t.Errorf("computeRelativeLiquidity(%v, %v) = %f, want %f", tt.gem, tt.avg, got, tt.want)
			}
		})
	}
}

func TestLiquidityTier(t *testing.T) {
	tests := []struct {
		relLiq float64
		want   string
	}{
		{0.0, "LOW"},
		{0.1, "LOW"},
		{0.29, "LOW"},
		{0.3, "MED"},
		{0.5, "MED"},
		{0.79, "MED"},
		{0.8, "HIGH"},
		{1.0, "HIGH"},
		{2.5, "HIGH"},
	}
	for _, tt := range tests {
		t.Run(tt.want, func(t *testing.T) {
			got := liquidityTier(tt.relLiq)
			if got != tt.want {
				t.Errorf("liquidityTier(%v) = %s, want %s", tt.relLiq, got, tt.want)
			}
		})
	}
}

func TestComputeWindowScore(t *testing.T) {
	tests := []struct {
		name                                  string
		roi, baseVel, transLst, relLiq        float64
		wantMin, wantMax                      float64
	}{
		// High ROI + draining base + low trans + low liquidity = high score
		{"ideal window", 300, -5, 10, 0.2, 80, 100},
		// Zero ROI, no drain, high listings, high liquidity
		{"no opportunity", 0, 0, 50, 1.5, 0, 5},
		// Only ROI contributes
		{"roi only", 200, 0, 50, 1.0, 15, 25},
		// Base draining only
		{"drain only", 0, -3, 50, 1.0, 10, 20},
		// Low trans listings only
		{"low trans only", 0, 0, 10, 1.0, 15, 25},
		// Low liquidity urgency only
		{"urgency only", 0, 0, 50, 0.3, 10, 20},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := computeWindowScore(tt.roi, tt.baseVel, tt.transLst, tt.relLiq)
			if got < tt.wantMin || got > tt.wantMax {
				t.Errorf("computeWindowScore(%v, %v, %v, %v) = %f, want [%f, %f]",
					tt.roi, tt.baseVel, tt.transLst, tt.relLiq, got, tt.wantMin, tt.wantMax)
			}
		})
	}
}

func TestComputeWindowScore_Capped(t *testing.T) {
	// Even with extreme inputs, score should never exceed 100.
	got := computeWindowScore(10000, -100, 0, 0)
	if got > 100 {
		t.Errorf("computeWindowScore with extreme inputs = %f, want <= 100", got)
	}
}

func TestClassifyWindowSignal(t *testing.T) {
	tests := []struct {
		name                              string
		score, baseVel, transListVel      float64
		baseLst                           int
		priceVel                          float64
		want                              string
	}{
		{"OPEN: high score + draining + momentum", 75, -3, 0, 30, 5, "OPEN"},
		{"OPENING: mid score + slight drain + momentum", 55, -1, 0, 30, 1, "OPENING"},
		{"CLOSING: mid score + herd arriving", 55, 0, 5, 30, 0, "CLOSING"},
		{"CLOSED: low score", 30, -5, 0, 30, 0, "CLOSED"},
		{"CLOSED: draining but no price momentum", 75, -3, 0, 30, 0, "CLOSED"},
		{"EXHAUSTED: base listings 0", 80, -5, 0, 0, 5, "EXHAUSTED"},
		{"EXHAUSTED: base listings 2", 80, -5, 0, 2, 5, "EXHAUSTED"},
		{"BREWING: price rising + trans listings falling + bases available", 20, 0, -2, 50, 3, "BREWING"},
		{"BREWING not if bases low", 20, 0, -2, 5, 3, "CLOSED"},
		{"BREWING needs pVel>2", 20, 0, -2, 50, 1, "CLOSED"},
		// Edge: relative drain threshold. baseLst=30 → threshold=max(-1.2,-1)=-1
		{"boundary: score=70 baseVel=-1 with momentum", 70, -1, 0, 30, 5, "OPENING"},
		{"boundary: score=70 baseVel=-1.01 is OPEN", 70, -1.01, 0, 30, 5, "OPEN"},
		// Large base: baseLst=200 → threshold=max(-8,-1)=-1, so -5 is still OPEN
		{"large base OPEN", 75, -5, 0, 200, 5, "OPEN"},
		// Sentinel: baseListings = -1 skips EXHAUSTED
		{"baseLst=-1 skips EXHAUSTED", 80, -5, 0, -1, 5, "OPEN"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifyWindowSignal(tt.score, tt.baseVel, tt.transListVel, tt.baseLst, tt.priceVel)
			if got != tt.want {
				t.Errorf("classifyWindowSignal(%v, %v, %v, %v, %v) = %s, want %s",
					tt.score, tt.baseVel, tt.transListVel, tt.baseLst, tt.priceVel, got, tt.want)
			}
		})
	}
}

func TestClassifyWindowSignal_RelativeDrain(t *testing.T) {
	tests := []struct {
		name                              string
		score, baseVel, transListVel      float64
		baseLst                           int
		priceVel                          float64
		want                              string
	}{
		// baseLst=200 → threshold=max(-8,-1)=-1. baseVel=-5 is only 2.5% drain, but exceeds -1 threshold
		{"large base small drain still OPEN", 75, -5, 0, 200, 5, "OPEN"},
		// baseLst=200 → threshold=-1. baseVel=-0.5 not below -1
		{"large base tiny drain not OPEN", 75, -0.5, 0, 200, 5, "CLOSED"},
		// baseLst=50 → threshold=max(-2,-1)=-1. baseVel=-3 (6% drain) below -1 → OPEN
		{"small base heavy drain OPEN", 75, -3, 0, 50, 5, "OPEN"},
		// baseLst=10 → threshold=max(-0.4,-1)=-0.4. baseVel=-0.1, threshold*0.5=-0.2, not below
		{"tiny base not draining enough", 75, -0.1, 0, 10, 5, "CLOSED"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := classifyWindowSignal(tt.score, tt.baseVel, tt.transListVel, tt.baseLst, tt.priceVel)
			if got != tt.want {
				t.Errorf("classifyWindowSignal(%v, %v, %v, %v, %v) = %s, want %s",
					tt.score, tt.baseVel, tt.transListVel, tt.baseLst, tt.priceVel, got, tt.want)
			}
		})
	}
}

func TestAnalyzeTrends_BaseSignals(t *testing.T) {
	now := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := now.Add(-90 * time.Minute)

	current := []GemPrice{
		// Transfigured gem
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 300, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
		// Its base gem
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 30, IsTransfigured: false, GemColor: "BLUE"},
	}

	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 280, Listings: 12},
				{Time: t0.Add(30 * time.Minute), Chaos: 290, Listings: 11},
				{Time: t0.Add(60 * time.Minute), Chaos: 295, Listings: 10},
				{Time: t0.Add(90 * time.Minute), Chaos: 300, Listings: 10},
			},
		},
	}

	baseHistory := map[string][]PricePoint{
		"Spark": {
			{Time: t0, Chaos: 50, Listings: 60},
			{Time: t0.Add(30 * time.Minute), Chaos: 55, Listings: 50},
			{Time: t0.Add(60 * time.Minute), Chaos: 58, Listings: 40},
			{Time: t0.Add(90 * time.Minute), Chaos: 60, Listings: 30},
		},
	}

	// Market average = 100 listings. Spark at 30 = 0.3 relative → MED (just at boundary).
	results := AnalyzeTrends(now, current, history, baseHistory, 100)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if r.BaseListings != 30 {
		t.Errorf("BaseListings = %d, want 30", r.BaseListings)
	}
	// Base velocity: (30-60)/1.5h = -20/h
	if r.BaseVelocity > -15 {
		t.Errorf("BaseVelocity = %f, want < -15 (draining)", r.BaseVelocity)
	}
	if math.Abs(r.RelativeLiquidity-0.3) > 0.01 {
		t.Errorf("RelativeLiquidity = %f, want 0.3", r.RelativeLiquidity)
	}
	if r.LiquidityTier != "MED" {
		t.Errorf("LiquidityTier = %s, want MED", r.LiquidityTier)
	}
	if r.WindowScore <= 0 {
		t.Errorf("WindowScore = %f, want > 0", r.WindowScore)
	}
	if r.WindowSignal == "" {
		t.Error("WindowSignal should not be empty")
	}
}

func TestAnalyzeTrends_BaseNotFound(t *testing.T) {
	now := time.Now()
	current := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	// No base history provided.
	results := AnalyzeTrends(now, current, nil, nil, 100)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if r.BaseListings != -1 {
		t.Errorf("BaseListings = %d, want -1 (base not found sentinel)", r.BaseListings)
	}
	if r.BaseVelocity != 0 {
		t.Errorf("BaseVelocity = %f, want 0 (no base history)", r.BaseVelocity)
	}
	// With marketAvg=100 and gem at 0, relLiq = 0 → LOW.
	if r.LiquidityTier != "LOW" {
		t.Errorf("LiquidityTier = %s, want LOW (no base data)", r.LiquidityTier)
	}
	if r.WindowSignal != "CLOSED" && r.WindowSignal != "OPENING" && r.WindowSignal != "OPEN" && r.WindowSignal != "CLOSING" {
		t.Errorf("WindowSignal = %s, want valid signal", r.WindowSignal)
	}
}

func TestAnalyzeTrends_ZeroMarketAvg(t *testing.T) {
	now := time.Now()
	current := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 30, IsTransfigured: false, GemColor: "BLUE"},
	}

	// marketAvg = 0 → relative liquidity defaults to 1.0 → HIGH.
	results := AnalyzeTrends(now, current, nil, nil, 0)

	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if math.Abs(r.RelativeLiquidity-1.0) > 0.001 {
		t.Errorf("RelativeLiquidity = %f, want 1.0 (default when marketAvg=0)", r.RelativeLiquidity)
	}
	if r.LiquidityTier != "HIGH" {
		t.Errorf("LiquidityTier = %s, want HIGH (default when marketAvg=0)", r.LiquidityTier)
	}
}

func TestDetectPriceManipulation(t *testing.T) {
	tests := []struct {
		name     string
		listings int
		price    float64
		priceVel float64
		cv       float64
		want     bool
	}{
		{"3 listings at 500c no movement high CV", 3, 500, 0, 90, true},
		{"1 listing at 300c", 1, 300, 0.5, 85, true},
		{"50 listings = false", 50, 500, 0, 90, false},
		{"low price = false", 2, 100, 0, 90, false},
		{"high velocity = false", 2, 500, 5, 90, false},
		{"low CV = false", 2, 500, 0, 50, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := detectPriceManipulation(tt.listings, tt.price, tt.priceVel, tt.cv)
			if got != tt.want {
				t.Errorf("detectPriceManipulation(%d, %v, %v, %v) = %v, want %v",
					tt.listings, tt.price, tt.priceVel, tt.cv, got, tt.want)
			}
		})
	}
}

func TestDetectRotationCandidate(t *testing.T) {
	tests := []struct {
		name       string
		histPos    float64
		priceVel   float64
		listingVel float64
		want       bool
	}{
		{"histPos=20, priceVel=3, listVel=-2", 20, 3, -2, true},
		{"histPos=80 = false", 80, 3, -2, false},
		{"priceVel negative = false", 20, -1, -2, false},
		{"listingVel positive = false", 20, 3, 2, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := detectRotationCandidate(tt.histPos, tt.priceVel, tt.listingVel)
			if got != tt.want {
				t.Errorf("detectRotationCandidate(%v, %v, %v) = %v, want %v",
					tt.histPos, tt.priceVel, tt.listingVel, got, tt.want)
			}
		})
	}
}

func TestDetectUndervalued(t *testing.T) {
	tests := []struct {
		name     string
		price    float64
		listings int
		priceVel float64
		histPos  float64
		want     bool
	}{
		{"price=80, 20 listings, priceVel=5, histPos=30", 80, 20, 5, 30, true},
		{"price=500 = false", 500, 20, 5, 30, false},
		{"price=10 too low = false", 10, 20, 5, 30, false},
		{"too many listings = false", 80, 50, 5, 30, false},
		{"low velocity = false", 80, 20, 1, 30, false},
		{"histPos=60 = false", 80, 20, 5, 60, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := detectUndervalued(tt.price, tt.listings, tt.priceVel, tt.histPos)
			if got != tt.want {
				t.Errorf("detectUndervalued(%v, %d, %v, %v) = %v, want %v",
					tt.price, tt.listings, tt.priceVel, tt.histPos, got, tt.want)
			}
		})
	}
}

func TestClassifyAdvancedSignal_Priority(t *testing.T) {
	// A gem matching both MANIPULATION and POTENTIAL criteria:
	// 2 listings, price 250 (>200 for manipulation, but also in undervalued range check fails at >200).
	// Actually test with a gem that could match manipulation and rotation.
	// Manipulation: listings<=3, price>200, |priceVel|<1, cv>80
	// Rotation: histPos<30, priceVel>0, listingVel<0
	// With priceVel=0.5 (satisfies manipulation |vel|<1, but not rotation priceVel>0... actually 0.5>0 is true)
	// So: manipulation wants |priceVel|<1, rotation wants priceVel>0
	// priceVel=0.5 satisfies both conditions.
	got := classifyAdvancedSignal(
		300,   // currentPrice >200
		2,     // listings <=3
		0.5,   // priceVel: |0.5|<1 for manipulation, >0 for rotation
		-1,    // listingVel <0 for rotation
		90,    // cv >80 for manipulation
		20,    // histPos <30 for rotation
	)
	if got != "PRICE_MANIPULATION" {
		t.Errorf("classifyAdvancedSignal (manipulation+rotation) = %s, want PRICE_MANIPULATION (higher priority)", got)
	}
}

func TestClassifyAdvancedSignal_RotationOverUndervalued(t *testing.T) {
	// Inputs that match both COMEBACK and POTENTIAL:
	// Rotation: histPos<30, priceVel>0, listingVel<0
	// Undervalued: price 30-200, listings<40, priceVel>2, histPos<50
	// NOT manipulation: listings>3
	got := classifyAdvancedSignal(
		80,   // currentPrice: in undervalued range (30-200)
		20,   // listings: >3 (not manipulation), <40 (undervalued)
		3,    // priceVel: >2 (undervalued), >0 (rotation)
		-1,   // listingVel: <0 (rotation)
		30,   // cv: <=80 (not manipulation)
		20,   // histPos: <30 (rotation), <50 (undervalued)
	)
	if got != "COMEBACK" {
		t.Errorf("classifyAdvancedSignal (rotation+undervalued) = %s, want COMEBACK (higher priority)", got)
	}
}

func TestClassifyAdvancedSignal_Undervalued(t *testing.T) {
	// Inputs matching ONLY undervalued (not rotation: listingVel >= 0).
	got := classifyAdvancedSignal(
		80,  // price in range 30-200
		20,  // listings < 40
		5,   // priceVel > 2
		0,   // listingVel = 0 (not rotation)
		30,  // cv low
		30,  // histPos < 50
	)
	if got != "POTENTIAL" {
		t.Errorf("classifyAdvancedSignal (undervalued only) = %s, want POTENTIAL", got)
	}
}

func TestClassifyAdvancedSignal_Breakout(t *testing.T) {
	// LOW-tier gem with collapsing supply + rising price = BREAKOUT.
	got := classifyAdvancedSignal(
		50,   // currentPrice < 200
		20,   // listings < 30
		1,    // priceVel > 0
		-6,   // listingVel < -5
		30,   // cv (normal)
		40,   // histPos (irrelevant for breakout)
	)
	if got != "BREAKOUT" {
		t.Errorf("classifyAdvancedSignal (breakout) = %s, want BREAKOUT", got)
	}
}

func TestClassifyAdvancedSignal_BreakoutOverridesComeback(t *testing.T) {
	// Inputs that match both BREAKOUT and COMEBACK.
	// BREAKOUT: price<200, listings<30, listingVel<-5, priceVel>0
	// COMEBACK: histPos<30, priceVel>0, listingVel<0
	got := classifyAdvancedSignal(
		80,   // price < 200 (breakout), in range for comeback
		20,   // listings < 30 (breakout)
		2,    // priceVel > 0 (both)
		-7,   // listingVel < -5 (breakout), < 0 (comeback)
		30,   // cv
		20,   // histPos < 30 (comeback)
	)
	if got != "BREAKOUT" {
		t.Errorf("classifyAdvancedSignal (breakout+comeback) = %s, want BREAKOUT (higher priority)", got)
	}
}

func TestDetectBreakout(t *testing.T) {
	tests := []struct {
		name       string
		price      float64
		listings   int
		priceVel   float64
		listingVel float64
		want       bool
	}{
		{"all conditions met", 50, 20, 1, -6, true},
		{"price too high", 250, 20, 1, -6, false},
		{"too many listings", 50, 35, 1, -6, false},
		{"price not rising", 50, 20, 0, -6, false},
		{"listings not collapsing", 50, 20, 1, -4, false},
		{"boundary: price=200 not breakout", 200, 20, 1, -6, false},
		{"boundary: listings=30 not breakout", 50, 30, 1, -6, false},
		{"boundary: listingVel=-5 not breakout", 50, 20, 1, -5, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := detectBreakout(tt.price, tt.listings, tt.priceVel, tt.listingVel)
			if got != tt.want {
				t.Errorf("detectBreakout(%v, %d, %v, %v) = %v, want %v",
					tt.price, tt.listings, tt.priceVel, tt.listingVel, got, tt.want)
			}
		})
	}
}

func TestClassifyAdvancedSignal_None(t *testing.T) {
	// Normal gem — no advanced signal.
	got := classifyAdvancedSignal(100, 50, 1, 0, 30, 60)
	if got != "" {
		t.Errorf("classifyAdvancedSignal (normal) = %q, want empty string", got)
	}
}

func TestAnalyzeTrends_AdvancedSignal(t *testing.T) {
	now := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := now.Add(-90 * time.Minute)

	// Gem with manipulation characteristics: few listings, high price, no movement, high CV
	current := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 500, Listings: 2, IsTransfigured: true, GemColor: "BLUE"},
	}

	// History with high CV (from wild historical swings) but recent stable price (low velocity).
	// Velocity uses last 4 points: 500→500→500→500 over 1.5h = 0 velocity.
	// CV computed over all points — mix of very low and very high prices = CV > 80.
	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0.Add(-48 * time.Hour), Chaos: 20, Listings: 2},
				{Time: t0.Add(-36 * time.Hour), Chaos: 30, Listings: 2},
				{Time: t0.Add(-24 * time.Hour), Chaos: 25, Listings: 2},
				{Time: t0.Add(-12 * time.Hour), Chaos: 15, Listings: 2},
				{Time: t0, Chaos: 500, Listings: 2},
				{Time: t0.Add(30 * time.Minute), Chaos: 500, Listings: 2},
				{Time: t0.Add(60 * time.Minute), Chaos: 500, Listings: 2},
				{Time: t0.Add(90 * time.Minute), Chaos: 500, Listings: 2},
			},
		},
	}

	results := AnalyzeTrends(now, current, history, nil, 0)
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	r := results[0]
	if r.AdvancedSignal != "PRICE_MANIPULATION" {
		t.Errorf("AdvancedSignal = %q, want PRICE_MANIPULATION", r.AdvancedSignal)
	}
	// Primary signal should still be set independently.
	if r.Signal == "" {
		t.Error("Signal should not be empty (primary signal independent of advanced)")
	}
}

func TestComputePriceTiers_KnownPrices(t *testing.T) {
	// 15 gems with known prices. Uses median of mid-band (#6-#15) for stability.
	gems := make([]GemPrice, 0, 15)
	prices := []float64{10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150}
	for _, p := range prices {
		gems = append(gems, GemPrice{Name: "Gem", Chaos: p, IsTransfigured: true})
	}

	topThresh, midThresh := computePriceTiers(gems)

	// p99 index = int(15*0.99) = 14 → p99 = 150. No capping.
	// Sorted winsorized: [10,20,30,40,50,60,70,80,90,100,110,120,130,140,150]
	// top15start = 15-15 = 0, top5start = 15-5 = 10
	// Mid-band: indices 0..9 = [10,20,30,40,50,60,70,80,90,100]
	// Median (index 5): 60
	// wt10 = 60 → topThresh = 60*0.70 = 42.0, midThresh = 60*0.20 = 12.0
	if math.Abs(topThresh-42.0) > 0.1 {
		t.Errorf("topThreshold = %f, want ~42.0", topThresh)
	}
	if math.Abs(midThresh-12.0) > 0.1 {
		t.Errorf("midThreshold = %f, want ~12.0", midThresh)
	}
}

func TestComputePriceTiers_OutlierProtection(t *testing.T) {
	// 200 gems at reasonable prices + one outlier at 16000c.
	// With enough data points, p99 winsorization caps the outlier.
	gems := make([]GemPrice, 0, 201)
	for i := 0; i < 200; i++ {
		gems = append(gems, GemPrice{Name: "Gem", Chaos: float64(20 + i), IsTransfigured: true})
	}
	// Add the outlier.
	gems = append(gems, GemPrice{Name: "Outlier", Chaos: 16000, IsTransfigured: true})

	topThresh, midThresh := computePriceTiers(gems)

	// p99 index = int(201*0.99) = 198 → p99 = 218.
	// Mid-band median (#6-#15 from top) should be around 205-213.
	// Thresholds should be reasonable despite the 16000c outlier.
	if topThresh > 200 {
		t.Errorf("topThreshold = %f, want < 200 (outlier should be capped by p99)", topThresh)
	}
	if midThresh > 60 {
		t.Errorf("midThreshold = %f, want < 60 (outlier should be capped by p99)", midThresh)
	}

	// Compare with no-outlier version to verify they're similar.
	gemsNoOutlier := gems[:200]
	topNoOutlier, _ := computePriceTiers(gemsNoOutlier)
	// With the outlier capped, thresholds should be close to the no-outlier version.
	diff := math.Abs(topThresh - topNoOutlier)
	if diff > 5 {
		t.Errorf("outlier shifted topThreshold by %f, want < 5 (winsorization should neutralize it)", diff)
	}
}

func TestComputePriceTiers_TooFewPrices(t *testing.T) {
	gems := []GemPrice{
		{Name: "A", Chaos: 100, IsTransfigured: true},
		{Name: "B", Chaos: 200, IsTransfigured: true},
	}
	topThresh, midThresh := computePriceTiers(gems)
	if topThresh != 100 || midThresh != 30 {
		t.Errorf("fallback thresholds = (%f, %f), want (100, 30)", topThresh, midThresh)
	}
}

func TestComputePriceTiers_ExcludesCorruptedAndNonTransfigured(t *testing.T) {
	gems := make([]GemPrice, 0, 15)
	for i := 0; i < 10; i++ {
		gems = append(gems, GemPrice{Name: "Gem", Chaos: float64(50 + i*10), IsTransfigured: true})
	}
	// These should be excluded from tier computation.
	gems = append(gems, GemPrice{Name: "Corrupted", Chaos: 5000, IsTransfigured: true, IsCorrupted: true})
	gems = append(gems, GemPrice{Name: "Base", Chaos: 5000, IsTransfigured: false})
	gems = append(gems, GemPrice{Name: "Zero", Chaos: 0, IsTransfigured: true})

	topThresh, _ := computePriceTiers(gems)
	// Without exclusion, 5000c gems would skew everything.
	if topThresh > 200 {
		t.Errorf("topThreshold = %f, want < 200 (corrupted/non-trans excluded)", topThresh)
	}
}

func TestClassifyPriceTier(t *testing.T) {
	tests := []struct {
		price, top, mid float64
		want            string
	}{
		{100, 70, 20, "TOP"},
		{70, 70, 20, "MID"},   // boundary: exactly at top threshold is MID (uses >)
		{70.01, 70, 20, "TOP"},
		{50, 70, 20, "MID"},
		{20, 70, 20, "LOW"},   // boundary: exactly at mid threshold is LOW
		{20.01, 70, 20, "MID"},
		{5, 70, 20, "LOW"},
	}
	for _, tt := range tests {
		got := classifyPriceTier(tt.price, tt.top, tt.mid)
		if got != tt.want {
			t.Errorf("classifyPriceTier(%v, %v, %v) = %s, want %s",
				tt.price, tt.top, tt.mid, got, tt.want)
		}
	}
}

func TestTierAction(t *testing.T) {
	tests := []struct {
		name                      string
		signal, window, priceTier string
		want                      string
	}{
		// TOP tier
		{"TOP+HERD", "HERD", "", "TOP", "WATCH — early stage, monitor closely"},
		{"TOP+DUMPING", "DUMPING", "", "TOP", "SELL IMMEDIATELY"},
		{"TOP+BREWING", "STABLE", "BREWING", "TOP", "URGENT — window opens in ~45min"},
		{"TOP+OPEN", "STABLE", "OPEN", "TOP", "HIGH RISK — act fast or skip"},
		// MID tier
		{"MID+HERD", "HERD", "", "MID", "SELL — move is over, exit position"},
		{"MID+RISING", "RISING", "", "MID", "CAUTIOUS — may reverse"},
		{"MID+BREWING", "STABLE", "BREWING", "MID", "WATCH — may reverse before opening"},
		// LOW tier
		{"LOW+HERD", "HERD", "", "LOW", "MOMENTUM — rising with crowd, watch for reversal"},
		{"LOW+OPEN", "STABLE", "OPEN", "LOW", "UNRELIABLE — low-value windows are traps"},
		{"LOW+BREWING", "STABLE", "BREWING", "LOW", "SKIP — not actionable at this price"},
		// No special guidance
		{"TOP+STABLE", "STABLE", "CLOSED", "TOP", ""},
		{"MID+FALLING", "FALLING", "CLOSED", "MID", ""},
		{"LOW+STABLE", "STABLE", "CLOSED", "LOW", ""},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tierAction(tt.signal, tt.window, tt.priceTier)
			if got != tt.want {
				t.Errorf("tierAction(%q, %q, %q) = %q, want %q",
					tt.signal, tt.window, tt.priceTier, got, tt.want)
			}
		})
	}
}

func TestTierAction_SignalOverridesWindow(t *testing.T) {
	// When signal matches (e.g., HERD on TOP), signal action takes precedence over window action.
	// TOP+HERD should return HERD action, even if window is BREWING.
	got := tierAction("HERD", "BREWING", "TOP")
	if got != "WATCH — early stage, monitor closely" {
		t.Errorf("tierAction(HERD, BREWING, TOP) = %q, want HERD action (signal precedence)", got)
	}
}

func TestAnalyzeTrends_TierAssignment(t *testing.T) {
	now := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	// Build a market with enough gems for dynamic tier computation (need >= 10).
	current := make([]GemPrice, 0, 15)
	for i := 0; i < 12; i++ {
		current = append(current, GemPrice{
			Name: "Filler Gem " + string(rune('A'+i)), Variant: "20/20",
			Chaos: float64(20 + i*10), Listings: 10, IsTransfigured: true, GemColor: "BLUE",
		})
	}
	// Add a clearly high-value gem.
	current = append(current, GemPrice{
		Name: "Expensive Gem of Power", Variant: "20/20",
		Chaos: 500, Listings: 10, IsTransfigured: true, GemColor: "RED",
	})
	// Add a clearly low-value gem.
	current = append(current, GemPrice{
		Name: "Cheap Gem of Nothing", Variant: "20/20",
		Chaos: 6, Listings: 50, IsTransfigured: true, GemColor: "GREEN",
	})

	results := AnalyzeTrends(now, current, nil, nil, 0)

	// Find the expensive and cheap gems.
	var expensive, cheap *TrendResult
	for i, r := range results {
		if r.Name == "Expensive Gem of Power" {
			expensive = &results[i]
		}
		if r.Name == "Cheap Gem of Nothing" {
			cheap = &results[i]
		}
	}

	if expensive == nil {
		t.Fatal("missing Expensive Gem of Power result")
	}
	if expensive.PriceTier != "TOP" {
		t.Errorf("Expensive gem tier = %s, want TOP", expensive.PriceTier)
	}

	if cheap == nil {
		t.Fatal("missing Cheap Gem of Nothing result")
	}
	if cheap.PriceTier != "LOW" {
		t.Errorf("Cheap gem tier = %s, want LOW", cheap.PriceTier)
	}
}
