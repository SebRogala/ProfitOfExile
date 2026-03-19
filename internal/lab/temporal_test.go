package lab

import (
	"math"
	"testing"
	"time"
)

// makePoint is a helper to create a PricePoint at a specific time with a chaos price.
func makePoint(t time.Time, chaos float64) PricePoint {
	return PricePoint{Time: t, Chaos: chaos, Listings: 10}
}

func TestComputeTemporalBiases_DirectionBias(t *testing.T) {
	// Create history where hour 8 has a -5% price drop and hour 20 has a +3% rise.
	// Use a Monday (weekday=1) for all points.
	monday8am := time.Date(2026, 3, 16, 8, 0, 0, 0, time.UTC)  // Monday
	monday20pm := time.Date(2026, 3, 16, 20, 0, 0, 0, time.UTC) // Monday

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(monday8am, 100),
				makePoint(monday8am.Add(30*time.Minute), 95), // -5% at hour 8
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(monday20pm, 100),
				makePoint(monday20pm.Add(30*time.Minute), 103), // +3% at hour 20
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 8: mean pctChange = -0.05, bias = 1.0 + (-0.05) = 0.95
	if !approxEqual(tb.HourlyBias[8], 0.95, 0.001) {
		t.Errorf("HourlyBias[8] = %.4f, want 0.95 (bearish)", tb.HourlyBias[8])
	}

	// Hour 20: mean pctChange = +0.03, bias = 1.0 + 0.03 = 1.03
	if !approxEqual(tb.HourlyBias[20], 1.03, 0.001) {
		t.Errorf("HourlyBias[20] = %.4f, want 1.03 (bullish)", tb.HourlyBias[20])
	}

	// Unpopulated hours should remain neutral.
	for _, h := range []int{0, 1, 5, 12, 15} {
		if tb.HourlyBias[h] != 1.0 {
			t.Errorf("HourlyBias[%d] = %f, want 1.0 (neutral/empty)", h, tb.HourlyBias[h])
		}
	}
}

func TestComputeTemporalBiases_WeekdayBias(t *testing.T) {
	// Monday (1) has a -4% drop, Saturday (6) has a +6% rise.
	monday := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)   // Monday
	saturday := time.Date(2026, 3, 21, 10, 0, 0, 0, time.UTC)  // Saturday

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(monday, 200),
				makePoint(monday.Add(30*time.Minute), 192), // -4%
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(saturday, 200),
				makePoint(saturday.Add(30*time.Minute), 212), // +6%
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Weekday 1 (Monday): bias = 1.0 + (-0.04) = 0.96
	if !approxEqual(tb.WeekdayBias[1], 0.96, 0.001) {
		t.Errorf("WeekdayBias[1] = %.4f, want 0.96 (bearish Monday)", tb.WeekdayBias[1])
	}

	// Weekday 6 (Saturday): bias = 1.0 + 0.06 = 1.06
	if !approxEqual(tb.WeekdayBias[6], 1.06, 0.001) {
		t.Errorf("WeekdayBias[6] = %.4f, want 1.06 (bullish Saturday)", tb.WeekdayBias[6])
	}

	// Other weekdays neutral.
	for _, d := range []int{0, 2, 3, 4, 5} {
		if tb.WeekdayBias[d] != 1.0 {
			t.Errorf("WeekdayBias[%d] = %f, want 1.0 (neutral)", d, tb.WeekdayBias[d])
		}
	}
}

func TestComputeTemporalBiases_Volatility(t *testing.T) {
	// Hour 10: two gems with divergent price changes → high volatility.
	// Hour 14: two gems with same price change → zero volatility.
	base := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC) // Monday 10:00
	calm := time.Date(2026, 3, 16, 14, 0, 0, 0, time.UTC)  // Monday 14:00

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(base, 100),
				makePoint(base.Add(30*time.Minute), 110), // +10%
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(base, 100),
				makePoint(base.Add(30*time.Minute), 90), // -10%
			},
		},
		{
			Name: "Gem C of X", Variant: "20/20", GemColor: "GREEN",
			Points: []PricePoint{
				makePoint(calm, 100),
				makePoint(calm.Add(30*time.Minute), 105), // +5%
			},
		},
		{
			Name: "Gem D of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(calm, 100),
				makePoint(calm.Add(30*time.Minute), 105), // +5%
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 10: pctChanges = [+0.10, -0.10], mean=0, σ=0.10
	if !approxEqual(tb.HourlyVolatility[10], 0.10, 0.001) {
		t.Errorf("HourlyVolatility[10] = %.4f, want ~0.10 (high)", tb.HourlyVolatility[10])
	}

	// Hour 14: pctChanges = [+0.05, +0.05], mean=0.05, σ=0.0
	if !approxEqual(tb.HourlyVolatility[14], 0.0, 0.001) {
		t.Errorf("HourlyVolatility[14] = %.4f, want ~0.0 (calm)", tb.HourlyVolatility[14])
	}

	// Volatile hour should have higher volatility than calm hour.
	if tb.HourlyVolatility[10] <= tb.HourlyVolatility[14] {
		t.Errorf("HourlyVolatility[10] (%.4f) should be > HourlyVolatility[14] (%.4f)",
			tb.HourlyVolatility[10], tb.HourlyVolatility[14])
	}
}

