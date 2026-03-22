package lab

import (
	"math"
	"testing"
	"time"
)

// testMarketContext returns a MarketContext suitable for testing ComputeGemFeatures.
func testMarketContext() MarketContext {
	return MarketContext{
		Time: time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC),
		PricePercentiles: map[string]float64{
			"P5": 10, "P10": 15, "P25": 30, "P50": 50,
			"P75": 100, "P90": 200, "P99": 500,
		},
		ListingPercentiles: map[string]float64{
			"P5": 2, "P10": 5, "P25": 10, "P50": 20,
			"P75": 50, "P90": 100, "P99": 200,
		},
		TotalGems:     100,
		TotalListings: 2000,
		TierBoundaries: TierBoundaries{
			Boundaries: []float64{300, 100, 30},
		},
		HourlyBias:        make([]float64, 24),
		HourlyVolatility:  make([]float64, 24),
		HourlyActivity:    make([]float64, 24),
		WeekdayBias:       make([]float64, 7),
		WeekdayVolatility: make([]float64, 7),
		WeekdayActivity:   make([]float64, 7),
	}
}

func TestComputeGemFeatures_BasicVelocities(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := snapTime.Add(-3 * time.Hour)

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 15, IsTransfigured: true, GemColor: "BLUE"},
	}

	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 160, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 165, Listings: 11},
				{Time: t0.Add(60 * time.Minute), Chaos: 170, Listings: 12},
				{Time: t0.Add(90 * time.Minute), Chaos: 175, Listings: 13},
				{Time: t0.Add(120 * time.Minute), Chaos: 180, Listings: 14},
				{Time: t0.Add(150 * time.Minute), Chaos: 190, Listings: 15},
				{Time: t0.Add(180 * time.Minute), Chaos: 200, Listings: 15},
			},
		},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}

	f := features[0]

	// Verify basic fields.
	if f.Name != "Spark of Nova" {
		t.Errorf("Name = %s, want Spark of Nova", f.Name)
	}
	if f.Chaos != 200 {
		t.Errorf("Chaos = %f, want 200", f.Chaos)
	}
	if f.Listings != 15 {
		t.Errorf("Listings = %d, want 15", f.Listings)
	}

	// Short velocity (1h window): last 2 points in 1h from t0+180min.
	// cutoff = t0+120min. Points at t0+120min, t0+150min, t0+180min.
	// (200-180)/1h = 20
	if f.VelShortPrice <= 0 {
		t.Errorf("VelShortPrice = %f, want > 0", f.VelShortPrice)
	}

	// Medium velocity (2h window).
	if f.VelMedPrice <= 0 {
		t.Errorf("VelMedPrice = %f, want > 0", f.VelMedPrice)
	}

	// Long velocity (6h window): all points within 6h.
	// (200-160)/3h = 13.33
	if math.Abs(f.VelLongPrice-13.33) > 0.5 {
		t.Errorf("VelLongPrice = %f, want ~13.33", f.VelLongPrice)
	}

	// All listing velocities should be non-negative (listings were rising).
	if f.VelShortListing < 0 {
		t.Errorf("VelShortListing = %f, want >= 0", f.VelShortListing)
	}
	if f.VelMedListing < 0 {
		t.Errorf("VelMedListing = %f, want >= 0", f.VelMedListing)
	}
	if f.VelLongListing < 0 {
		t.Errorf("VelLongListing = %f, want >= 0", f.VelLongListing)
	}
}

func TestComputeGemFeatures_Tier(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Expensive of Power", Variant: "20/20", Chaos: 500, Listings: 3, IsTransfigured: true, GemColor: "RED"},
		{Name: "Mid of Range", Variant: "20/20", Chaos: 50, Listings: 20, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cheap of Nothing", Variant: "20/20", Chaos: 10, Listings: 100, IsTransfigured: true, GemColor: "GREEN"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 3 {
		t.Fatalf("got %d features, want 3", len(features))
	}

	tierMap := make(map[string]string)
	for _, f := range features {
		tierMap[f.Name] = f.Tier
	}

	if tierMap["Expensive of Power"] != "TOP" {
		t.Errorf("Expensive tier = %s, want TOP", tierMap["Expensive of Power"])
	}
	// With boundaries [300, 100, 30]: 50 >= 30 -> TierNames[2] = "MID-HIGH"
	if tierMap["Mid of Range"] != "MID-HIGH" {
		t.Errorf("Mid tier = %s, want MID-HIGH", tierMap["Mid of Range"])
	}
	// 10 < 30 -> TierNames[3] = "MID"
	if tierMap["Cheap of Nothing"] != "MID" {
		t.Errorf("Cheap tier = %s, want MID", tierMap["Cheap of Nothing"])
	}
}

