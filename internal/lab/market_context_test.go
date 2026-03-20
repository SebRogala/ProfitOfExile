package lab

import (
	"math"
	"testing"
	"time"
)

// helper: approximate float comparison
func approxEqual(a, b, tolerance float64) bool {
	return math.Abs(a-b) <= tolerance
}

func TestComputeMarketContext_Percentiles(t *testing.T) {
	// 10 transfigured gems with known prices and listings.
	// Prices: 5, 10, 20, 30, 50, 80, 100, 200, 500, 1000
	// Listings: 1, 2, 3, 5, 8, 10, 15, 20, 30, 50
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 5, Listings: 1, IsTransfigured: true},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 10, Listings: 2, IsTransfigured: true},
		{Name: "Gem C of X", Variant: "20/20", Chaos: 20, Listings: 3, IsTransfigured: true},
		{Name: "Gem D of X", Variant: "20/20", Chaos: 30, Listings: 5, IsTransfigured: true},
		{Name: "Gem E of X", Variant: "20/20", Chaos: 50, Listings: 8, IsTransfigured: true},
		{Name: "Gem F of X", Variant: "20/20", Chaos: 80, Listings: 10, IsTransfigured: true},
		{Name: "Gem G of X", Variant: "20/20", Chaos: 100, Listings: 15, IsTransfigured: true},
		{Name: "Gem H of X", Variant: "20/20", Chaos: 200, Listings: 20, IsTransfigured: true},
		{Name: "Gem I of X", Variant: "20/20", Chaos: 500, Listings: 30, IsTransfigured: true},
		{Name: "Gem J of X", Variant: "20/20", Chaos: 1000, Listings: 50, IsTransfigured: true},
	}

	mc := ComputeMarketContext(snapTime, gems, nil)

	if mc.Time != snapTime {
		t.Errorf("Time = %v, want %v", mc.Time, snapTime)
	}

	// Percentile method: p * (n-1) with linear interpolation.
	// For prices sorted: [5, 10, 20, 30, 50, 80, 100, 200, 500, 1000], n=10
	// P50: 0.50 * 9 = 4.5 → lerp(50, 80, 0.5) = 65
	// P10: 0.10 * 9 = 0.9 → lerp(5, 10, 0.9) = 9.5
	// P25: 0.25 * 9 = 2.25 → lerp(20, 30, 0.25) = 22.5
	// P75: 0.75 * 9 = 6.75 → lerp(100, 200, 0.75) = 175
	// P90: 0.90 * 9 = 8.1 → lerp(500, 1000, 0.1) = 550
	// P99: 0.99 * 9 = 8.91 → lerp(500, 1000, 0.91) = 955
	// P5: 0.05 * 9 = 0.45 → lerp(5, 10, 0.45) = 7.25

	priceTests := map[string]float64{
		"P5":  7.25,
		"P10": 9.5,
		"P25": 22.5,
		"P50": 65,
		"P75": 175,
		"P90": 550,
		"P99": 955,
	}
	for key, want := range priceTests {
		got, ok := mc.PricePercentiles[key]
		if !ok {
			t.Errorf("PricePercentiles[%q] missing", key)
			continue
		}
		if !approxEqual(got, want, 0.01) {
			t.Errorf("PricePercentiles[%q] = %.2f, want %.2f", key, got, want)
		}
	}

	// Listings sorted: [1, 2, 3, 5, 8, 10, 15, 20, 30, 50], n=10
	// P50: 0.50 * 9 = 4.5 → lerp(8, 10, 0.5) = 9
	// P10: 0.10 * 9 = 0.9 → lerp(1, 2, 0.9) = 1.9
	// P25: 0.25 * 9 = 2.25 → lerp(3, 5, 0.25) = 3.5
	// P75: 0.75 * 9 = 6.75 → lerp(15, 20, 0.75) = 18.75
	// P90: 0.90 * 9 = 8.1 → lerp(30, 50, 0.1) = 32
	// P99: 0.99 * 9 = 8.91 → lerp(30, 50, 0.91) = 48.2
	// P5: 0.05 * 9 = 0.45 → lerp(1, 2, 0.45) = 1.45

	listingTests := map[string]float64{
		"P5":  1.45,
		"P10": 1.9,
		"P25": 3.5,
		"P50": 9,
		"P75": 18.75,
		"P90": 32,
		"P99": 48.2,
	}
	for key, want := range listingTests {
		got, ok := mc.ListingPercentiles[key]
		if !ok {
			t.Errorf("ListingPercentiles[%q] missing", key)
			continue
		}
		if !approxEqual(got, want, 0.01) {
			t.Errorf("ListingPercentiles[%q] = %.2f, want %.2f", key, got, want)
		}
	}
}

