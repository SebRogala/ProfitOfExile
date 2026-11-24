package exchange

import (
	"encoding/json"
	"math"
	"os"
	"reflect"
	"sort"
	"strconv"
	"testing"
)

// parityFixture is testdata/wire/page-parity.json: plays with the display values the
// desktop's TS helpers computed for them, and filter cases over the same plays.
// It was generated once from the TS chain (ADR-030 WI-6); the regeneration
// script is recorded in the WI-6 implementation note.
type parityFixture struct {
	DivineChaosRate float64 `json:"divineChaosRate"`
	Plays           []struct {
		Play      Play              `json:"play"`
		NonFinite map[string]string `json:"nonFinite"`
		Display   PlayDisplay       `json:"display"`
	} `json:"plays"`
}

func loadParity(t *testing.T) parityFixture {
	t.Helper()
	raw, err := os.ReadFile("testdata/wire/page-parity.json")
	if err != nil {
		t.Fatalf("read parity fixture: %v", err)
	}
	var f parityFixture
	if err := json.Unmarshal(raw, &f); err != nil {
		t.Fatalf("decode parity fixture: %v", err)
	}
	if len(f.Plays) == 0 {
		t.Fatal("parity fixture carries no plays")
	}
	return f
}

// applyNonFinite sets the play fields JSON cannot carry (NaN, ±Inf).
func applyNonFinite(t *testing.T, p *Play, fields map[string]string) {
	t.Helper()
	for field, raw := range fields {
		v, err := strconv.ParseFloat(raw, 64)
		if err != nil {
			t.Fatalf("nonFinite %s=%q: %v", field, raw, err)
		}
		switch field {
		case "expectedRoi":
			p.ExpectedRoi = v
		case "depth":
			p.Depth = v
		case "roi":
			p.Roi = v
		case "investment":
			p.Investment = v
		default:
			t.Fatalf("nonFinite field %q is not handled by the test", field)
		}
	}
}

func sameNum(a, b DisplayNumber) bool {
	fa, fb := float64(a), float64(b)
	if math.IsNaN(fa) || math.IsNaN(fb) {
		return math.IsNaN(fa) && math.IsNaN(fb)
	}
	return fa == fb || math.Abs(fa-fb) <= 1e-9*math.Max(math.Abs(fa), math.Abs(fb))
}

func displayDiff(got, want PlayDisplay) []string {
	var d []string
	add := func(name string, ok bool, g, w any) {
		if !ok {
			d = append(d, name+": got "+jsonOf(g)+", want "+jsonOf(w))
		}
	}
	add("units", got.Units == want.Units, got.Units, want.Units)
	add("basis", got.Basis == want.Basis, got.Basis, want.Basis)
	add("money.investment", sameNum(got.Money.Investment, want.Money.Investment), got.Money.Investment, want.Money.Investment)
	add("money.roi", sameNum(got.Money.Roi, want.Money.Roi), got.Money.Roi, want.Money.Roi)
	add("money.expectedRoi", sameNum(got.Money.ExpectedRoi, want.Money.ExpectedRoi), got.Money.ExpectedRoi, want.Money.ExpectedRoi)
	add("expectedPrinted", got.ExpectedPrinted == want.ExpectedPrinted, got.ExpectedPrinted, want.ExpectedPrinted)
	add("runInvestment", sameNum(got.RunInvestment, want.RunInvestment), got.RunInvestment, want.RunInvestment)
	add("sortKeys.expected", sameNum(got.SortKeys.Expected, want.SortKeys.Expected), got.SortKeys.Expected, want.SortKeys.Expected)
	add("sortKeys.roi", sameNum(got.SortKeys.Roi, want.SortKeys.Roi), got.SortKeys.Roi, want.SortKeys.Roi)
	add("sortKeys.fastestHours", sameNum(got.SortKeys.FastestHours, want.SortKeys.FastestHours), got.SortKeys.FastestHours, want.SortKeys.FastestHours)
	switch {
	case (got.Scale == nil) != (want.Scale == nil):
		d = append(d, "scale: got "+jsonOf(got.Scale)+", want "+jsonOf(want.Scale))
	case got.Scale != nil:
		g, w := *got.Scale, *want.Scale
		add("scale.flips", sameNum(g.Flips, w.Flips), g.Flips, w.Flips)
		add("scale.gain", sameNum(g.Gain, w.Gain), g.Gain, w.Gain)
		add("scale.investment", sameNum(g.Investment, w.Investment), g.Investment, w.Investment)
		add("scale.hours", sameNum(g.Hours, w.Hours), g.Hours, w.Hours)
	}
	return d
}

func jsonOf(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		return err.Error()
	}
	return string(b)
}

