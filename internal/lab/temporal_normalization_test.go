package lab

import (
	"encoding/json"
	"math"
	"testing"
	"time"
)

func TestComputeTemporalCoefficients_EmptyHistory(t *testing.T) {
	snapTime := time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC)
	coeff, mode, buckets := computeTemporalCoefficients(snapTime, nil)

	if coeff != 1.0 {
		t.Errorf("coeff = %f, want 1.0", coeff)
	}
	if mode != "none" {
		t.Errorf("mode = %q, want %q", mode, "none")
	}
	if string(buckets) != "{}" {
		t.Errorf("buckets = %q, want %q", string(buckets), "{}")
	}
}

func TestComputeTemporalCoefficients_InsufficientData(t *testing.T) {
	// Only 1 data point per hour — insufficient for any mode.
	snapTime := time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC)
	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(time.Date(2026, 3, 14, 10, 0, 0, 0, time.UTC), 100),
				makePoint(time.Date(2026, 3, 14, 11, 0, 0, 0, time.UTC), 110),
			},
		},
	}

	coeff, mode, _ := computeTemporalCoefficients(snapTime, history)

	if mode != "none" {
		t.Errorf("mode = %q, want %q (insufficient data)", mode, "none")
	}
	if coeff != 1.0 {
		t.Errorf("coeff = %f, want 1.0", coeff)
	}
}

func TestComputeTemporalCoefficients_HourlyMode(t *testing.T) {
	// Create enough data points so each hourly bucket has >= 3 samples.
	// Use 3 gems, each with points at hours 10 and 14, spread across 3 different days.
	// Hour 10: prices ~200 (2x baseline), Hour 14: prices ~100 (1x baseline).
	snapTime := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)

	var history []GemPriceHistory
	gemNames := []string{"Gem A of X", "Gem B of X", "Gem C of X"}
	days := []int{10, 11, 12, 13, 14, 15}

	for _, name := range gemNames {
		var points []PricePoint
		for _, day := range days {
			// Hour 10: high prices
			points = append(points, makePoint(
				time.Date(2026, 3, day, 10, 0, 0, 0, time.UTC), 200))
			// Hour 14: low prices
			points = append(points, makePoint(
				time.Date(2026, 3, day, 14, 0, 0, 0, time.UTC), 100))
		}
		history = append(history, GemPriceHistory{
			Name:     name,
			Variant:  "20/20",
			GemColor: "RED",
			Points:   points,
		})
	}

	coeff, mode, bucketsJSON := computeTemporalCoefficients(snapTime, history)

	// With consistent data, mode should be "hourly" (or "weekday_hour" if enough data).
	if mode == "none" {
		t.Fatalf("mode = %q, want hourly or weekday_hour", mode)
	}

	// The coefficient at hour 14 should be less than at hour 10.
	// Global median ~ 150, hour 14 median ~ 100, so coefficient ~ 0.67
	// hour 10 median ~ 200, so coefficient ~ 1.33
	// Current time is hour 14, so current coefficient should be < 1.0.
	if coeff >= 1.0 {
		t.Errorf("coeff = %f, want < 1.0 (hour 14 is below baseline)", coeff)
	}

	// Verify bucket data is valid JSON.
	var buckets map[string][]TemporalBucket
	if err := json.Unmarshal(bucketsJSON, &buckets); err != nil {
		t.Fatalf("failed to unmarshal bucket data: %v", err)
	}
	if _, ok := buckets["20/20"]; !ok {
		t.Error("bucket data missing variant 20/20")
	}
}

func TestComputeTemporalCoefficients_2xPriceAtCertainHours(t *testing.T) {
	// Known data: prices are 2x at hour 10 and 1x at hour 14.
	// Baseline = mean of bucket medians = (200 + 100) / 2 = 150.
	// Coefficient at hour 10 = 200/150 ≈ 1.33
	// Coefficient at hour 14 = 100/150 ≈ 0.67
	snapTime := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)

	var points []PricePoint
	for day := 10; day <= 15; day++ {
		points = append(points,
			makePoint(time.Date(2026, 3, day, 10, 0, 0, 0, time.UTC), 200),
			makePoint(time.Date(2026, 3, day, 14, 0, 0, 0, time.UTC), 100),
		)
	}

	history := []GemPriceHistory{
		{Name: "Gem A of X", Variant: "20/20", GemColor: "RED", Points: points},
	}

	coeff, mode, _ := computeTemporalCoefficients(snapTime, history)

	if mode == "none" {
		t.Fatalf("mode = %q, want hourly or weekday_hour", mode)
	}

	// At hour 10 (high time), coefficient should be > 1.0
	if coeff <= 1.0 {
		t.Errorf("coeff at hour 10 = %f, want > 1.0 (prices are 2x)", coeff)
	}
	// Should be approximately 200/150 = 1.333
	if !approxEqual(coeff, 1.333, 0.1) {
		t.Errorf("coeff at hour 10 = %f, want ≈1.33", coeff)
	}
}

