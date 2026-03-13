package gemcolor

import (
	"sort"
	"testing"
)

func TestResolveTransfigured(t *testing.T) {
	colors := map[string]Color{
		"Rain of Arrows":   ColorGreen,
		"Cleave":           ColorRed,
		"Arc":              ColorBlue,
		"Ball Lightning":   ColorBlue,
		"Raise Zombie":     ColorBlue,
		"Herald of Purity": ColorRed,
	}

	tests := []struct {
		name      string
		input     string
		wantColor Color
		wantFound bool
	}{
		{
			name:      "single of suffix",
			input:     "Arc of Surging",
			wantColor: ColorBlue,
			wantFound: true,
		},
		{
			name:      "double of - strips rightmost first",
			input:     "Rain of Arrows of Saturation",
			wantColor: ColorGreen,
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
			wantColor: ColorRed,
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


func TestResolve_heuristics(t *testing.T) {
	// Build a resolver with a pre-populated color map (no DB needed).
	r := &Resolver{
		colors: map[string]Color{
			"Cleave":                        ColorRed,
			"Arc":                           ColorBlue,
			"Rain of Arrows":               ColorGreen,
			"Multiple Projectiles Support":  ColorGreen,
		},
		discovered: make(map[string]Color),
		unresolved: make(map[string]struct{}),
	}

	tests := []struct {
		name      string
		gem       string
		wantColor Color
		wantFound bool
	}{
		{"direct lookup", "Cleave", ColorRed, true},
		{"vaal prefix", "Vaal Cleave", ColorRed, true},
		{"greater prefix", "Greater Multiple Projectiles Support", ColorGreen, true},
		{"transfigured suffix", "Arc of Surging", ColorBlue, true},
		{"vaal + transfigured", "Vaal Rain of Arrows of Saturation", ColorGreen, true},
		{"greater + transfigured", "Greater Multiple Projectiles Support of Spreading", ColorGreen, true},
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
		colors: map[string]Color{
			"Arc": ColorBlue,
		},
		discovered: make(map[string]Color),
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
	if color1 != ColorBlue {
		t.Errorf("color = %q, want BLUE", color1)
	}
}

func TestParseColor(t *testing.T) {
	tests := []struct {
		input   string
		want    Color
		wantErr bool
	}{
		{"RED", ColorRed, false},
		{"GREEN", ColorGreen, false},
		{"BLUE", ColorBlue, false},
		{"WHITE", ColorWhite, false},
		{"red", "", true},
		{"YELLOW", "", true},
		{"", "", true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got, err := ParseColor(tt.input)
			if (err != nil) != tt.wantErr {
				t.Errorf("ParseColor(%q) error = %v, wantErr %v", tt.input, err, tt.wantErr)
			}
			if got != tt.want {
				t.Errorf("ParseColor(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}
