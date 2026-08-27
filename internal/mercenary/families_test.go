package mercenary

import (
	"encoding/json"
	"os"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// families.go is generated from the mercenary vocabulary fixture, and a
// generated file goes stale silently: a league that adds support links changes
// the fixture, the desktop picks the new families up on its next build, and the
// server keeps rejecting them as unknown with nothing to say why.
//
// This test re-derives the set from the fixture at test time and compares. It
// does NOT prove the derivation rule is right — the rule is stated once here
// and once in the generator, so a shared misreading survives both. What it
// proves is that the committed map still matches the committed fixture, which
// is the failure mode that actually happens.
const vocabFixturePath = "../../desktop/src/lib/mercenaries/__fixtures__/mercenary-stats.json"

// supportIDPrefix marks a support-link entry. Skills carry no tier and are
// never read from a cell icon, so they contribute no families
// (vocab.rs:40-43).
const supportIDPrefix = "mercenary.support_"

// gradePrefixes are the grade words a support name can lead with. Stripping one
// is what collapses `Lesser Chain (Tier 1)`, `Chain (Tier 2)` and
// `Gilded Chain (Tier 3)` onto the single family `Chain` (vocab.rs:49); the
// alias table below then folds that result onto its destination family, which
// is the third and last step of the derivation.
var gradePrefixes = []string{"Lesser ", "Greater ", "Gilded "}

// splitTier drops a trailing " (Tier N)", mirroring vocab.rs:131-145: the
// suffix counts only when the text ends in ')' and the characters between are
// digits.
func splitTier(text string) string {
	trimmed := strings.TrimRight(text, " \t")
	open := strings.LastIndex(trimmed, " (Tier ")
	if open < 0 || !strings.HasSuffix(trimmed, ")") {
		return trimmed
	}
	digits := trimmed[open+len(" (Tier ") : len(trimmed)-1]
	if _, err := strconv.Atoi(digits); err != nil {
		return trimmed
	}
	return trimmed[:open]
}

// stripGrade removes one leading grade word, mirroring vocab.rs:147-155.
func stripGrade(name string) string {
	for _, prefix := range gradePrefixes {
		if rest, found := strings.CutPrefix(name, prefix); found {
			return rest
		}
	}
	return name
}

// familyAliases folds display names that are two spellings of ONE icon family
// onto one key, mirroring vocab.rs:51-68. Applied AFTER the grade strip, and an
// EXACT match on the whole derived name: `Increased Angle` is a real
// Gilded-only support with no `Angle` sibling, so a generic `Increased ` strip
// would invent a family the vocabulary does not carry (POE-211).
var familyAliases = map[string]string{
	"Increased Area of Effect": "Area of Effect",
}

// aliasFamily applies familyAliases, mirroring vocab.rs:157-173.
func aliasFamily(name string) string {
	if to, ok := familyAliases[name]; ok {
		return to
	}
	return name
}

// supportTextsFromFixture is every support entry's display text, verbatim.
func supportTextsFromFixture(t *testing.T) []string {
	t.Helper()

	raw, err := os.ReadFile(vocabFixturePath)
	if err != nil {
		t.Fatalf("read the mercenary vocabulary fixture: %v", err)
	}
	var vocab struct {
		Entries []struct {
			ID   string `json:"id"`
			Text string `json:"text"`
		} `json:"entries"`
	}
	if err := json.Unmarshal(raw, &vocab); err != nil {
		t.Fatalf("parse the mercenary vocabulary fixture: %v", err)
	}
	if len(vocab.Entries) == 0 {
		t.Fatal("the mercenary vocabulary fixture carries no entries")
	}

	var texts []string
	for _, entry := range vocab.Entries {
		if !strings.HasPrefix(entry.ID, supportIDPrefix) {
			continue
		}
		texts = append(texts, entry.Text)
	}
	return texts
}

// derivedFamily is the whole three-step rule, in the order vocab.rs applies it.
func derivedFamily(displayText string) string {
	return aliasFamily(stripGrade(splitTier(displayText)))
}

func familiesFromFixture(t *testing.T) map[string]struct{} {
	t.Helper()

	families := make(map[string]struct{})
	for _, text := range supportTextsFromFixture(t) {
		families[derivedFamily(text)] = struct{}{}
	}
	return families
}

func TestKnownFamilies_MatchTheShippedVocabularyFixture(t *testing.T) {
	want := familiesFromFixture(t)

	var missing, extra []string
	for family := range want {
		if _, ok := knownFamilies[family]; !ok {
			missing = append(missing, family)
		}
	}
	for family := range knownFamilies {
		if _, ok := want[family]; !ok {
			extra = append(extra, family)
		}
	}
	sort.Strings(missing)
	sort.Strings(extra)

	if len(missing) > 0 {
		t.Errorf("families in the fixture but not in families.go (regenerate it): %v", missing)
	}
	if len(extra) > 0 {
		t.Errorf("families in families.go but not in the fixture (regenerate it): %v", extra)
	}
}

// The count is asserted separately so a drift failure names the size change
// before it names 153 individual strings.
func TestKnownFamilies_CountMatchesTheFixture(t *testing.T) {
	if got, want := KnownFamilyCount(), len(familiesFromFixture(t)); got != want {
		t.Fatalf("KnownFamilyCount() = %d, fixture derives %d", got, want)
	}
}

// The derivation collapses grades onto one family. If it stopped, `Chain` would
// be three families and a tier-1 confirmation would no longer bootstrap tier 3.
func TestKnownFamilies_CollapseGradePrefixesOntoOneFamily(t *testing.T) {
	if _, ok := knownFamilies["Chain"]; !ok {
		t.Fatal("family \"Chain\" is absent; the grade prefixes are not being stripped")
	}
	for _, graded := range []string{"Lesser Chain", "Greater Chain", "Gilded Chain"} {
		if _, ok := knownFamilies[graded]; ok {
			t.Errorf("%q was kept as its own family; the grade word must be stripped", graded)
		}
	}
}

// The alias folds two display spellings onto one family. GGG names the tiers of
// one support `Lesser Increased Area of Effect (Tier 1)`,
// `Increased Area of Effect (Tier 2)` and `Greater Area of Effect (Tier 3)`, so
// the grade strip alone derived TWO families out of one support and the same
// art was uploaded under two keys — neither of which could then ever be matched
// (POE-211).
func TestKnownFamilies_FoldTheAliasedFamily(t *testing.T) {
	fixture := make(map[string]struct{})
	for _, text := range supportTextsFromFixture(t) {
		fixture[text] = struct{}{}
	}
	for _, display := range []string{
		"Lesser Increased Area of Effect (Tier 1)",
		"Increased Area of Effect (Tier 2)",
		"Greater Area of Effect (Tier 3)",
	} {
		if _, ok := fixture[display]; !ok {
			t.Fatalf("precondition: %q is no longer in the vocabulary fixture", display)
		}
		if got := derivedFamily(display); got != "Area of Effect" {
			t.Errorf("%q derives family %q, want %q", display, got, "Area of Effect")
		}
	}

	if _, ok := knownFamilies["Area of Effect"]; !ok {
		t.Error("family \"Area of Effect\" is absent; the fold has no destination")
	}
	if _, ok := knownFamilies["Increased Area of Effect"]; ok {
		t.Error("\"Increased Area of Effect\" is still its own family; the alias must fold it")
	}

	// The alias matches the whole name, never a prefix: `Increased Angle` is a
	// real Gilded-only support with no `Angle` sibling to fold onto.
	if _, ok := knownFamilies["Increased Angle"]; !ok {
		t.Error("\"Increased Angle\" was folded; the alias table is a closed list of whole names")
	}
	if _, ok := knownFamilies["Angle"]; ok {
		t.Error("\"Angle\" is not a vocabulary family; a prefix strip invented it")
	}
}

// A tier suffix is part of the display text, never part of the family.
func TestKnownFamilies_CarryNoTierSuffix(t *testing.T) {
	for family := range knownFamilies {
		if strings.Contains(family, "(Tier ") {
			t.Errorf("family %q still carries a tier suffix", family)
		}
	}
}