func TestCoefficientAt_NoneMode(t *testing.T) {
	mc := MarketContext{
		TemporalMode:    "none",
		TemporalBuckets: []byte("{}"),
	}

	coeff := mc.CoefficientAt(time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), "20/20")
	if coeff != 1.0 {
		t.Errorf("CoefficientAt with mode=none = %f, want 1.0", coeff)
	}
}

func TestCoefficientAt_EmptyMode(t *testing.T) {
	mc := MarketContext{
		TemporalMode: "",
	}

	coeff := mc.CoefficientAt(time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), "20/20")
	if coeff != 1.0 {
		t.Errorf("CoefficientAt with empty mode = %f, want 1.0", coeff)
	}
}

func TestCoefficientAt_HourlyMode(t *testing.T) {
	buckets := map[string][]TemporalBucket{
		"20/20": {
			{Hour: 10, Coeff: 1.5, N: 10},
			{Hour: 14, Coeff: 0.8, N: 10},
		},
		"1": {
			{Hour: 10, Coeff: 1.2, N: 8},
			{Hour: 14, Coeff: 0.9, N: 8},
		},
	}
	bucketsJSON, _ := json.Marshal(buckets)

	mc := MarketContext{
		TemporalMode:    "hourly",
		TemporalBuckets: bucketsJSON,
	}

	// Variant 20/20 at hour 10 should return 1.5.
	coeff := mc.CoefficientAt(time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), "20/20")
	if !approxEqual(coeff, 1.5, 0.001) {
		t.Errorf("CoefficientAt(hour=10, variant=20/20) = %f, want 1.5", coeff)
	}

	// Variant 1 at hour 14 should return 0.9.
	coeff = mc.CoefficientAt(time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), "1")
	if !approxEqual(coeff, 0.9, 0.001) {
		t.Errorf("CoefficientAt(hour=14, variant=1) = %f, want 0.9", coeff)
	}

	// Unknown variant should return 1.0.
	coeff = mc.CoefficientAt(time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), "unknown")
	if coeff != 1.0 {
		t.Errorf("CoefficientAt(unknown variant) = %f, want 1.0", coeff)
	}

	// Unknown hour should return 1.0.
	coeff = mc.CoefficientAt(time.Date(2026, 3, 16, 3, 0, 0, 0, time.UTC), "20/20")
	if coeff != 1.0 {
		t.Errorf("CoefficientAt(hour=3, no data) = %f, want 1.0", coeff)
	}
}

func TestCoefficientAt_WeekdayHourMode(t *testing.T) {
	// weekday_hour key = weekday*24+hour
	// Monday (1) hour 10 = 1*24+10 = 34
	buckets := map[string][]TemporalBucket{
		"20/20": {
			{Hour: 34, Coeff: 1.3, N: 5}, // Monday 10:00
			{Hour: 38, Coeff: 0.7, N: 5}, // Monday 14:00
		},
	}
	bucketsJSON, _ := json.Marshal(buckets)

	mc := MarketContext{
		TemporalMode:    "weekday_hour",
		TemporalBuckets: bucketsJSON,
	}

	// Monday 10:00 = weekday 1, key = 1*24+10 = 34
	coeff := mc.CoefficientAt(time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), "20/20")
	if !approxEqual(coeff, 1.3, 0.001) {
		t.Errorf("CoefficientAt(Monday 10:00) = %f, want 1.3", coeff)
	}

	// Monday 14:00 = weekday 1, key = 1*24+14 = 38
	coeff = mc.CoefficientAt(time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), "20/20")
	if !approxEqual(coeff, 0.7, 0.001) {
		t.Errorf("CoefficientAt(Monday 14:00) = %f, want 0.7", coeff)
	}

	// Tuesday 10:00 — no weekday_hour bucket for Tuesday, but hour 10 exists
	// in Monday data. Fallback to hour-only: should return Monday's hour 10 coefficient.
	coeff = mc.CoefficientAt(time.Date(2026, 3, 17, 10, 0, 0, 0, time.UTC), "20/20")
	if !approxEqual(coeff, 1.3, 0.001) {
		t.Errorf("CoefficientAt(Tuesday 10:00, hour-only fallback) = %f, want 1.3", coeff)
	}

	// Tuesday 3:00 — no weekday_hour bucket and no hour-only match, should return 1.0.
	coeff = mc.CoefficientAt(time.Date(2026, 3, 17, 3, 0, 0, 0, time.UTC), "20/20")
	if coeff != 1.0 {
		t.Errorf("CoefficientAt(Tuesday 3:00, no data) = %f, want 1.0", coeff)
	}
}

