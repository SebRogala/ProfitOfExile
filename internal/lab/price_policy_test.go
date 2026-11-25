package lab

import (
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/price/evidence"
	"profitofexile/internal/price/resolve"
)

func TestGemPricePolicy_MaxRevisitPreservesThresholds(t *testing.T) {
	want := resolve.Policy{ListingCount: 3, Disagreement: 3, ThinDisagreement: 2, ThinSample: 5, MaxRevisitAge: 6 * time.Hour}
	if got := GemPricePolicy(); got != want {
		t.Fatalf("policy = %+v, want %+v", got, want)
	}
}

func TestFontTradeVariants_FreshFourMarketSet(t *testing.T) {
	want := []string{"1", "1/20", "20", "20/20"}
	first := FontTradeVariants()
	if !reflect.DeepEqual(first, want) {
		t.Fatalf("variants = %v, want %v", first, want)
	}
	first[0] = "corrupted"
	if got := FontTradeVariants(); !reflect.DeepEqual(got, want) {
		t.Fatalf("variants after caller mutation = %v, want %v", got, want)
	}
}

func TestGemPricePolicy_thinSampleMatchesTheLabThinRule(t *testing.T) {
	evidenceFloor := GemEvidencePolicy().NinjaSampleAbove
	resolverThin := GemPricePolicy().ThinSample

	if evidenceFloor != resolverThin {
		t.Errorf("GemEvidencePolicy().NinjaSampleAbove = %d, GemPricePolicy().ThinSample = %d — the evidence floor and the resolver's thin rule must agree", evidenceFloor, resolverThin)
	}
}

// keep:critical — silent: an off-by-one at the floor admits a 1-2 ask price
// into tiers, TOP and Font EV, which later steps trust.
func TestKeepCritical_GemEvidencePolicyGradesAtTheRuledFloor(t *testing.T) {
	sample := func(n int) *int { return &n }
	tests := []struct {
		name  string
		depth evidence.Depth
		want  evidence.Evidence
	}{
		{"trade 3 listings 2 sellers", evidence.Depth{PricedBy: evidence.SourceTrade, TradeListings: 3, TradeSellers: 2}, evidence.Solid},
		{"trade 3 listings 1 seller", evidence.Depth{PricedBy: evidence.SourceTrade, TradeListings: 3, TradeSellers: 1}, evidence.Thin},
		{"trade 2 listings 2 sellers", evidence.Depth{PricedBy: evidence.SourceTrade, TradeListings: 2, TradeSellers: 2}, evidence.Thin},
		{"ninja sample 6 listings 3", evidence.Depth{PricedBy: evidence.SourceNinja, NinjaListings: 3, NinjaSample: sample(6)}, evidence.Solid},
		{"ninja sample 5 listings 3", evidence.Depth{PricedBy: evidence.SourceNinja, NinjaListings: 3, NinjaSample: sample(5)}, evidence.Thin},
		{"ninja sample 6 listings 2", evidence.Depth{PricedBy: evidence.SourceNinja, NinjaListings: 2, NinjaSample: sample(6)}, evidence.Thin},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := evidence.Grade(tt.depth, GemEvidencePolicy()); got != tt.want {
				t.Errorf("evidence.Grade(%+v, GemEvidencePolicy()) = %v, want %v", tt.depth, got, tt.want)
			}
		})
	}
}
