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
// (vocab.rs:42-45).
const supportIDPrefix = "mercenary.support_"

// gradePrefixes are the grade words a support name can lead with. Stripping one
// is what collapses `Lesser Chain (Tier 1)`, `Chain (Tier 2)` and
// `Gilded Chain (Tier 3)` onto the single family `Chain` (vocab.rs:49).
var gradePrefixes = []string{"Lesser ", "Greater ", "Gilded "}

// splitTier drops a trailing " (Tier N)", mirroring vocab.rs:110-122: the
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

// stripGrade removes one leading grade word, mirroring vocab.rs:125-131.
func stripGrade(name string) string {
	for _, prefix := range gradePrefixes {
		if rest, found := strings.CutPrefix(name, prefix); found {
			return rest
		}
	}
	return name
}

func familiesFromFixture(t *testing.T) map[string]struct{} {
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

	families := make(map[string]struct{})
	for _, entry := range vocab.Entries {
		if !strings.HasPrefix(entry.ID, supportIDPrefix) {
			continue
		}
		families[stripGrade(splitTier(entry.Text))] = struct{}{}
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
// before it names 154 individual strings.
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

// A tier suffix is part of the display text, never part of the family.
func TestKnownFamilies_CarryNoTierSuffix(t *testing.T) {
	for family := range knownFamilies {
		if strings.Contains(family, "(Tier ") {
			t.Errorf("family %q still carries a tier suffix", family)
		}
	}
}
