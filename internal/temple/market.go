package temple

import (
	"regexp"
	"sort"
	"time"
)

// THE SERVED OBJECT
//
// One aggregated, league-keyed object the desktop temple module polls (POE-258):
// every room-tier's sale delta, every recipe member's price, and the recipe
// table that ties them together. It is computed off the collector's item ticks
// and served from memory — see Cache and Service — so the request path never
// queries Postgres.
//
// Two derived readings carry the project's existing thin-market vocabulary
// rather than inventing one:
//
//   - lowConfidence (POE-131) — the line's listing count is below 40% of its own
//     market's median. Relative, never an absolute count: an absolute cutoff
//     encodes a league-age assumption (AGENTS.md, modeling rules).
//   - windowPriced / windowHours / windowSamples (POE-252) — a thin line is
//     priced from a trailing window instead of the newest print, and the row
//     says so.
//
// Both are FLAGS. Nothing here hides, reorders or partitions on them (ADR-015,
// ADR-017, ADR-018): every room-tier and every recipe member is served on every
// response, and the client decides what to dim.

// WindowHours is the trailing span a thin line is priced over. Twenty-four hours
// covers a full daily price cycle at the collector's cadence, which is the span
// the currency-exchange day horizon settled on for the same reason.
const WindowHours = 24

// FloorRule is served with the payload so a client can show the rule it is
// reading rather than hard-coding a copy of it.
const FloorRule = "median of room lines; above floor at >= 2x"

// lowConfidenceDepth is the fraction of its market's median listing count below
// which a line is flagged thin. It is POE-131's gem rule applied unchanged —
// internal/lab/classification.go, detectLowConfidence — because it answers the
// same question about the same kind of feed.
const lowConfidenceDepth = 0.4

// aboveFloorMultiple is how far above the floor a room-tier's price must sit
// before the room is read as carrying a sellable outcome. Two times the floor,
// inclusive: 84 of the 86 room lines sit at the base price the floor IS, so any
// smaller multiple would start reporting feed noise as a sale.
const aboveFloorMultiple = 2.0

// tierSuffix matches poe.ninja's room-tier naming, e.g. "Locus of Corruption
// (Tier 3)". The 11 lines with no such suffix are real rooms — Apex of Atzoatl
// and the ten connectors (Halls, Antechamber, Passageways, Banquet Hall, Chasm,
// Tunnels, Pits, Cellar, Tombs, Cloister) — not malformed data, so they are
// served with tier 0 rather than dropped.
var tierSuffix = regexp.MustCompile(`^(.+) \(Tier (\d+)\)$`)

// Line is one item_snapshots row as the aggregation reads it. Category is
// poe.ninja's own type name, stored verbatim by the collector.
//
// NinjaID is poe.ninja's own line id — the table's key alongside league, time
// and category. Nothing is served from it; it exists here so pickLines can break
// an equal-listings tie inside one category on something the feed owns rather
// than on row order.
type Line struct {
	Category string
	Time     time.Time
	NinjaID  int64
	Name     string
	Chaos    float64
	Divine   float64
	Listings int
}

// WindowPrice is one trailing-window observation of a line's price.
type WindowPrice struct {
	Chaos  float64
	Divine float64
}

// Room is one temple room-tier line.
//
// SaleDelta is the whole point of the room half: the feed price minus the floor
// when the room-tier is above the floor, and zero otherwise. On a typical
// snapshot 84 of the 86 rooms read zero, and the two that do not are the rooms
// worth selling.
type Room struct {
	Name          string  `json:"name"`
	Line          string  `json:"line"`
	Tier          int     `json:"tier"`
	Chaos         float64 `json:"chaos"`
	SaleDelta     float64 `json:"saleDelta"`
	AboveFloor    bool    `json:"aboveFloor"`
	Listings      int     `json:"listings"`
	LowConfidence bool    `json:"lowConfidence"`
	Icon          string  `json:"icon"`
}