func TestComputeMarketContext_VelocityStats(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true},
	}

	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: t0, Chaos: 40, Listings: 12},
				{Time: t0.Add(time.Hour), Chaos: 50, Listings: 10},
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: t0, Chaos: 90, Listings: 25},
				{Time: t0.Add(time.Hour), Chaos: 100, Listings: 20},
			},
		},
	}

	mc := ComputeMarketContext(snapTime, gems, history)

	// Gem A: priceVel = (50-40)/1h = 10, listingVel = (10-12)/1h = -2
	// Gem B: priceVel = (100-90)/1h = 10, listingVel = (20-25)/1h = -5
	// VelocityMean = (10+10)/2 = 10
	// VelocitySigma = stddev([10,10]) = 0
	// ListingVelMean = (-2 + -5)/2 = -3.5
	// ListingVelSigma = stddev([-2,-5]) = 1.5

	if !approxEqual(mc.VelocityMean, 10, 0.01) {
		t.Errorf("VelocityMean = %.2f, want 10.0", mc.VelocityMean)
	}
	if !approxEqual(mc.VelocitySigma, 0, 0.01) {
		t.Errorf("VelocitySigma = %.2f, want 0.0", mc.VelocitySigma)
	}
	if !approxEqual(mc.ListingVelMean, -3.5, 0.01) {
		t.Errorf("ListingVelMean = %.2f, want -3.5", mc.ListingVelMean)
	}
	if !approxEqual(mc.ListingVelSigma, 1.5, 0.01) {
		t.Errorf("ListingVelSigma = %.2f, want 1.5", mc.ListingVelSigma)
	}
}

func TestComputeMarketContext_Totals(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true},
		{Name: "Gem C of X", Variant: "20/20", Chaos: 200, Listings: 5, IsTransfigured: true},
		// Excluded: corrupted
		{Name: "Gem D of X", Variant: "20/20", Chaos: 300, Listings: 8, IsTransfigured: true, IsCorrupted: true},
		// Excluded: not transfigured
		{Name: "Gem E", Variant: "20/20", Chaos: 400, Listings: 12, IsTransfigured: false},
		// Excluded: Trarthus
		{Name: "Trarthus of X", Variant: "20/20", Chaos: 500, Listings: 15, IsTransfigured: true},
	}

	mc := ComputeMarketContext(snapTime, gems, nil)

	if mc.TotalGems != 3 {
		t.Errorf("TotalGems = %d, want 3", mc.TotalGems)
	}
	// 10 + 20 + 5 = 35
	if mc.TotalListings != 35 {
		t.Errorf("TotalListings = %d, want 35", mc.TotalListings)
	}
}

func TestComputeMarketContext_EmptyGems(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	mc := ComputeMarketContext(snapTime, nil, nil)

	if mc.TotalGems != 0 {
		t.Errorf("TotalGems = %d, want 0", mc.TotalGems)
	}
	if mc.TotalListings != 0 {
		t.Errorf("TotalListings = %d, want 0", mc.TotalListings)
	}
	if mc.VelocityMean != 0 {
		t.Errorf("VelocityMean = %f, want 0", mc.VelocityMean)
	}
	if mc.VelocitySigma != 0 {
		t.Errorf("VelocitySigma = %f, want 0", mc.VelocitySigma)
	}

	// Maps must be initialized (not nil) for JSON marshaling.
	if mc.PricePercentiles == nil {
		t.Error("PricePercentiles is nil, must be initialized map")
	}
	if mc.ListingPercentiles == nil {
		t.Error("ListingPercentiles is nil, must be initialized map")
	}
	// All temporal slices must be initialized (not nil) for JSON marshaling.
	temporalSlices := map[string][]float64{
		"HourlyBias":        mc.HourlyBias,
		"HourlyVolatility":  mc.HourlyVolatility,
		"HourlyActivity":    mc.HourlyActivity,
		"WeekdayBias":       mc.WeekdayBias,
		"WeekdayVolatility": mc.WeekdayVolatility,
		"WeekdayActivity":   mc.WeekdayActivity,
	}
	for name, s := range temporalSlices {
		if s == nil {
			t.Errorf("%s is nil, must be initialized slice", name)
			continue
		}
		wantLen := 24
		if name[:7] == "Weekday" {
			wantLen = 7
		}
		if len(s) != wantLen {
			t.Errorf("%s length = %d, want %d", name, len(s), wantLen)
		}
	}

	// With no history, hourly/weekday biases should be neutral 1.0.
	for i, v := range mc.HourlyBias {
		if v != 1.0 {
			t.Errorf("HourlyBias[%d] = %f, want 1.0 (neutral)", i, v)
		}
	}
	for i, v := range mc.WeekdayBias {
		if v != 1.0 {
			t.Errorf("WeekdayBias[%d] = %f, want 1.0 (neutral)", i, v)
		}
	}
	// Volatility and activity should be zero with no history.
	for i, v := range mc.HourlyVolatility {
		if v != 0 {
			t.Errorf("HourlyVolatility[%d] = %f, want 0", i, v)
		}
	}
	for i, v := range mc.WeekdayActivity {
		if v != 0 {
			t.Errorf("WeekdayActivity[%d] = %f, want 0", i, v)
		}
	}
}