func TestNormalizeHistory_Basic(t *testing.T) {
	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 200, Listings: 5},
				{Time: time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), Chaos: 100, Listings: 10},
			},
		},
		{
			Name: "Gem B of X", Variant: "1", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 50, Listings: 20},
			},
		},
	}

	// Coefficient function: 2.0 for variant 20/20, 1.0 for variant 1.
	coeffFn := func(t time.Time, variant string) float64 {
		if variant == "20/20" {
			return 2.0
		}
		return 1.0
	}

	normalized := NormalizeHistory(history, coeffFn)

	if len(normalized) != 2 {
		t.Fatalf("len(normalized) = %d, want 2", len(normalized))
	}

	// Variant 20/20: prices divided by 2.0
	if !approxEqual(normalized[0].Points[0].Chaos, 100, 0.01) {
		t.Errorf("normalized[0].Points[0].Chaos = %f, want 100 (200/2)", normalized[0].Points[0].Chaos)
	}
	if !approxEqual(normalized[0].Points[1].Chaos, 50, 0.01) {
		t.Errorf("normalized[0].Points[1].Chaos = %f, want 50 (100/2)", normalized[0].Points[1].Chaos)
	}
	// Listings should be preserved.
	if normalized[0].Points[0].Listings != 5 {
		t.Errorf("normalized[0].Points[0].Listings = %d, want 5", normalized[0].Points[0].Listings)
	}
	if normalized[0].Points[1].Listings != 10 {
		t.Errorf("normalized[0].Points[1].Listings = %d, want 10", normalized[0].Points[1].Listings)
	}

	// Variant 1: coefficient = 1.0, price unchanged.
	if !approxEqual(normalized[1].Points[0].Chaos, 50, 0.01) {
		t.Errorf("normalized[1].Points[0].Chaos = %f, want 50 (50/1)", normalized[1].Points[0].Chaos)
	}
	if normalized[1].Points[0].Listings != 20 {
		t.Errorf("normalized[1].Points[0].Listings = %d, want 20", normalized[1].Points[0].Listings)
	}
}

func TestNormalizeHistory_DoesNotMutateInput(t *testing.T) {
	original := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 200, Listings: 5},
			},
		},
	}

	NormalizeHistory(original, func(t time.Time, v string) float64 { return 2.0 })

	// Original should be unchanged.
	if original[0].Points[0].Chaos != 200 {
		t.Errorf("original mutated: Chaos = %f, want 200", original[0].Points[0].Chaos)
	}
}

func TestNormalizeHistory_Empty(t *testing.T) {
	result := NormalizeHistory(nil, func(t time.Time, v string) float64 { return 1.0 })
	if result != nil {
		t.Errorf("NormalizeHistory(nil) = %v, want nil", result)
	}
}

func TestNormalizeHistory_ZeroCoefficient(t *testing.T) {
	// Coefficient of 0 or negative should be treated as 1.0 (no adjustment).
	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 100, Listings: 5},
			},
		},
	}

	normalized := NormalizeHistory(history, func(t time.Time, v string) float64 { return 0 })

	if !approxEqual(normalized[0].Points[0].Chaos, 100, 0.01) {
		t.Errorf("with zero coeff: Chaos = %f, want 100 (fallback to 1.0)", normalized[0].Points[0].Chaos)
	}
}