// TestPlayDisplay_MatchesTheParityCorpusUnderTheLotRule decorates the parity
// fixture's plays, which stay the historical TS-chain input and output, and
// compares the Go display against currentParityDisplay: the TS display for
// every field the POE-489 lot rule leaves alone, the lotParityRunCosts row
// where it changes the run cost, scale or fastest key.
func TestPlayDisplay_MatchesTheParityCorpusUnderTheLotRule(t *testing.T) {
	f := loadParity(t)
	for _, entry := range f.Plays {
		play := entry.Play
		applyNonFinite(t, &play, entry.NonFinite)
		got := Decorate(play).Display
		if got == nil {
			t.Fatalf("%s: Decorate left Display nil", play.Key)
		}
		if diff := displayDiff(*got, currentParityDisplay(play.Key, entry.Display)); len(diff) > 0 {
			t.Errorf("%s: display differs from the current parity display:\n%v", play.Key, diff)
		}
	}
}

// nullNumber is a display number JSON carries as null.
var nullNumber = DisplayNumber(math.NaN())

// nullScale is the scale of a positive expectation over a run with no
// executable lot: every count and amount is non-finite, so null.
func nullScale() *PlayScale {
	return &PlayScale{Flips: nullNumber, Gain: nullNumber, Investment: nullNumber, Hours: nullNumber}
}

// lotParityRunCost is one parity play's current Go run cost and scale under
// the POE-489 lot rule, worked from the play's own feed pairs and ticks: a unit
// price is q*(n+1)/(i*n) to buy and q*(n-1)/(i*n) to sell, with n = 1/tick; the
// lot N is the fewest items whose sell (and, on a hop, converted) proceeds are
// whole; the lot costs ceil(N * buy unit) entry-quote units, valued at the
// entry's rate (1 for chaos, the fixture's 200 for divine). A positive
// expectation needs flips = ceil(100/E), lots = max(1, ceil(flips/N)), and the
// scale counts lots*N items with hours = ceil(lots*N/depth). The fixture's
// recent and day: rows share one feed, so each change is listed for both.
type lotParityRunCost struct {
	key   string
	run   float64 // math.Inf(1): no executable lot
	scale *PlayScale
}

