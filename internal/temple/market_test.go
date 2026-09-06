package temple

import (
	"reflect"
	"testing"
	"time"
)

var testTime = time.Date(2026, 9, 6, 2, 35, 27, 0, time.UTC)

func room(name string, chaos float64, listings int) Line {
	return Line{Category: RoomCategory, Time: testTime, Name: name, Chaos: chaos, Listings: listings}
}

func item(category, name string, chaos float64, listings int) Line {
	return Line{Category: category, Time: testTime, Name: name, Chaos: chaos, Divine: chaos / 400, Listings: listings}
}

func roomsByName(m Market) map[string]Room {
	out := make(map[string]Room, len(m.Rooms))
	for _, r := range m.Rooms {
		out[r.Name] = r
	}
	return out
}

func itemsByName(m Market) map[string]Item {
	out := make(map[string]Item, len(m.Items))
	for _, i := range m.Items {
		out[i.Name] = i
	}
	return out
}

func TestParseRoomName_splitsTheTierSuffixOffTheRoomLine(t *testing.T) {
	line, tier := parseRoomName("Locus of Corruption (Tier 3)")
	if line != "Locus of Corruption" || tier != 3 {
		t.Errorf("parseRoomName = (%q, %d), want (%q, %d)", line, tier, "Locus of Corruption", 3)
	}
}

func TestParseRoomName_readsAMultiDigitTier(t *testing.T) {
	// A single-digit-only parse would read "(Tier 12)" as tier 1 or as untiered.
	line, tier := parseRoomName("Hall of Mettle (Tier 12)")
	if line != "Hall of Mettle" || tier != 12 {
		t.Errorf("parseRoomName = (%q, %d), want (%q, %d)", line, tier, "Hall of Mettle", 12)
	}
}

func TestParseRoomName_untieredNameKeepsItsWholeNameAndReportsTierZero(t *testing.T) {
	// Apex of Atzoatl and the ten connector rooms have no tier. They are real
	// rooms, so they are served rather than dropped, and their line must not be
	// truncated to an empty string.
	line, tier := parseRoomName("Apex of Atzoatl")
	if line != "Apex of Atzoatl" || tier != 0 {
		t.Errorf("parseRoomName = (%q, %d), want (%q, 0)", line, tier, "Apex of Atzoatl")
	}
}

func TestBuild_untieredRoomIsServedNotDropped(t *testing.T) {
	m := Build("Allflame", []Line{
		room("Locus of Corruption (Tier 3)", 800, 2000),
		room("Apex of Atzoatl", 10, 700),
		room("Halls", 10, 700),
	}, nil)

	if len(m.Rooms) != 3 {
		t.Fatalf("rooms = %d, want all 3 served", len(m.Rooms))
	}
	if got := roomsByName(m)["Apex of Atzoatl"]; got.Tier != 0 || got.Line != "Apex of Atzoatl" {
		t.Errorf("Apex of Atzoatl = %+v, want tier 0 and its own name as the line", got)
	}
}

func TestBuild_floorIsTheMedianOfTheRoomLines(t *testing.T) {
	// Five lines, four of them at the base price and one far above it. The median
	// is the base price; a mean would be dragged to 168 by the outlier and would
	// then report every base line as below floor by a wide margin.
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
		room("C (Tier 2)", 10, 700),
		room("D (Tier 2)", 10, 700),
		room("Locus of Corruption (Tier 3)", 800, 2000),
	}, nil)

	if m.Floor != 10 {
		t.Errorf("Floor = %v, want 10 (the median room price)", m.Floor)
	}
}

func TestBuild_roomAtExactlyTwiceTheFloorIsAboveIt(t *testing.T) {
	// The boundary is inclusive: >= 2x, not > 2x.
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
		room("Boundary (Tier 3)", 20, 700),
	}, nil)

	got := roomsByName(m)["Boundary (Tier 3)"]
	if !got.AboveFloor {
		t.Errorf("a room at exactly 2x the floor (%v) reported aboveFloor = false", m.Floor)
	}
	if got.SaleDelta != 10 {
		t.Errorf("saleDelta = %v, want 10 (20 - the floor of 10)", got.SaleDelta)
	}
}

