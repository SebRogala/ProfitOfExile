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
	if got := points[0].FuturePct; !approxEqual(got, 10.0) {
		t.Errorf("Spark futurePct: got %.2f, want 10.0", got)
	}
	// Ice Nova: (180-200)/200 * 100 = -10%
	if got := points[1].FuturePct; !approxEqual(got, -10.0) {
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
	if got := points[0].FuturePct; !approxEqual(got, 15.0) {
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
	if got := points[0].FuturePct; !approxEqual(got, 30.0) {
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
	if got := points[0].FuturePct; !approxEqual(got, 20.0) {
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

// approxEqual compares two floats within a small tolerance.
func approxEqual(a, b float64) bool {
	const epsilon = 0.01
	diff := a - b
	if diff < 0 {
		diff = -diff
	}
	return diff < epsilon
}
