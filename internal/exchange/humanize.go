package exchange

import (
	"strings"
	"unicode"
)

// categoryPrefixes are the leading words stripped from an item id's last
// segment. Humanize strips the first one that matches; order does not matter
// because no entry is a prefix of another.
var categoryPrefixes = []string{"DivinationCard", "Currency", "Scarab"}

// Humanize turns a feed item id into a display name.
//
// It takes the last path segment, strips one leading category word (Currency,
// DivinationCard, Scarab) when something is left, and splits the remaining
// CamelCase on lower-to-upper and letter-to-digit boundaries:
//
//	Metadata/Items/Currency/CurrencyRerollRare                      -> "Reroll Rare"
//	Metadata/Items/Scarabs/ScarabTormentNew4                        -> "Torment New 4"
//	Metadata/Items/DivinationCards/DivinationCardThunderousSkies    -> "Thunderous Skies"
//	Metadata/Items/Currency/AncestralOmenOnJewellersMakeFullSockets -> "Ancestral Omen On Jewellers Make Full Sockets"
//
// An unknown shape returns its CamelCase-split last segment. This is the
// FALLBACK for ids the embedded asset in items.go does not cover, so names are
// readable but not guaranteed to match the in-game wording.
func Humanize(id string) string {
	name := id
	if idx := strings.LastIndex(name, "/"); idx >= 0 {
		name = name[idx+1:]
	}
	for _, prefix := range categoryPrefixes {
		if len(name) > len(prefix) && strings.HasPrefix(name, prefix) {
			name = name[len(prefix):]
			break
		}
	}
	return splitCamelCase(name)
}

// splitCamelCase inserts a space at every lower-to-upper and letter-to-digit
// boundary.
func splitCamelCase(s string) string {
	var b strings.Builder
	b.Grow(len(s) + len(s)/4)
	var prev rune
	for i, r := range s {
		if i > 0 && boundary(prev, r) {
			b.WriteRune(' ')
		}
		b.WriteRune(r)
		prev = r
	}
	return b.String()
}

// boundary reports whether a space belongs between prev and cur.
func boundary(prev, cur rune) bool {
	if unicode.IsLower(prev) && unicode.IsUpper(cur) {
		return true
	}
	return unicode.IsLetter(prev) && unicode.IsDigit(cur)
}