func TestBuild_roomJustBelowTwiceTheFloorReportsNoSale(t *testing.T) {
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
		room("Boundary (Tier 3)", 19.9, 700),
	}, nil)

	got := roomsByName(m)["Boundary (Tier 3)"]
	if got.AboveFloor {
		t.Errorf("a room below 2x the floor reported aboveFloor = true")
	}
	if got.SaleDelta != 0 {
		t.Errorf("saleDelta = %v, want 0 — a room that is not above the floor has no sale", got.SaleDelta)
	}
	if got.Chaos != 19.9 {
		t.Errorf("chaos = %v, want the feed price 19.9 served regardless of the floor rule", got.Chaos)
	}
}

func TestBuild_saleDeltaIsThePriceMinusTheFloor(t *testing.T) {
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
		room("Locus of Corruption (Tier 3)", 853.6, 2489),
	}, nil)

	got := roomsByName(m)["Locus of Corruption (Tier 3)"]
	if got.SaleDelta != 843.6 {
		t.Errorf("saleDelta = %v, want 843.6 (853.6 - 10); the feed price alone is not the sale", got.SaleDelta)
	}
}

func TestBuild_roomsAreSortedByNameSoThePayloadIsDeterministic(t *testing.T) {
	m := Build("Allflame", []Line{
		room("C (Tier 1)", 10, 700),
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 700),
	}, nil)

	got := []string{m.Rooms[0].Name, m.Rooms[1].Name, m.Rooms[2].Name}
	want := []string{"A (Tier 1)", "B (Tier 1)", "C (Tier 1)"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("room order = %v, want %v", got, want)
	}
}

func TestBuild_lowConfidenceIsRelativeToTheLinesOwnCategory(t *testing.T) {
	// Two markets of very different depth, as the real feed has them: five
	// UniqueArmour lines around 1000 listings against four Vial lines around 10.
	//
	// Vial of Awakening at 100 listings is TEN TIMES its own market's median and
	// plainly confident. Pool the two categories into one median and the
	// threshold becomes 400, which flags it. Vial of the Ghost at 3 is thin
	// either way, so it is the flag firing at all; Awakening is the flag firing
	// against the right market.
	m := Build("Allflame", []Line{
		item("UniqueArmour", "Apep's Slumber", 8, 1000),
		item("UniqueArmour", "Apep's Supremacy", 51, 1000),
		item("UniqueArmour", "Architect's Hand", 75.4, 1000),
		item("UniqueArmour", "Slavedriver's Hand", 89.6, 1000),
		item("UniqueArmour", "Omeyocan", 424.3, 1000),
		item("Vial", "Vial of Awakening", 9, 100),
		item("Vial", "Vial of Fate", 1, 10),
		item("Vial", "Vial of Dominance", 1, 10),
		item("Vial", "Vial of the Ghost", 1689, 3),
	}, nil)

	items := itemsByName(m)
	if !items["Vial of the Ghost"].Price.LowConfidence {
		t.Errorf("Vial of the Ghost (3 of a 10 median) reported lowConfidence = false")
	}
	if items["Vial of Awakening"].Price.LowConfidence {
		t.Errorf("Vial of Awakening (100 against its own market's median of 10) reported lowConfidence = true; the threshold was pooled across categories")
	}
}

func TestBuild_lineAtExactlyFortyPercentOfTheMedianIsConfident(t *testing.T) {
	// The rule is depth < 0.4, so the boundary itself is confident. Three lines
	// give a median of 100; 40 is exactly 40% of it.
	m := Build("Allflame", []Line{
		item("UniqueJewel", "Tempered Mind", 1, 40),
		item("UniqueJewel", "Tempered Flesh", 8, 100),
		item("UniqueJewel", "Tempered Spirit", 3, 200),
	}, nil)

	items := itemsByName(m)
	if items["Tempered Mind"].Price.LowConfidence {
		t.Errorf("a line at exactly 40%% of the median reported lowConfidence = true; the boundary belongs to the confident side")
	}
}

func TestBuild_lineJustBelowFortyPercentOfTheMedianIsThin(t *testing.T) {
	m := Build("Allflame", []Line{
		item("UniqueJewel", "Tempered Mind", 1, 39),
		item("UniqueJewel", "Tempered Flesh", 8, 100),
		item("UniqueJewel", "Tempered Spirit", 3, 200),
	}, nil)

	items := itemsByName(m)
	if !items["Tempered Mind"].Price.LowConfidence {
		t.Errorf("a line at 39%% of the median reported lowConfidence = false")
	}
}

