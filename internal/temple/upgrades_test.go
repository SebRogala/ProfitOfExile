package temple

import (
	"encoding/json"
	"os"
	"reflect"
	"strings"
	"testing"
)

func TestUniqueUpgrades_transformsTheBaseIntoTheUpgrade(t *testing.T) {
	// The direction is the fact that is easy to invert: Coward's Chains is the
	// base; Coward's Legacy is the upgrade. A table with those two swapped would
	// price the pair backwards. The three Tempered jewels each upgrade to their
	// own Transcendent counterpart.
	want := map[string]string{
		"Apep's Slumber":             "Apep's Supremacy",
		"Architect's Hand":           "Slavedriver's Hand",
		"Coward's Chains":            "Coward's Legacy",
		"Dance of the Offered":       "Omeyocan",
		"Mask of the Spirit Drinker": "Mask of the Stitched Demon",
		"Sacrificial Heart":          "Zerphi's Heart",
		"Soul Catcher":               "Soul Ripper",
		"Story of the Vaal":          "Fate of the Vaal",
		"Tempered Flesh":             "Transcendent Flesh",
		"Tempered Mind":              "Transcendent Mind",
		"Tempered Spirit":            "Transcendent Spirit",
	}

	got := make(map[string]string)
	for _, u := range uniqueUpgrades {
		got[u.Base] = u.Upgraded
	}

	if !reflect.DeepEqual(got, want) {
		t.Errorf("unique upgrades = %v, want %v", got, want)
	}
}

func TestItemNames_isTheBasesAndUpgradesWithNoVial(t *testing.T) {
	// 3.30 removed vials; the served set is the eleven pairs' 22 uniques only.
	want := []string{
		"Apep's Slumber", "Apep's Supremacy", "Architect's Hand", "Coward's Chains",
		"Coward's Legacy", "Dance of the Offered", "Fate of the Vaal",
		"Mask of the Spirit Drinker", "Mask of the Stitched Demon", "Omeyocan",
		"Sacrificial Heart", "Slavedriver's Hand", "Soul Catcher", "Soul Ripper",
		"Story of the Vaal", "Tempered Flesh", "Tempered Mind", "Tempered Spirit",
		"Transcendent Flesh", "Transcendent Mind", "Transcendent Spirit", "Zerphi's Heart",
	}

	got := ItemNames()
	if !reflect.DeepEqual(got, want) {
		t.Errorf("ItemNames() = %v, want the sorted 22 bases and upgrades %v", got, want)
	}
	for _, name := range got {
		if strings.HasPrefix(name, "Vial ") {
			t.Errorf("ItemNames() still serves vial %q", name)
		}
	}
}

func TestItemNames_returnsACopyTheCallerCannotWriteThrough(t *testing.T) {
	first := ItemNames()
	first[0] = "mutated by the caller"

	if second := ItemNames(); second[0] == "mutated by the caller" {
		t.Errorf("ItemNames()[0] = %q after a caller wrote to an earlier result; want the set unchanged", second[0])
	}
}

// TestUniqueUpgrades_everyMemberIsALiveFeedName pins the table's names to poe.ninja's,
// which is the only reason the price lookup works: Market.Items is keyed by the
// feed's `name`, so a name that reads correctly to a human but differs from the
// feed by an apostrophe or a space prices as unpriced, silently and forever.
//
// The fixture is an independent capture — every distinct name the seven
// item-overview categories served for league Allflame on 2026-09-06, taken from
// the feed rather than derived from this table.
func TestUniqueUpgrades_everyMemberIsALiveFeedName(t *testing.T) {
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
			t.Errorf("%q is in the upgrade table but not in the captured poe.ninja feed; the price lookup would report it unpriced", name)
		}
	}
}

// TestUniqueUpgrades_noMemberIsARoomLine guards the other direction: a room-tier name
// dropped into the upgrade table would be looked up in the item half and priced
// against the wrong market's median.
func TestUniqueUpgrades_noMemberIsARoomLine(t *testing.T) {
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
			t.Errorf("upgrade member %q is a %s room line, not an item", name, RoomCategory)
		}
	}
}
