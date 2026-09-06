package temple

import "sort"

// THE VIAL RECIPE TABLE — the one normative home for item-level temple facts.
//
// A vial is consumed at the Altar of Sacrifice together with a base unique and
// transforms it into an upgraded unique. Three names, one direction, and the
// direction is the part that is easy to get backwards: the vial's own currency
// description names the item you SACRIFICE, not the item you receive.
//
// Verified 2026-09-06 against poedb.tw's per-vial pages (https://poedb.tw/us/
// <Vial_Name>). Four independent signals agree on each row — the game's currency
// text ("Sacrifice this item on the Altar of Sacrifice along with <BASE> to
// transform it"), the vial's icon file name (VialCowardsChains, VialSlumber, …),
// poedb's prose blurb ("<BASE> to <UPGRADED>") and its Recipe block ("Offer:
// <UPGRADED>; Your Offer: 1x <BASE>, 1x <VIAL>"). The game text is the one that
// settles a disagreement, because it is the item's own description rather than
// a site's rendering of it.
//
// Every one of the 31 distinct names below was then checked to exist verbatim in
// poe.ninja's live item-overview feed for league Allflame on the same day — the
// names have to match the feed exactly, because the feed's `name` is the only
// key the price lookup has. TestRecipes_everyMemberIsALiveFeedName re-checks that
// against the captured feed fixture on every run.
//
// Vial of Transcendence is the one vial with three recipes: it upgrades any of
// Tempered Flesh / Mind / Spirit, so it appears three times below. Nine vials,
// eleven recipes.
//
// The desktop (POE-256) stores room → base unique + vial and never the upgraded
// name; the upgrade is derived here, so a corrected direction is a one-line
// change in one file rather than an edit to the drop table.

// Recipe is one vial transformation: Vial plus Base yields Upgraded.
//
// The three fields are poe.ninja item names, not display labels — they are the
// keys Market.Items is looked up by, and the client resolves prices by name
// rather than receiving them duplicated inside the recipe (see Market).
type Recipe struct {
	Vial     string `json:"vial"`
	Base     string `json:"base"`
	Upgraded string `json:"upgraded"`
}

// recipes is the eleven vial transformations, ordered by vial name so the served
// payload is deterministic.
var recipes = []Recipe{
	{Vial: "Vial of Awakening", Base: "Apep's Slumber", Upgraded: "Apep's Supremacy"},
	{Vial: "Vial of Consequence", Base: "Coward's Chains", Upgraded: "Coward's Legacy"},
	{Vial: "Vial of Dominance", Base: "Architect's Hand", Upgraded: "Slavedriver's Hand"},
	{Vial: "Vial of Fate", Base: "Story of the Vaal", Upgraded: "Fate of the Vaal"},
	{Vial: "Vial of Sacrifice", Base: "Sacrificial Heart", Upgraded: "Zerphi's Heart"},
	{Vial: "Vial of Summoning", Base: "Mask of the Spirit Drinker", Upgraded: "Mask of the Stitched Demon"},
	{Vial: "Vial of Transcendence", Base: "Tempered Flesh", Upgraded: "Transcendent Flesh"},
	{Vial: "Vial of Transcendence", Base: "Tempered Mind", Upgraded: "Transcendent Mind"},
	{Vial: "Vial of Transcendence", Base: "Tempered Spirit", Upgraded: "Transcendent Spirit"},
	{Vial: "Vial of the Ghost", Base: "Soul Catcher", Upgraded: "Soul Ripper"},
	{Vial: "Vial of the Ritual", Base: "Dance of the Offered", Upgraded: "Omeyocan"},
}

// Recipes returns a fresh copy of the recipe table.
//
// The copy is deliberate: the table is package state for the process's lifetime
// and the caller marshals what it is given, so handing out the package's own
// slice would make a committed fact mutable through a shared reference.
func Recipes() []Recipe {
	out := make([]Recipe, len(recipes))
	copy(out, recipes)
	return out
}

// itemNames is the served item set: the union of every recipe's three members,
// sorted, with duplicates removed (Vial of Transcendence contributes one vial
// name across three recipes).
//
// The union IS the definition — there is no second list to keep in step with
// the recipes, so adding a vial adds its items.
var itemNames = buildItemNames()

func buildItemNames() []string {
	seen := make(map[string]struct{}, len(recipes)*3)
	names := make([]string, 0, len(recipes)*3)
	add := func(name string) {
		if _, dup := seen[name]; dup {
			return
		}
		seen[name] = struct{}{}
		names = append(names, name)
	}
	for _, r := range recipes {
		add(r.Vial)
		add(r.Base)
		add(r.Upgraded)
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