func TestComputeGemFeatures_CV(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := snapTime.Add(-90 * time.Minute)

	gems := []GemPrice{
		{Name: "Volatile of Storm", Variant: "20/20", Chaos: 200, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	// Wildly varying prices → high CV.
	history := []GemPriceHistory{
		{
			Name: "Volatile of Storm", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 50, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 300, Listings: 10},
				{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 10},
				{Time: t0.Add(90 * time.Minute), Chaos: 200, Listings: 10},
			},
		},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	if features[0].CV <= 30 {
		t.Errorf("CV = %f, want > 30 (volatile prices)", features[0].CV)
	}
}

func TestComputeGemFeatures_CVShort(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Mixed of History", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	// Old volatile data (> 6h ago) + recent stable data (within 6h).
	// CVShort should be much lower than CV since recent prices are stable.
	history := []GemPriceHistory{
		{
			Name: "Mixed of History", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				// Old volatile data: > 6h before snapTime.
				{Time: snapTime.Add(-48 * time.Hour), Chaos: 20, Listings: 10},
				{Time: snapTime.Add(-24 * time.Hour), Chaos: 200, Listings: 10},
				{Time: snapTime.Add(-12 * time.Hour), Chaos: 50, Listings: 10},
				{Time: snapTime.Add(-8 * time.Hour), Chaos: 180, Listings: 10},
				// Recent stable data: within 6h of snapTime.
				{Time: snapTime.Add(-5 * time.Hour), Chaos: 98, Listings: 10},
				{Time: snapTime.Add(-3 * time.Hour), Chaos: 100, Listings: 10},
				{Time: snapTime.Add(-1 * time.Hour), Chaos: 101, Listings: 10},
				{Time: snapTime.Add(-30 * time.Minute), Chaos: 100, Listings: 10},
			},
		},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// 7-day CV should be high (old volatile data included).
	if f.CV <= 20 {
		t.Errorf("CV = %f, want > 20 (volatile 7-day history)", f.CV)
	}

	// 6h CVShort should be low (recent stable data only).
	if f.CVShort >= 5 {
		t.Errorf("CVShort = %f, want < 5 (stable recent prices)", f.CVShort)
	}

	// CVShort should be much less than CV.
	if f.CVShort >= f.CV {
		t.Errorf("CVShort (%f) should be < CV (%f)", f.CVShort, f.CV)
	}

	// StabilityDiscount should be computed from CVShort (near 1.0 for stable recent prices).
	expectedDiscount := stabilityDiscount(f.CVShort)
	if math.Abs(f.StabilityDiscount-expectedDiscount) > 0.001 {
		t.Errorf("StabilityDiscount = %f, want %f (from CVShort)", f.StabilityDiscount, expectedDiscount)
	}
	// With low CVShort, discount should be near 1.0 (minimal penalty).
	if f.StabilityDiscount < 0.95 {
		t.Errorf("StabilityDiscount = %f, want >= 0.95 (low CVShort)", f.StabilityDiscount)
	}
}

func TestComputeGemFeatures_CVShort_NoHistory(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "New of Gem", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "GREEN"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// No history: CVShort should be 0.
	if f.CVShort != 0 {
		t.Errorf("CVShort = %f, want 0 (no history)", f.CVShort)
	}
	// StabilityDiscount from CVShort=0 should be 1.0.
	if f.StabilityDiscount != 1.0 {
		t.Errorf("StabilityDiscount = %f, want 1.0 (CVShort=0)", f.StabilityDiscount)
	}
}

func TestComputeGemFeatures_HistPosition(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := snapTime.Add(-90 * time.Minute)

	gems := []GemPrice{
		{Name: "Peak of Pride", Variant: "20/20", Chaos: 200, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	// Current price (200) is at the historical high.
	history := []GemPriceHistory{
		{
			Name: "Peak of Pride", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: t0, Chaos: 100, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 150, Listings: 10},
				{Time: t0.Add(60 * time.Minute), Chaos: 180, Listings: 10},
				{Time: t0.Add(90 * time.Minute), Chaos: 200, Listings: 10},
			},
		},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	if math.Abs(f.HistPosition-100) > 0.01 {
		t.Errorf("HistPosition = %f, want 100 (at historical high)", f.HistPosition)
	}
	if f.High7d != 200 {
		t.Errorf("High7d = %f, want 200", f.High7d)
	}
	if f.Low7d != 100 {
		t.Errorf("Low7d = %f, want 100", f.Low7d)
	}
}

func TestComputeGemFeatures_RelativeMetrics(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Average of Joe", Variant: "20/20", Chaos: 50, Listings: 20, IsTransfigured: true, GemColor: "BLUE"},
	}

	mc := testMarketContext()
	// P50 = 50, avgListings = 2000/100 = 20
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// RelativePrice = 50/50 = 1.0
	if math.Abs(f.RelativePrice-1.0) > 0.001 {
		t.Errorf("RelativePrice = %f, want 1.0", f.RelativePrice)
	}
	// RelativeListings = 20/20 = 1.0
	if math.Abs(f.RelativeListings-1.0) > 0.001 {
		t.Errorf("RelativeListings = %f, want 1.0", f.RelativeListings)
	}
}

func TestComputeGemFeatures_NoHistory(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "New of Gem", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "GREEN"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// No history: all velocities = 0.
	if f.VelShortPrice != 0 || f.VelShortListing != 0 {
		t.Errorf("short vel = (%f, %f), want (0, 0)", f.VelShortPrice, f.VelShortListing)
	}
	if f.VelMedPrice != 0 || f.VelMedListing != 0 {
		t.Errorf("med vel = (%f, %f), want (0, 0)", f.VelMedPrice, f.VelMedListing)
	}
	if f.VelLongPrice != 0 || f.VelLongListing != 0 {
		t.Errorf("long vel = (%f, %f), want (0, 0)", f.VelLongPrice, f.VelLongListing)
	}

	if f.CV != 0 {
		t.Errorf("CV = %f, want 0", f.CV)
	}
	if f.HistPosition != 50 {
		t.Errorf("HistPosition = %f, want 50", f.HistPosition)
	}
	if f.High7d != 100 {
		t.Errorf("High7d = %f, want 100", f.High7d)
	}
	if f.Low7d != 100 {
		t.Errorf("Low7d = %f, want 100", f.Low7d)
	}
}

func TestComputeGemFeatures_FilterCorrupted(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Corrupted of Gem", Variant: "20/20", Chaos: 500, Listings: 5, IsTransfigured: true, IsCorrupted: true, GemColor: "RED"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 0 {
		t.Errorf("got %d features, want 0 (corrupted filtered)", len(features))
	}
}

