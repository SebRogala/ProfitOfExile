package lab

import (
	"fmt"
	"testing"
	"time"
)

var holdAsOf = time.Date(2026, 11, 16, 6, 44, 0, 0, time.UTC)

const heldName = "Held Gem of Test"

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
		{"last SOLID at 24h plus 1s", []evidencePoint{heldPoint(holdAsOf.Add(-24*time.Hour-time.Second), 120, 9, 9)}, false},
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
