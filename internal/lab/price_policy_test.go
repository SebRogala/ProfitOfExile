package lab

import (
	"reflect"
	"testing"
	"time"

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
	policy := GemPricePolicy()
	five := 5

	if policy.ThinSample != 5 {
		t.Errorf("GemPricePolicy().ThinSample = %d, want 5", policy.ThinSample)
	}
	if !(GemPrice{SampleCount: &five}).ThinSample() {
		t.Error("GemPrice{SampleCount: 5}.ThinSample() = false, want true — the policy and the lab rule must agree at 5")
	}
}