func TestComputeGemFeatures_FilterNonTransfigured(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Spark", Variant: "20/20", Chaos: 50, Listings: 100, IsTransfigured: false, GemColor: "BLUE"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 0 {
		t.Errorf("got %d features, want 0 (non-transfigured filtered)", len(features))
	}
}

func TestComputeGemFeatures_FilterTrarthus(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Wave of Trarthus", Variant: "20/20", Chaos: 200, Listings: 5, IsTransfigured: true, GemColor: "RED"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 0 {
		t.Errorf("got %d features, want 0 (Trarthus filtered)", len(features))
	}
}

func TestComputeGemFeatures_FilterCheap(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Cheap of Nothing", Variant: "20/20", Chaos: 3, Listings: 50, IsTransfigured: true, GemColor: "GREEN"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 0 {
		t.Errorf("got %d features, want 0 (cheap gem filtered)", len(features))
	}
}

func TestComputeGemFeatures_Stubs(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Stub of Test", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// POE-59 stubs.
	if f.FloodCount != 0 {
		t.Errorf("FloodCount = %d, want 0 (stub)", f.FloodCount)
	}
	if f.CrashCount != 0 {
		t.Errorf("CrashCount = %d, want 0 (stub)", f.CrashCount)
	}
	if f.ListingElasticity != 0 {
		t.Errorf("ListingElasticity = %f, want 0 (stub)", f.ListingElasticity)
	}
}

