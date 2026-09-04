package exchange

import (
	"bytes"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"math/rand"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// The INCIDENT CORPUS: one deterministic multi-hour feed carrying every
// currency-exchange regression this surface has actually shipped, driven through
// BestPlays so that ABSENCE is an assertion.
//
// Every regression that cost the owner a real market was a COMPOSITION of two
// individually-correct rules, and every single-predicate unit test in this
// package stayed green through all of them:
//
//   - MinEdge-as-a-drop x the newest-hour rule deleted the Apocalypse card
//     (2026-08-22, ADR-017's first amendment). The drop removed one quiet hour's
//     candidate; the newest-hour rule then removed the recipe, and with it the
//     five window hours that had printed 70-92%.
//   - The both-sides stock gate x the newest-hour rule deleted the Journey Tattoo
//     market (2026-08-23, ADR-017's second amendment). 1121 chaos of bids stood
//     against zero asks — the shape a seller wants most — and the sell leg was
//     dropped for the empty side it was never going to trade on.
//
// Neither rule was wrong on its own, which is why neither had a failing test.
// What was missing was a fixture in which a NAMED real market has to come back
// SERVED, with named flags, out of a realistic feed. That is this file: each
// scenario test is named after its incident, so a future "fix" to a gate, a
// default or the newest-hour rule that silently deletes one of these markets
// trips a test that says which incident it just re-opened.
//
// The corpus is also the source of the committed wire fixtures under
// testdata/wire, which the desktop's vitest suite consumes — see
// TestCorpus_wireGolden for what shape those carry and why.
//
// FEED IDS. Only the Apocalypse card's id is a recorded one (ADR-017 quotes it);
// the rest are synthetic stand-ins shaped like feed ids, because the engine never
// reads an id for anything but identity and the handler is what resolves display
// names. Chaos, divine and the scarab reuse the package's existing test ids.
const (
	// apocalypseID is the real id from ADR-017's incident record.
	apocalypseID = "Metadata/Items/DivinationCards/DivinationCardApocalypse"
	// tattooID stands in for the Journey Tattoo of the 2026-08-23 incident.
	tattooID = "Metadata/Items/Tattoos/JourneyTattoo"
	// tattooTwinID is the same market shape with both book sides alive in the
	// newest hour: the control arm of the stock-gate incident.
	tattooTwinID = "Metadata/Items/Tattoos/JourneyTattooTwoSided"
	// spreadless170ID stands in for the market in the 2026-08-23 screenshot whose
	// newest hour printed low == high == 170.
	spreadless170ID = "Metadata/Items/Currency/CurrencySpreadless170"
	// mawrID and mawrJunkID are the POE-188 blend incident and its mirror: the
	// junk lows sit in older hours on the first and in the NEWEST hour on the
	// second.
	mawrID     = "Metadata/Items/Maps/MapMawrBlaidd"
	mawrJunkID = "Metadata/Items/Maps/MapMawrBlaiddJunkNewest"
	// vesselID is POE-184's measured noise market: a 100% tick.
	vesselID = "Metadata/Items/Currency/CurrencyDivineVessel"
	// cleanFlipID is the happy shape the incident markets are ranked against.
	cleanFlipID = "Metadata/Items/Currency/CurrencyCleanFlip"
	// youngID appears in the newest few hours only, so the twelve-entry coverage
	// guard has something to flag.
	youngID = "Metadata/Items/Currency/CurrencyYoungRecipe"
	// apocalypseWindowID is the SAME card as apocalypseID, measured a fortnight
	// later (POE-252, prod read 2026-09-04): seventeen published hours in which
	// the newest traded one card and printed no spread at all. It is a separate
	// market id rather than an edit to apocalypseSpec, because the 2026-08-22
	// incident that spec records has its own assertions and must keep passing
	// untouched.
	apocalypseWindowID = "Metadata/Items/DivinationCards/DivinationCardApocalypseWindow"
	// deadWindowID is the control the window path is measured against: a market
	// whose whole clock window is silent, which must stay OUT of the served set.
	deadWindowID = "Metadata/Items/Currency/CurrencyDeadWindow"
)

// corpusNewest is the corpus feed's newest hour: 07:00 UTC on 2026-08-23, the
// hour both composition incidents were measured in. It is a fixed constant and
// never time.Now, because the committed wire fixtures are compared byte for byte.
var corpusNewest = time.Date(2026, 8, 23, 7, 0, 0, 0, time.UTC)

// corpusHours is how many distinct hours the feed carries.
//
// Twenty-six rather than twenty-four, so both windows the engine cuts are
// EXERCISED rather than merely satisfied: the 24-hour fill simulation and the
// 24-hour day horizon both fall short of the feed, which is the only arrangement
// in which a widened window shows up as a changed answer.
const corpusHours = 26

// youngRecipeHours is how many of the newest hours the young recipe trades in.
// Five hours yield four simulable entries — the newest hour can never be one —
// which is under Config.MinSimEntries, so the play comes back LowCoverage.
const youngRecipeHours = 5

// corpusHour names an hour by how many hours back from the newest it sits, so
// every fixture below reads in the same direction the engine walks the feed.
func corpusHour(back int) time.Time {
	return corpusNewest.Add(-time.Duration(back) * time.Hour)
}

// corpusRows renders the whole corpus as the []StoredRow Repository.LoadRows
// hands BestPlays.
//
// Row order is the loop's and nothing else reads it: BestPlays groups by hour and
// sorts every id it walks, which is what
// TestCorpus_shuffledFeedOrder_producesTheSameWireBytes holds it to.
func corpusRows() []StoredRow {
	var rows []StoredRow
	for back := 0; back < corpusHours; back++ {
		hour := corpusHour(back)
		specs := []rowSpec{
			divineChaosAnchor(),
			apocalypseSpec(back),
			journeyTattooChaosSpec(back),
			journeyTattooDivineSpec(),
			tattooTwinSpec(),
			spreadless170Spec(back),
			mawrBlaiddSpec(back),
			mawrBlaiddJunkNewestSpec(back),
			divineVesselSpec(),
			cleanFlipSpec(),
			corpusScarabChaosSpec(),
			corpusScarabDivineSpec(),
			deadWindowSpec(back),
		}
		if spec, published := apocalypseWindowSpec(back); published {
			specs = append(specs, spec)
		}
		if back < youngRecipeHours {
			specs = append(specs, youngRecipeSpec())
		}
		for _, spec := range specs {
			rows = append(rows, StoredRow{Hour: hour, Row: spec.row()})
		}
	}
	return rows
}

// corpusRowsShifted is the same corpus with the newest `by` hours removed and
// the remainder moved forward, so the hour the fixtures call `back == by` is now
// the newest one BestPlays scores.
//
// It is how a per-hour claim is asserted against the ONE table rather than
// against a second fixture per hour: nothing about any row changes, only which
// of them the engine is standing in. corpusRowsShifted(0) is corpusRows().
func corpusRowsShifted(by int) []StoredRow {
	newest := corpusHour(by)
	forward := time.Duration(by) * time.Hour
	rows := corpusRows()
	shifted := make([]StoredRow, 0, len(rows))
	for _, stored := range rows {
		if stored.Hour.After(newest) {
			continue
		}
		shifted = append(shifted, StoredRow{Hour: stored.Hour.Add(forward), Row: stored.Row})
	}
	return shifted
}

// apocalypseSpec is the MinEdge incident (ADR-017, first amendment, 2026-08-22).
//
// The card prints a 76% undercut return in every hour it traded, except two. In
// the NEWEST hour it traded 2 cards at a single 223:1 print, so both extremes are
// the same price and the round trip pays two ticks against no spread: −0.89%, the
// measured number that used to fail MinEdge and take the whole recipe with it.
// Three hours back the card did not trade at all, which is the one thing that
// still drops an hour — an unpriced hour has no price to serve — and is what
// makes HoursSeen a count rather than the window's size.
func apocalypseSpec(back int) rowSpec {
	switch back {
	case 0:
		return rowSpec{
			itemA:        chaosID,
			itemB:        apocalypseID,
			volume:       [2]int64{446, 2},
			lowestStock:  [2]int64{300, 3},
			highestStock: [2]int64{600, 6},
			lowestRatio:  [2]int64{223, 1},
			highestRatio: [2]int64{223, 1},
		}
	case 3:
		quiet := liquidChaosMarket(apocalypseID, 100, 180)
		quiet.volume = [2]int64{0, 0}
		return quiet
	default:
		return liquidChaosMarket(apocalypseID, 100, 180)
	}
}

// journeyTattooChaosSpec is the stock-gate incident (ADR-017, second amendment,
// 2026-08-23): in the newest hour 1121 chaos of bids stand against zero asks.
//
// Nobody is offering a tattoo, and real money is standing behind the ones that
// are wanted. A SELL into those bids is postable; a BUY off the empty ask side is
// not, which is why the direct flip on this market cannot form in that hour and
// the recipe that survives is the one-hop that sells into it.
func journeyTattooChaosSpec(back int) rowSpec {
	spec := liquidChaosMarket(tattooID, 100, 130)
	if back == 0 {
		spec.lowestStock = [2]int64{1121, 0}
		spec.highestStock = [2]int64{1121, 0}
	}
	return spec
}

// journeyTattooDivineSpec prices the same tattoo against divine, which is what
// gives the incident a one-hop route to be rescued as: buy the tattoo in divine,
// sell it into the chaos bids, convert the chaos back on the divine/chaos market.
//
// The quantity pairs are reduced and coarse enough to be realistic (13 divine for
// 25 tattoos at the hour's cheapest, 17 for 25 at its dearest, a 4% tick), and at
// the corpus rate of 200 chaos to the divine they price the tattoo at 104-136
// chaos — the same market the chaos-quoted rows above describe.
func journeyTattooDivineSpec() rowSpec {
	return rowSpec{
		itemA:        divineID,
		itemB:        tattooID,
		volume:       [2]int64{600, 1000},
		lowestStock:  [2]int64{200, 300},
		highestStock: [2]int64{400, 600},
		lowestRatio:  [2]int64{13, 25},
		highestRatio: [2]int64{17, 25},
	}
}

// tattooTwinSpec is the tattoo market with both book sides alive in every hour,
// including the newest. It differs from journeyTattooChaosSpec in the stock
// numbers and in nothing else, so the pair isolates the composition: one is
// served as a direct flip and the other is not.
func tattooTwinSpec() rowSpec {
	return liquidChaosMarket(tattooTwinID, 100, 130)
}

// spreadless170Spec is the 2026-08-23 screenshot case.
//
// The newest hour prints low == high == 170 on a 170:1 pair — a live hour with
// volume and stock on both sides — while every older hour prices the same item at
// 100/101 with its mass at 100. So the row that gets served shows a spreadless
// 170 whose round trip is 169/171 − 1, and the fill simulation, which enters in
// the older hours, cannot make the trip pay either: the one-chaos spread is
// narrower than the two ticks it costs, and the last entry chases the 170 print
// and then fire-sales next to the hour's own fair.
func spreadless170Spec(back int) rowSpec {
	if back == 0 {
		return rowSpec{
			itemA:        chaosID,
			itemB:        spreadless170ID,
			volume:       [2]int64{1700, 10},
			lowestStock:  [2]int64{900, 4},
			highestStock: [2]int64{1700, 8},
			lowestRatio:  [2]int64{170, 1},
			highestRatio: [2]int64{170, 1},
		}
	}
	return rowSpec{
		itemA:        chaosID,
		itemB:        spreadless170ID,
		volume:       [2]int64{100000, 1000},
		lowestStock:  [2]int64{5000, 400},
		highestStock: [2]int64{9000, 800},
		lowestRatio:  [2]int64{100, 1},
		highestRatio: [2]int64{101, 1},
	}
}

// mawrBlaiddSpec is the blend incident (POE-188): four CONSECUTIVE hours printed
// lows of 62-81 chaos against a volume-weighted price near 250, and the newest
// hour is ordinary.
//
// It is the fixture that says a served price belongs to ONE hour. Any aggregation
// that reached across hours for a cheaper low — a min, a mean, a "best seen" —
// would show a trade nobody could have made, and would show it here.
func mawrBlaiddSpec(back int) rowSpec {
	low := int64(240)
	switch back {
	case 4:
		low = 62
	case 3:
		low = 70
	case 2:
		low = 75
	case 1:
		low = 81
	}
	return mawrBlaiddRow(mawrID, low)
}

// mawrBlaiddJunkNewestSpec is the same market with the junk low in the NEWEST
// hour instead of behind it. The engine cannot refuse to price that hour — it is
// the last snapshot — so the play is served with the extreme MARKED, which is
// what Config.SuspectLowBand was calibrated for: 62 chaos sits under two thirds
// of the hour's own 250-chaos fair.
func mawrBlaiddJunkNewestSpec(back int) rowSpec {
	low := int64(240)
	if back == 0 {
		low = 62
	}
	return mawrBlaiddRow(mawrJunkID, low)
}

// mawrBlaiddRow renders one hour of a Mawr Blaidd market: the hour's low varies,
// the 260-chaos high and the 250-chaos volume-weighted price do not.
func mawrBlaiddRow(item string, low int64) rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        item,
		volume:       [2]int64{250000, 1000},
		lowestStock:  [2]int64{8000, 300},
		highestStock: [2]int64{15000, 600},
		lowestRatio:  [2]int64{low, 1},
		highestRatio: [2]int64{260, 1},
	}
}