// lotParityRunCosts lists every parity play whose display the lot rule
// changes. The Apocalypse and Spreadless 170 plays are one-item lots at a whole
// entry cost (223*224/223 = 224, 170*171/170 = 171), so they keep Investment x
// flips; the expected-roi-tiny and expected-roi-1e-18 rows land on the same
// numbers (625 divides 1e20 flips) and are left out with them.
var lotParityRunCosts = []lotParityRunCost{
	// buy 1000*1001/(1*1000) = 1001
	// sell 1200*999/(1*1000) = 5994/5
	// proceeds per item 5994/5 -> N=5
	// lot cost ceil(5*1001)=5005 chaos = 5005c
	// expectedRoi 195.5531521739132: ceil(100/E)=1 flips, lots=max(1,ceil(1/5))=1, scale flips=5, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyCleanFlip", 5005, &PlayScale{Flips: 5, Gain: 195.5531521739132 * 5, Investment: 5005, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyCleanFlip", 5005, &PlayScale{Flips: 5, Gain: 195.5531521739132 * 5, Investment: 5005, Hours: 1}},
	// buy 240*241/(1*240) = 241
	// sell 260*239/(1*240) = 3107/12
	// proceeds per item 3107/12 -> N=12
	// lot cost ceil(12*241)=2892 chaos = 2892c
	// expectedRoi 38.84784779269354: ceil(100/E)=3 flips, lots=max(1,ceil(3/12))=1, scale flips=12, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Maps/MapMawrBlaidd", 2892, &PlayScale{Flips: 12, Gain: 38.84784779269354 * 12, Investment: 2892, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Maps/MapMawrBlaidd", 2892, &PlayScale{Flips: 12, Gain: 38.84784779269354 * 12, Investment: 2892, Hours: 1}},
	// buy 100*101/(1*100) = 101
	// sell 130*99/(1*100) = 1287/10
	// proceeds per item 1287/10 -> N=10
	// lot cost ceil(10*101)=1010 chaos = 1010c
	// expectedRoi 27.380826086956503: ceil(100/E)=4 flips, lots=max(1,ceil(4/10))=1, scale flips=10, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Tattoos/JourneyTattooTwoSided", 1010, &PlayScale{Flips: 10, Gain: 27.380826086956503 * 10, Investment: 1010, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Tattoos/JourneyTattooTwoSided", 1010, &PlayScale{Flips: 10, Gain: 27.380826086956503 * 10, Investment: 1010, Hours: 1}},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 22.170434782608687: ceil(100/E)=5 flips, lots=max(1,ceil(5/625))=1, scale flips=625, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Tattoos/JourneyTattoo", 67600, &PlayScale{Flips: 625, Gain: 22.170434782608687 * 625, Investment: 67600, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Tattoos/JourneyTattoo", 67600, &PlayScale{Flips: 625, Gain: 22.170434782608687 * 625, Investment: 67600, Hours: 1}},
	// buy 13*26/(25*25) = 338/625
	// sell 130*99/(1*100) = 1287/10
	// convert 1*199/(200*200) = 199/40000
	// proceeds per item 1287/10, through conversion 256113/400000 -> N=400000
	// N above 10000: run cost +Inf
	// positive expectedRoi 20.220826086956503 over no executable lot: every scale number is null.
	{"1-hop:Metadata/Items/Tattoos/JourneyTattoo|Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Currency/CurrencyRerollRare", math.Inf(1), nullScale()},
	{"day:1-hop:Metadata/Items/Tattoos/JourneyTattoo|Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Currency/CurrencyRerollRare", math.Inf(1), nullScale()},
	// buy 10*11/(1*10) = 11
	// sell 1*9/(10*10) = 9/100
	// convert 200*199/(1*200) = 199
	// proceeds per item 9/100, through conversion 1791/100 -> N=100
	// lot cost ceil(100*11)=1100 chaos = 1100c
	// expectedRoi 6.913043478260871: ceil(100/E)=15 flips, lots=max(1,ceil(15/100))=1, scale flips=100, hours=ceil(flips/depth 8000).
	{"1-hop:Metadata/Items/Threads/BreachThread|Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyModValues", 1100, &PlayScale{Flips: 100, Gain: 6.913043478260871 * 100, Investment: 1100, Hours: 1}},
	{"day:1-hop:Metadata/Items/Threads/BreachThread|Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyModValues", 1100, &PlayScale{Flips: 100, Gain: 6.913043478260871 * 100, Investment: 1100, Hours: 1}},
	// buy 1*11/(20*10) = 11/200
	// sell 1*9/(10*10) = 9/100
	// proceeds per item 9/100 -> N=100
	// lot cost ceil(100*11/200)=6 div x 200 = 1200c
	// expectedRoi 6.913043478260869: ceil(100/E)=15 flips, lots=max(1,ceil(15/100))=1, scale flips=100, hours=ceil(flips/depth 8000).
	{"direct:Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Threads/BreachThread", 1200, &PlayScale{Flips: 100, Gain: 6.913043478260869 * 100, Investment: 1200, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Threads/BreachThread", 1200, &PlayScale{Flips: 100, Gain: 6.913043478260869 * 100, Investment: 1200, Hours: 1}},
	// buy 1*201/(200*200) = 201/40000
	// sell 1*199/(200*200) = 199/40000
	// proceeds per item 199/40000 -> N=40000
	// N above 10000: run cost +Inf
	// expectedRoi -0.009891304347826037 is not positive finite: one lot, no scale.
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyModValues", math.Inf(1), nil},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyModValues", math.Inf(1), nil},
	// buy 10*11/(1*10) = 11
	// sell 12*9/(1*10) = 54/5
	// proceeds per item 54/5 -> N=5
	// lot cost ceil(5*11)=55 chaos = 55c
	// expectedRoi -0.21739130434782505 is not positive finite: one lot, no scale.
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Threads/BreachThread", 55, nil},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Threads/BreachThread", 55, nil},
	// buy 1*11/(20*10) = 11/200
	// sell 12*9/(1*10) = 54/5
	// convert 1*199/(200*200) = 199/40000
	// proceeds per item 54/5, through conversion 5373/100000 -> N=100000
	// N above 10000: run cost +Inf
	// expectedRoi -0.21739130434782625 is not positive finite: one lot, no scale.
	{"1-hop:Metadata/Items/Threads/BreachThread|Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Currency/CurrencyRerollRare", math.Inf(1), nil},
	{"day:1-hop:Metadata/Items/Threads/BreachThread|Metadata/Items/Currency/CurrencyModValues|Metadata/Items/Currency/CurrencyRerollRare", math.Inf(1), nil},
	// buy 200*201/(1*200) = 201
	// sell 260*199/(1*200) = 2587/10
	// proceeds per item 2587/10 -> N=10
	// lot cost ceil(10*201)=2010 chaos = 2010c
	// expectedRoi 53.866875000000036: ceil(100/E)=2 flips, lots=max(1,ceil(2/10))=1, scale flips=10, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyYoungRecipe", 2010, &PlayScale{Flips: 10, Gain: 53.866875000000036 * 10, Investment: 2010, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyYoungRecipe", 2010, &PlayScale{Flips: 10, Gain: 53.866875000000036 * 10, Investment: 2010, Hours: 1}},
	// buy 486*553/(1*552) = 44793/92
	// sell 1148*551/(1*552) = 158137/138
	// proceeds per item 158137/138 -> N=138
	// lot cost ceil(138*44793/92)=67190 chaos = 67190c
	// expectedRoi 91.54339193448517: ceil(100/E)=2 flips, lots=max(1,ceil(2/138))=1, scale flips=138, hours=ceil(flips/depth 1).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/DivinationCards/DivinationCardApocalypseWindow", 67190, &PlayScale{Flips: 138, Gain: 91.54339193448517 * 138, Investment: 67190, Hours: 138}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/DivinationCards/DivinationCardApocalypseWindow", 67190, &PlayScale{Flips: 138, Gain: 91.54339193448517 * 138, Investment: 67190, Hours: 138}},
	// buy 62*63/(1*62) = 63
	// sell 260*61/(1*62) = 7930/31
	// proceeds per item 7930/31 -> N=31
	// lot cost ceil(31*63)=1953 chaos = 1953c
	// expectedRoi 17.655212716222568: ceil(100/E)=6 flips, lots=max(1,ceil(6/31))=1, scale flips=31, hours=ceil(flips/depth 1000).
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Maps/MapMawrBlaiddJunkNewest", 1953, &PlayScale{Flips: 31, Gain: 17.655212716222568 * 31, Investment: 1953, Hours: 1}},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Maps/MapMawrBlaiddJunkNewest", 1953, &PlayScale{Flips: 31, Gain: 17.655212716222568 * 31, Investment: 1953, Hours: 1}},
	// buy 1*2/(35*1) = 2/35
	// sell 1*0/(1*1) = 0
	// no executable lot: run cost +Inf
	// expectedRoi -0.05240372670807454 is not positive finite: one lot, no scale.
	{"direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyDivineVessel", math.Inf(1), nil},
	{"day:direct:Metadata/Items/Currency/CurrencyRerollRare|Metadata/Items/Currency/CurrencyDivineVessel", math.Inf(1), nil},
	// buy pair 13:0 not positive
	// sell 17*24/(25*25) = 408/625
	// no executable lot: run cost +Inf
	// positive expectedRoi 22.170434782608687 over no executable lot: every scale number is null.
	{"synthetic:no-quantity-pair", math.Inf(1), nullScale()},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 0 is not positive finite: one lot, no scale.
	{"synthetic:expected-roi-zero", 67600, nil},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi -2.5 is not positive finite: one lot, no scale.
	{"synthetic:expected-roi-negative", 67600, nil},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi nan is not positive finite: one lot, no scale.
	{"synthetic:expected-roi-nan", 67600, nil},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 22.170434782608687: ceil(100/E)=5 flips, lots=max(1,ceil(5/625))=1, scale flips=625, hours=ceil(flips/depth 0).
	{"synthetic:depth-zero", 67600, &PlayScale{Flips: 625, Gain: 22.170434782608687 * 625, Investment: 67600, Hours: nullNumber}},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 22.170434782608687: ceil(100/E)=5 flips, lots=max(1,ceil(5/625))=1, scale flips=625, hours=ceil(flips/depth -1).
	{"synthetic:depth-negative", 67600, &PlayScale{Flips: 625, Gain: 22.170434782608687 * 625, Investment: 67600, Hours: nullNumber}},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 22.170434782608687: ceil(100/E)=5 flips, lots=max(1,ceil(5/625))=1, scale flips=625, hours=ceil(flips/depth 5e-324).
	{"synthetic:depth-tiny", 67600, &PlayScale{Flips: 625, Gain: 22.170434782608687 * 625, Investment: 67600, Hours: nullNumber}},
	// buy 13*26/(25*25) = 338/625
	// sell 17*24/(25*25) = 408/625
	// proceeds per item 408/625 -> N=625
	// lot cost ceil(625*338/625)=338 div x 200 = 67600c
	// expectedRoi 0 is not positive finite: one lot, no scale.
	{"synthetic:sim-entries-zero", 67600, nil},
}