func TestDetrendPrices_RemovesLinearTrend(t *testing.T) {
	// Prices increase linearly: 100, 110, 120, 130, 140 over 5 hours.
	// After detrending, all prices should be approximately equal (~120, the mean).
	obs := []observation{
		{t: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), chaos: 100},
		{t: time.Date(2026, 3, 16, 11, 0, 0, 0, time.UTC), chaos: 110},
		{t: time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC), chaos: 120},
		{t: time.Date(2026, 3, 16, 13, 0, 0, 0, time.UTC), chaos: 130},
		{t: time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), chaos: 140},
	}

	detrended := detrendPrices(obs)

	// All detrended prices should be approximately the mean (120).
	for i, d := range detrended {
		if !approxEqual(d.chaos, 120, 1.0) {
			t.Errorf("detrended[%d].chaos = %f, want ≈120 (linear trend removed)", i, d.chaos)
		}
	}

	// Standard deviation of detrended values should be very small.
	vals := make([]float64, len(detrended))
	for i, d := range detrended {
		vals[i] = d.chaos
	}
	_, sigma := meanStddev(vals)
	if sigma > 1.0 {
		t.Errorf("sigma of detrended prices = %f, want ≈0 (trend removed)", sigma)
	}
}

func TestDetrendPrices_PreservesTimestamps(t *testing.T) {
	t0 := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)
	t1 := time.Date(2026, 3, 16, 11, 0, 0, 0, time.UTC)

	obs := []observation{
		{t: t0, chaos: 100},
		{t: t1, chaos: 200},
	}

	detrended := detrendPrices(obs)

	if !detrended[0].t.Equal(t0) || !detrended[1].t.Equal(t1) {
		t.Error("detrendPrices altered timestamps")
	}
}

func TestDetrendPrices_SinglePoint(t *testing.T) {
	obs := []observation{
		{t: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), chaos: 100},
	}

	detrended := detrendPrices(obs)
	if len(detrended) != 1 || detrended[0].chaos != 100 {
		t.Errorf("single point detrending: got chaos=%f, want 100", detrended[0].chaos)
	}
}

func TestMedianFloat64(t *testing.T) {
	tests := []struct {
		name string
		vals []float64
		want float64
	}{
		{"empty", nil, 0},
		{"single", []float64{42}, 42},
		{"two", []float64{10, 20}, 15},
		{"odd", []float64{3, 1, 2}, 2},
		{"even", []float64{4, 1, 3, 2}, 2.5},
		{"already sorted", []float64{1, 2, 3, 4, 5}, 3},
		{"reverse sorted", []float64{5, 4, 3, 2, 1}, 3},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := medianFloat64(tt.vals)
			if !approxEqual(got, tt.want, 0.001) {
				t.Errorf("medianFloat64(%v) = %f, want %f", tt.vals, got, tt.want)
			}
		})
	}
}

func TestMedianFloat64_DoesNotMutateInput(t *testing.T) {
	vals := []float64{5, 3, 1, 4, 2}
	original := make([]float64, len(vals))
	copy(original, vals)

	medianFloat64(vals)

	for i := range vals {
		if vals[i] != original[i] {
			t.Errorf("medianFloat64 mutated input: vals[%d] = %f, was %f", i, vals[i], original[i])
		}
	}
}

func TestDetermineMode_Sufficient(t *testing.T) {
	// All buckets have >= 3 samples at weekday_hour level.
	whBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			34: {values: []float64{1, 2, 3}},
			38: {values: []float64{4, 5, 6}},
		},
	}
	hBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			10: {values: []float64{1, 2, 3}},
			14: {values: []float64{4, 5, 6}},
		},
	}

	mode := determineMode(whBuckets, hBuckets)
	if mode != "weekday_hour" {
		t.Errorf("mode = %q, want weekday_hour", mode)
	}
}

func TestDetermineMode_FallbackToHourly(t *testing.T) {
	// weekday_hour has a bucket with < 3 samples, but hourly has enough.
	whBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			34: {values: []float64{1, 2}}, // only 2 samples
			38: {values: []float64{4, 5, 6}},
		},
	}
	hBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			10: {values: []float64{1, 2, 3}},
			14: {values: []float64{4, 5, 6}},
		},
	}

	mode := determineMode(whBuckets, hBuckets)
	if mode != "hourly" {
		t.Errorf("mode = %q, want hourly (weekday_hour insufficient)", mode)
	}
}

