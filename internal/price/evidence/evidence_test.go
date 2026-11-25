package evidence

import (
	"testing"
	"time"
)

func intPtr(n int) *int { return &n }

// rulingPolicy is the ruled Lab gem floor (trade 3 listings from 2 sellers;
// ninja sample above 5 with 3 listings), written literally because this leaf
// imports no profitofexile package.
var rulingPolicy = Policy{TradeListings: 3, TradeSellers: 2, NinjaSampleAbove: 5, NinjaListings: 3}

// keep:happy — the four ruled acceptance examples.
func TestKeepHappy_GradeCovensAndSmiteExamples(t *testing.T) {
	tests := []struct {
		name  string
		depth Depth
		want  Evidence
	}{
		{"Covens BLUE 20/20 gem A ninja 1 listing sample 1", Depth{PricedBy: SourceNinja, NinjaListings: 1, NinjaSample: intPtr(1)}, Thin},
		{"Covens BLUE 20/20 gem B ninja 2 listings sample 2", Depth{PricedBy: SourceNinja, NinjaListings: 2, NinjaSample: intPtr(2)}, Thin},
		{"Covens BLUE 20/20 gem C ninja 1 listing sample 1", Depth{PricedBy: SourceNinja, NinjaListings: 1, NinjaSample: intPtr(1)}, Thin},
		{"Smite of Divine Judgement 20/20 ninja 23 listings sample 23", Depth{PricedBy: SourceNinja, NinjaListings: 23, NinjaSample: intPtr(23)}, Solid},
		{"trade-priced key 3 listings from 2 sellers", Depth{PricedBy: SourceTrade, TradeListings: 3, TradeSellers: 2}, Solid},
		{"priced by none despite deep trade and ninja depth", Depth{PricedBy: SourceNone, TradeListings: 10, TradeSellers: 5, NinjaListings: 23, NinjaSample: intPtr(23)}, None},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Grade(tt.depth, rulingPolicy); got != tt.want {
				t.Errorf("Grade(%+v) = %v, want %v", tt.depth, got, tt.want)
			}
		})
	}
}

// keep:critical — silent: an unmeasured ninja line graded Solid counts in Font EV.
func TestKeepCritical_GradeUnknownSampleIsNotSolid(t *testing.T) {
	d := Depth{PricedBy: SourceNinja, NinjaListings: 20, NinjaSample: nil}
	if got := Grade(d, rulingPolicy); got != Thin {
		t.Errorf("Grade(ninja, sample nil, listings 20) = %v, want Thin (%v)", got, Thin)
	}
}

// keep:critical — silent: trade depth certifying a ninja 1-listing price would
// admit that price into tiers, TOP and Font EV.
func TestKeepCritical_GradeReadsOnlyThePricingSource(t *testing.T) {
	d := Depth{PricedBy: SourceNinja, NinjaListings: 1, NinjaSample: intPtr(1), TradeListings: 10, TradeSellers: 5}
	if got := Grade(d, rulingPolicy); got != Thin {
		t.Errorf("Grade(ninja-priced 1/1 with trade depth 10/5) = %v, want Thin (%v)", got, Thin)
	}
}

// keep:happy — ruled acceptance: a sold-through Smite holds its last SOLID
// grade; a key that was never SOLID has no hold and stays Thin.
func TestKeepHappy_GradeHoldsSoldThroughSmite(t *testing.T) {
	policy := rulingPolicy
	policy.HoldWindow = 24 * time.Hour
	policy.HoldBand = 2

	// Smite of Divine Judgement 20/20 at 2026-11-16 06:44Z: 111c with 1 listing,
	// SOLID at 04:44Z at 138c with 9 listings.
	smiteAt := time.Date(2026, 11, 16, 6, 44, 0, 0, time.UTC)
	lastSolidAt := time.Date(2026, 11, 16, 4, 44, 0, 0, time.UTC)

	tests := []struct {
		name  string
		depth Depth
		want  Evidence
	}{
		{"Smite of Divine Judgement 20/20 111c 1 listing held by 138c SOLID two hours earlier",
			Depth{PricedBy: SourceNinja, NinjaListings: 1, NinjaSample: intPtr(1), Hold: &Hold{LastSolidPrice: 138, LastSolidAge: smiteAt.Sub(lastSolidAt)}}, Solid},
		{"never-SOLID 527c 1 listing 20/20 key",
			Depth{PricedBy: SourceNinja, NinjaListings: 1, NinjaSample: intPtr(1)}, Thin},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Grade(tt.depth, policy); got != tt.want {
				t.Errorf("Grade(%+v) = %v, want %v", tt.depth, got, tt.want)
			}
		})
	}
}