func TestComputeTemporalBiases_Activity(t *testing.T) {
	// Hour 6: all gems move > 2% → activity = 1.0
	// Hour 12: all gems flat (< 2%) → activity = 0.0
	active := time.Date(2026, 3, 16, 6, 0, 0, 0, time.UTC) // Monday 06:00
	flat := time.Date(2026, 3, 16, 12, 0, 0, 0, time.UTC)  // Monday 12:00

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(active, 100),
				makePoint(active.Add(30*time.Minute), 110), // +10%, moving
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(active, 100),
				makePoint(active.Add(30*time.Minute), 95), // -5%, moving
			},
		},
		{
			Name: "Gem C of X", Variant: "20/20", GemColor: "GREEN",
			Points: []PricePoint{
				makePoint(flat, 100),
				makePoint(flat.Add(30*time.Minute), 100.5), // +0.5%, flat
			},
		},
		{
			Name: "Gem D of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(flat, 100),
				makePoint(flat.Add(30*time.Minute), 99.5), // -0.5%, flat
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 6: 2/2 moving → activity = 1.0
	if !approxEqual(tb.HourlyActivity[6], 1.0, 0.001) {
		t.Errorf("HourlyActivity[6] = %.4f, want 1.0 (all moving)", tb.HourlyActivity[6])
	}

	// Hour 12: 0/2 moving → activity = 0.0
	if !approxEqual(tb.HourlyActivity[12], 0.0, 0.001) {
		t.Errorf("HourlyActivity[12] = %.4f, want 0.0 (all flat)", tb.HourlyActivity[12])
	}
}

func TestComputeTemporalBiases_MixedActivity(t *testing.T) {
	// Hour 8: 1 moving + 1 flat → activity = 0.5
	base := time.Date(2026, 3, 16, 8, 0, 0, 0, time.UTC) // Monday 08:00

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(base, 100),
				makePoint(base.Add(30*time.Minute), 110), // +10%, moving
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(base, 100),
				makePoint(base.Add(30*time.Minute), 101), // +1%, flat
			},
		},
	}

	tb := computeTemporalBiases(history)

	if !approxEqual(tb.HourlyActivity[8], 0.5, 0.001) {
		t.Errorf("HourlyActivity[8] = %.4f, want 0.5 (half moving)", tb.HourlyActivity[8])
	}
}

func TestComputeTemporalBiases_EmptyHistory(t *testing.T) {
	tb := computeTemporalBiases(nil)

	for i := 0; i < 24; i++ {
		if tb.HourlyBias[i] != 1.0 {
			t.Errorf("HourlyBias[%d] = %f, want 1.0", i, tb.HourlyBias[i])
		}
		if tb.HourlyVolatility[i] != 0 {
			t.Errorf("HourlyVolatility[%d] = %f, want 0", i, tb.HourlyVolatility[i])
		}
		if tb.HourlyActivity[i] != 0 {
			t.Errorf("HourlyActivity[%d] = %f, want 0", i, tb.HourlyActivity[i])
		}
	}
	for i := 0; i < 7; i++ {
		if tb.WeekdayBias[i] != 1.0 {
			t.Errorf("WeekdayBias[%d] = %f, want 1.0", i, tb.WeekdayBias[i])
		}
		if tb.WeekdayVolatility[i] != 0 {
			t.Errorf("WeekdayVolatility[%d] = %f, want 0", i, tb.WeekdayVolatility[i])
		}
		if tb.WeekdayActivity[i] != 0 {
			t.Errorf("WeekdayActivity[%d] = %f, want 0", i, tb.WeekdayActivity[i])
		}
	}
}

func TestComputeTemporalBiases_SingleHour(t *testing.T) {
	// Only hour 15 has data; all others should be neutral/zero.
	base := time.Date(2026, 3, 16, 15, 0, 0, 0, time.UTC) // Monday 15:00

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(base, 100),
				makePoint(base.Add(30*time.Minute), 108), // +8%
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 15: bias = 1.0 + 0.08 = 1.08
	if !approxEqual(tb.HourlyBias[15], 1.08, 0.001) {
		t.Errorf("HourlyBias[15] = %.4f, want 1.08", tb.HourlyBias[15])
	}

	// All other hours neutral.
	for i := 0; i < 24; i++ {
		if i == 15 {
			continue
		}
		if tb.HourlyBias[i] != 1.0 {
			t.Errorf("HourlyBias[%d] = %f, want 1.0 (neutral)", i, tb.HourlyBias[i])
		}
		if tb.HourlyVolatility[i] != 0 {
			t.Errorf("HourlyVolatility[%d] = %f, want 0 (empty)", i, tb.HourlyVolatility[i])
		}
	}
}