// divineVesselSpec is POE-184's measured noise market, served flagged rather than
// cut since 2026-08-22: 109 chaos an hour, a volume-weighted price of 0.219
// chaos, and a 1/35-to-1/1 extreme pair whose tick is a full 100%. The undercut
// sell price is therefore Price*(1-1) = 0 and the round trip returns exactly
// −100%.
func divineVesselSpec() rowSpec {
	return rowSpec{
		itemA:        chaosID,
		itemB:        vesselID,
		volume:       [2]int64{109, 500},
		lowestStock:  [2]int64{300, 2000},
		highestStock: [2]int64{600, 4000},
		lowestRatio:  [2]int64{1, 35},
		highestRatio: [2]int64{1, 1},
	}
}

// cleanFlipSpec is the happy shape: a thousand chaos to the item at the hour's
// cheapest, twelve hundred at its dearest, in every hour of the feed. It is the
// play the incident markets are ranked against, and the one that would still be
// there if every flag in this file regressed.
func cleanFlipSpec() rowSpec {
	return liquidChaosMarket(cleanFlipID, 1000, 1200)
}

// youngRecipeSpec trades in the newest youngRecipeHours hours only, so its
// expectation is averaged over four entries against a guard of twelve.
func youngRecipeSpec() rowSpec {
	return liquidChaosMarket(youngID, 200, 260)
}