func TestDetermineMode_FallbackToNone(t *testing.T) {
	// Both levels have insufficient samples.
	whBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			34: {values: []float64{1}},
		},
	}
	hBuckets := map[string]map[int]*bucketEntry{
		"20/20": {
			10: {values: []float64{1, 2}},
		},
	}

	mode := determineMode(whBuckets, hBuckets)
	if mode != "none" {
		t.Errorf("mode = %q, want none (all insufficient)", mode)
	}
}

func TestComputeTemporalCoefficients_NaNSafe(t *testing.T) {
	// Ensure no NaN/Inf in output with extreme values.
	snapTime := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)
	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(time.Date(2026, 3, 10, 10, 0, 0, 0, time.UTC), math.SmallestNonzeroFloat64),
				makePoint(time.Date(2026, 3, 10, 11, 0, 0, 0, time.UTC), math.MaxFloat64/2),
			},
		},
	}

	coeff, _, _ := computeTemporalCoefficients(snapTime, history)

	if math.IsNaN(coeff) || math.IsInf(coeff, 0) {
		t.Errorf("coefficient is NaN/Inf with extreme input values")
	}
}

func TestEndToEnd_TemporalNormalization(t *testing.T) {
	// End-to-end: ComputeMarketContext should populate temporal fields,
	// and NormalizeHistory should use them correctly.
	snapTime := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)
	gems := []GemPrice{
		{Name: "Gem A of X", Variant: "20/20", Chaos: 200, Listings: 10, IsTransfigured: true},
	}

	// Create history with enough data for hourly bucketing.
	var points []PricePoint
	for day := 10; day <= 16; day++ {
		points = append(points,
			makePoint(time.Date(2026, 3, day, 10, 0, 0, 0, time.UTC), 200),
			makePoint(time.Date(2026, 3, day, 14, 0, 0, 0, time.UTC), 100),
		)
	}
	history := []GemPriceHistory{
		{Name: "Gem A of X", Variant: "20/20", GemColor: "RED", Points: points},
	}

	mc := ComputeMarketContext(snapTime, gems, history, ClassificationResult{})

	// Temporal fields should be populated.
	if mc.TemporalMode == "" {
		t.Error("TemporalMode is empty after ComputeMarketContext")
	}
	if mc.TemporalMode != "none" && mc.TemporalCoefficient == 0 {
		t.Error("TemporalCoefficient is 0 with non-none mode")
	}

	// NormalizeHistory should work with the computed coefficients.
	normalized := NormalizeHistory(history, mc.CoefficientAt)
	if len(normalized) != len(history) {
		t.Errorf("NormalizeHistory returned %d entries, want %d", len(normalized), len(history))
	}

	// Verify no NaN/Inf in normalized prices.
	for _, nh := range normalized {
		for _, p := range nh.Points {
			if math.IsNaN(p.Chaos) || math.IsInf(p.Chaos, 0) {
				t.Errorf("NaN/Inf in normalized price for %s at %v", nh.Name, p.Time)
			}
		}
	}
}

