package temple

import (
	"encoding/json"
	"os"
	"reflect"
	"sort"
	"testing"
)

func TestRecipes_transformsTheBaseIntoTheUpgrade(t *testing.T) {
	// The direction is the fact that is easy to invert: the vial's own text names
	// the item you SACRIFICE. Coward's Chains is the belt you give up; Coward's
	// Legacy is what you get. A table with those two swapped would price the
	// recipe backwards and tell a player to buy the expensive item as input.
	want := map[string][2]string{
		"Vial of Awakening":   {"Apep's Slumber", "Apep's Supremacy"},
		"Vial of Consequence": {"Coward's Chains", "Coward's Legacy"},
		"Vial of Dominance":   {"Architect's Hand", "Slavedriver's Hand"},
		"Vial of Fate":        {"Story of the Vaal", "Fate of the Vaal"},
		"Vial of Sacrifice":   {"Sacrificial Heart", "Zerphi's Heart"},
		"Vial of Summoning":   {"Mask of the Spirit Drinker", "Mask of the Stitched Demon"},
		"Vial of the Ghost":   {"Soul Catcher", "Soul Ripper"},
		"Vial of the Ritual":  {"Dance of the Offered", "Omeyocan"},
	}

	got := make(map[string][2]string)
	for _, r := range Recipes() {
		if r.Vial == "Vial of Transcendence" {
			continue // three recipes; covered by its own test
		}
		got[r.Vial] = [2]string{r.Base, r.Upgraded}
	}

	if !reflect.DeepEqual(got, want) {
		t.Errorf("single-recipe vials = %v, want %v", got, want)
	}
}

func TestRecipes_vialOfTranscendenceUpgradesAllThreeTemperedJewels(t *testing.T) {
	want := map[string]string{
		"Tempered Flesh":  "Transcendent Flesh",
		"Tempered Mind":   "Transcendent Mind",
		"Tempered Spirit": "Transcendent Spirit",
	}

	got := make(map[string]string)
	for _, r := range Recipes() {
		if r.Vial == "Vial of Transcendence" {
			got[r.Base] = r.Upgraded
		}
	}

	if !reflect.DeepEqual(got, want) {
		t.Errorf("Vial of Transcendence recipes = %v, want %v", got, want)
	}
}

func TestRecipes_hasElevenRecipesAcrossNineVials(t *testing.T) {
	got := Recipes()
	if len(got) != 11 {
		t.Errorf("recipe count = %d, want 11", len(got))
	}

	vials := make(map[string]struct{})
	for _, r := range got {
		vials[r.Vial] = struct{}{}
	}
	if len(vials) != 9 {
		t.Errorf("distinct vial count = %d, want 9", len(vials))
	}
}

func TestRecipes_returnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	first := Recipes()
	first[0].Upgraded = "mutated by the caller"

	if second := Recipes(); second[0].Upgraded == "mutated by the caller" {
		t.Errorf("Recipes()[0].Upgraded = %q after a caller wrote to an earlier result; want the table unchanged", second[0].Upgraded)
	}
}

func TestItemNames_isTheDeduplicatedUnionOfEveryRecipeMember(t *testing.T) {
	want := make(map[string]struct{})
	for _, r := range Recipes() {
		want[r.Vial] = struct{}{}
		want[r.Base] = struct{}{}
		want[r.Upgraded] = struct{}{}
	}

	got := ItemNames()
	if len(got) != len(want) {
		t.Errorf("ItemNames() length = %d, want %d (the 31 distinct members of 11 recipes)", len(got), len(want))
	}
	for _, name := range got {
		if _, ok := want[name]; !ok {
			t.Errorf("ItemNames() carries %q, which is in no recipe", name)
		}
		delete(want, name)
	}
	for name := range want {
		t.Errorf("ItemNames() is missing recipe member %q", name)
	}
	if !sort.StringsAreSorted(got) {
		t.Errorf("ItemNames() = %v, want it sorted so the payload is deterministic", got)
	}
}

func TestItemNames_returnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	first := ItemNames()
	first[0] = "mutated by the caller"

	if second := ItemNames(); second[0] == "mutated by the caller" {
		t.Errorf("ItemNames()[0] = %q after a caller wrote to an earlier result; want the set unchanged", second[0])
	}
}

// TestRecipes_everyMemberIsALiveFeedName pins the table's names to poe.ninja's,
// which is the only reason the price lookup works: Market.Items is keyed by the
// feed's `name`, so a name that reads correctly to a human but differs from the
// feed by an apostrophe or a space prices as unpriced, silently and forever.
//
// The fixture is an independent capture — every distinct name the seven
// item-overview categories served for league Allflame on 2026-09-06, taken from
// the feed rather than derived from this table.
func TestRecipes_everyMemberIsALiveFeedName(t *testing.T) {
	raw, err := os.ReadFile("testdata/ninja-item-names-allflame-20260906.json")
	if err != nil {
		t.Fatalf("read the captured feed names: %v", err)
	}
	var byCategory map[string][]string
	if err := json.Unmarshal(raw, &byCategory); err != nil {
		t.Fatalf("parse the captured feed names: %v", err)
	}

	live := make(map[string]string)
	for category, names := range byCategory {
		for _, name := range names {
			live[name] = category
		}
	}
	if len(live) < 1000 {
		t.Fatalf("captured feed holds %d distinct names; the fixture looks truncated", len(live))
	}

	for _, name := range ItemNames() {
		if _, ok := live[name]; !ok {
			t.Errorf("%q is in the recipe table but not in the captured poe.ninja feed; the price lookup would report it unpriced", name)
		}
	}
}

// TestRecipes_noMemberIsARoomLine guards the other direction: a room-tier name
// dropped into the recipe table would be looked up in the item half and priced
// against the wrong market's median.
func TestRecipes_noMemberIsARoomLine(t *testing.T) {
	raw, err := os.ReadFile("testdata/ninja-item-names-allflame-20260906.json")
	if err != nil {
		t.Fatalf("read the captured feed names: %v", err)
	}
	var byCategory map[string][]string
	if err := json.Unmarshal(raw, &byCategory); err != nil {
		t.Fatalf("parse the captured feed names: %v", err)
	}

	rooms := make(map[string]struct{}, len(byCategory[RoomCategory]))
	for _, name := range byCategory[RoomCategory] {
		rooms[name] = struct{}{}
	}
	if len(rooms) != 86 {
		t.Fatalf("captured %s names = %d, want the 86 room-tier lines", RoomCategory, len(rooms))
	}

	for _, name := range ItemNames() {
		if _, isRoom := rooms[name]; isRoom {
			t.Errorf("recipe member %q is a %s room line, not an item", name, RoomCategory)
		}
	}
}