// apocalypseWindowMeasurement is the 2026-09-04 prod read of Chaos <-> Apocalypse, the
// measurement POE-252 was specified from, indexed by how many CLOCK hours back
// from the newest corpus hour each row sits.
//
// The table is the task's own, mapped by subtracting each measured hour from
// 2026-09-04 17:00Z: 17:00 is back 0, 16:00 is back 1, and 09-03 16:00 is back
// 25. Seventeen hours published a row and nine did not, which is 26 —
// corpusHours exactly, so the measurement fills the corpus with no invented
// hour. The nine ABSENT backs (6, 13, 14, 16, 18, 19, 21, 23, 24) are the zero
// value of this array and render NO ROW AT ALL, because an hour the feed never
// published is not the same shape as an hour that published a zero.
//
// Back 9 is the one row that is present with nothing traded. The feed
// republishes such an hour with the market's LAST ratios rather than with nulls
// — Row carries plain int64 quantities and no nullable price — so it renders
// back 10's 1196:1 with volume {0, 0}. A zero pair would set PriceValid false
// and change what the row tests; what this row tests is that a stale ratio
// nobody traded at cannot reach a window's extremes.
//
// WHAT THE OWNER SAW at back 0: one card changed hands at 552, and it was his
// own purchase, so the served row read 552/552 for -0% while the game's book
// had stood near 550/1150 all day and the six hours behind it had realized
// 486/1148.
var apocalypseWindowMeasurement = [corpusHours]struct {
	present          bool
	cards, low, high int64
}{
	0:  {present: true, cards: 1, low: 552, high: 552},
	1:  {present: true, cards: 2, low: 511, high: 1148},
	2:  {present: true, cards: 3, low: 500, high: 1148},
	3:  {present: true, cards: 1, low: 500, high: 500},
	4:  {present: true, cards: 3, low: 487, high: 1140},
	5:  {present: true, cards: 1, low: 486, high: 486},
	7:  {present: true, cards: 3, low: 487, high: 1196},
	8:  {present: true, cards: 1, low: 1000, high: 1000},
	9:  {present: true, cards: 0, low: 1196, high: 1196},
	10: {present: true, cards: 2, low: 1196, high: 1196},
	11: {present: true, cards: 8, low: 485, high: 1196},
	12: {present: true, cards: 2, low: 485, high: 486},
	15: {present: true, cards: 5, low: 486, high: 1200},
	17: {present: true, cards: 31, low: 485, high: 1100},
	20: {present: true, cards: 4, low: 748, high: 899},
	22: {present: true, cards: 12, low: 471, high: 899},
	25: {present: true, cards: 6, low: 502, high: 940},
}

// apocalypseWindowSpec renders one hour of the measured card market, and reports
// whether the feed published that hour at all.
//
// THE CHAOS SIDE is derived and not measured, by one rule stated here so every
// number below is reproducible: VolumeA = round(cards * (low+high)/2). The hour's
// volume-weighted price is then the arithmetic centre of its own extremes, which
// is where a real hour's mass sits between two prints, and it makes the POOLED
// window VWAP at back 0 come out at 8110 chaos over 11 cards = 737.27 — the
// anchor the suspect bands judge that hour's 486/1148 against, and the reason
// TestCorpus_apocalypseThinNewestHour_isPricedFromTheWindow expects a FLAGGED
// row rather than a clean one.
//
// Stock is liquid on both sides in every published row, traded or not: this
// fixture is about what the hours PRICED, and a book that emptied would be a
// different incident (ADR-017's second amendment, journeyTattooChaosSpec).
func apocalypseWindowSpec(back int) (rowSpec, bool) {
	if back < 0 || back >= len(apocalypseWindowMeasurement) {
		return rowSpec{}, false
	}
	hour := apocalypseWindowMeasurement[back]
	if !hour.present {
		return rowSpec{}, false
	}
	chaos := int64(math.Round(float64(hour.cards) * float64(hour.low+hour.high) / 2))
	return rowSpec{
		itemA:        chaosID,
		itemB:        apocalypseWindowID,
		volume:       [2]int64{chaos, hour.cards},
		lowestStock:  [2]int64{2000, 200},
		highestStock: [2]int64{5000, 500},
		lowestRatio:  [2]int64{hour.low, 1},
		highestRatio: [2]int64{hour.high, 1},
	}, true
}

// deadWindowSilentHours is how many of the newest hours the dead-window control
// publishes without a trade. The whole clock window a leg could look back over
// is silent, which is why six is the number: it leaves the control standing
// after WI-3 relaxes liveness, because a window with no realized print anywhere
// inside it has nothing for a relaxed gate to price from either. Under WI-2 the
// market is held out by the MinVolumePerHour liveness gate alone.
//
// It is read from the config rather than written as 6 so that raising
// WindowPriceHours cannot silently leave a traded hour inside the window and
// un-control the fixture.
var deadWindowSilentHours = DefaultConfig().WindowPriceHours

// deadWindowSpec is the control that says the window path is not a resurrection
// machine: a market that traded normally until six hours ago and has published
// nothing but its last ratios since.
//
// Its newest six hours carry volume {0, 0} with the 400/900 pair intact, which
// is exactly the shape the feed publishes for an untraded hour. Nothing inside
// the clock window traded, so windowPriceIn has no contributor to draw an
// extreme from and the market must be ABSENT from the served set — a window may
// price an hour that is alive, never revive one that is not.
//
// It is deliberately NOT named in INCIDENT_NAMES on the desktop tier: that map's
// size is compared against the served count, so a market asserted absent cannot
// have an entry there.
func deadWindowSpec(back int) rowSpec {
	spec := liquidChaosMarket(deadWindowID, 400, 900)
	if back < deadWindowSilentHours {
		spec.volume = [2]int64{0, 0}
	}
	return spec
}

// corpusScarabChaosSpec and corpusScarabDivineSpec are the package's standing one-hop
// fixture, reused so the corpus carries a clean triangle beside the incidents
// without a second description of what a liquid triangle looks like.
func corpusScarabChaosSpec() rowSpec {
	chaosLeg, _, _ := liquidTriangle()
	return chaosLeg
}

func corpusScarabDivineSpec() rowSpec {
	_, divineLeg, _ := liquidTriangle()
	return divineLeg
}

// corpusConfig is the served configuration for one horizon: DefaultConfig with
// that horizon's window and persistence overlay, exactly as Service.Recompute
// applies it. Tests ask for a horizon rather than hand-building a Config, so a
// change to what the server ships reaches this file.
func corpusConfig(t *testing.T, horizon Horizon) Config {
	t.Helper()
	base := DefaultConfig()
	for _, h := range base.Horizons {
		if h.Horizon == horizon {
			return horizonConfig(base, h)
		}
	}
	t.Fatalf("no horizon %q in DefaultConfig().Horizons", horizon)
	return Config{}
}

// corpusResult ranks the whole corpus for one horizon.
func corpusResult(t *testing.T, horizon Horizon) Result {
	t.Helper()
	return corpusResultShifted(t, horizon, 0)
}

// corpusResultShifted ranks the corpus as it stood `by` hours ago — the same
// feed with its newest `by` hours not yet published.
func corpusResultShifted(t *testing.T, horizon Horizon, by int) Result {
	t.Helper()
	result := BestPlays(corpusLeague, corpusRowsShifted(by), corpusConfig(t, horizon))
	result.Horizon = string(horizon)
	return result
}

// corpusLeague is the league every corpus row is stamped with.
const corpusLeague = "Allflame"

// updateWireGolden rewrites the committed fixtures instead of comparing against
// them: go test ./internal/exchange/ -run TestCorpus_wireGolden -update.
var updateWireGolden = flag.Bool("update", false, "rewrite the committed wire fixtures under testdata/wire")

// wireGoldenPath names one horizon's committed fixture.
func wireGoldenPath(horizon Horizon) string {
	return filepath.Join("testdata", "wire", string(horizon)+".json")
}