func TestBuild_thinLineIsPricedFromTheWindowMedianNotTheNewestPrint(t *testing.T) {
	newest := []Line{
		item("Vial", "Vial of the Ghost", 1689, 7),
		item("Vial", "Vial of Fate", 1, 343),
		item("Vial", "Vial of Dominance", 1, 107),
	}
	// The divine axis is deliberately NOT ordered with the chaos axis: sorted on
	// its own it medians to 5, and only the print that carries the median chaos
	// carries 3.8. That is what separates "one print, both axes" from two
	// independent medians.
	window := map[string][]WindowPrice{
		"Vial|Vial of the Ghost": {
			{Chaos: 1500, Divine: 9},
			{Chaos: 1600, Divine: 3.8},
			{Chaos: 1689, Divine: 5},
		},
	}

	got := itemsByName(Build("Allflame", newest, window))["Vial of the Ghost"].Price
	if got.Chaos != 1600 {
		t.Errorf("chaos = %v, want the window median 1600 rather than the newest print 1689", got.Chaos)
	}
	if got.Divine != 3.8 {
		t.Errorf("divine = %v, want 3.8 — the median-chaos print's own divine, not the divine axis's separate median of 5", got.Divine)
	}
	if !got.WindowPriced || got.WindowHours != WindowHours || got.WindowSamples != 3 {
		t.Errorf("window disclosure = (priced %v, hours %d, samples %d), want (true, %d, 3)",
			got.WindowPriced, got.WindowHours, got.WindowSamples, WindowHours)
	}
	if got.Listings != 7 {
		t.Errorf("listings = %d, want the newest print's 7 — the window prices, it does not restate depth", got.Listings)
	}
}

func TestBuild_confidentLineKeepsItsNewestPrintEvenWhenAWindowIsSupplied(t *testing.T) {
	newest := []Line{
		item("Vial", "Vial of Fate", 1, 343),
		item("Vial", "Vial of Dominance", 1, 300),
		item("Vial", "Vial of Awakening", 9, 250),
	}
	window := map[string][]WindowPrice{
		"Vial|Vial of Fate": {{Chaos: 99}, {Chaos: 99}, {Chaos: 99}},
	}

	got := itemsByName(Build("Allflame", newest, window))["Vial of Fate"].Price
	if got.WindowPriced {
		t.Errorf("a confident line reported windowPriced = true")
	}
	if got.Chaos != 1 {
		t.Errorf("chaos = %v, want its newest print of 1", got.Chaos)
	}
}

func TestBuild_thinLineWithNoWindowRowsKeepsItsNewestPrint(t *testing.T) {
	// A window read that came back empty must not price the item at zero.
	newest := []Line{
		item("Vial", "Vial of the Ghost", 1689, 7),
		item("Vial", "Vial of Fate", 1, 343),
		item("Vial", "Vial of Dominance", 1, 107),
	}

	got := itemsByName(Build("Allflame", newest, nil))["Vial of the Ghost"].Price
	if got.Chaos != 1689 {
		t.Errorf("chaos = %v, want the newest print 1689 when the window read returned nothing", got.Chaos)
	}
	if got.WindowPriced {
		t.Errorf("windowPriced = true with no window rows; the row would claim a reading it does not have")
	}
	if !got.LowConfidence {
		t.Errorf("lowConfidence = false; the flag is independent of whether a window was available")
	}
}

func TestBuild_itemAbsentFromTheSnapshotIsUnpricedNotZero(t *testing.T) {
	m := Build("Allflame", []Line{item("Vial", "Vial of Fate", 1, 343)}, nil)

	got := itemsByName(m)["Zerphi's Heart"]
	if !got.Unpriced {
		t.Errorf("Zerphi's Heart = %+v, want unpriced", got)
	}
	if got.Price != nil {
		t.Errorf("price = %+v, want null — zero is a real price and 'nobody is selling this' is not one", *got.Price)
	}
	if got.Category != "" {
		t.Errorf("category = %q, want empty; nothing observed the item, so nothing can say which feed serves it", got.Category)
	}
	if got.Icon == "" {
		t.Errorf("icon = %q, want the committed artwork path — an unpriced item still renders", got.Icon)
	}
}

func TestBuild_itemWithZeroListingsIsUnpriced(t *testing.T) {
	// poe.ninja keeps serving a price for a line nobody lists. That is a stale
	// estimate, not a market.
	m := Build("Allflame", []Line{
		item("Vial", "Vial of Sacrifice", 422.3, 0),
		item("Vial", "Vial of Fate", 1, 343),
	}, nil)

	got := itemsByName(m)["Vial of Sacrifice"]
	if !got.Unpriced || got.Price != nil {
		t.Errorf("Vial of Sacrifice = %+v, want unpriced with a null price despite the feed's 422.3", got)
	}
}