// currentParityDisplay is the Go display the parity play keyed key must have
// now: the historical TS display with its run cost, scale and fastest key
// replaced where lotParityRunCosts lists the play.
func currentParityDisplay(key string, historical PlayDisplay) PlayDisplay {
	for _, c := range lotParityRunCosts {
		if c.key != key {
			continue
		}
		d := historical
		d.RunInvestment = DisplayNumber(c.run)
		if math.IsInf(c.run, 0) {
			d.RunInvestment = nullNumber
		}
		d.Scale = c.scale
		d.SortKeys.FastestHours = nullNumber
		if c.scale != nil {
			d.SortKeys.FastestHours = c.scale.Hours
		}
		return d
	}
	return historical
}

// displayPlay is a positive, posting-sized play: 4 per posting, 10c in, 3c best
// case, 2c expected, depth 5/h. Its chaos legs (40:4 and 52:4, tick 0) are
// whole for one item, so its run cost is Investment x flips.
func displayPlay() Play {
	return Play{
		Key:         "direct:test",
		Legs:        []Leg{{Action: "buy", Price: 10, PriceItemQty: 4, PriceQuoteQty: 40}, {Action: "sell", Price: 13, PriceItemQty: 4, PriceQuoteQty: 52}},
		Investment:  10,
		Roi:         3,
		ExpectedRoi: 2,
		SimEntries:  12,
		Depth:       5,
	}
}