// The recipe keys the corpus is about, named once so a test says which market it
// is defending rather than spelling a key inline.
var (
	apocalypseKey   = directKey(chaosID, apocalypseID)
	tattooDirectKey = directKey(chaosID, tattooID)
	tattooTwinKey   = directKey(chaosID, tattooTwinID)
	tattooSellKey   = oneHopKey(tattooID, divineID, chaosID)
	tattooBuyKey    = oneHopKey(tattooID, chaosID, divineID)
	spreadlessKey   = directKey(chaosID, spreadless170ID)
	mawrKey         = directKey(chaosID, mawrID)
	mawrJunkKey     = directKey(chaosID, mawrJunkID)
	vesselKey       = directKey(chaosID, vesselID)
	cleanFlipKey    = directKey(chaosID, cleanFlipID)
	youngKey        = directKey(chaosID, youngID)

	apocalypseWindowKey = directKey(chaosID, apocalypseWindowID)
	deadWindowKey       = directKey(chaosID, deadWindowID)

	scarabHopKey    = oneHopKey(scarabID, chaosID, divineID)
	scarabDivineKey = directKey(divineID, scarabID)
)

func TestCorpus_everyIncidentMarket_isServedInBothHorizons(t *testing.T) {
	// The corpus's cheapest tripwire, and the one that says what this tier is
	// FOR: each row below is a market a shipped regression deleted, or a shape
	// whose disappearance would mean a gate had started binding by default. A
	// change that removes any of them names the incident it re-opened here,
	// before any flag assertion is even reached.
	incidents := []struct {
		incident string
		key      string
	}{
		{incident: "Apocalypse card, spreadless newest hour (2026-08-22)", key: apocalypseKey},
		{incident: "Journey Tattoo, sold into a one-sided book (2026-08-23)", key: tattooSellKey},
		{incident: "Journey Tattoo twin, both sides alive", key: tattooTwinKey},
		{incident: "the 2026-08-23 screenshot's spreadless 170 print", key: spreadlessKey},
		{incident: "Mawr Blaidd, junk lows behind the newest hour (POE-188)", key: mawrKey},
		{incident: "Mawr Blaidd, junk low IN the newest hour (POE-188)", key: mawrJunkKey},
		{incident: "Divine Vessel, a 100% tick (POE-184)", key: vesselKey},
		{incident: "a clean profitable flip", key: cleanFlipKey},
		{incident: "a divine-quoted flip", key: scarabDivineKey},
		{incident: "a one-hop triangle", key: scarabHopKey},
		{incident: "a recipe too young to have an expectation", key: youngKey},
		{incident: "Apocalypse card, thin newest hour priced from the window (POE-252, 2026-09-04)", key: apocalypseWindowKey},
	}

	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			served := playKeys(corpusResult(t, horizon).Plays)
			for _, tt := range incidents {
				if indexOf(served, tt.key) < 0 {
					t.Errorf("%s is not served: %s missing from %v", tt.incident, tt.key, served)
				}
			}
		})
	}
}

func TestCorpus_apocalypseSpreadlessNewestHour_isServedFlaggedNotDeleted(t *testing.T) {
	// THE INCIDENT (2026-08-22, ADR-017's first amendment). The card's 07:00 hour
	// traded 2 cards at one 223:1 print. Both extremes are that price, so the
	// round trip pays two ticks against no spread and returns 222/224 − 1 =
	// −0.89%. MinEdge-as-a-drop removed that hour's candidate; the newest-hour
	// rule then removed the recipe, and the owner was flipping the market by hand
	// while the server answered that it did not exist.
	//
	// What has to hold is that the row is HERE, carrying its measured loss: a row
	// that is not served cannot be argued with.
	play := playByKey(t, corpusResult(t, HorizonRecent), apocalypseKey)

	if !play.LowLiquidity {
		t.Errorf("LowLiquidity = false at a newest-hour return of %v, want true — the hour priced the card and showed no spread", play.RoiPct)
	}
	tick := 1.0 / 223.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(223, tick, [2]float64{223, tick}))
	if play.RoiPct >= 0 {
		t.Errorf("RoiPct = %v, want the loss two ticks against a closed spread produce", play.RoiPct)
	}
	if play.RoiPctRaw != 0 {
		t.Errorf("RoiPctRaw = %v, want 0 — the hour's two extremes are the same price", play.RoiPctRaw)
	}
	if !play.LastHour.Equal(corpusNewest) {
		t.Errorf("LastHour = %v, want the newest hour %v", play.LastHour, corpusNewest)
	}
}

func TestCorpus_apocalypseSpreadlessNewestHour_ranksOnItsSimulationNotItsQuietHour(t *testing.T) {
	// The second half of why the drop was wrong. One quiet hour is not a dead
	// market: the recipe's other window hours printed 70-92%, the fill simulation
	// reads them, and the card therefore ranks near the top with a NEGATIVE
	// displayed return. A ranking that sorted on the served hour's RoiPct would
	// bury it under every market that happened to print a spread this hour.
	got := corpusResult(t, HorizonRecent)
	play := playByKey(t, got, apocalypseKey)

	if !(play.ExpectedRoi > 0) {
		t.Errorf("ExpectedRoi = %v, want a positive expectation — the quiet hour is the exception, not the recipe", play.ExpectedRoi)
	}
	if !(play.RoiPct < 0) {
		t.Fatalf("RoiPct = %v, want the negative served hour — the fixture no longer separates the displayed number from the simulated one", play.RoiPct)
	}
	keys := playKeys(got.Plays)
	for _, loser := range []string{spreadlessKey, vesselKey, mawrJunkKey} {
		if indexOf(keys, apocalypseKey) > indexOf(keys, loser) {
			t.Errorf("the card ranks below %s; a flagged hour must not outweigh the expectation (order %v)", loser, keys)
		}
	}
}

func TestCorpus_apocalypseUntradedHour_isNotCountedInHoursSeen(t *testing.T) {
	// HoursSeen counts the window hours the recipe was PRICED in, and the card
	// went untraded three hours back — the one thing that still drops an hour,
	// because an hour with no trade has no price to serve. The count is what tells
	// a reader the spreadless newest hour is one hour out of many.
	tests := []struct {
		horizon   Horizon
		wantHours int
		wantSeen  int
	}{
		{horizon: HorizonRecent, wantHours: 6, wantSeen: 5},
		{horizon: HorizonDay, wantHours: 24, wantSeen: 23},
	}

	for _, tt := range tests {
		t.Run(string(tt.horizon), func(t *testing.T) {
			got := corpusResult(t, tt.horizon)
			if got.Hours != tt.wantHours {
				t.Fatalf("Hours = %d, want the %s window's %d", got.Hours, tt.horizon, tt.wantHours)
			}
			if seen := playByKey(t, got, apocalypseKey).HoursSeen; seen != tt.wantSeen {
				t.Errorf("HoursSeen = %d, want %d — every window hour but the untraded one", seen, tt.wantSeen)
			}
		})
	}
}