// Price is a recipe member's current price.
//
// When WindowPriced is true, Chaos and Divine come from the line's MEDIAN-CHAOS
// print over the trailing WindowHours rather than from its newest print, and
// WindowSamples says how many prints that median was taken over. One print, both
// axes: taking each axis's own median would let the served pair come from two
// different hours, so a client dividing one by the other would read an implied
// divine rate no print ever carried.
type Price struct {
	Chaos         float64 `json:"chaos"`
	Divine        float64 `json:"divine"`
	Listings      int     `json:"listings"`
	LowConfidence bool    `json:"lowConfidence"`
	WindowPriced  bool    `json:"windowPriced"`
	WindowHours   int     `json:"windowHours"`
	WindowSamples int     `json:"windowSamples"`
}

// Item is one recipe member — a vial, a base unique or an upgraded unique.
//
// Price is nil and Unpriced true when the newest snapshot carries no usable line
// for the name: either no line at all, or a line with zero listings, which is a
// stale poe.ninja estimate rather than a market. An unpriced item is never
// served as a zero price; zero is a real price and "nobody is selling this" is
// not one.
//
// Category is the feed category the line was actually found in, so it is empty
// exactly when the item is unpriced — the feed is the only thing that knows
// which overview an item is served under, and this package does not keep a
// second copy of that to answer with when the feed has not spoken.
type Item struct {
	Name     string `json:"name"`
	Category string `json:"category"`
	Price    *Price `json:"price"`
	Unpriced bool   `json:"unpriced"`
	Icon     string `json:"icon"`
}

// Market is the whole served payload.
//
// AsOf is nil on a COLD cache and on a league whose seven feeds have stored
// nothing; CategoriesSeen then holds none of them. The client decides what
// staleness means (POE-258) — this package reports the age and never withholds
// on it.
//
// Recipes carry names only. Their prices live in Items and the client resolves
// them by name, so a member that appears in two recipes is priced once and the
// two readings cannot disagree.
type Market struct {
	League         string     `json:"league"`
	AsOf           *time.Time `json:"asOf"`
	Floor          float64    `json:"floor"`
	FloorRule      string     `json:"floorRule"`
	CategoriesSeen []string   `json:"categoriesSeen"`
	Rooms          []Room     `json:"rooms"`
	Items          []Item     `json:"items"`
	Recipes        []Recipe   `json:"recipes"`
}

// EmptyMarket is the COLD answer: the league, the static recipe table and the
// rule, with no observations.
//
// It is a real answer rather than an error because the server accepts traffic
// against a cold cache after every deploy (docs/ANALYSIS-CACHE.md, the
// cold-start window) and handlers.MarketOverview — the endpoint this one is
// modelled on — answers 200 with its zero values in exactly that window. A
// caller tells cold from empty by AsOf being null, not by the slices.
func EmptyMarket(leagueID string) Market {
	return Market{
		League:         leagueID,
		FloorRule:      FloorRule,
		CategoriesSeen: []string{},
		Rooms:          []Room{},
		Items:          []Item{},
		Recipes:        Recipes(),
	}
}

// Build aggregates one recompute's rows into the served object.
//
// newest holds every line of the newest stored snapshot of each of the seven
// categories. window holds, per line (category and name, as Repository
// .WindowPrices keys it), that line's prints over the trailing WindowHours; only
// thin lines need an entry, and a line with no entry — or an empty one — keeps
// its newest print.
//
// It is pure: no clock, no query, no logging. The untiered room lines the caller
// may want to report are visible in the result as Tier == 0.
func Build(leagueID string, newest []Line, window map[string][]WindowPrice) Market {
	market := EmptyMarket(leagueID)

	byCategory := groupByCategory(newest)

	for _, feed := range feeds {
		lines := byCategory[feed.Category]
		if len(lines) == 0 {
			continue
		}
		market.CategoriesSeen = append(market.CategoriesSeen, feed.Category)
		for _, line := range lines {
			if market.AsOf == nil || line.Time.After(*market.AsOf) {
				at := line.Time
				market.AsOf = &at
			}
		}
	}

	thin := thinLines(byCategory)

	// Rooms are never window-priced. Locus of Corruption and Doryani's Institute
	// carry thousands of listings, and the 84 floor lines' price is not used for
	// anything — their sale delta is zero by the rule, not by their price — so a
	// trailing window would only blur the two prints that matter. Their thinness
	// is still reported as a flag.
	rooms := byCategory[RoomCategory]
	market.Floor = medianOf(rooms, func(l Line) float64 { return l.Chaos })
	market.Rooms = buildRooms(rooms, market.Floor, thin)
	market.Items = buildItems(pickLines(byCategory), thin, window)

	return market
}

