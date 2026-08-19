package exchange

import "testing"

func TestHumanize(t *testing.T) {
	tests := []struct {
		name string
		id   string
		want string
	}{
		{
			name: "strips the Currency category word and splits CamelCase",
			id:   "Metadata/Items/Currency/CurrencyRerollRare",
			want: "Reroll Rare",
		},
		{
			name: "divine orb id",
			id:   "Metadata/Items/Currency/CurrencyModValues",
			want: "Mod Values",
		},
		{
			name: "strips only the leading category word, not later occurrences",
			id:   "Metadata/Items/Currency/CurrencyRerollRareVeiledChaos",
			want: "Reroll Rare Veiled Chaos",
		},
		{
			name: "splits a trailing digit off the last word",
			id:   "Metadata/Items/Scarabs/ScarabTormentNew4",
			want: "Torment New 4",
		},
		{
			name: "DivinationCard wins over the shorter category words",
			id:   "Metadata/Items/DivinationCards/DivinationCardThunderousSkies",
			want: "Thunderous Skies",
		},
		{
			name: "an id with no category word keeps every word",
			id:   "Metadata/Items/Currency/AncestralOmenOnJewellersMakeFullSockets",
			want: "Ancestral Omen On Jewellers Make Full Sockets",
		},
		{
			name: "a bare CamelCase name without a path is split in place",
			id:   "AstrolabeGeneric",
			want: "Astrolabe Generic",
		},
		{
			name: "a digit does not start a new word after itself",
			id:   "Metadata/Items/Currency/AncestralTattooKitava1",
			want: "Ancestral Tattoo Kitava 1",
		},
		{
			name: "a last segment equal to a category word keeps that word",
			id:   "Metadata/Items/Currency/Currency",
			want: "Currency",
		},
		{
			name: "an already spaced acronym-free single word is unchanged",
			id:   "Chaos",
			want: "Chaos",
		},
		{
			name: "an id ending in a slash yields an empty name",
			id:   "Metadata/Items/Currency/",
			want: "",
		},
		{
			name: "an empty id yields an empty name",
			id:   "",
			want: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := Humanize(tt.id); got != tt.want {
				t.Errorf("Humanize(%q) = %q, want %q", tt.id, got, tt.want)
			}
		})
	}
}