func TestCorpus_journeyTattooOneSidedBook_theSellSideRouteIsServedAndMarkedDepleted(t *testing.T) {
	// THE INCIDENT (2026-08-23, ADR-017's second amendment). The tattoo's newest
	// hour stood at 1121 chaos of bids and zero asks — the shape a SELLER wants
	// most, and the largest edge in its hour. The stock gate demanded both sides
	// of every leg, so the sell leg was dropped for the empty ask side it was
	// never going to trade on; the newest-hour rule then deleted the recipe.
	//
	// The recipe that has to survive is the one whose every leg executes against a
	// side that had stock: buy the tattoo against divine, sell it INTO those chaos
	// bids, convert the chaos back. A direct flip cannot form on a book with no
	// asks, which is the next test.
	play := playByKey(t, corpusResult(t, HorizonRecent), tattooSellKey)

	sell := play.Legs[1]
	if sell.Action != "sell" || sell.Item != tattooID || sell.Quote != chaosID {
		t.Fatalf("leg 1 = %s %s in %s, want the tattoo sold into chaos", sell.Action, sell.Item, sell.Quote)
	}
	if !sell.DepletedSide {
		t.Errorf("sell leg DepletedSide = false, want true — no tattoo was on offer this hour")
	}
	if sell.Stock != 1121 {
		t.Errorf("sell leg Stock = %d, want the 1121 chaos of bids it is paid out of", sell.Stock)
	}
	// The mark belongs to the one leg it describes: the other two executed against
	// live sides, and a flag that spread would stop naming the book.
	for _, i := range []int{0, 2} {
		if leg := play.Legs[i]; leg.DepletedSide {
			t.Errorf("leg %d (%s %s in %s) DepletedSide = true, want false", i, leg.Action, leg.Item, leg.Quote)
		}
	}
	if !play.LastHour.Equal(corpusNewest) {
		t.Errorf("LastHour = %v, want the newest hour %v — the one-sided hour is the served one", play.LastHour, corpusNewest)
	}
}

func TestCorpus_journeyTattooOneSidedBook_theBuySideRoutesStayDropped(t *testing.T) {
	// The boundary that says the gate FOLLOWED the action rather than being
	// deleted. Both routes below would have to take an ask off a book that had
	// none: the direct flip's buy leg, and the mirror triangle that enters in
	// chaos. 1121 chaos of bids buys nobody a tattoo, so neither is served —
	// serving them would be the opposite regression to the one this file records.
	served := playKeys(corpusResult(t, HorizonRecent).Plays)

	for _, key := range []string{tattooDirectKey, tattooBuyKey} {
		if indexOf(served, key) >= 0 {
			t.Errorf("%s was served; nothing may buy off an empty ask side (got %v)", key, served)
		}
	}
}

func TestCorpus_journeyTattooTwoSidedTwin_isServedAsAFlipCarryingNoDepletedSide(t *testing.T) {
	// The control arm. This market is the tattoo's twin — same prices, same
	// volumes, same hours — differing only in that its newest hour kept stock on
	// BOTH sides. It is served as a direct flip, which the one-sided market cannot
	// be, and neither of its legs is marked: a flip's two legs demand the item side
	// and the quote side between them, so a served flip has both by construction.
	play := playByKey(t, corpusResult(t, HorizonRecent), tattooTwinKey)

	if play.Mode != ModeDirect {
		t.Fatalf("Mode = %q, want %q", play.Mode, ModeDirect)
	}
	for i, leg := range play.Legs {
		if leg.DepletedSide {
			t.Errorf("leg %d (%s) DepletedSide = true, want false — both sides of this book were alive", i, leg.Action)
		}
	}
	if play.Legs[0].Stock != 500 {
		t.Errorf("buy leg Stock = %d, want the 500 tattoos it takes off the book", play.Legs[0].Stock)
	}
	if play.Legs[1].Stock != 5000 {
		t.Errorf("sell leg Stock = %d, want the 5000 chaos it is paid out of", play.Legs[1].Stock)
	}
}

func TestCorpus_spreadless170NewestHour_isServedWithTheLossTwoTicksProduce(t *testing.T) {
	// The 2026-08-23 screenshot case, at engine level. The newest hour printed low
	// == high == 170 on a 170:1 pair — volume alive, stock on both sides — so the
	// round trip buys at 170*(1+1/170) and sells at 170*(1−1/170): 169/171 − 1.
	// The raw spread is exactly nothing, and the served row says so in both
	// numbers instead of vanishing.
	//
	// Nothing here asserts a DISPLAY treatment: how a client renders a spreadless
	// row is an open owner decision, and pinning it in an engine test would freeze
	// a choice that has not been made.
	play := playByKey(t, corpusResult(t, HorizonRecent), spreadlessKey)

	if !play.LowLiquidity {
		t.Errorf("LowLiquidity = false at a return of %v, want true", play.RoiPct)
	}
	tick := 1.0 / 170.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(170, tick, [2]float64{170, tick}))
	if play.RoiPctRaw != 0 {
		t.Errorf("RoiPctRaw = %v, want 0 — 170 is both the hour's low and its high", play.RoiPctRaw)
	}
	if play.Legs[0].Price != 170 || play.Legs[1].Price != 170 {
		t.Errorf("legs priced %v / %v, want the 170 both extremes printed", play.Legs[0].Price, play.Legs[1].Price)
	}
	if play.Legs[0].PriceQuoteQty != 170 || play.Legs[0].PriceItemQty != 1 {
		t.Errorf("buy leg pair = %d:%d, want the 170:1 the feed posted", play.Legs[0].PriceQuoteQty, play.Legs[0].PriceItemQty)
	}
}

func TestCorpus_spreadless170AgainstItsOlderFairValue_expectsALoss(t *testing.T) {
	// The reading the flagged hour cannot give. Every older hour prices this item
	// at 100/101 with its mass at 100, and a one-chaos spread is narrower than the
	// two ticks a round trip costs there — so posting these orders hour after hour
	// LOSES, and the last entry chases the 170 print and then meets the market
	// halfway to that hour's own fair. A displayed 170 next to a positive
	// expectation would be the misreading; the expectation is negative.
	play := playByKey(t, corpusResult(t, HorizonRecent), spreadlessKey)

	if !(play.ExpectedRoi < 0) {
		t.Errorf("ExpectedRoi = %v chaos, want a loss", play.ExpectedRoi)
	}
	if !(play.ExpectedRoiPct < 0) {
		t.Errorf("ExpectedRoiPct = %v, want a loss", play.ExpectedRoiPct)
	}
	if play.LowCoverage {
		t.Errorf("LowCoverage = true over %d entries, want false — the loss is measured, not unmeasured", play.SimEntries)
	}
}

func TestCorpus_mawrBlaiddJunkLowsBehindTheNewestHour_priceTheServedPlayFromTheNewestHourAlone(t *testing.T) {
	// THE BLEND INCIDENT (POE-188). Four CONSECUTIVE hours of this market printed
	// lows of 62, 70, 75 and 81 chaos against a volume-weighted price near 250.
	// They sit inside the six-hour window, so any aggregation that reached across
	// hours for a cheaper low would show one of them here — and a price assembled
	// from two hours is a trade nobody could have made.
	//
	// The newest hour priced the map at 240/260, and that is the whole of what the
	// served row may show.
	play := playByKey(t, corpusResult(t, HorizonRecent), mawrKey)

	if play.Legs[0].Price != 240 || play.Legs[1].Price != 260 {
		t.Errorf("legs priced %v / %v, want the newest hour's 240 / 260", play.Legs[0].Price, play.Legs[1].Price)
	}
	for _, junk := range []float64{62, 70, 75, 81} {
		if play.Legs[0].Price == junk {
			t.Errorf("buy leg priced at %v — a junk low from a window hour reached the served row", junk)
		}
	}
	if !play.LastHour.Equal(corpusNewest) {
		t.Errorf("LastHour = %v, want the newest hour %v", play.LastHour, corpusNewest)
	}
	if play.Suspect {
		t.Errorf("Suspect = true, want false — the newest hour's low sits beside its own 250-chaos fair, whatever the hours behind it printed")
	}
}

