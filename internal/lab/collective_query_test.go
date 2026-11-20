package lab

import "testing"

func TestKeepCritical_CollectiveVariantsMatchSparklineVariants(t *testing.T) {
	if len(collectiveVariants) != len(sparklineVariants) {
		t.Fatalf("collectiveVariants has %d entries, sparklineVariants has %d", len(collectiveVariants), len(sparklineVariants))
	}
	for _, v := range sparklineVariants {
		if !collectiveVariants[v] {
			t.Errorf("collectiveVariants is missing %q", v)
		}
	}
}