func TestComputeGemFeatures_MultipleGems(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	t0 := snapTime.Add(-90 * time.Minute)

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 15, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 80, Listings: 30, IsTransfigured: true, GemColor: "RED"},
		{Name: "Ice Shot of Frost", Variant: "20/20", Chaos: 400, Listings: 5, IsTransfigured: true, GemColor: "GREEN"},
	}

	history := []GemPriceHistory{
		{
			Name: "Spark of Nova", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 180, Listings: 12},
				{Time: t0.Add(30 * time.Minute), Chaos: 190, Listings: 14},
				{Time: t0.Add(60 * time.Minute), Chaos: 195, Listings: 15},
				{Time: t0.Add(90 * time.Minute), Chaos: 200, Listings: 15},
			},
		},
		{
			Name: "Cleave of Rage", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: t0, Chaos: 120, Listings: 10},
				{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 20},
				{Time: t0.Add(60 * time.Minute), Chaos: 90, Listings: 25},
				{Time: t0.Add(90 * time.Minute), Chaos: 80, Listings: 30},
			},
		},
	}

	mc := testMarketContext()
	features := ComputeGemFeatures(snapTime, gems, history, mc)

	if len(features) != 3 {
		t.Fatalf("got %d features, want 3", len(features))
	}

	featureMap := make(map[string]GemFeature)
	for _, f := range features {
		featureMap[f.Name] = f
	}

	// Spark: rising price, rising listings.
	spark := featureMap["Spark of Nova"]
	if spark.VelMedPrice <= 0 {
		t.Errorf("Spark VelMedPrice = %f, want > 0 (rising)", spark.VelMedPrice)
	}

	// Cleave: falling price, rising listings.
	cleave := featureMap["Cleave of Rage"]
	if cleave.VelMedPrice >= 0 {
		t.Errorf("Cleave VelMedPrice = %f, want < 0 (falling)", cleave.VelMedPrice)
	}
	if cleave.VelMedListing <= 0 {
		t.Errorf("Cleave VelMedListing = %f, want > 0 (rising)", cleave.VelMedListing)
	}

	// Ice Shot: no history, velocities should be 0.
	iceShot := featureMap["Ice Shot of Frost"]
	if iceShot.VelMedPrice != 0 || iceShot.VelMedListing != 0 {
		t.Errorf("Ice Shot vel = (%f, %f), want (0, 0)", iceShot.VelMedPrice, iceShot.VelMedListing)
	}
}