func TestPrecomputeMarketDepth(t *testing.T) {
	mc := MarketContext{
		TotalGems:     100,
		TotalListings: 2000,
		VariantStats: map[string]VariantBaseline{
			"20/20": {MedianListings: 50},
			"1":     {MedianListings: 30},
		},
		PricePercentiles:   make(map[string]float64),
		ListingPercentiles: make(map[string]float64),
		HourlyBias:         make([]float64, 24),
		HourlyVolatility:   make([]float64, 24),
		HourlyActivity:     make([]float64, 24),
		WeekdayBias:        make([]float64, 7),
		WeekdayVolatility:  make([]float64, 7),
		WeekdayActivity:    make([]float64, 7),
	}

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 200, Listings: 25, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cleave of Rage", Variant: "1", Chaos: 50, Listings: 15, IsTransfigured: true, GemColor: "RED"},
		// Corrupted gem — should be excluded.
		{Name: "Bad of Gem", Variant: "20/20", Chaos: 300, Listings: 10, IsTransfigured: true, IsCorrupted: true, GemColor: "RED"},
		// Non-transfigured — should be excluded.
		{Name: "Spark", Variant: "20/20", Chaos: 5, Listings: 100, IsTransfigured: false, GemColor: "BLUE"},
	}

	depthMap := PrecomputeMarketDepth(gems, mc)

	// Spark of Nova: 25/50 = 0.5
	sparkKey := "Spark of Nova|20/20"
	if d, ok := depthMap[sparkKey]; !ok {
		t.Errorf("depthMap missing key %q", sparkKey)
	} else if math.Abs(d-0.5) > 0.001 {
		t.Errorf("depthMap[%q] = %f, want 0.5", sparkKey, d)
	}

	// Cleave of Rage: 15/30 = 0.5
	cleaveKey := "Cleave of Rage|1"
	if d, ok := depthMap[cleaveKey]; !ok {
		t.Errorf("depthMap missing key %q", cleaveKey)
	} else if math.Abs(d-0.5) > 0.001 {
		t.Errorf("depthMap[%q] = %f, want 0.5", cleaveKey, d)
	}

	// Corrupted and non-transfigured gems should be excluded.
	if _, ok := depthMap["Bad of Gem|20/20"]; ok {
		t.Error("depthMap should not contain corrupted gem")
	}
	if _, ok := depthMap["Spark|20/20"]; ok {
		t.Error("depthMap should not contain non-transfigured gem")
	}
}

func TestNormalizeHistoryDepthGated_SkipsCascade(t *testing.T) {
	// Build a MarketContext with temporal normalization data.
	buckets := map[string][]TemporalBucket{
		"20/20": {
			{Hour: 10, Coeff: 2.0, N: 10},
			{Hour: 14, Coeff: 0.5, N: 10},
		},
	}
	bucketsJSON, _ := json.Marshal(buckets)

	mc := MarketContext{
		TemporalMode:    "hourly",
		TemporalBuckets: bucketsJSON,
	}

	// Two gems: one CASCADE (depth=0.1) and one TEMPORAL (depth=2.0).
	depthMap := map[string]float64{
		"Cascade of Gem|20/20":  0.1, // CASCADE: depth < 0.4
		"Temporal of Gem|20/20": 2.0, // TEMPORAL: depth >= 0.4
	}

	history := []GemPriceHistory{
		{
			Name: "Cascade of Gem", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 200, Listings: 5},
				{Time: time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), Chaos: 100, Listings: 5},
			},
		},
		{
			Name: "Temporal of Gem", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 200, Listings: 20},
				{Time: time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC), Chaos: 100, Listings: 20},
			},
		},
	}

	normalized := NormalizeHistoryDepthGated(history, mc, depthMap)
	if len(normalized) != 2 {
		t.Fatalf("got %d histories, want 2", len(normalized))
	}

	// CASCADE gem: prices should be UNCHANGED (raw).
	cascade := normalized[0]
	if cascade.Points[0].Chaos != 200 {
		t.Errorf("CASCADE gem point[0].Chaos = %f, want 200 (unchanged)", cascade.Points[0].Chaos)
	}
	if cascade.Points[1].Chaos != 100 {
		t.Errorf("CASCADE gem point[1].Chaos = %f, want 100 (unchanged)", cascade.Points[1].Chaos)
	}

	// TEMPORAL gem: prices should be DIVIDED by coefficient.
	// At hour 10, coeff=2.0 → 200/2.0 = 100
	// At hour 14, coeff=0.5 → 100/0.5 = 200
	temporal := normalized[1]
	if !approxEqual(temporal.Points[0].Chaos, 100, 0.01) {
		t.Errorf("TEMPORAL gem point[0].Chaos = %f, want 100 (200/2.0)", temporal.Points[0].Chaos)
	}
	if !approxEqual(temporal.Points[1].Chaos, 200, 0.01) {
		t.Errorf("TEMPORAL gem point[1].Chaos = %f, want 200 (100/0.5)", temporal.Points[1].Chaos)
	}

	// Listings should be preserved for both.
	if cascade.Points[0].Listings != 5 {
		t.Errorf("CASCADE listings changed: %d, want 5", cascade.Points[0].Listings)
	}
	if temporal.Points[0].Listings != 20 {
		t.Errorf("TEMPORAL listings changed: %d, want 20", temporal.Points[0].Listings)
	}
}

