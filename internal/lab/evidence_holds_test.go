package lab

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

var holdAsOf = time.Date(2026, 11, 16, 6, 44, 0, 0, time.UTC)

const heldName = "Held Gem of Test"

// keep:critical — an unnecessary history read can prevent an already-SOLID
// snapshot from reaching classification when history is unavailable.
func TestKeepCritical_LoadEvidenceHoldsSkipsHistoryForSolidSnapshot(t *testing.T) {
	gems := []GemPrice{
		{Name: "Smite of Divine Judgement", Variant: "20/20", Chaos: 138, Listings: 9, SampleCount: solidSample(), IsTransfigured: true, GemColor: "RED"},
		{Name: "Spark of Nova", Variant: "1/20", Chaos: 75, Listings: 3, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"},
	}

	// A nil repository makes any history query fail instead of hiding it.
	got, err := LoadEvidenceHolds(context.Background(), nil, league.Historical("Covens"), gems, holdAsOf)
	if err != nil {
		t.Fatalf("all-SOLID snapshot: %v", err)
	}
	if !reflect.DeepEqual(got, gems) {
		t.Fatalf("all-SOLID snapshot changed: got %+v, want %+v", got, gems)
	}
}

// heldSnapshot is a shallow 20/20 market of floor-SOLID peers (3 listings)
// plus heldName at 100c with 2 listings and a thin sample: it fails the floor
// and is relatively deep enough (2 ≥ 0.4 × 3) that only the evidence grade
// decides whether it is low-confidence.
func heldSnapshot() []GemPrice {
	var gems []GemPrice
	for i, price := range []float64{300, 250, 200, 150, 120, 90, 80, 60, 40} {
		gems = append(gems, GemPrice{Name: fmt.Sprintf("Peer %d of Test", i), Variant: "20/20", Chaos: price,
			Listings: 3, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"})
	}
	thin := 1
	return append(gems, GemPrice{Name: heldName, Variant: "20/20", Chaos: 100, Listings: 2,
		SampleCount: &thin, IsTransfigured: true, GemColor: "BLUE"})
}

func heldPoint(at time.Time, chaos float64, listings, sample int) evidencePoint {
	return evidencePoint{Name: heldName, Variant: "20/20", Time: at, Chaos: chaos, Listings: listings, SampleCount: &sample}
}

// keep:critical — silent: an outlier ask or an endlessly chained hold would be
// admitted to EV.
func TestKeepCritical_HoldRejectsNeverSolidRepricedStaleAndChainedHolds(t *testing.T) {
	tests := []struct {
		name     string
		points   []evidencePoint
		wantHeld bool
	}{
		// The one accepting row: without it every rejection row would pass
		// against a derivation that holds nothing.
		{"SOLID two hours ago within 2x is held", []evidencePoint{heldPoint(holdAsOf.Add(-2*time.Hour), 120, 9, 9)}, true},
		{"never SOLID", []evidencePoint{heldPoint(holdAsOf.Add(-2*time.Hour), 110, 1, 1), heldPoint(holdAsOf.Add(-time.Hour), 105, 2, 2)}, false},
		{"price over 2x the last SOLID price", []evidencePoint{heldPoint(holdAsOf.Add(-2*time.Hour), 40, 9, 9)}, false},
		{"last SOLID at 12h", []evidencePoint{heldPoint(holdAsOf.Add(-12*time.Hour), 120, 9, 9)}, true},
		{"last SOLID at 12h plus 1s", []evidencePoint{heldPoint(holdAsOf.Add(-12*time.Hour-time.Second), 120, 9, 9)}, false},
		{"last SOLID was itself only a hold", []evidencePoint{
			heldPoint(holdAsOf.Add(-26*time.Hour), 120, 9, 9), // floor-SOLID, now stale
			heldPoint(holdAsOf.Add(-10*time.Hour), 110, 1, 1), // held by the point above, not floor-SOLID
		}, false},
		{"SOLID point after asOf is ignored", []evidencePoint{heldPoint(holdAsOf.Add(time.Hour), 120, 9, 9)}, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			held := deriveEvidenceHolds(heldSnapshot(), tt.points, holdAsOf)

			got, ok := ComputeGemClassification(held).Gems[GemClassificationKey{heldName, "20/20"}]
			if !ok {
				t.Fatalf("%s missing from the classification", heldName)
			}
			if got.LowConfidence == tt.wantHeld {
				t.Errorf("LowConfidence = %v, want %v (held = %v)", got.LowConfidence, !tt.wantHeld, tt.wantHeld)
			}
		})
	}
}

// keep:critical — silent: an inflated ask becomes the valuation used by EV consumers.
func TestKeepCritical_HeldPriceIsCappedWithoutChangingObservedInput(t *testing.T) {
	for _, tt := range []struct {
		current, want float64
	}{{120, 100}, {90, 90}} {
		t.Run(fmt.Sprint(tt.current), func(t *testing.T) {
			gems := heldSnapshot()
			gems[len(gems)-1].Chaos = tt.current
			before := append([]GemPrice(nil), gems...)
			points := []evidencePoint{heldPoint(holdAsOf.Add(-time.Hour), 100, 9, 9)}
			got := deriveEvidenceHolds(gems, points, holdAsOf)
			g := got[len(got)-1]
			if g.hold == nil || g.Chaos != tt.want {
				t.Errorf("held price = %g, hold = %+v, want %g with provenance", g.Chaos, g.hold, tt.want)
			}
			if !reflect.DeepEqual(gems, before) {
				t.Errorf("observed input changed: got %+v, want %+v", gems, before)
			}
			if g.Listings != before[len(before)-1].Listings || g.SampleCount != before[len(before)-1].SampleCount {
				t.Error("held valuation changed observed evidence depth")
			}
		})
	}
}

// keep:critical — silent: an over-band ask receives a plausible trusted price tier.
func TestKeepCritical_OverBandHoldRejectionForcesChaotic(t *testing.T) {
	gems := heldSnapshot()
	key := GemClassificationKey{heldName, "20/20"}
	ordinary := tier.Classify(tierItems(gems, isFontOutcome), labPolicy).Items[tier.Key{Group: key.Variant, Key: key.Name}]
	if ordinary.Tier == tier.Chaotic || ordinary.Tier == "" {
		t.Fatalf("fixture ordinary tier = %+v, want a non-CHAOTIC price band", ordinary)
	}
	held := deriveEvidenceHolds(gems, []evidencePoint{heldPoint(holdAsOf.Add(-time.Hour), 40, 9, 9)}, holdAsOf)
	if held[len(held)-1].hold != nil {
		t.Error("price above 2x was held")
	}
	got := ComputeGemClassification(held).Gems[key]
	if got.Tier != tier.Chaotic || !got.LowConfidence {
		t.Errorf("over-band classification = %+v, want CHAOTIC and low confidence (ordinary tier %s)", got, ordinary.Tier)
	}
}

func TestKeepHappy_HeldSmiteCountsAtObservedLowerPrice(t *testing.T) {
	gems := heldSnapshot()
	for i := range gems {
		gems[i].Listings = 40
	}
	smite := &gems[len(gems)-1]
	smite.Name, smite.Chaos, smite.Listings = "Smite of Divine Judgement", 111, 1
	point := heldPoint(holdAsOf.Add(-2*time.Hour), 138, 9, 9)
	point.Name = smite.Name
	held := deriveEvidenceHolds(gems, []evidencePoint{point}, holdAsOf)
	got, ok := ComputeGemClassification(held).Gems[GemClassificationKey{smite.Name, smite.Variant}]
	if !ok || got.LowConfidence || held[len(held)-1].Chaos != 111 || held[len(held)-1].hold == nil {
		t.Errorf("Smite = %+v, classification = %+v (present %v), want held at 111c and confident", held[len(held)-1], got, ok)
	}
}