func TestComputeGemFeatures_ZeroMarketContext(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	gems := []GemPrice{
		{Name: "Test of Gem", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	// Zero market context: no percentiles, no gems/listings totals.
	mc := MarketContext{
		Time:               snapTime,
		PricePercentiles:   make(map[string]float64),
		ListingPercentiles: make(map[string]float64),
		HourlyBias:         make([]float64, 24),
		HourlyVolatility:   make([]float64, 24),
		HourlyActivity:     make([]float64, 24),
		WeekdayBias:        make([]float64, 7),
		WeekdayVolatility:  make([]float64, 7),
		WeekdayActivity:    make([]float64, 7),
	}

	features := ComputeGemFeatures(snapTime, gems, nil, mc)

	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// With P50=0 and avgListings=0, relative metrics should be 0 (guarded).
	if f.RelativePrice != 0 {
		t.Errorf("RelativePrice = %f, want 0 (P50=0)", f.RelativePrice)
	}
	if f.RelativeListings != 0 {
		t.Errorf("RelativeListings = %f, want 0 (avgListings=0)", f.RelativeListings)
	}

	// Verify no NaN/Inf.
	if math.IsNaN(f.RelativePrice) || math.IsInf(f.RelativePrice, 0) {
		t.Error("RelativePrice is NaN/Inf")
	}
	if math.IsNaN(f.RelativeListings) || math.IsInf(f.RelativeListings, 0) {
		t.Error("RelativeListings is NaN/Inf")
	}
}

func TestComputeGemFeatures_MarketDepth(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	mc := testMarketContext()
	mc.VariantStats = map[string]VariantBaseline{
		"20/20": {MedianListings: 50},
	}

	tests := []struct {
		name          string
		listings      int
		wantDepth     float64
		wantRegime    string
		depthTol      float64
	}{
		{"half median", 25, 0.5, "TEMPORAL", 0.001},
		{"low depth", 10, 0.2, "CASCADE", 0.001},
		{"high depth", 200, 4.0, "TEMPORAL", 0.001},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gems := []GemPrice{
				{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: tt.listings, IsTransfigured: true, GemColor: "BLUE"},
			}
			features := ComputeGemFeatures(snapTime, gems, nil, mc)
			if len(features) != 1 {
				t.Fatalf("got %d features, want 1", len(features))
			}
			f := features[0]
			if math.Abs(f.MarketDepth-tt.wantDepth) > tt.depthTol {
				t.Errorf("MarketDepth = %f, want %f", f.MarketDepth, tt.wantDepth)
			}
			if f.MarketRegime != tt.wantRegime {
				t.Errorf("MarketRegime = %q, want %q", f.MarketRegime, tt.wantRegime)
			}
		})
	}
}

func TestComputeGemFeatures_MarketDepth_Fallback(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	// MarketContext WITHOUT VariantStats for 20/20 — should fall back to avgListings.
	mc := testMarketContext()
	// mc has TotalGems=100, TotalListings=2000, so avgListings = 20.

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 10, IsTransfigured: true, GemColor: "BLUE"},
	}

	features := ComputeGemFeatures(snapTime, gems, nil, mc)
	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	f := features[0]

	// avgListings = 2000/100 = 20, listings=10 => depth = 10/20 = 0.5
	expectedDepth := 10.0 / 20.0
	if math.Abs(f.MarketDepth-expectedDepth) > 0.001 {
		t.Errorf("MarketDepth fallback = %f, want %f (listings/avgListings)", f.MarketDepth, expectedDepth)
	}
}

func TestComputeGemFeatures_MarketRegime_Boundary(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	mc := testMarketContext()
	mc.VariantStats = map[string]VariantBaseline{
		"20/20": {MedianListings: 50},
	}

	// Listings=20, depth=20/50=0.4 => TEMPORAL (>= 0.4)
	gems := []GemPrice{
		{Name: "Border of Light", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true, GemColor: "BLUE"},
	}
	features := ComputeGemFeatures(snapTime, gems, nil, mc)
	if len(features) != 1 {
		t.Fatalf("got %d features, want 1", len(features))
	}
	if features[0].MarketRegime != "TEMPORAL" {
		t.Errorf("depth=0.4: MarketRegime = %q, want TEMPORAL (boundary)", features[0].MarketRegime)
	}
	if math.Abs(features[0].MarketDepth-0.4) > 0.001 {
		t.Errorf("depth=0.4: MarketDepth = %f, want 0.4", features[0].MarketDepth)
	}

	// Listings=19, depth=19/50=0.38 => CASCADE (< 0.4)
	gems2 := []GemPrice{
		{Name: "Border of Shadow", Variant: "20/20", Chaos: 100, Listings: 19, IsTransfigured: true, GemColor: "BLUE"},
	}
	features2 := ComputeGemFeatures(snapTime, gems2, nil, mc)
	if len(features2) != 1 {
		t.Fatalf("got %d features, want 1", len(features2))
	}
	if features2[0].MarketRegime != "CASCADE" {
		t.Errorf("depth=0.38: MarketRegime = %q, want CASCADE (below boundary)", features2[0].MarketRegime)
	}
	if features2[0].MarketDepth >= 0.4 {
		t.Errorf("depth=0.38: MarketDepth = %f, want < 0.4", features2[0].MarketDepth)
	}
}
