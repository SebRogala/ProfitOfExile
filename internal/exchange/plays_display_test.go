package exchange

import (
	"encoding/json"
	"math"
	"os"
	"strconv"
	"testing"
)

// parityFixture is testdata/page-parity.json: plays with the display values the
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
	raw, err := os.ReadFile("testdata/page-parity.json")
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

func TestPlayDisplay_MatchesTheTSHelpersOnTheParityCorpus(t *testing.T) {
	f := loadParity(t)
	for _, entry := range f.Plays {
		play := entry.Play
		applyNonFinite(t, &play, entry.NonFinite)
		got := Decorate(play).Display
		if got == nil {
			t.Fatalf("%s: Decorate left Display nil", play.Key)
		}
		if diff := displayDiff(*got, entry.Display); len(diff) > 0 {
			t.Errorf("%s: display differs from the TS helpers:\n%v", play.Key, diff)
		}
	}
}

// displayPlay is a positive, posting-sized play: 4 per posting, 10c in, 3c best
// case, 2c expected, depth 5/h.
func displayPlay() Play {
	return Play{
		Key:         "direct:test",
		Legs:        []Leg{{Action: "buy", PriceItemQty: 4, PriceQuoteQty: 40}, {Action: "sell", PriceItemQty: 4, PriceQuoteQty: 52}},
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