func TestBuild_servesEveryRecipeMemberEvenWhenTheFeedIsEmpty(t *testing.T) {
	m := Build("Allflame", nil, nil)

	if len(m.Items) != len(ItemNames()) {
		t.Errorf("items = %d, want all %d recipe members served as unpriced", len(m.Items), len(ItemNames()))
	}
	for _, i := range m.Items {
		if !i.Unpriced {
			t.Errorf("%s reported priced from an empty snapshot", i.Name)
		}
	}
}

func TestBuild_categoriesSeenListsOnlyTheCategoriesThatStoredLines(t *testing.T) {
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 700),
		item("Vial", "Vial of Fate", 1, 343),
	}, nil)

	want := []string{RoomCategory, "Vial"}
	if !reflect.DeepEqual(m.CategoriesSeen, want) {
		t.Errorf("categoriesSeen = %v, want %v — a category with no snapshot must not be claimed", m.CategoriesSeen, want)
	}
}

func TestBuild_asOfIsTheNewestTimeAcrossEveryCategory(t *testing.T) {
	older := testTime.Add(-2 * time.Hour)
	newer := testTime

	m := Build("Allflame", []Line{
		{Category: RoomCategory, Time: older, Name: "A (Tier 1)", Chaos: 10, Listings: 700},
		{Category: "Vial", Time: newer, Name: "Vial of Fate", Chaos: 1, Listings: 343},
	}, nil)

	if m.AsOf == nil {
		t.Fatalf("asOf = null on a snapshot with lines")
	}
	if !m.AsOf.Equal(newer) {
		t.Errorf("asOf = %v, want the newest category's %v", m.AsOf, newer)
	}
}

func TestBuild_carriesTheLeagueAndTheFloorRule(t *testing.T) {
	m := Build("Allflame", []Line{room("A (Tier 1)", 10, 700)}, nil)

	if m.League != "Allflame" {
		t.Errorf("league = %q, want Allflame", m.League)
	}
	if m.FloorRule != FloorRule {
		t.Errorf("floorRule = %q, want %q so the client can show the rule it is reading", m.FloorRule, FloorRule)
	}
	if len(m.Recipes) != 11 {
		t.Errorf("recipes = %d, want the 11-row table on every response", len(m.Recipes))
	}
}

func TestEmptyMarket_hasNoObservationsButKeepsTheStaticAnswer(t *testing.T) {
	m := EmptyMarket("Allflame")

	if m.AsOf != nil {
		t.Errorf("asOf = %v, want null — that is how a client tells a cold answer from a warm one", m.AsOf)
	}
	if len(m.Rooms) != 0 || len(m.Items) != 0 || len(m.CategoriesSeen) != 0 {
		t.Errorf("cold market carries observations: %d rooms, %d items, %d categories", len(m.Rooms), len(m.Items), len(m.CategoriesSeen))
	}
	if m.Rooms == nil || m.Items == nil || m.CategoriesSeen == nil {
		t.Errorf("cold market has a nil slice; it would marshal as null rather than []")
	}
	if len(m.Recipes) != 11 || m.League != "Allflame" || m.FloorRule != FloorRule {
		t.Errorf("cold market dropped the static answer: league %q, floorRule %q, %d recipes", m.League, m.FloorRule, len(m.Recipes))
	}
}

func TestThinLines_namesOnlyThePricedServedLinesThatAreThin(t *testing.T) {
	newest := []Line{
		// Rooms: the shallowest is thin against the room median, and must still
		// not be window-priced.
		room("Locus of Corruption (Tier 3)", 853.6, 2489),
		room("A (Tier 1)", 10, 700),
		room("B (Tier 1)", 10, 40),
		// Items: one thin, one confident, one unpriced.
		item("Vial", "Vial of the Ghost", 1689, 7),
		item("Vial", "Vial of Fate", 1, 343),
		item("Vial", "Vial of Dominance", 1, 300),
	}

	got := ThinLines(newest)
	if len(got) != 1 {
		t.Fatalf("ThinLines = %+v, want exactly the one thin priced item line", got)
	}
	if got[0].Name != "Vial of the Ghost" || got[0].Category != "Vial" {
		t.Errorf("ThinLines[0] = %+v, want the Vial of the Ghost line", got[0])
	}
}