func TestNormalizeHistoryDepthGated_NoneMode(t *testing.T) {
	mc := MarketContext{
		TemporalMode:    "none",
		TemporalBuckets: []byte("{}"),
	}
	depthMap := map[string]float64{
		"Gem of Test|20/20": 2.0,
	}

	history := []GemPriceHistory{
		{
			Name: "Gem of Test", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				{Time: time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC), Chaos: 200, Listings: 5},
			},
		},
	}

	result := NormalizeHistoryDepthGated(history, mc, depthMap)

	// With mode="none", function should return original history unchanged.
	if len(result) != 1 {
		t.Fatalf("got %d histories, want 1", len(result))
	}
	if result[0].Points[0].Chaos != 200 {
		t.Errorf("mode=none: Chaos = %f, want 200 (unchanged)", result[0].Points[0].Chaos)
	}
}

func TestPrecomputeMarketDepth_ConsistencyWithFeatures(t *testing.T) {
	snapTime := time.Date(2026, 3, 15, 12, 0, 0, 0, time.UTC)

	mc := testMarketContext()
	mc.VariantStats = map[string]VariantBaseline{
		"20/20": {MedianListings: 50},
	}

	gems := []GemPrice{
		{Name: "Spark of Nova", Variant: "20/20", Chaos: 100, Listings: 25, IsTransfigured: true, GemColor: "BLUE"},
		{Name: "Cleave of Rage", Variant: "20/20", Chaos: 200, Listings: 10, IsTransfigured: true, GemColor: "RED"},
	}

	// Compute features.
	features := ComputeGemFeatures(snapTime, gems, nil, mc, nil)
	// Compute depth map.
	depthMap := PrecomputeMarketDepth(gems, mc)

	if len(features) != 2 {
		t.Fatalf("got %d features, want 2", len(features))
	}

	for _, f := range features {
		key := f.Name + "|" + f.Variant
		depth, ok := depthMap[key]
		if !ok {
			t.Errorf("depthMap missing key %q", key)
			continue
		}
		if math.Abs(depth-f.MarketDepth) > 0.001 {
			t.Errorf("depthMap[%q] = %f, feature.MarketDepth = %f — should match", key, depth, f.MarketDepth)
		}
	}
}

func TestNormalizeHistoryDepthGated_MissingKeyDefaultsCascade(t *testing.T) {
	// A gem in history but NOT in depthMap should default to depth=0 (CASCADE),
	// meaning its prices are NOT normalized (raw history preserved).
	buckets := map[string][]TemporalBucket{
		"20/20": {{Hour: 12, Coeff: 2.0, N: 10}},
	}
	bucketsJSON, _ := json.Marshal(buckets)
	mc := MarketContext{
		TemporalMode:    "hourly",
		TemporalBuckets: bucketsJSON,
	}

	snapTime := time.Date(2026, 3, 20, 12, 0, 0, 0, time.UTC)
	history := []GemPriceHistory{{
		Name: "Unknown Gem", Variant: "20/20",
		Points: []PricePoint{{Time: snapTime, Chaos: 100, Listings: 5}},
	}}

	// Empty depthMap — gem not found, defaults to 0 < 0.4 → CASCADE → skip normalization.
	depthMap := map[string]float64{}

	result := NormalizeHistoryDepthGated(history, mc, depthMap)
	if result[0].Points[0].Chaos != 100 {
		t.Errorf("missing depthMap entry should skip normalization: got Chaos=%f, want 100", result[0].Points[0].Chaos)
	}
}

func TestComputeMarketDepthForGem_AllSourcesZero(t *testing.T) {
	// When VariantStats is empty AND fallbackAvg is 0, depth should be 0 → CASCADE regime.
	mc := MarketContext{
		TotalGems:    0,
		TotalListings: 0,
		VariantStats: map[string]VariantBaseline{},
	}
	depth := computeMarketDepthForGem(50, "20/20", mc, 0)
	if depth != 0 {
		t.Errorf("all sources zero: depth = %f, want 0", depth)
	}

	// This 0 depth → CASCADE regime in ComputeGemFeatures.
	// Intentional: empty market = no reliable normalization baseline.
}