func TestCorpus_mawrBlaiddJunkLowInTheNewestHour_isServedWithTheExtremeMarkedSuspect(t *testing.T) {
	// The mirror market: the same 62-chaos low, in the hour the engine cannot
	// refuse to price because it is the last snapshot. The row is served with the
	// extreme MARKED rather than dropped — Config.SuspectLowBand was calibrated on
	// exactly this print, a low under two thirds of its hour's own fair.
	play := playByKey(t, corpusResult(t, HorizonRecent), mawrJunkKey)

	if !play.Suspect {
		t.Errorf("Suspect = false at a low of %v against a fair of %v, want true", play.Legs[0].Price, play.Legs[0].Fair)
	}
	if !play.Legs[0].Suspect {
		t.Errorf("buy leg Suspect = false, want true — the junk low is the buy leg's own extreme")
	}
	if play.Legs[1].Suspect {
		t.Errorf("sell leg Suspect = true, want false — 260 sits inside the high band of a 250-chaos fair")
	}
	if play.Legs[0].Price != 62 {
		t.Errorf("buy leg priced at %v, want the 62 the hour printed — a flagged row still has to be recheckable against the game", play.Legs[0].Price)
	}
}

func TestCorpus_mawrBlaiddJunkLowInTheNewestHour_ranksAfterEveryCleanPlay(t *testing.T) {
	// What the flag buys: the biggest printed return in the corpus (+306%) sits
	// below every unflagged row. Ranking is how an unrepeatable extreme is answered
	// — hiding it would delete a reading, and leaving it at the top would sell a
	// trade nobody could make.
	plays := corpusResult(t, HorizonRecent).Plays
	keys := playKeys(plays)
	junk := indexOf(keys, mawrJunkKey)
	if junk < 0 {
		t.Fatalf("%s is not served (got %v)", mawrJunkKey, keys)
	}

	for i, play := range plays {
		if play.Suspect || i <= junk {
			continue
		}
		t.Errorf("clean play %s ranks at %d, below the suspect %s at %d", play.Key, i, mawrJunkKey, junk)
	}
}

func TestCorpus_divineVesselHundredPercentTick_isServedAtMinusOneHundredPercent(t *testing.T) {
	// POE-184's measured noise market, and the visible cost of the 2026-08-22
	// demotion recorded rather than hidden. A 1/35-to-1/1 extreme pair ticks at a
	// full 100%, so the undercut sell price is Price*(1−1) = 0 and the round trip
	// returns exactly −100%. That arithmetic used to fail the positivity floor and
	// keep the market out of the list; now it is in the list, flagged, and left to
	// the ranking.
	play := playByKey(t, corpusResult(t, HorizonRecent), vesselKey)

	if play.RoiPct != -1 {
		t.Errorf("RoiPct = %v, want exactly -1 — the undercut sell price is zero", play.RoiPct)
	}
	if play.Tick != 1 {
		t.Errorf("Tick = %v, want 1 — the market's next representable price is 100%% away", play.Tick)
	}
	if !play.LowLiquidity {
		t.Errorf("LowLiquidity = false at a return of %v, want true", play.RoiPct)
	}
}

func TestCorpus_divineVesselHundredPercentTick_ranksBelowEveryPositiveExpectationPlay(t *testing.T) {
	// The ranking is what keeps the class off the top of the table, and it is the
	// whole answer: no gate is armed by default to remove it. A reader who wants
	// the class gone arms MinEdgeTickRatio or MinROIChaos.
	plays := corpusResult(t, HorizonRecent).Plays
	keys := playKeys(plays)
	vessel := indexOf(keys, vesselKey)
	if vessel < 0 {
		t.Fatalf("%s is not served (got %v)", vesselKey, keys)
	}

	positives := 0
	for i, play := range plays {
		if play.ExpectedRoi <= 0 {
			continue
		}
		positives++
		if i > vessel {
			t.Errorf("%s expects %v chaos and ranks at %d, below the -100%% Divine Vessel at %d", play.Key, play.ExpectedRoi, i, vessel)
		}
	}
	if positives == 0 {
		t.Fatal("no play in the corpus expects a gain — the fixture no longer separates the noise market from the opportunities")
	}
}

func TestCorpus_youngRecipe_isServedWithItsExpectationMarkedLowCoverage(t *testing.T) {
	// The twelve-entry coverage guard, exercised rather than merely satisfied.
	// This recipe traded in the newest five hours only, which yields four
	// simulable entries — the newest hour can never be one — so its expectation is
	// LABELLED rather than trusted. "We could not measure this" and "we measured
	// this and it is bad" are different claims, and only the second belongs above
	// a measured play.
	got := corpusResult(t, HorizonRecent)
	play := playByKey(t, got, youngKey)

	if play.SimEntries != 4 {
		t.Errorf("SimEntries = %d, want 4 — five data hours offer four entries", play.SimEntries)
	}
	if !play.LowCoverage {
		t.Errorf("LowCoverage = false over %d entries against a guard of %d, want true", play.SimEntries, DefaultConfig().MinSimEntries)
	}
	keys := playKeys(got.Plays)
	if indexOf(keys, youngKey) < indexOf(keys, spreadlessKey) {
		t.Errorf("the unmeasured play ranks above the measured loss %s (order %v)", spreadlessKey, keys)
	}
}

func TestCorpus_apocalypseThinNewestHour_isPricedFromTheWindow(t *testing.T) {
	// THE INCIDENT (2026-09-04, POE-252). The card's 17:00Z hour traded ONE card,
	// at 552, and that trade was the owner's own purchase. One trade collapses the
	// low and the high onto the same number, so the served row read 552/552 for
	// -0% while the game's book had stood near 550/1150 all day.
	//
	// The six clock hours behind it had REALIZED 486 (12:00Z) and 1148 (16:00Z),
	// and that is the spread the row must serve, marked with the span it came
	// from. The window is the closed clock span [12:00, 17:00]: its lows are
	// {552, 511, 500, 500, 487, 486} and its highs {552, 1148, 1148, 500, 1140,
	// 486}. The task's acceptance says "low ~ 487-511"; the table's own arithmetic
	// says 486, which is what the "~" covers (plan C7), so the bound is asserted
	// as a range and the computed number as a literal inside it.
	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			play := playByKey(t, corpusResult(t, horizon), apocalypseWindowKey)

			if !play.WindowPriced {
				t.Fatalf("WindowPriced = false at a one-card hour, want true — the row is back to serving %v/%v", play.Legs[0].Price, play.Legs[1].Price)
			}
			if play.WindowHours != 6 {
				t.Errorf("WindowHours = %d, want 6 — all six hours of [12:00, 17:00] priced", play.WindowHours)
			}
			if play.WindowVolume != 11 {
				t.Errorf("WindowVolume = %v, want the 11 cards those six hours traded between them", play.WindowVolume)
			}

			buy, sell := play.Legs[0], play.Legs[1]
			if buy.Price < 485 || buy.Price > 511 {
				t.Errorf("buy leg Price = %v, want the window's low inside the acceptance band [485, 511]", buy.Price)
			}
			if buy.Price != 486 {
				t.Errorf("buy leg Price = %v, want the 486 the 12:00Z hour realized", buy.Price)
			}
			if sell.Price != 1148 {
				t.Errorf("sell leg Price = %v, want the 1148 the 16:00Z hour realized", sell.Price)
			}
			// The whole posting travels with the price, never a synthesized number:
			// the pair is what a reader types into the game and what
			// docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md sizes the row from.
			if buy.PriceQuoteQty != 486 || buy.PriceItemQty != 1 {
				t.Errorf("buy leg pair = %d:%d, want the 486:1 the 12:00Z hour posted", buy.PriceQuoteQty, buy.PriceItemQty)
			}
			if sell.PriceQuoteQty != 1148 || sell.PriceItemQty != 1 {
				t.Errorf("sell leg pair = %d:%d, want the 1148:1 the 16:00Z hour posted", sell.PriceQuoteQty, sell.PriceItemQty)
			}

			// C5: the price is the window's, the STEP is the scored hour's. On this
			// row tick is 1/552 and not 1/486, so tick == 1/max(pair) deliberately
			// no longer rechecks — the price and the step come from different hours
			// because the step is what the market can express NOW.
			tick := 1.0 / 552.0
			if play.Tick != tick {
				t.Errorf("Tick = %v, want the scored hour's %v — tick is what the market is doing now", play.Tick, tick)
			}
			if !play.LastHour.Equal(corpusNewest) {
				t.Errorf("LastHour = %v, want the scored hour %v", play.LastHour, corpusNewest)
			}

			wantClose(t, "RoiPct", play.RoiPct, undercutRoi(486, tick, [2]float64{1148, tick}))
			if play.RoiPct <= 0 {
				t.Errorf("RoiPct = %v, want the positive return the window's spread pays", play.RoiPct)
			}
			if play.LowLiquidity {
				t.Errorf("LowLiquidity = true at a return of %v, want false — the window found the spread the hour could not print", play.RoiPct)
			}

			// ACCEPTED, not a defect (plan C4). The pooled window VWAP is 8110 chaos
			// over 11 cards = 737.27, so 486 sits under 737.27*0.67 = 493.9 and 1148
			// over 737.27*1.5 = 1105.9: both bands are crossed and both legs are
			// flagged. An hour-priced row showing the same 486/1148 against the same
			// fair would be flagged identically — a 2.4x spread genuinely IS wide —
			// and ADR-018 keeps the flagged row readable and orderable at face value.
			if !play.Suspect {
				t.Errorf("Suspect = false against a pooled window fair of %v, want true — a 2.4x spread is wide however it was priced", buy.Fair)
			}
			if !buy.Suspect || !sell.Suspect {
				t.Errorf("leg Suspect = %v / %v, want both true — each extreme is outside its own band of the window's fair", buy.Suspect, sell.Suspect)
			}
		})
	}
}