func TestThinLines_skipsAnItemWithZeroListings(t *testing.T) {
	// A zero-listing line is unpriced, so window-pricing it would query for a
	// price nothing serves.
	newest := []Line{
		item("Vial", "Vial of Sacrifice", 422.3, 0),
		item("Vial", "Vial of Fate", 1, 343),
		item("Vial", "Vial of Dominance", 1, 300),
	}

	if got := ThinLines(newest); len(got) != 0 {
		t.Errorf("ThinLines = %+v, want none", got)
	}
}

func TestPickLines_takesTheDeepestLineWhenANameIsServedTwice(t *testing.T) {
	// poe.ninja serves some names on several lines, one per base. The pick has to
	// be deterministic rather than left to map order.
	newest := []Line{
		item("UniqueAccessory", "Zerphi's Heart", 100, 10),
		item("UniqueAccessory", "Zerphi's Heart", 1273, 29),
		item("UniqueAccessory", "Sacrificial Heart", 5, 306),
	}

	got := itemsByName(Build("Allflame", newest, nil))["Zerphi's Heart"]
	if got.Price == nil || got.Price.Listings != 29 || got.Price.Chaos != 1273 {
		t.Errorf("Zerphi's Heart = %+v, want the deepest line (29 listings, 1273 chaos)", got.Price)
	}
}

func TestPickLines_equalListingsInOneCategoryResolveOnTheLowerNinjaID(t *testing.T) {
	// Same name, same category, same depth, and the deeper-market rule cannot
	// separate them. Row order must not decide it: NewestLines does not order
	// its rows, so a pick that fell back to position would flip the served price
	// between recomputes with no price change behind it. The higher id is listed
	// first here so "whichever came first" and "the lower id" disagree.
	newest := []Line{
		{Category: "UniqueAccessory", Time: testTime, NinjaID: 7002, Name: "Zerphi's Heart", Chaos: 1273, Listings: 29},
		{Category: "UniqueAccessory", Time: testTime, NinjaID: 7001, Name: "Zerphi's Heart", Chaos: 100, Listings: 29},
		item("UniqueAccessory", "Sacrificial Heart", 5, 306),
	}

	got := itemsByName(Build("Allflame", newest, nil))["Zerphi's Heart"]
	if got.Price == nil || got.Price.Chaos != 100 {
		t.Errorf("Zerphi's Heart = %+v, want the ninja_id 7001 line's 100 chaos", got.Price)
	}
}

func TestBuild_everyLineOfAZeroMedianCategoryIsLowConfidence(t *testing.T) {
	// A category whose median line carries no listings has no depth to measure
	// anything against, so no line in it is confident — including its deepest.
	// Rooms are where that matters: they are never served unpriced, so the flag
	// is the only thing that can say the feed had nothing behind it.
	m := Build("Allflame", []Line{
		room("A (Tier 1)", 10, 0),
		room("B (Tier 1)", 10, 0),
		room("C (Tier 1)", 10, 5),
	}, nil)

	rooms := roomsByName(m)
	for _, name := range []string{"A (Tier 1)", "C (Tier 1)"} {
		if !rooms[name].LowConfidence {
			t.Errorf("%s reported lowConfidence = false in a category whose median listing count is 0", name)
		}
	}
}

func TestMedianOf_takesTheUpperMiddleOfAnEvenCount(t *testing.T) {
	// The convention matches internal/lab/classification.go's detectLowConfidence
	// so the two thin-market rules in this repository agree on "median".
	lines := []Line{room("a", 1, 0), room("b", 2, 0), room("c", 3, 0), room("d", 4, 0)}
	if got := medianOf(lines, func(l Line) float64 { return l.Chaos }); got != 3 {
		t.Errorf("medianOf = %v, want 3 (the upper middle of 1,2,3,4)", got)
	}
}

func TestMedianOf_isZeroForNoLines(t *testing.T) {
	if got := medianOf(nil, func(l Line) float64 { return l.Chaos }); got != 0 {
		t.Errorf("medianOf(nil) = %v, want 0", got)
	}
}

func TestBuild_noRoomsMeansNoFloorAndNoSales(t *testing.T) {
	// A zero floor must not make every item-only snapshot report every room-less
	// payload as above floor; there are no rooms to report on at all.
	m := Build("Allflame", []Line{item("Vial", "Vial of Fate", 1, 343)}, nil)

	if m.Floor != 0 {
		t.Errorf("floor = %v, want 0 with no room lines", m.Floor)
	}
	if len(m.Rooms) != 0 {
		t.Errorf("rooms = %d, want none", len(m.Rooms))
	}
}
