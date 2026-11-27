package lab

import (
	"testing"
	"time"

	"profitofexile/internal/price/tier"
)

var tierInputAsOf = time.Date(2026, 11, 20, 12, 0, 0, 0, time.UTC)

// keep:critical — silent: stale prices re-enter the tier ladder and move stable gems.
func TestKeepCritical_DeriveTierInputsUsesOnlyConsecutivePointsEndingAtAsOf(t *testing.T) {
	for _, tc := range []struct {
		name      string
		points    []PricePoint
		raw, want float64
		depth     int
	}{
		{"gap breaks chain", []PricePoint{{Time: tierInputAsOf.Add(-200 * time.Minute), Chaos: 100, Listings: 100}, {Time: tierInputAsOf.Add(-30 * time.Minute), Chaos: 30, Listings: 9}, {Time: tierInputAsOf, Chaos: 34, Listings: 21}}, 34, 32, 9},
		{"three newest", []PricePoint{{Time: tierInputAsOf.Add(-90 * time.Minute), Chaos: 900, Listings: 90}, {Time: tierInputAsOf.Add(-60 * time.Minute), Chaos: 18, Listings: 18}, {Time: tierInputAsOf.Add(-30 * time.Minute), Chaos: 30, Listings: 30}, {Time: tierInputAsOf, Chaos: 24, Listings: 24}}, 24, 24, 24},
		{"no asOf point", []PricePoint{{Time: tierInputAsOf.Add(-30 * time.Minute), Chaos: 100, Listings: 100}}, 4, 4, 12},
		{"no history", nil, 4, 4, 12},
	} {
		t.Run(tc.name, func(t *testing.T) {
			g := GemPrice{Name: "Input of Test", Variant: "20/20", Chaos: tc.raw, Listings: 12, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"}
			history := []GemPriceHistory{{Name: g.Name, Variant: g.Variant, Points: tc.points}, {Name: g.Name, Variant: "1", Points: []PricePoint{{Time: tierInputAsOf, Chaos: 9999, Listings: 9999}}}}
			got := deriveTierInputs([]GemPrice{g}, history, nil, time.Time{}, false, nil, tierInputAsOf)
			items := tierItems(got, isFontOutcome)
			if len(items) != 1 {
				t.Fatalf("tier inputs = %+v, want one Font input", items)
			}
			if items[0].Price != tc.want || items[0].Depth != tc.depth {
				t.Errorf("tier price/depth = %g/%d, want %g/%d", items[0].Price, items[0].Depth, tc.want, tc.depth)
			}
			if got[0].Chaos != g.Chaos || got[0].Listings != g.Listings {
				t.Errorf("displayed price/depth = %g/%d, want observed %g/%d", got[0].Chaos, got[0].Listings, g.Chaos, g.Listings)
			}
		})
	}
}

// keep:critical — silent: a stale previous label freezes a new market's tier.
func TestKeepCritical_DeriveTierInputsDropsAPreviousTierOlderThanTheMaxGap(t *testing.T) {
	g := GemPrice{Name: "Memory of Test", Variant: "20/20", Chaos: 49, Listings: 20, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"}
	prev := GemClassificationMap{{Name: g.Name, Variant: g.Variant}: {Tier: tier.High}}
	for _, tc := range []struct {
		age  time.Duration
		want string
	}{{60 * time.Minute, ""}, {30 * time.Minute, tier.High}} {
		t.Run(tc.age.String(), func(t *testing.T) {
			items := tierItems(deriveTierInputs([]GemPrice{g}, nil, prev, tierInputAsOf.Add(-tc.age), true, nil, tierInputAsOf), isFontOutcome)
			if len(items) != 1 {
				t.Fatalf("tier inputs = %+v, want one", items)
			}
			if items[0].Prev != tc.want {
				t.Errorf("Prev = %q, want %q", items[0].Prev, tc.want)
			}
		})
	}
}

// keep:critical — silent: an absent previous member re-enters the cutoff at 5.4c.
func TestKeepCritical_DeriveTierInputsMarksAFontGemMissingFromThePreviousSnapshotExcluded(t *testing.T) {
	g := GemPrice{Name: "Missing of Test", Variant: "20/20", Chaos: 5.4, Listings: 20, SampleCount: solidSample(), IsTransfigured: true, GemColor: "BLUE"}
	for _, tc := range []struct {
		name  string
		prev  GemClassificationMap
		found bool
		want  string
	}{
		{"absent key", GemClassificationMap{{Name: g.Name, Variant: "1"}: {Tier: tier.High}}, true, tier.Excluded},
		{"empty tier", GemClassificationMap{{Name: g.Name, Variant: g.Variant}: {}}, true, tier.Excluded},
		{"cold start", nil, false, ""},
	} {
		t.Run(tc.name, func(t *testing.T) {
			items := tierItems(deriveTierInputs([]GemPrice{g}, nil, tc.prev, tierInputAsOf.Add(-30*time.Minute), tc.found, nil, tierInputAsOf), isFontOutcome)
			if len(items) != 1 {
				t.Fatalf("tier inputs = %+v, want one", items)
			}
			if items[0].Prev != tc.want {
				t.Errorf("Prev = %q, want %q", items[0].Prev, tc.want)
			}
		})
	}
}