func TestComputeTemporalBiases_SkipsZeroPrev(t *testing.T) {
	// Points with prev.Chaos = 0 should be skipped (no division by zero).
	base := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(base, 0),
				makePoint(base.Add(30*time.Minute), 100),
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 10 should still be neutral since the pair was skipped.
	if tb.HourlyBias[10] != 1.0 {
		t.Errorf("HourlyBias[10] = %f, want 1.0 (skipped zero prev)", tb.HourlyBias[10])
	}
}

func TestComputeTemporalBiases_MultiplePointsPerGem(t *testing.T) {
	// A gem with 3 points produces 2 pctChange pairs at different hours.
	t0 := time.Date(2026, 3, 16, 8, 0, 0, 0, time.UTC)  // Monday 08:00
	t1 := time.Date(2026, 3, 16, 9, 0, 0, 0, time.UTC)  // Monday 09:00 (pair 0→1 at hour 8)
	t2 := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC) // Monday 10:00 (pair 1→2 at hour 9)

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(t0, 100),
				makePoint(t1, 110), // +10% at hour 8
				makePoint(t2, 99),  // -10% at hour 9
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Hour 8: pctChange = +0.10, bias = 1.10
	if !approxEqual(tb.HourlyBias[8], 1.10, 0.001) {
		t.Errorf("HourlyBias[8] = %.4f, want 1.10", tb.HourlyBias[8])
	}

	// Hour 9: pctChange = (99-110)/110 = -0.1, bias = 1.0 + (-0.1) = 0.9
	if !approxEqual(tb.HourlyBias[9], 0.9, 0.001) {
		t.Errorf("HourlyBias[9] = %.4f, want 0.90", tb.HourlyBias[9])
	}
}

func TestComputeTemporalBiases_WeekdayVolatility(t *testing.T) {
	// Monday: two gems with opposing changes → σ > 0.
	// Tuesday: one gem with single change → σ = 0 (only 1 sample).
	monday := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)
	tuesday := time.Date(2026, 3, 17, 10, 0, 0, 0, time.UTC)

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(monday, 100),
				makePoint(monday.Add(30*time.Minute), 120), // +20%
			},
		},
		{
			Name: "Gem B of X", Variant: "20/20", GemColor: "BLUE",
			Points: []PricePoint{
				makePoint(monday, 100),
				makePoint(monday.Add(30*time.Minute), 80), // -20%
			},
		},
		{
			Name: "Gem C of X", Variant: "20/20", GemColor: "GREEN",
			Points: []PricePoint{
				makePoint(tuesday, 100),
				makePoint(tuesday.Add(30*time.Minute), 105), // +5%
			},
		},
	}

	tb := computeTemporalBiases(history)

	// Monday (1): pctChanges = [+0.20, -0.20], mean=0, σ=0.20
	if !approxEqual(tb.WeekdayVolatility[1], 0.20, 0.001) {
		t.Errorf("WeekdayVolatility[1] = %.4f, want ~0.20", tb.WeekdayVolatility[1])
	}

	// Tuesday (2): single observation → σ=0
	if !approxEqual(tb.WeekdayVolatility[2], 0.0, 0.001) {
		t.Errorf("WeekdayVolatility[2] = %.4f, want 0.0", tb.WeekdayVolatility[2])
	}
}

func TestComputeTemporalBiases_NaNSafe(t *testing.T) {
	// Ensure no NaN or Inf in output even with extreme values.
	base := time.Date(2026, 3, 16, 10, 0, 0, 0, time.UTC)

	history := []GemPriceHistory{
		{
			Name: "Gem A of X", Variant: "20/20", GemColor: "RED",
			Points: []PricePoint{
				makePoint(base, math.SmallestNonzeroFloat64),
				makePoint(base.Add(30*time.Minute), math.MaxFloat64),
			},
		},
	}

	tb := computeTemporalBiases(history)

	for i := 0; i < 24; i++ {
		if math.IsNaN(tb.HourlyBias[i]) || math.IsInf(tb.HourlyBias[i], 0) {
			t.Errorf("HourlyBias[%d] is NaN/Inf", i)
		}
		if math.IsNaN(tb.HourlyVolatility[i]) || math.IsInf(tb.HourlyVolatility[i], 0) {
			t.Errorf("HourlyVolatility[%d] is NaN/Inf", i)
		}
		if math.IsNaN(tb.HourlyActivity[i]) || math.IsInf(tb.HourlyActivity[i], 0) {
			t.Errorf("HourlyActivity[%d] is NaN/Inf", i)
		}
	}
	for i := 0; i < 7; i++ {
		if math.IsNaN(tb.WeekdayBias[i]) || math.IsInf(tb.WeekdayBias[i], 0) {
			t.Errorf("WeekdayBias[%d] is NaN/Inf", i)
		}
		if math.IsNaN(tb.WeekdayVolatility[i]) || math.IsInf(tb.WeekdayVolatility[i], 0) {
			t.Errorf("WeekdayVolatility[%d] is NaN/Inf", i)
		}
		if math.IsNaN(tb.WeekdayActivity[i]) || math.IsInf(tb.WeekdayActivity[i], 0) {
			t.Errorf("WeekdayActivity[%d] is NaN/Inf", i)
		}
	}
}