func TestPlayDisplay_SingleBasisWithoutAQuantityPair(t *testing.T) {
	p := displayPlay()
	p.Legs[0].PriceQuoteQty = 0
	d := Decorate(p).Display
	if d.Basis != BasisSingle || d.Units != 1 || float64(d.Money.Investment) != 10 {
		t.Fatalf("got basis %q units %d investment %v, want single, 1, 10", d.Basis, d.Units, d.Money.Investment)
	}
}

func TestPlayDisplay_PostingBasisCountsTheBuyLegItemQuantity(t *testing.T) {
	d := Decorate(displayPlay()).Display
	if d.Basis != BasisPosting || d.Units != 4 || float64(d.Money.Roi) != 12 {
		t.Fatalf("got basis %q units %d roi %v, want posting, 4, 12", d.Basis, d.Units, d.Money.Roi)
	}
}

func TestPlayDisplay_NoScaleForNonPositiveExpectedRoi(t *testing.T) {
	p := displayPlay()
	p.ExpectedRoi = 0
	d := Decorate(p).Display
	if d.Scale != nil {
		t.Fatalf("scale = %s, want null", jsonOf(d.Scale))
	}
	if float64(d.RunInvestment) != 10 {
		t.Fatalf("runInvestment = %v, want the per-exchange investment 10", d.RunInvestment)
	}
}

func TestPlayDisplay_NoScaleForNaNExpectedRoi(t *testing.T) {
	p := displayPlay()
	p.ExpectedRoi = math.NaN()
	d := Decorate(p).Display
	if d.Scale != nil {
		t.Fatalf("scale = %s, want null", jsonOf(d.Scale))
	}
	if !math.IsNaN(float64(d.SortKeys.Expected)) {
		t.Fatalf("sortKeys.expected = %v, want a missing key", d.SortKeys.Expected)
	}
}

func TestPlayDisplay_NilHoursForNonPositiveDepth(t *testing.T) {
	p := displayPlay()
	p.Depth = 0
	d := Decorate(p).Display
	if d.Scale == nil || !math.IsNaN(float64(d.Scale.Hours)) || d.Scale.Flips != 50 {
		t.Fatalf("scale = %s, want flips 50 with null hours", jsonOf(d.Scale))
	}
}

func TestPlayDisplay_HugeFlipsStayExactBeyondInt64(t *testing.T) {
	p := displayPlay()
	p.ExpectedRoi = 1e-18
	p.Depth = 1
	d := Decorate(p).Display
	if d.Scale == nil || float64(d.Scale.Flips) != 1e20 || float64(d.Scale.Hours) != 1e20 || float64(d.SortKeys.FastestHours) != 1e20 {
		t.Fatalf("scale = %s, sortKeys = %s; want flips, hours and the fastest key 1e20", jsonOf(d.Scale), jsonOf(d.SortKeys))
	}
}

func TestPlayDisplay_InfiniteFlipsFromATinyExpectationAreNull(t *testing.T) {
	p := displayPlay()
	p.ExpectedRoi = math.SmallestNonzeroFloat64
	raw, err := json.Marshal(Decorate(p).Display)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back struct {
		Scale struct {
			Flips, Gain, Investment, Hours *float64
		} `json:"scale"`
		RunInvestment *float64 `json:"runInvestment"`
		SortKeys      struct {
			FastestHours *float64 `json:"fastestHours"`
		} `json:"sortKeys"`
	}
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("decode %s: %v", raw, err)
	}
	s := back.Scale
	if s.Flips != nil || s.Gain != nil || s.Investment != nil || s.Hours != nil || back.RunInvestment != nil || back.SortKeys.FastestHours != nil {
		t.Fatalf("body %s: want the infinite scale, run investment and fastest key null", raw)
	}
}

