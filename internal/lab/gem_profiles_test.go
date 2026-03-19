package lab

import (
	"math"
	"testing"
	"time"
)

// --- countFloods tests ---

func TestCountFloods_SingleMassiveSpike(t *testing.T) {
	// 10 points with one massive listing spike. σ of positive deltas is small,
	// so a +20 jump on a gem where normal deltas are ~1 should exceed 4σ.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 11},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 12},
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 13},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 14},
		// Massive flood: +20 listings in one interval
		{Time: t0.Add(150 * time.Minute), Chaos: 100, Listings: 34},
		{Time: t0.Add(180 * time.Minute), Chaos: 100, Listings: 35},
		{Time: t0.Add(210 * time.Minute), Chaos: 100, Listings: 36},
		{Time: t0.Add(240 * time.Minute), Chaos: 100, Listings: 37},
		{Time: t0.Add(270 * time.Minute), Chaos: 100, Listings: 38},
	}
	got := countFloods(points)
	if got != 1 {
		t.Errorf("countFloods(single spike) = %d, want 1", got)
	}
}

func TestCountFloods_TwoSpikes(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 11},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 12},
		// First flood
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 35},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 36},
		{Time: t0.Add(150 * time.Minute), Chaos: 100, Listings: 37},
		// Second flood
		{Time: t0.Add(180 * time.Minute), Chaos: 100, Listings: 60},
		{Time: t0.Add(210 * time.Minute), Chaos: 100, Listings: 61},
		{Time: t0.Add(240 * time.Minute), Chaos: 100, Listings: 62},
		{Time: t0.Add(270 * time.Minute), Chaos: 100, Listings: 63},
	}
	got := countFloods(points)
	if got != 2 {
		t.Errorf("countFloods(two spikes) = %d, want 2", got)
	}
}

func TestCountFloods_GradualGrowth(t *testing.T) {
	// Gradual listing growth — no spikes.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 12},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 14},
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 16},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 18},
		{Time: t0.Add(150 * time.Minute), Chaos: 100, Listings: 20},
	}
	got := countFloods(points)
	if got != 0 {
		t.Errorf("countFloods(gradual) = %d, want 0", got)
	}
}

func TestCountFloods_SmallAbsoluteDelta_BelowFloor(t *testing.T) {
	// Delta of +4 on σ=1 gem: exceeds 4σ but absolute delta < 5 → no flood.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 2},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 2},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 2},
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 2},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 2},
		// +4 jump: big in σ terms, but absolute < 5
		{Time: t0.Add(150 * time.Minute), Chaos: 100, Listings: 6},
	}
	got := countFloods(points)
	if got != 0 {
		t.Errorf("countFloods(below floor) = %d, want 0", got)
	}
}

func TestCountFloods_AboveFloor(t *testing.T) {
	// Delta of +6 on a gem with zero-variation history: MAD=0 triggers fallback
	// (effectiveMAD=1), threshold = 0 + 4*1 = 4. Delta 6 >= 4 AND 6 >= 5 → flood.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 3},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 3},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 3},
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 3},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 3},
		// +6 jump on a dead gem: absolute >= 5 and exceeds fallback threshold
		{Time: t0.Add(150 * time.Minute), Chaos: 100, Listings: 9},
	}
	got := countFloods(points)
	if got != 1 {
		t.Errorf("countFloods(spike on dead gem) = %d, want 1", got)
	}
}

func TestCountFloods_TooFewPoints(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 30},
	}
	got := countFloods(points)
	if got != 0 {
		t.Errorf("countFloods(<5 points) = %d, want 0", got)
	}
}

func TestCountFloods_AllSameListings(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 100, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 100, Listings: 10},
		{Time: t0.Add(60 * time.Minute), Chaos: 100, Listings: 10},
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 10},
		{Time: t0.Add(120 * time.Minute), Chaos: 100, Listings: 10},
	}
	got := countFloods(points)
	if got != 0 {
		t.Errorf("countFloods(constant listings) = %d, want 0", got)
	}
}

// --- countCrashes tests ---

func TestCountCrashes_PriceDropWithListingRise(t *testing.T) {
	// Price drops sharply while listings rise → crash.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 300, Listings: 5},
		{Time: t0.Add(30 * time.Minute), Chaos: 295, Listings: 6},
		{Time: t0.Add(60 * time.Minute), Chaos: 290, Listings: 7},
		{Time: t0.Add(90 * time.Minute), Chaos: 285, Listings: 8},
		{Time: t0.Add(120 * time.Minute), Chaos: 280, Listings: 9},
		// Massive crash: price drops from ~280 to 60 while listings spike
		{Time: t0.Add(150 * time.Minute), Chaos: 60, Listings: 28},
	}
	got := countCrashes(points)
	if got != 1 {
		t.Errorf("countCrashes(price drop + listing rise) = %d, want 1", got)
	}
}

func TestCountCrashes_PriceDropButListingsDrop(t *testing.T) {
	// Price drops but listings also dropped — not a flood-crash.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 300, Listings: 20},
		{Time: t0.Add(30 * time.Minute), Chaos: 290, Listings: 18},
		{Time: t0.Add(60 * time.Minute), Chaos: 280, Listings: 16},
		{Time: t0.Add(90 * time.Minute), Chaos: 270, Listings: 14},
		{Time: t0.Add(120 * time.Minute), Chaos: 260, Listings: 12},
		{Time: t0.Add(150 * time.Minute), Chaos: 60, Listings: 10},
	}
	got := countCrashes(points)
	if got != 0 {
		t.Errorf("countCrashes(listings also dropped) = %d, want 0", got)
	}
}

