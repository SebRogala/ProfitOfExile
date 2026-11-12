package temple

import "sort"

// THE UNIQUE UPGRADE TABLE — the one normative home for item-level temple facts
// and the source of the served item set.
//
// Each row pairs a base unique with the upgraded unique it becomes. 3.30 removed
// the vials that drove the Altar of Sacrifice transformation; the pairs stay
// because they define which uniques the temple market prices (POE-366).
//
// The pairs were verified 2026-09-06 against poedb.tw's per-vial pages
// (https://poedb.tw/us/<Vial_Name>): the game's currency text, the vial's icon
// file name, poedb's prose blurb and its Recipe block agreed on each direction.
//
// Every name below was then checked to exist verbatim in poe.ninja's live
// item-overview feed for league Allflame on the same day — the names have to
// match the feed exactly, because the feed's `name` is the only key the price
// lookup has. TestUniqueUpgrades_everyMemberIsALiveFeedName re-checks that
// against the captured feed fixture on every run.

// uniqueUpgrade is one Base → Upgraded pair. Both fields are poe.ninja item
// names, the keys Market.Items is looked up by.
type uniqueUpgrade struct {
	Base     string
	Upgraded string
}

// uniqueUpgrades is the eleven pairs, ordered by Base.
var uniqueUpgrades = []uniqueUpgrade{
	{Base: "Apep's Slumber", Upgraded: "Apep's Supremacy"},
	{Base: "Architect's Hand", Upgraded: "Slavedriver's Hand"},
	{Base: "Coward's Chains", Upgraded: "Coward's Legacy"},
	{Base: "Dance of the Offered", Upgraded: "Omeyocan"},
	{Base: "Mask of the Spirit Drinker", Upgraded: "Mask of the Stitched Demon"},
	{Base: "Sacrificial Heart", Upgraded: "Zerphi's Heart"},
	{Base: "Soul Catcher", Upgraded: "Soul Ripper"},
	{Base: "Story of the Vaal", Upgraded: "Fate of the Vaal"},
	{Base: "Tempered Flesh", Upgraded: "Transcendent Flesh"},
	{Base: "Tempered Mind", Upgraded: "Transcendent Mind"},
	{Base: "Tempered Spirit", Upgraded: "Transcendent Spirit"},
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