func TestPlayDisplay_InfiniteHoursFromATinyDepthAreNull(t *testing.T) {
	p := displayPlay()
	p.Depth = math.SmallestNonzeroFloat64
	d := Decorate(p).Display
	if d.Scale == nil || d.Scale.Flips != 50 || !math.IsNaN(float64(d.Scale.Hours)) || !math.IsNaN(float64(d.SortKeys.FastestHours)) {
		t.Fatalf("scale = %s, sortKeys = %s; want flips 50, null hours and a missing fastest key", jsonOf(d.Scale), jsonOf(d.SortKeys))
	}
}

func TestPlayDisplay_ExpectedNotPrintedWithoutSimEntries(t *testing.T) {
	p := displayPlay()
	p.SimEntries = 0
	d := Decorate(p).Display
	if d.ExpectedPrinted || !math.IsNaN(float64(d.SortKeys.Expected)) {
		t.Fatalf("expectedPrinted %v, sortKeys.expected %v; want false and a missing key", d.ExpectedPrinted, d.SortKeys.Expected)
	}
}

func TestPlayDisplay_NonFiniteNumbersEncodeAsNull(t *testing.T) {
	p := displayPlay()
	p.ExpectedRoi = math.Inf(1)
	raw, err := json.Marshal(Decorate(p).Display)
	if err != nil {
		t.Fatalf("marshal display: %v", err)
	}
	var back struct {
		Money struct {
			ExpectedRoi *float64 `json:"expectedRoi"`
		} `json:"money"`
		Scale *struct{} `json:"scale"`
	}
	if err := json.Unmarshal(raw, &back); err != nil {
		t.Fatalf("decode %s: %v", raw, err)
	}
	if back.Money.ExpectedRoi != nil || back.Scale != nil {
		t.Fatalf("body %s: want money.expectedRoi and scale null", raw)
	}
}

func TestPlay_WithoutDisplayOmitsTheKey(t *testing.T) {
	raw, err := json.Marshal(displayPlay())
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(raw, &fields)
	if _, ok := fields["display"]; ok {
		t.Fatalf("undecorated play carries a display key: %s", raw)
	}
}