func TestCountCrashes_TwoCrashes(t *testing.T) {
	// Two distinct crash events separated in time.
	t0 := time.Date(2026, 3, 15, 0, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 300, Listings: 5},
		{Time: t0.Add(30 * time.Minute), Chaos: 295, Listings: 6},
		{Time: t0.Add(60 * time.Minute), Chaos: 290, Listings: 7},
		// First crash
		{Time: t0.Add(90 * time.Minute), Chaos: 100, Listings: 25},
		// Recovery
		{Time: t0.Add(24 * time.Hour), Chaos: 300, Listings: 5},
		{Time: t0.Add(24*time.Hour + 30*time.Minute), Chaos: 295, Listings: 6},
		{Time: t0.Add(24*time.Hour + 60*time.Minute), Chaos: 290, Listings: 7},
		// Second crash
		{Time: t0.Add(24*time.Hour + 90*time.Minute), Chaos: 80, Listings: 30},
	}
	got := countCrashes(points)
	if got != 2 {
		t.Errorf("countCrashes(two crashes) = %d, want 2", got)
	}
}

func TestCountCrashes_GradualDecline(t *testing.T) {
	// Gradual price decline — no sharp drops.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 200, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 195, Listings: 11},
		{Time: t0.Add(60 * time.Minute), Chaos: 190, Listings: 12},
		{Time: t0.Add(90 * time.Minute), Chaos: 185, Listings: 13},
		{Time: t0.Add(120 * time.Minute), Chaos: 180, Listings: 14},
		{Time: t0.Add(150 * time.Minute), Chaos: 175, Listings: 15},
	}
	got := countCrashes(points)
	if got != 0 {
		t.Errorf("countCrashes(gradual decline) = %d, want 0", got)
	}
}

func TestCountCrashes_TooFewPoints(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 300, Listings: 5},
		{Time: t0.Add(30 * time.Minute), Chaos: 60, Listings: 30},
	}
	got := countCrashes(points)
	if got != 0 {
		t.Errorf("countCrashes(<5 points) = %d, want 0", got)
	}
}

// --- computeListingElasticity tests ---

func TestComputeListingElasticity_Healthy(t *testing.T) {
	// Price drops when listings rise → negative elasticity.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 200, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 190, Listings: 12},
		{Time: t0.Add(60 * time.Minute), Chaos: 180, Listings: 14},
		{Time: t0.Add(90 * time.Minute), Chaos: 175, Listings: 15},
		{Time: t0.Add(120 * time.Minute), Chaos: 170, Listings: 16},
	}
	got := computeListingElasticity(points)
	if got >= 0 {
		t.Errorf("computeListingElasticity(healthy) = %f, want < 0", got)
	}
}

func TestComputeListingElasticity_Insensitive(t *testing.T) {
	// Price flat when listings change → near-zero elasticity.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 200, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 200, Listings: 15},
		{Time: t0.Add(60 * time.Minute), Chaos: 200, Listings: 20},
		{Time: t0.Add(90 * time.Minute), Chaos: 200, Listings: 25},
		{Time: t0.Add(120 * time.Minute), Chaos: 200, Listings: 30},
	}
	got := computeListingElasticity(points)
	if math.Abs(got) > 0.01 {
		t.Errorf("computeListingElasticity(insensitive) = %f, want ~0", got)
	}
}

func TestComputeListingElasticity_TooFewPoints(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 200, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 150, Listings: 30},
	}
	got := computeListingElasticity(points)
	if got != 0 {
		t.Errorf("computeListingElasticity(<5 points) = %f, want 0", got)
	}
}

func TestComputeListingElasticity_NoListingChange(t *testing.T) {
	// Listings don't change → can't compute elasticity → 0.
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 200, Listings: 10},
		{Time: t0.Add(30 * time.Minute), Chaos: 180, Listings: 10},
		{Time: t0.Add(60 * time.Minute), Chaos: 160, Listings: 10},
		{Time: t0.Add(90 * time.Minute), Chaos: 140, Listings: 10},
		{Time: t0.Add(120 * time.Minute), Chaos: 120, Listings: 10},
	}
	got := computeListingElasticity(points)
	if got != 0 {
		t.Errorf("computeListingElasticity(no listing change) = %f, want 0", got)
	}
}

func TestComputeListingElasticity_NoNaN(t *testing.T) {
	t0 := time.Date(2026, 3, 15, 8, 0, 0, 0, time.UTC)
	points := []PricePoint{
		{Time: t0, Chaos: 0, Listings: 0},
		{Time: t0.Add(30 * time.Minute), Chaos: 0, Listings: 0},
		{Time: t0.Add(60 * time.Minute), Chaos: 0, Listings: 0},
		{Time: t0.Add(90 * time.Minute), Chaos: 0, Listings: 0},
		{Time: t0.Add(120 * time.Minute), Chaos: 0, Listings: 0},
	}
	got := computeListingElasticity(points)
	if math.IsNaN(got) || math.IsInf(got, 0) {
		t.Error("computeListingElasticity returned NaN/Inf, want 0")
	}
}

func TestComputeListingElasticity_Nil(t *testing.T) {
	got := computeListingElasticity(nil)
	if got != 0 {
		t.Errorf("computeListingElasticity(nil) = %f, want 0", got)
	}
}