func TestComputeMarketContext_SingleGem(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 42, Listings: 7, IsTransfigured: true},
	}

	mc := ComputeMarketContext(snapTime, gems, nil)

	if mc.TotalGems != 1 {
		t.Errorf("TotalGems = %d, want 1", mc.TotalGems)
	}
	if mc.TotalListings != 7 {
		t.Errorf("TotalListings = %d, want 7", mc.TotalListings)
	}
	// Single gem: all percentiles equal that gem's price.
	for _, key := range PercentileKeys {
		if got := mc.PricePercentiles[key]; !approxEqual(got, 42, 0.01) {
			t.Errorf("PricePercentiles[%q] = %.2f, want 42", key, got)
		}
		if got := mc.ListingPercentiles[key]; !approxEqual(got, 7, 0.01) {
			t.Errorf("ListingPercentiles[%q] = %.2f, want 7", key, got)
		}
	}
}

func TestComputeMarketContext_AllSamePrice(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)
	gems := make([]GemPrice, 5)
	for i := range gems {
		gems[i] = GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          100,
			Listings:       10,
			IsTransfigured: true,
		}
	}

	mc := ComputeMarketContext(snapTime, gems, nil)

	for _, key := range PercentileKeys {
		if got := mc.PricePercentiles[key]; !approxEqual(got, 100, 0.01) {
			t.Errorf("PricePercentiles[%q] = %.2f, want 100", key, got)
		}
	}

	// Velocity sigma = 0 when no history
	if mc.VelocitySigma != 0 {
		t.Errorf("VelocitySigma = %f, want 0", mc.VelocitySigma)
	}
}

func TestComputeMarketContext_TemporalBiases(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC) // Monday
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 50, Listings: 10, IsTransfigured: true},
		{Name: "Gem B of X", Variant: "20/20", Chaos: 100, Listings: 20, IsTransfigured: true},
	}

	// Hour 8 (Monday): bearish -5%. Hour 20 (Monday): bullish +3%.
	monday8am := time.Date(2026, 3, 16, 8, 0, 0, 0, time.UTC)
	monday20pm := time.Date(2026, 3, 16, 20, 0, 0, 0, time.UTC)

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: monday8am, Chaos: 100, Listings: 10},
				{Time: monday8am.Add(30 * time.Minute), Chaos: 95, Listings: 10},
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: monday20pm, Chaos: 100, Listings: 20},
				{Time: monday20pm.Add(30 * time.Minute), Chaos: 103, Listings: 20},
			},
		},
	}

	mc := ComputeMarketContext(snapTime, gems, history)

	// Verify slice lengths.
	if len(mc.HourlyBias) != 24 {
		t.Fatalf("HourlyBias length = %d, want 24", len(mc.HourlyBias))
	}
	if len(mc.HourlyVolatility) != 24 {
		t.Fatalf("HourlyVolatility length = %d, want 24", len(mc.HourlyVolatility))
	}
	if len(mc.HourlyActivity) != 24 {
		t.Fatalf("HourlyActivity length = %d, want 24", len(mc.HourlyActivity))
	}
	if len(mc.WeekdayBias) != 7 {
		t.Fatalf("WeekdayBias length = %d, want 7", len(mc.WeekdayBias))
	}
	if len(mc.WeekdayVolatility) != 7 {
		t.Fatalf("WeekdayVolatility length = %d, want 7", len(mc.WeekdayVolatility))
	}
	if len(mc.WeekdayActivity) != 7 {
		t.Fatalf("WeekdayActivity length = %d, want 7", len(mc.WeekdayActivity))
	}

	// Bearish hour should have bias < 1.0.
	if mc.HourlyBias[8] >= 1.0 {
		t.Errorf("HourlyBias[8] = %.4f, want < 1.0 (bearish)", mc.HourlyBias[8])
	}

	// Bullish hour should have bias > 1.0.
	if mc.HourlyBias[20] <= 1.0 {
		t.Errorf("HourlyBias[20] = %.4f, want > 1.0 (bullish)", mc.HourlyBias[20])
	}

	// Empty hours should be neutral.
	if mc.HourlyBias[0] != 1.0 {
		t.Errorf("HourlyBias[0] = %f, want 1.0 (neutral/empty)", mc.HourlyBias[0])
	}

	// Monday (1) should have a non-neutral weekday bias (both data points are Monday).
	if mc.WeekdayBias[1] == 1.0 {
		t.Error("WeekdayBias[1] (Monday) = 1.0, expected non-neutral with data")
	}

	// Other weekdays should be neutral.
	for _, d := range []int{0, 2, 3, 4, 5, 6} {
		if mc.WeekdayBias[d] != 1.0 {
			t.Errorf("WeekdayBias[%d] = %f, want 1.0 (neutral)", d, mc.WeekdayBias[d])
		}
	}

	// Activity for populated hours should be > 0 (both changes are > 2%).
	if mc.HourlyActivity[8] <= 0 {
		t.Errorf("HourlyActivity[8] = %f, want > 0", mc.HourlyActivity[8])
	}
	if mc.HourlyActivity[20] <= 0 {
		t.Errorf("HourlyActivity[20] = %f, want > 0", mc.HourlyActivity[20])
	}
}

