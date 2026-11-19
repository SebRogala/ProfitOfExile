package temple

import (
	_ "embed"
	"encoding/json"
	"fmt"
	"sort"
)

// THE UNIQUE UPGRADE TABLE — the server's consumer of the item-level temple
// facts, and the source of the served item set.
//
// The pairs' one normative home is the chest uniques in
// desktop/src-tauri/src/temple/drops.rs. unique_upgrades.json is the projection
// generated from that table and embedded here; this file holds no pair of its
// own.
//
// Each row pairs a base unique with the upgraded unique it becomes. 3.30 removed
// the vials that drove the Altar of Sacrifice transformation; the pairs stay
// because they define which uniques the temple market prices (POE-366).
//
// The pairs were verified 2026-09-06 against poedb.tw's per-vial pages
// (https://poedb.tw/us/<Vial_Name>): the game's currency text, the vial's icon
// file name, poedb's prose blurb and its Recipe block agreed on each direction.
//
// Every name was then checked to exist verbatim in poe.ninja's live
// item-overview feed for league Allflame on the same day — the names have to
// match the feed exactly, because the feed's `name` is the only key the price
// lookup has. TestUniqueUpgrades_everyMemberIsALiveFeedName re-checks that
// against the captured feed fixture on every run.

// uniqueUpgrade is one Base → Upgraded pair. Both fields are poe.ninja item
// names, the keys Market.Items is looked up by.
type uniqueUpgrade struct {
	Base     string `json:"base"`
	Upgraded string `json:"upgraded"`
}

//go:embed unique_upgrades.json
var uniqueUpgradesJSON []byte

// uniqueUpgrades is the pairs decoded from the embedded table.
var uniqueUpgrades = decodeUniqueUpgrades(uniqueUpgradesJSON)

// decodeUniqueUpgrades panics on a malformed, empty or blank-named table: a
// server that would otherwise serve an empty item set must not start silently.
func decodeUniqueUpgrades(raw []byte) []uniqueUpgrade {
	var pairs []uniqueUpgrade
	if err := json.Unmarshal(raw, &pairs); err != nil {
		panic(fmt.Sprintf("temple: decode unique_upgrades.json: %v", err))
	}
	if len(pairs) == 0 {
		panic("temple: unique_upgrades.json holds no pairs")
	}
	for i, p := range pairs {
		if p.Base == "" || p.Upgraded == "" {
			panic(fmt.Sprintf("temple: unique_upgrades.json pair %d has an empty name", i))
		}
	}
	return pairs
}

// itemNames is the served item set: the union of every pair's two members,
// sorted, with duplicates removed.
//
// The union IS the definition — there is no second list to keep in step with
// the table, so adding a pair adds its items.
var itemNames = buildItemNames()

func buildItemNames() []string {
	seen := make(map[string]struct{}, len(uniqueUpgrades)*2)
	names := make([]string, 0, len(uniqueUpgrades)*2)
	add := func(name string) {
		if _, dup := seen[name]; dup {
			return
		}
		seen[name] = struct{}{}
		names = append(names, name)
	}
	for _, u := range uniqueUpgrades {
		add(u.Base)
		add(u.Upgraded)
	}
	sort.Strings(names)
	return names
}

// ItemNames returns a fresh copy of the served item set, sorted by name.
func ItemNames() []string {
	out := make([]string, len(itemNames))
	copy(out, itemNames)
	return out
}