// ThinLines returns the priced lines of the served item set whose newest print
// is thin — exactly the set Build will window-price. The caller reads it between
// the two repository calls so the window query names only the lines that need
// one.
//
// It shares thinLines and pickLines with Build rather than restating either
// rule, so the lines queried and the lines window-priced cannot come apart.
// Rooms are never returned: they are not window-priced (see Build).
func ThinLines(newest []Line) []Line {
	byCategory := groupByCategory(newest)
	thin := thinLines(byCategory)
	picked := pickLines(byCategory)

	lines := make([]Line, 0, len(itemNames))
	for _, name := range itemNames {
		line, ok := picked[name]
		if !ok || line.Listings == 0 {
			continue
		}
		if thin[lineKey(line)] {
			lines = append(lines, line)
		}
	}
	return lines
}

func groupByCategory(lines []Line) map[string][]Line {
	byCategory := make(map[string][]Line, len(feeds))
	for _, line := range lines {
		byCategory[line.Category] = append(byCategory[line.Category], line)
	}
	return byCategory
}

// thinLines flags every line whose listing count is below lowConfidenceDepth of
// its own category's median listing count.
//
// Per category, because a category is a market: UniqueArmour's median line
// carries 174 listings and UniqueAccessory's 678 on the same snapshot, so one
// shared threshold would call most of one category thin and none of the other.
// The median follows internal/lab's convention — the upper middle element of the
// sorted values — so the two thin-market rules in this repository agree on what
// "median" means.
func thinLines(byCategory map[string][]Line) map[string]bool {
	thin := make(map[string]bool)
	for _, lines := range byCategory {
		median := medianOf(lines, func(l Line) float64 { return float64(l.Listings) })
		// No stand-in median; a category with no depth is all low-confidence.
		// A stand-in of 1 would make the threshold 0.4, an absolute cutoff
		// nothing chose and one only a zero-listing line can fall under — so it
		// would read the deepest line of a listing-less feed as confident. There
		// is no confident line in a market whose median carries no listings, and
		// rooms are the case that shows it: they are never served unpriced, so
		// the flag is the only thing that can say so.
		if median <= 0 {
			for _, line := range lines {
				thin[lineKey(line)] = true
			}
			continue
		}
		threshold := lowConfidenceDepth * median
		for _, line := range lines {
			if float64(line.Listings) < threshold {
				thin[lineKey(line)] = true
			}
		}
	}
	return thin
}

// pickLines resolves each served item name to the one line that prices it.
//
// poe.ninja serves some names on more than one line (Precursor's Emblem appears
// five times in UniqueAccessory, once per ring base), and none of the 31 names
// here did on the 2026-09-06 Allflame feed — but "did not on one snapshot" is
// not a guarantee, so the tie is broken rather than left to row or map order.
// The deepest market wins; see betterLine for how an exact tie resolves.
func pickLines(byCategory map[string][]Line) map[string]Line {
	served := make(map[string]struct{}, len(itemNames))
	for _, name := range itemNames {
		served[name] = struct{}{}
	}

	picked := make(map[string]Line, len(itemNames))
	for _, category := range sortedKeys(byCategory) {
		for _, line := range byCategory[category] {
			if _, want := served[line.Name]; !want {
				continue
			}
			current, seen := picked[line.Name]
			if !seen || betterLine(line, current) {
				picked[line.Name] = line
			}
		}
	}
	return picked
}

