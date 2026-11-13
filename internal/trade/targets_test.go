package trade

import (
	"strings"
	"testing"
)

func TestFixedTargets_RegistryIdentitiesInOrder(t *testing.T) {
	got := FixedTargets()
	want := []struct {
		gem  string
		role FixedTargetRole
	}{
		{"font-lens-60-outcome", FixedTargetRoleOutcome},
		{"font-lens-60-feeder", FixedTargetRoleFeeder},
	}
	if len(got) != len(want) {
		t.Fatalf("registry has %d targets, want %d", len(got), len(want))
	}
	for i, w := range want {
		if got[i].Gem != w.gem || got[i].Variant != "font-craft" || got[i].Role != w.role {
			t.Errorf("target %d = %s|%s %s, want %s|font-craft %s", i, got[i].Gem, got[i].Variant, got[i].Role, w.gem, w.role)
		}
		if strings.Contains(got[i].Gem, " of ") {
			t.Errorf("identity %q contains \" of \" and would collide with the transfigured filter", got[i].Gem)
		}
		if got[i].Key() != CacheKey(w.gem, "font-craft") {
			t.Errorf("key = %q, want CacheKey of the identity", got[i].Key())
		}
	}
}

func TestLookupFixedTarget_RejectsIdentitiesOutsideRegistry(t *testing.T) {
	for _, tc := range []struct{ gem, variant string }{
		{"font-lens-60-outcome", "20/20"},
		{"Empower Support", "font-craft"},
		{"font-lens-30-outcome", "font-craft"},
	} {
		if _, ok := LookupFixedTarget(tc.gem, tc.variant); ok {
			t.Errorf("LookupFixedTarget(%q, %q) found a target", tc.gem, tc.variant)
		}
		if IsFixedTargetKey(CacheKey(tc.gem, tc.variant)) {
			t.Errorf("IsFixedTargetKey(%q|%q) = true", tc.gem, tc.variant)
		}
	}
	if !IsFixedTargetKey(CacheKey(FixedTargetLens60Feeder, FixedTargetVariantFont)) {
		t.Error("feeder key not recognised as a fixed target")
	}
}