func TestComputeMarketContext_TierBoundaries(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	// Create enough gems for DetectTierBoundaries to produce real thresholds.
	// We need at least 4 unique transfigured gems with Chaos > 0.
	var gems []GemPrice
	prices := []float64{5, 10, 15, 20, 30, 40, 50, 60, 80, 100, 120, 150, 200, 300, 500, 800, 1000}
	for i, p := range prices {
		gems = append(gems, GemPrice{
			Name:           "Gem " + string(rune('A'+i)) + " of X",
			Variant:        "20/20",
			Chaos:          p,
			Listings:       10,
			IsTransfigured: true,
		})
	}

	mc := ComputeMarketContext(snapTime, gems, nil)

	// Tier boundaries should be non-empty and strictly descending.
	if len(mc.TierBoundaries.Boundaries) == 0 {
		t.Fatal("TierBoundaries.Boundaries is empty, want at least 1 boundary")
	}
	for i, b := range mc.TierBoundaries.Boundaries {
		if b <= 0 {
			t.Errorf("TierBoundaries.Boundaries[%d] = %f, want > 0", i, b)
		}
	}
	for i := 1; i < len(mc.TierBoundaries.Boundaries); i++ {
		if mc.TierBoundaries.Boundaries[i] >= mc.TierBoundaries.Boundaries[i-1] {
			t.Errorf("Boundaries[%d] (%.2f) should be < Boundaries[%d] (%.2f)",
				i, mc.TierBoundaries.Boundaries[i], i-1, mc.TierBoundaries.Boundaries[i-1])
		}
	}
}

func TestPercentile(t *testing.T) {
	sorted := []float64{5, 10, 20, 30, 50, 80, 100, 200, 500, 1000}

	tests := []struct {
		p    float64
		want float64
	}{
		{0.0, 5},
		{1.0, 1000},
		{0.5, 65},     // index 4.5 → lerp(50, 80, 0.5) = 65
		{0.25, 22.5},  // index 2.25 → lerp(20, 30, 0.25) = 22.5
		{0.10, 9.5},   // index 0.9 → lerp(5, 10, 0.9) = 9.5
		{0.75, 175},   // index 6.75 → lerp(100, 200, 0.75) = 175
		{0.90, 550},   // index 8.1 → lerp(500, 1000, 0.1) = 550
		{0.99, 955},   // index 8.91 → lerp(500, 1000, 0.91) = 955
		{0.05, 7.25},  // index 0.45 → lerp(5, 10, 0.45) = 7.25
	}

	for _, tt := range tests {
		got := percentile(sorted, tt.p)
		if !approxEqual(got, tt.want, 0.01) {
			t.Errorf("percentile(sorted, %.2f) = %.2f, want %.2f", tt.p, got, tt.want)
		}
	}
}

func TestPercentile_SingleElement(t *testing.T) {
	sorted := []float64{42}
	got := percentile(sorted, 0.5)
	if got != 42 {
		t.Errorf("percentile([42], 0.5) = %f, want 42", got)
	}
}

func TestPercentile_TwoElements(t *testing.T) {
	sorted := []float64{10, 20}
	// P50: 0.5 * 1 = 0.5 → lerp(10, 20, 0.5) = 15
	got := percentile(sorted, 0.5)
	if !approxEqual(got, 15, 0.01) {
		t.Errorf("percentile([10,20], 0.5) = %f, want 15", got)
	}
}
