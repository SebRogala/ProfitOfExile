package gemrule

import "testing"

func TestKeepCritical_IsHeistOnlyNameMatchesTrarthusAnywhereInTheName(t *testing.T) {
	cases := []struct {
		name string
		want bool
	}{
		{"Sunder of Trarthus", true},
		{"Wave of Conviction of Trarthus", true},
		{"Trarthus Ire", true},
		{"Cyclone", false},
		{"Empower Support", false},
		{"", false},
	}
	for _, c := range cases {
		if got := IsHeistOnlyName(c.name); got != c.want {
			t.Errorf("IsHeistOnlyName(%q) = %v, want %v", c.name, got, c.want)
		}
	}
}