func TestCorpus_apocalypseThickNewestHour_isPricedFromTheHourAndUnmarked(t *testing.T) {
	// The same market one hour earlier, and the boundary the whole feature turns
	// on. 16:00Z traded TWO cards, at 511 and 1148, which is ThinHourVolume
	// exactly — thin means UNDER the level — so the hour printed a spread of its
	// own and the window is never consulted. The task's acceptance says this hour
	// is served hour-priced and unmarked, and it is the arm that says the window
	// path did not simply take over pricing.
	play := playByKey(t, corpusResultShifted(t, HorizonRecent, 1), apocalypseWindowKey)

	if play.WindowPriced {
		t.Fatalf("WindowPriced = true at a two-card hour, want false — %d hours of window reached a row that printed its own spread", play.WindowHours)
	}
	if play.WindowHours != 0 || play.WindowVolume != 0 {
		t.Errorf("WindowHours/WindowVolume = %d/%v on an hour-priced row, want 0/0", play.WindowHours, play.WindowVolume)
	}
	if play.Legs[0].Price != 511 || play.Legs[1].Price != 1148 {
		t.Errorf("legs priced %v / %v, want the 16:00Z hour's own 511 / 1148", play.Legs[0].Price, play.Legs[1].Price)
	}
	tick := 1.0 / 511.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(511, tick, [2]float64{1148, tick}))
	if play.RoiPct <= 0 {
		t.Errorf("RoiPct = %v, want the positive return the hour's own spread pays", play.RoiPct)
	}
}

func TestCorpus_apocalypseSinglePrintThickHour_servesTheResidualLoss(t *testing.T) {
	// The residual ThinHourVolume = 2 deliberately does NOT catch, pinned rather
	// than left latent. 07:00Z traded two cards at ONE 1196:1 print: two units is
	// thick by the level, so the hour is priced from itself, both extremes are the
	// same number, and the round trip pays two ticks against no spread.
	//
	// Raising the level to 3 would catch this hour and would also break the
	// acceptance, which fixes 16:00Z's two-card hour as thick and unmarked
	// (TestCorpus_apocalypseThickNewestHour_isPricedFromTheHourAndUnmarked). The
	// two facts are the same knob read from both sides, which is why this test
	// exists next to that one.
	play := playByKey(t, corpusResultShifted(t, HorizonRecent, 10), apocalypseWindowKey)

	if play.WindowPriced {
		t.Errorf("WindowPriced = true, want false — two units is thick at ThinHourVolume %v", DefaultConfig().ThinHourVolume)
	}
	if play.Legs[0].Price != 1196 || play.Legs[1].Price != 1196 {
		t.Errorf("legs priced %v / %v, want the single 1196 print both extremes carry", play.Legs[0].Price, play.Legs[1].Price)
	}
	tick := 1.0 / 1196.0
	wantClose(t, "RoiPct", play.RoiPct, undercutRoi(1196, tick, [2]float64{1196, tick}))
	if play.RoiPct >= 0 {
		t.Errorf("RoiPct = %v, want the loss two ticks against a closed spread produce", play.RoiPct)
	}
}

func TestCorpus_deadWindow_isNotServedInEitherHorizon(t *testing.T) {
	// The control that says a window PRICES a live hour and never revives a dead
	// one. This market traded normally until six hours ago and has published
	// nothing but its last 400/900 ratios since, so its whole clock window is
	// silent and there is no realized print anywhere inside it to serve.
	//
	// It is the guard on the liveness gate, and it is what would fail if that gate
	// were simply removed: the row still PRICES — the stale ratios are intact and
	// both sides carry stock — so a leg dropped only for its zero volume would
	// come straight back at 400/900 and be served as current. WI-3 relaxes exactly
	// that gate and re-asserts this market's absence afterwards.
	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			served := playKeys(corpusResult(t, horizon).Plays)
			if indexOf(served, deadWindowKey) >= 0 {
				t.Errorf("%s was served; nothing in its %d-hour window traded (got %v)", deadWindowKey, deadWindowSilentHours, served)
			}
		})
	}
}

func TestCorpus_deadWindow_isServedOnceItsWindowTradesAgain(t *testing.T) {
	// The positive arm of the control above, and what pins that control to THIS
	// fixture: shifted back to the newest hour the dead-window market still traded,
	// the same rows are served, priced from that hour's own 400/900.
	//
	// Without it the absence assertion passes just as well when the fixture is gone
	// from the corpus entirely, so a market deleted by accident would read as the
	// liveness gate working.
	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			play := playByKey(t, corpusResultShifted(t, horizon, deadWindowSilentHours), deadWindowKey)

			if play.Legs[0].Price != 400 || play.Legs[1].Price != 900 {
				t.Errorf("legs priced %v / %v, want the hour's own 400 / 900", play.Legs[0].Price, play.Legs[1].Price)
			}
			if play.WindowPriced {
				t.Errorf("WindowPriced = true over %d hours, want the live hour's own price", play.WindowHours)
			}
		})
	}
}

