package lab

import "testing"

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
