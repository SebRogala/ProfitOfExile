package gemcolor

import (
	"sort"
	"testing"
)

func TestResolveTransfigured(t *testing.T) {
	colors := map[string]string{
		"Rain of Arrows":  "GREEN",
		"Cleave":          "RED",
		"Arc":             "BLUE",
		"Ball Lightning":  "BLUE",
		"Raise Zombie":    "BLUE",
		"Herald of Purity": "RED",
	}

	tests := []struct {
		name      string
		input     string
		wantColor string
		wantFound bool
	}{
		{
			name:      "single of suffix",
			input:     "Arc of Surging",
			wantColor: "BLUE",
			wantFound: true,
		},
		{
			name:      "double of - strips rightmost first",
			input:     "Rain of Arrows of Saturation",
			wantColor: "GREEN",
			wantFound: true,
		},
		{
			name:      "no of suffix - no match",
			input:     "Spark",
			wantColor: "",
			wantFound: false,
		},
		{
			name:      "of in base name - strips rightmost only",
			input:     "Herald of Purity of Zeal",
			wantColor: "RED",
			wantFound: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			color, found := resolveTransfigured(tt.input, colors)
			if found != tt.wantFound {
				t.Errorf("found = %v, want %v", found, tt.wantFound)
			}
			if color != tt.wantColor {
				t.Errorf("color = %q, want %q", color, tt.wantColor)
			}
		})
	}
}

func TestFindAllPositions(t *testing.T) {
	tests := []struct {
		name   string
		s      string
		substr string
		want   []int
	}{
		{"no match", "hello", " of ", nil},
		{"single", "Rain of Arrows", " of ", []int{4}},
		{"double", "Rain of Arrows of Saturation", " of ", []int{4, 14}},
		{"triple", "A of B of C of D", " of ", []int{1, 6, 11}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := findAllPositions(tt.s, tt.substr)
			if len(got) != len(tt.want) {
				t.Fatalf("len = %d, want %d; got %v", len(got), len(tt.want), got)
			}
			for i, v := range got {
				if v != tt.want[i] {
					t.Errorf("pos[%d] = %d, want %d", i, v, tt.want[i])
				}
			}
		})
	}
}

func TestResolve_heuristics(t *testing.T) {
	// Build a resolver with a pre-populated color map (no DB needed).
	r := &Resolver{
		colors: map[string]string{
			"Cleave":              "RED",
			"Arc":                 "BLUE",
			"Rain of Arrows":     "GREEN",
			"Multiple Projectiles Support": "GREEN",
		},
		discovered: make(map[string]string),
		unresolved: make(map[string]struct{}),
	}

	tests := []struct {
		name      string
		gem       string
		wantColor string
		wantFound bool
	}{
		{"direct lookup", "Cleave", "RED", true},
		{"vaal prefix", "Vaal Cleave", "RED", true},
		{"greater prefix", "Greater Multiple Projectiles Support", "GREEN", true},
		{"transfigured suffix", "Arc of Surging", "BLUE", true},
		{"vaal + transfigured", "Vaal Rain of Arrows of Saturation", "GREEN", true},
		{"unknown gem", "Sparkle Beam", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			color, found := r.Resolve(tt.gem)
			if found != tt.wantFound {
				t.Errorf("found = %v, want %v", found, tt.wantFound)
			}
			if color != tt.wantColor {
				t.Errorf("color = %q, want %q", color, tt.wantColor)
			}
		})
	}

	// Verify discovered gems were cached.
	if _, ok := r.discovered["Vaal Cleave"]; !ok {
		t.Error("expected Vaal Cleave in discovered map")
	}
	if _, ok := r.discovered["Arc of Surging"]; !ok {
		t.Error("expected Arc of Surging in discovered map")
	}

	// Verify unresolved tracking.
	unresolved := r.UnresolvedGems()
	sort.Strings(unresolved)
	if len(unresolved) != 1 || unresolved[0] != "Sparkle Beam" {
		t.Errorf("unresolved = %v, want [Sparkle Beam]", unresolved)
	}
}

func TestResolve_caching(t *testing.T) {
	r := &Resolver{
		colors: map[string]string{
			"Arc": "BLUE",
		},
		discovered: make(map[string]string),
		unresolved: make(map[string]struct{}),
	}

	// First call resolves via heuristic.
	color1, ok1 := r.Resolve("Arc of Surging")
	// Second call should hit the cache.
	color2, ok2 := r.Resolve("Arc of Surging")

	if !ok1 || !ok2 {
		t.Fatal("expected both calls to resolve")
	}
	if color1 != color2 {
		t.Errorf("cached result differs: %q vs %q", color1, color2)
	}
	if color1 != "BLUE" {
		t.Errorf("color = %q, want BLUE", color1)
	}
}