// betterLine reports whether candidate should displace current as the line that
// prices their shared name.
//
// Deepest market first. On an exact tie the two cases are different and neither
// may be left to iteration order:
//
//   - Across categories, the categories are visited in name order, so the first
//     one to price the name keeps it.
//   - Inside one category, name order says nothing, so the lower ninja_id wins.
//     That is poe.ninja's own line id and it is stable across polls, which the
//     row's position in the result is not: NewestLines does not order its rows,
//     and Postgres is free to return them differently between recomputes.
func betterLine(candidate, current Line) bool {
	if candidate.Listings != current.Listings {
		return candidate.Listings > current.Listings
	}
	if candidate.Category != current.Category {
		return false
	}
	return candidate.NinjaID < current.NinjaID
}

func buildRooms(lines []Line, floor float64, thin map[string]bool) []Room {
	rooms := make([]Room, 0, len(lines))
	for _, line := range lines {
		name, tier := parseRoomName(line.Name)
		room := Room{
			Name:          line.Name,
			Line:          name,
			Tier:          tier,
			Chaos:         line.Chaos,
			Listings:      line.Listings,
			LowConfidence: thin[lineKey(line)],
			Icon:          IconPath(RoomIconName),
		}
		// Inclusive at exactly the multiple: a room-tier printing at twice the
		// floor is above it, so the boundary belongs to the sellable side.
		if floor > 0 && line.Chaos >= aboveFloorMultiple*floor {
			room.AboveFloor = true
			room.SaleDelta = line.Chaos - floor
		}
		rooms = append(rooms, room)
	}
	sort.SliceStable(rooms, func(i, j int) bool { return rooms[i].Name < rooms[j].Name })
	return rooms
}

func buildItems(picked map[string]Line, thin map[string]bool, window map[string][]WindowPrice) []Item {
	items := make([]Item, 0, len(itemNames))
	for _, name := range itemNames {
		item := Item{Name: name, Icon: IconPath(name)}

		line, ok := picked[name]
		if !ok || line.Listings == 0 {
			item.Unpriced = true
			items = append(items, item)
			continue
		}

		item.Category = line.Category
		price := Price{
			Chaos:         line.Chaos,
			Divine:        line.Divine,
			Listings:      line.Listings,
			LowConfidence: thin[lineKey(line)],
		}
		if price.LowConfidence {
			if prints := window[lineKey(line)]; len(prints) > 0 {
				// Sort the prints, not the two axes separately, and serve the
				// median-chaos print whole. Independent medians would pair a
				// chaos from one hour with a divine from another.
				sorted := append(make([]WindowPrice, 0, len(prints)), prints...)
				sort.SliceStable(sorted, func(i, j int) bool { return sorted[i].Chaos < sorted[j].Chaos })
				median := sorted[len(sorted)/2]
				price.Chaos = median.Chaos
				price.Divine = median.Divine
				price.WindowPriced = true
				price.WindowHours = WindowHours
				price.WindowSamples = len(prints)
			}
		}
		item.Price = &price
		items = append(items, item)
	}
	return items
}

// parseRoomName splits poe.ninja's room-tier name into the room line and its
// tier. A name with no tier suffix keeps its whole name as the line and reports
// tier 0.
func parseRoomName(name string) (string, int) {
	match := tierSuffix.FindStringSubmatch(name)
	if match == nil {
		return name, 0
	}
	tier := 0
	for _, digit := range match[2] {
		tier = tier*10 + int(digit-'0')
	}
	return match[1], tier
}

// lineKey identifies one feed line. Name alone is not unique across categories,
// and this key is only ever compared against keys built the same way.
func lineKey(l Line) string { return l.Category + "|" + l.Name }

// medianOf returns the upper middle of the sorted projections of lines, matching
// internal/lab/classification.go's convention, and 0 for no lines.
func medianOf(lines []Line, of func(Line) float64) float64 {
	if len(lines) == 0 {
		return 0
	}
	values := make([]float64, len(lines))
	for i, line := range lines {
		values[i] = of(line)
	}
	sort.Float64s(values)
	return values[len(values)/2]
}

func sortedKeys(m map[string][]Line) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