func TestCorpus_thickMarkets_areNeverWindowPriced(t *testing.T) {
	// THE POE-220 BACKTEST, as a standing assertion rather than a one-off diff:
	// no thick pair's row changes price because the window path exists.
	//
	// It holds by CONSTRUCTION at the shipped level. Every pre-POE-252 corpus
	// market traded at least two units of its item in every hour — liquidChaosMarket
	// puts 1000 on the item side, and the four hand-built newest-hour overrides
	// carry 2 (Apocalypse), 10 (spreadless 170), 500 (Divine Vessel) and
	// 8000/20000 (the triangle legs). The hand-built quiet hours (Apocalypse three
	// hours back, volume {0, 0}) are not the exception they look like: an hour that
	// traded nothing is dropped by MinVolumePerHour before the window path is
	// reached, so the only hour a window can price is one that traded exactly one
	// unit, and no pre-POE-252 market has one. At ThinHourVolume 2 none of them is
	// ever thin, so none of them can take a window price. The committed wire fixtures
	// carry the same claim byte for byte; this states it in one sentence a reader
	// can check without a diff.
	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			marked := 0
			for _, play := range corpusResult(t, horizon).Plays {
				if play.Key == apocalypseWindowKey {
					marked++
					continue
				}
				if play.WindowPriced {
					t.Errorf("%s is window-priced over %d hours, want its own hour's price", play.Key, play.WindowHours)
				}
				for i, leg := range play.Legs {
					if leg.WindowPriced {
						t.Errorf("%s leg %d (%s) is window-priced, want its own hour's price", play.Key, i, leg.Action)
					}
				}
			}
			// Without this the loop would also pass with the window path deleted,
			// and would then be asserting nothing at all.
			if marked != 1 {
				t.Errorf("%d window-priced plays in the corpus, want exactly the Apocalypse-window market", marked)
			}
		})
	}
}

func TestCorpus_apocalypseWindowShifts_markTheThinHoursAndOnlyThose(t *testing.T) {
	// The measured feed read hour by hour, for every hour that PUBLISHED a traded
	// row. Each shift ranks the corpus as if that hour were the newest, so the
	// same seventeen-hour table answers seventeen different questions about which
	// reading a row takes.
	//
	// The rule is one line and the table is what proves it is applied per hour and
	// not once: an hour that traded under two cards is priced from the window it
	// looks back over, and every other hour is priced from itself. WindowHours is
	// the number of hours in the closed clock span [back, back+5] that published a
	// TRADED row — the gaps at backs 6, 13, 14, 16, 18, 19, 21, 23, 24 and the
	// untraded row at back 9 contribute nothing, which is why the three thin hours
	// behind back 0 look back over fewer than six.
	//
	// The ten hours NOT listed here are the ones with no traded row of their own.
	// Whether the pair is served in those hours at all is WI-3's question — the
	// no-flicker acceptance — and nothing about them is asserted in this WI.
	shifts := []struct {
		back         int
		cards        int64
		windowPriced bool
		windowHours  int
	}{
		{back: 0, cards: 1, windowPriced: true, windowHours: 6},
		{back: 1, cards: 2},
		{back: 2, cards: 3},
		{back: 3, cards: 1, windowPriced: true, windowHours: 5},
		{back: 4, cards: 3},
		{back: 5, cards: 1, windowPriced: true, windowHours: 4},
		{back: 7, cards: 3},
		{back: 8, cards: 1, windowPriced: true, windowHours: 4},
		{back: 10, cards: 2},
		{back: 11, cards: 8},
		{back: 12, cards: 2},
		{back: 15, cards: 5},
		{back: 17, cards: 31},
		{back: 20, cards: 4},
		{back: 22, cards: 12},
		{back: 25, cards: 6},
	}

	for _, tt := range shifts {
		t.Run(fmt.Sprintf("back%d", tt.back), func(t *testing.T) {
			// The table is the fixture's own, so a fixture edit cannot silently
			// leave this list describing hours that no longer exist.
			if hour := apocalypseWindowMeasurement[tt.back]; !hour.present || hour.cards != tt.cards {
				t.Fatalf("back %d is %+v in the feed, want a published row of %d cards", tt.back, hour, tt.cards)
			}

			play := playByKey(t, corpusResultShifted(t, HorizonRecent, tt.back), apocalypseWindowKey)

			if play.WindowPriced != tt.windowPriced {
				t.Errorf("WindowPriced = %v over %d cards, want %v", play.WindowPriced, tt.cards, tt.windowPriced)
			}
			if play.WindowHours != tt.windowHours {
				t.Errorf("WindowHours = %d, want %d", play.WindowHours, tt.windowHours)
			}
		})
	}
}

func TestCorpus_wireGolden(t *testing.T) {
	// The committed wire fixtures the desktop's vitest suite reads.
	//
	// SHAPE: this is the ENGINE's exchange.Result, marshalled exactly as the HTTP
	// handler marshals the same values. internal/server/handlers embeds
	// exchange.Play and exchange.Leg rather than copying them field by field, so
	// every key and every value below reaches the desktop byte-identical; what the
	// handler adds around them is an envelope (lastUpdated/warm/mode/count/
	// categories) and six display fields per leg (itemName/itemIcon/itemCategory
	// and their quote twins). Those are the transport layer's, and the handler
	// package cannot be imported from here — it imports this one.
	//
	// The fixture is REGENERATED, never hand-edited:
	//
	//	go test ./internal/exchange/ -run TestCorpus_wireGolden -update
	//
	// A diff here is not a failure to silence. It is the corpus telling you which
	// numbers your change moved, for markets named after the incidents that
	// produced them.
	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			got := marshalWire(t, corpusResult(t, horizon))
			path := wireGoldenPath(horizon)

			if *updateWireGolden {
				if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
					t.Fatalf("create %s: %v", filepath.Dir(path), err)
				}
				if err := os.WriteFile(path, got, 0o644); err != nil {
					t.Fatalf("write %s: %v", path, err)
				}
				t.Logf("wrote %s (%d bytes)", path, len(got))
				return
			}

			want, err := os.ReadFile(path)
			if err != nil {
				t.Fatalf("read %s: %v (regenerate with -update)", path, err)
			}
			if !bytes.Equal(got, want) {
				t.Errorf("%s is stale: %s", path, firstDifference(want, got))
			}
		})
	}
}

func TestCorpus_shuffledFeedOrder_producesTheSameWireBytes(t *testing.T) {
	// The property the golden fixtures rest on: the answer is a function of the
	// feed, not of the order storage happened to return it in. BestPlays groups by
	// hour, sorts every id it walks and sorts the simulation's observations, and a
	// single map range that reached the output would show up here as a diff no
	// amount of rerunning would settle.
	shuffled := corpusRows()
	rand.New(rand.NewSource(7)).Shuffle(len(shuffled), func(i, j int) {
		shuffled[i], shuffled[j] = shuffled[j], shuffled[i]
	})

	for _, horizon := range []Horizon{HorizonRecent, HorizonDay} {
		t.Run(string(horizon), func(t *testing.T) {
			ordered := corpusResult(t, horizon)

			out := BestPlays(corpusLeague, shuffled, corpusConfig(t, horizon))
			out.Horizon = string(horizon)

			if got, want := marshalWire(t, out), marshalWire(t, ordered); !bytes.Equal(got, want) {
				t.Errorf("shuffling the feed changed the answer: %s", firstDifference(want, got))
			}
		})
	}
}

// marshalWire renders a Result the way the fixtures carry it: indented, with a
// trailing newline so the committed file is a well-formed text file.
func marshalWire(t *testing.T, result Result) []byte {
	t.Helper()
	data, err := json.MarshalIndent(result, "", "  ")
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return append(data, '\n')
}

// firstDifference names the first line two JSON documents disagree on, because a
// byte offset in a thousand-line fixture tells a reader nothing about which play
// moved.
func firstDifference(want, got []byte) string {
	wantLines := strings.Split(string(want), "\n")
	gotLines := strings.Split(string(got), "\n")
	for i := 0; i < len(wantLines) || i < len(gotLines); i++ {
		w, g := "<end of file>", "<end of file>"
		if i < len(wantLines) {
			w = wantLines[i]
		}
		if i < len(gotLines) {
			g = gotLines[i]
		}
		if w != g {
			return fmt.Sprintf("first difference at line %d:\n  committed: %s\n  computed:  %s", i+1, w, g)
		}
	}
	return "no line differs (trailing bytes only)"
}