// TestPageParity_CaseOrderMatchesNormalizedKeys checks the fixture against the
// API contract's sort rule: the recorded display sort key (non-finite already
// null), descending for expected/roi and ascending for fastest, null keys last,
// ties in served (fixture) order. WI-7's SQL parity test reads these orders.
func TestPageParity_CaseOrderMatchesNormalizedKeys(t *testing.T) {
	raw, err := os.ReadFile("testdata/wire/page-parity.json")
	if err != nil {
		t.Fatalf("read parity fixture: %v", err)
	}
	var f struct {
		Plays []struct {
			Play struct {
				Key string `json:"key"`
			} `json:"play"`
			Display PlayDisplay `json:"display"`
		} `json:"plays"`
		Cases []struct {
			Name        string              `json:"name"`
			Params      map[string][]string `json:"params"`
			OrderedKeys []string            `json:"orderedKeys"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(raw, &f); err != nil {
		t.Fatalf("decode parity fixture: %v", err)
	}
	rank := map[string]int{}
	keys := map[string]PlaySortKeys{}
	for i, p := range f.Plays {
		rank[p.Play.Key] = i
		keys[p.Play.Key] = p.Display.SortKeys
	}
	for _, c := range f.Cases {
		sortBy := SortExpected
		if s := c.Params["sort"]; len(s) > 0 {
			sortBy = PlaysSort(s[0])
		}
		key := func(k string) float64 {
			switch sortBy {
			case SortRoi:
				return float64(keys[k].Roi)
			case SortFastest:
				return float64(keys[k].FastestHours)
			}
			return float64(keys[k].Expected)
		}
		want := append([]string(nil), c.OrderedKeys...)
		sort.SliceStable(want, func(i, j int) bool {
			a, b := key(want[i]), key(want[j])
			an, bn := math.IsNaN(a), math.IsNaN(b)
			switch {
			case an != bn:
				return bn
			case !an && a != b:
				if sortBy == SortFastest {
					return a < b
				}
				return a > b
			}
			return rank[want[i]] < rank[want[j]]
		})
		if !reflect.DeepEqual(want, c.OrderedKeys) {
			t.Errorf("case %q: orderedKeys\n got %v\nwant %v", c.Name, c.OrderedKeys, want)
		}
	}
}

// lotLeg is one leg of a synthetic lot play: its action, entry/sell quote, feed
// pair q:i and tick step 1/n (n 0 is a tick of 0).
type lotLeg struct {
	action string
	quote  string
	q, i   int64
	n      int64
}

// lotPlay builds a play from its legs the way evaluate prices one: Price is the
// pair q/i, Investment is the entry's undercut price Price*(1+Tick) valued at
// entryRate chaos per quote unit.
func lotPlay(mode Mode, entryRate, expectedRoi, depth float64, legs ...lotLeg) Play {
	p := Play{Key: "lot:" + string(mode), Mode: mode, ExpectedRoi: expectedRoi, SimEntries: 12, Depth: depth}
	for _, l := range legs {
		leg := Leg{Action: l.action, Item: lotDirectID, Quote: l.quote, PriceItemQty: l.i, PriceQuoteQty: l.q}
		if l.i != 0 {
			leg.Price = float64(l.q) / float64(l.i)
		}
		if l.n != 0 {
			leg.Tick = 1 / float64(l.n)
		}
		p.Legs = append(p.Legs, leg)
	}
	p.Investment = p.Legs[0].Price * (1 + p.Legs[0].Tick) * entryRate
	return p
}

// keep:critical — silent: every row is a run cost the budget filter compares
// and the display prints as "N c in". An understated or diverging number is a
// plausible value the player's max-run-cost filter trusts, admitting a run the
// player cannot execute within the budget.
func TestKeepCritical_LotRunCostBoundaries(t *testing.T) {
	buy := func(quote string, q, i, n int64) lotLeg { return lotLeg{"buy", quote, q, i, n} }
	sell := func(quote string, q, i, n int64) lotLeg { return lotLeg{"sell", quote, q, i, n} }
	inf := math.Inf(1)
	for _, tc := range []struct {
		name string
		play Play
		// run is the stored raw run cost (+Inf kept); scale is nil when no
		// positive finite expectation scales the run.
		run   float64
		scale *PlayScale
	}{
		{
			// 5/2 and 7/2 per item with no undercut: sell is whole at 2 items,
			// entry ceil(2 * 5/2) = 5 chaos.
			name: "tick zero prices at the feed pair",
			play: lotPlay(ModeDirect, 1, 0, 10, buy(ChaosID, 5, 2, 0), sell(ChaosID, 7, 2, 0)),
			run:  5,
		},
		{
			// Sell 1114*1113/1114 = 1113 is whole for one card; the entry
			// 157*1115/1114 = 157.13 rounds up to ceil(157.13) = 158 chaos.
			name: "fractional entry of a one-item lot rounds up",
			play: lotPlay(ModeDirect, 1, -1, 10, buy(ChaosID, 157, 1, 1114), sell(ChaosID, 1114, 1, 1114)),
			run:  158,
		},
		{
			// Sell 130*119/120 = 1547/12 → 12 items; entry ceil(12*96*121/120) =
			// ceil(1161.6) = 1162, not 12 x ceil(96.8) = 1164.
			name: "entry rounds up once per lot not per item",
			play: lotPlay(ModeDirect, 1, 0, 10, buy(ChaosID, 96, 1, 120), sell(ChaosID, 130, 1, 120)),
			run:  1162,
		},
		{
			// Sell 8*7/(3*8) = 7/3 divine → 3 items alone; converted at
			// 530*529/(3*530) = 529/3 chaos, 7/3 * 529/3 = 3703/9 → 9 items.
			// Entry ceil(9 * 10*11/10) = 99 chaos (33 if the conversion were ignored).
			name: "conversion proceeds enlarge the lot",
			play: lotPlay(ModeOneHop, 1, 0, 10, buy(ChaosID, 10, 1, 10), sell(DivineID, 8, 3, 8), sell(ChaosID, 530, 3, 530)),
			run:  99,
		},
		{
			// A 13:0 pair names no item quantity: no lot can be formed.
			name:  "zero item quantity has no executable lot",
			play:  lotPlay(ModeDirect, 1, 20, 10, buy(ChaosID, 13, 0, 25), sell(ChaosID, 17, 25, 25)),
			run:   inf,
			scale: nullScale(),
		},
		{
			// A full tick sells at 1*(1-1)/1 = 0: there are no proceeds to make whole.
			name: "a sell worth nothing has no executable lot",
			play: lotPlay(ModeDirect, 1, -1, 10, buy(ChaosID, 1, 35, 1), sell(ChaosID, 1, 1, 1)),
			run:  inf,
		},
		{
			// Sell 1/10000 chaos an item → exactly 10000 items, entry 10000 x 1.
			name: "a lot of exactly 10000 items executes",
			play: lotPlay(ModeDirect, 1, 0, 10, buy(ChaosID, 1, 1, 0), sell(ChaosID, 1, 10000, 0)),
			run:  10000,
		},
		{
			// Sell 1/10007 chaos an item → 10007 items, above the 10000 guard.
			name: "a lot above 10000 items has no executable lot",
			play: lotPlay(ModeDirect, 1, 0, 10, buy(ChaosID, 1, 1, 0), sell(ChaosID, 1, 10007, 0)),
			run:  inf,
		},
		{
			// An infinite expectation scales nothing: one 12-item lot, 1162 chaos.
			name: "a non-finite expectation runs one lot",
			play: lotPlay(ModeDirect, 1, inf, 10, buy(ChaosID, 96, 1, 120), sell(ChaosID, 130, 1, 120)),
			run:  1162,
		},
		{
			// The Fiend triangle with E = 5: ceil(100/5) = 20 flips need
			// ceil(20/8) = 3 lots of 8 cards: 24 cards, 3 x 1258 = 3774 chaos,
			// gain 5 x 24 = 120, ceil(24/10) = 3 hours.
			name:  "flips beyond one lot take whole lots",
			play:  lotPlay(ModeOneHop, 1, 5, 10, buy(ChaosID, 157, 1, 1114), sell(DivineID, 27, 1, 24), sell(ChaosID, 176, 1, 176)),
			run:   3774,
			scale: &PlayScale{Flips: 24, Gain: 120, Investment: 3774, Hours: 3},
		},
		{
			// Entry 24*25/24 = 25 divine, sell 27*23/24 = 207/8 → 8 cards:
			// ceil(8 x 25) = 200 divine at the entry hour's 176 = 35200 chaos.
			name: "a divine entry is valued at its scored hour's rate",
			play: lotPlay(ModeDirect, 176, 0, 10, buy(DivineID, 24, 1, 24), sell(DivineID, 27, 1, 24)),
			run:  35200,
		},
		{
			// E = 1e-18: 1e20 flips in ceil(1e20/12) lots of 12 items, about
			// 1e20 items at 1162/12 chaos each, gain 1e-18 x 1e20 = 100,
			// 1e20 hours at depth 1 — finite beyond int64.
			name:  "an enormous flip count stays finite",
			play:  lotPlay(ModeDirect, 1, 1e-18, 1, buy(ChaosID, 96, 1, 120), sell(ChaosID, 130, 1, 120)),
			run:   1162 * 1e20 / 12,
			scale: &PlayScale{Flips: 1e20, Gain: 100, Investment: 1162 * 1e20 / 12, Hours: 1e20},
		},
		{
			// 100/5e-324 overflows: infinitely many flips cost +Inf, all null.
			name:  "an infinite flip count is null",
			play:  lotPlay(ModeDirect, 1, math.SmallestNonzeroFloat64, 1, buy(ChaosID, 96, 1, 120), sell(ChaosID, 130, 1, 120)),
			run:   inf,
			scale: nullScale(),
		},
		{
			// 40:4 and 52:4 are 10 and 13 chaos with no undercut: a whole one-item
			// lot keeps Investment x ceil(100/2) = 10 x 50 = 500.
			name:  "a whole one-item lot keeps investment times flips",
			play:  lotPlay(ModeDirect, 1, 2, 5, buy(ChaosID, 40, 4, 0), sell(ChaosID, 52, 4, 0)),
			run:   500,
			scale: &PlayScale{Flips: 50, Gain: 100, Investment: 500, Hours: 10},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			raw := rawRunInvestment(tc.play)
			if math.IsInf(tc.run, 1) {
				if !math.IsInf(raw, 1) {
					t.Errorf("stored run cost = %v, want +Inf so every ceiling excludes it", raw)
				}
			} else if !sameNum(DisplayNumber(raw), DisplayNumber(tc.run)) {
				t.Errorf("stored run cost = %v, want %v", raw, tc.run)
			}

			d := Decorate(tc.play).Display
			wantRun := DisplayNumber(tc.run)
			if math.IsInf(tc.run, 0) {
				wantRun = nullNumber
			}
			if !sameNum(d.RunInvestment, wantRun) {
				t.Errorf("display runInvestment = %v, want %v", d.RunInvestment, wantRun)
			}
			switch {
			case (d.Scale == nil) != (tc.scale == nil):
				t.Errorf("scale = %s, want %s", jsonOf(d.Scale), jsonOf(tc.scale))
			case d.Scale != nil:
				g, w := *d.Scale, *tc.scale
				if !sameNum(g.Flips, w.Flips) || !sameNum(g.Gain, w.Gain) || !sameNum(g.Investment, w.Investment) || !sameNum(g.Hours, w.Hours) {
					t.Errorf("scale = flips %v gain %v investment %v hours %v, want %v %v %v %v",
						g.Flips, g.Gain, g.Investment, g.Hours, w.Flips, w.Gain, w.Investment, w.Hours)
				}
			}
		})
	}
}
