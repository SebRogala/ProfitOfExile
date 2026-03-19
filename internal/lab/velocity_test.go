package lab

import (
	"math"
	"testing"
	"time"
)

func TestVelocityWindow_Empty(t *testing.T) {
	v := velocityWindow(nil, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocityWindow(nil) = %f, want 0", v)
	}
}

func TestVelocityWindow_SinglePoint(t *testing.T) {
	points := []PricePoint{
		{Time: time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC), Chaos: 100, Listings: 10},
	}
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocityWindow(1 point) = %f, want 0", v)
	}
}

func TestVelocityWindow_SameTimestamp(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0, Chaos: 110, Listings: 15},
	}
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocityWindow(same time) = %f, want 0", v)
	}
}

func TestVelocityWindow_TwoPoints(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(time.Hour), Chaos: 110, Listings: 15},
	}
	// 2h window includes both points: (110-100)/1h = 10
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-10) > 0.01 {
		t.Errorf("velocityWindow price = %f, want 10", v)
	}
	vl := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return float64(p.Listings) })
	if math.Abs(vl-5) > 0.01 {
		t.Errorf("velocityWindow listings = %f, want 5", vl)
	}
}

func TestVelocityWindow_2hSelectsCorrectPoints(t *testing.T) {
	// 6 points spanning 2.5 hours at 30min intervals.
	// 2h window from last point (t0+150min): cutoff = t0+30min.
	// First point >= cutoff is index 1 (t0+30min).
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},                           // outside 2h window
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 8},     // first in 2h window
		{Time: t0.Add(60 * time.Minute), Chaos: 70, Listings: 10},
		{Time: t0.Add(90 * time.Minute), Chaos: 75, Listings: 12},
		{Time: t0.Add(120 * time.Minute), Chaos: 80, Listings: 14},
		{Time: t0.Add(150 * time.Minute), Chaos: 90, Listings: 16},   // last point
	}
	// Delta = (90-60) / 2h = 15
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-15) > 0.01 {
		t.Errorf("velocityWindow 2h = %f, want 15", v)
	}
}

func TestVelocityWindow_1hSelectsCorrectPoints(t *testing.T) {
	// Same 6 points, 1h window should use only points in last hour.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 8},
		{Time: t0.Add(60 * time.Minute), Chaos: 70, Listings: 10},
		{Time: t0.Add(90 * time.Minute), Chaos: 75, Listings: 12},
		{Time: t0.Add(120 * time.Minute), Chaos: 80, Listings: 14},
		{Time: t0.Add(150 * time.Minute), Chaos: 90, Listings: 16},
	}
	// cutoff = t0+150min - 1h = t0+90min. First point >= cutoff is index 3 (t0+90min).
	// Delta = (90-75) / 1h = 15
	v := velocityWindow(points, 1*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-15) > 0.01 {
		t.Errorf("velocityWindow 1h = %f, want 15", v)
	}
}

func TestVelocityWindow_6hUsesAllPoints(t *testing.T) {
	// Same 6 points spanning 2.5h, 6h window should include all.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 8},
		{Time: t0.Add(60 * time.Minute), Chaos: 70, Listings: 10},
		{Time: t0.Add(90 * time.Minute), Chaos: 75, Listings: 12},
		{Time: t0.Add(120 * time.Minute), Chaos: 80, Listings: 14},
		{Time: t0.Add(150 * time.Minute), Chaos: 90, Listings: 16},
	}
	// All points within 6h window: (90-50) / 2.5h = 16
	v := velocityWindow(points, 6*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-16) > 0.01 {
		t.Errorf("velocityWindow 6h = %f, want 16", v)
	}
}

func TestVelocityWindow_FewerThan2InWindow(t *testing.T) {
	// Only 1 point falls within a very short window.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},
		{Time: t0.Add(2 * time.Hour), Chaos: 100, Listings: 10},
	}
	// 30min window: cutoff = t0+2h-30min = t0+90min. Only last point qualifies.
	v := velocityWindow(points, 30*time.Minute, func(p PricePoint) float64 { return p.Chaos })
	if v != 0 {
		t.Errorf("velocityWindow(<2 in window) = %f, want 0", v)
	}
}

func TestVelocityWindow_5minCadence(t *testing.T) {
	// Future stream cadence: 5-min intervals. Same 2h window should still work.
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	var points []PricePoint
	// 25 points over 2 hours at 5-min intervals: price goes 100 -> 120 linearly
	for i := 0; i <= 24; i++ {
		points = append(points, PricePoint{
			Time:     t0.Add(time.Duration(i) * 5 * time.Minute),
			Chaos:    100 + float64(i)*20.0/24.0,
			Listings: 10,
		})
	}
	// Last point at t0+120min, first in 2h window is t0 (all within 2h).
	// Delta = (120-100) / 2h = 10
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.Abs(v-10) > 0.1 {
		t.Errorf("velocityWindow 5min cadence = %f, want ~10", v)
	}
}

func TestVelocity_BackwardCompatible(t *testing.T) {
	// Verify that velocity() produces the same result as velocityWindow(2h)
	// for standard 30-min cadence data.
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 105, Listings: 12},
		{Time: t0.Add(60 * time.Minute), Chaos: 110, Listings: 14},
		{Time: t0.Add(90 * time.Minute), Chaos: 120, Listings: 16},
	}
	extract := func(p PricePoint) float64 { return p.Chaos }
	vOld := velocity(points, extract)
	vNew := velocityWindow(points, 2*time.Hour, extract)
	if math.Abs(vOld-vNew) > 0.001 {
		t.Errorf("velocity() = %f, velocityWindow(2h) = %f, want equal", vOld, vNew)
	}
}

func TestVelocity_BackwardCompatible_5Points(t *testing.T) {
	// With 5 points spanning 2h, old velocity used last 4 (point-count).
	// New velocity uses 2h window (time-based). Both should use all points
	// that fall within the 2h window.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 50, Listings: 5},                         // t0
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 8},   // t0+30min
		{Time: t0.Add(60 * time.Minute), Chaos: 70, Listings: 10},  // t0+60min
		{Time: t0.Add(90 * time.Minute), Chaos: 75, Listings: 12},  // t0+90min
		{Time: t0.Add(120 * time.Minute), Chaos: 80, Listings: 14}, // t0+120min
	}
	// 2h window from t0+120min: cutoff = t0. All 5 points are in window.
	// Old velocity used last 4: points[1] to points[4] = (80-60)/1.5h = 13.33
	// New velocity 2h window: all 5 in window: (80-50)/2h = 15
	// These differ because the old implementation was point-count-based.
	// The new implementation is time-based and should use all points in window.
	v := velocity(points, func(p PricePoint) float64 { return p.Chaos })
	// (80-50)/2h = 15
	if math.Abs(v-15) > 0.01 {
		t.Errorf("velocity(5 points, 2h span) = %f, want 15", v)
	}
}

func TestVelocityWindow_NaNProtection(t *testing.T) {
	// Ensure NaN/Inf from extreme values are sanitized.
	t0 := time.Date(2026, 3, 15, 10, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 0, Listings: 0},
		{Time: t0.Add(time.Hour), Chaos: 0, Listings: 0},
	}
	v := velocityWindow(points, 2*time.Hour, func(p PricePoint) float64 { return p.Chaos })
	if math.IsNaN(v) || math.IsInf(v, 0) {
		t.Errorf("velocityWindow returned NaN/Inf, want 0")
	}
}
