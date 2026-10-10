package lab

import (
	"bytes"
	"encoding/json"
	"flag"
	"math"
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"
)

// Characterisation of the tier classification, recorded before the tier
// algorithm moves out of this package. The golden files are what the code
// produced at the commit that added them; a diff means Lab output changed.
//
// Regenerate (only when an output change is intended):
//
//	go test -run 'TestClassificationCharacterisation|TestDoubleCorruptCharacterisation' ./internal/lab/ -update
//
// Only public entry points are called, so this file and its goldens survive the
// move unchanged.

var updateGolden = flag.Bool("update", false, "rewrite the committed classification goldens under testdata/classification")

var characterisationTime = time.Date(2026, 10, 10, 0, 0, 0, 0, time.UTC)

const (
	classificationCorpusPath = "testdata/classification/corpus.json"
	classificationGoldenPath = "testdata/classification/golden.json"
	doubleCorruptGoldenPath  = "testdata/classification/golden-doublecorrupt.json"
)

type classificationCase struct {
	Case string     `json:"case"`
	Gems []GemPrice `json:"gems"`
}

func loadClassificationCorpus(t *testing.T) []classificationCase {
	t.Helper()
	raw, err := os.ReadFile(classificationCorpusPath)
	if err != nil {
		t.Fatalf("read corpus: %v", err)
	}
	var cases []classificationCase
	if err := json.Unmarshal(raw, &cases); err != nil {
		t.Fatalf("parse corpus: %v", err)
	}
	if len(cases) == 0 {
		t.Fatal("corpus is empty")
	}
	return cases
}

type classifiedGem struct {
	Name          string `json:"name"`
	Variant       string `json:"variant"`
	Tier          string `json:"tier"`
	LowConfidence bool   `json:"lowConfidence"`
}

type canonicalClassification struct {
	Gems        []classifiedGem           `json:"gems"`
	Boundaries  map[string]TierBoundaries `json:"boundaries"`
	TopBoundary map[string]float64        `json:"topBoundary"`
}

func canonicalise(cls ClassificationResult) canonicalClassification {
	gems := make([]classifiedGem, 0, len(cls.Gems))
	for k, v := range cls.Gems {
		gems = append(gems, classifiedGem{k.Name, k.Variant, v.Tier, v.LowConfidence})
	}
	sort.Slice(gems, func(a, b int) bool {
		if gems[a].Name != gems[b].Name {
			return gems[a].Name < gems[b].Name
		}
		return gems[a].Variant < gems[b].Variant
	})
	return canonicalClassification{Gems: gems, Boundaries: cls.Boundaries, TopBoundary: cls.TopBoundary}
}

type characterisedMarketContext struct {
	// TierBoundaries is recorded only when the Font result has a 20/20 pool:
	// without one, ComputeMarketContext takes "the first available variant"
	// in map order, which is not deterministic.
	TierBoundaries *TierBoundaries           `json:"tierBoundaries,omitempty"`
	VariantTiers   map[string]TierBoundaries `json:"variantTiers"`
}

type characterisedCase struct {
	Font          canonicalClassification            `json:"font"`
	Dedication    map[string]canonicalClassification `json:"dedication"`
	MarketContext characterisedMarketContext         `json:"marketContext"`
}

func characteriseClassification(gems []GemPrice) characterisedCase {
	font := ComputeGemClassification(gems)
	out := characterisedCase{Font: canonicalise(font), Dedication: map[string]canonicalClassification{}}
	for _, transfigured := range []bool{false, true} {
		pool := "skills"
		if transfigured {
			pool = "transfigured"
		}
		for _, variant := range []string{"21/23c", "21/20c"} {
			out.Dedication[pool+" "+variant] = canonicalise(ComputeDedicationClassification(gems, transfigured, variant))
		}
	}
	mc := ComputeMarketContext(characterisationTime, gems, nil, font)
	if _, ok := font.Boundaries["20/20"]; ok {
		tb := mc.TierBoundaries
		out.MarketContext.TierBoundaries = &tb
	}
	out.MarketContext.VariantTiers = make(map[string]TierBoundaries, len(mc.VariantStats))
	for v, s := range mc.VariantStats {
		out.MarketContext.VariantTiers[v] = s.Tiers
	}
	return out
}

func writeGolden(t *testing.T, path string, v any) {
	t.Helper()
	raw, err := json.MarshalIndent(v, "", " ")
	if err != nil {
		t.Fatalf("marshal %s: %v", path, err)
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, append(raw, '\n'), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

// Font, Dedication (both pools × both corrupted variants) and the market
// context's tier fields, per corpus case, are byte-identical to golden.json.
func TestClassificationCharacterisation(t *testing.T) {
	cases := loadClassificationCorpus(t)

	got := make(map[string]characterisedCase, len(cases))
	for _, c := range cases {
		got[c.Case] = characteriseClassification(c.Gems)
	}
	if *updateGolden {
		writeGolden(t, classificationGoldenPath, got)
		return
	}

	raw, err := os.ReadFile(classificationGoldenPath)
	if err != nil {
		t.Fatalf("read golden (regenerate with -update): %v", err)
	}
	var want map[string]json.RawMessage
	if err := json.Unmarshal(raw, &want); err != nil {
		t.Fatalf("parse golden: %v", err)
	}
	if len(want) != len(cases) {
		t.Errorf("golden holds %d cases, corpus %d", len(want), len(cases))
	}
	for _, c := range cases {
		wantCase, ok := want[c.Case]
		if !ok {
			t.Errorf("case %q: missing from golden", c.Case)
			continue
		}
		gotRaw, err := json.Marshal(got[c.Case])
		if err != nil {
			t.Fatalf("case %q: marshal: %v", c.Case, err)
		}
		var wantCompact bytes.Buffer
		if err := json.Compact(&wantCompact, wantCase); err != nil {
			t.Fatalf("case %q: compact golden: %v", c.Case, err)
		}
		if !bytes.Equal(gotRaw, wantCompact.Bytes()) {
			t.Errorf("case %q: classification changed\n got: %s\nwant: %s", c.Case, gotRaw, wantCompact.Bytes())
		}
	}
}

type doubleCorruptCaseGolden struct {
	Results []DoubleCorruptResult `json:"results"`
	// NoResultInputs are the uncorrupted 20/20 rows priced above 0 that
	// produced no result (the input eligibility rule and the input
	// low-confidence gate), sorted.
	NoResultInputs []string `json:"noResultInputs"`
}

type doubleCorruptGolden struct {
	Note  string                             `json:"note"`
	Cases map[string]doubleCorruptCaseGolden `json:"cases"`
}

const doubleCorruptGoldenNote = "Probability-derived floats (outcome probability; result ev, evRaw, profit, " +
	"pricedProbability, unpricedProbability) are compared within 1e-12 relative, every other field exactly: " +
	"doubleCorruptCellDistribution (internal/lab/doublecorrupt.go:299-330) sums float probabilities in Go map " +
	"order, so those floats differ in the last bits between runs, and outcomes are compared as a set keyed by " +
	"(name, variant) because a last-bit difference can reorder two equal probabilities."

func characteriseDoubleCorrupt(gems []GemPrice) doubleCorruptCaseGolden {
	results := AnalyzeDoubleCorrupt(characterisationTime, gems, nil, 0)
	produced := make(map[string]bool, len(results))
	for _, r := range results {
		produced[r.Name] = true
	}
	noResult := []string{}
	for _, g := range gems {
		if !g.IsCorrupted && g.Variant == "20/20" && g.Chaos > 0 && !produced[g.Name] {
			noResult = append(noResult, g.Name)
		}
	}
	sort.Strings(noResult)
	if results == nil {
		results = []DoubleCorruptResult{}
	}
	return doubleCorruptCaseGolden{Results: results, NoResultInputs: noResult}
}

// assertProbabilityClose compares a probability-derived float within the
// tolerance the golden note names: |got−want| ≤ 1e-12 × max(1, |want|).
// A NaN or infinite operand fails: it is within no tolerance of anything.
func assertProbabilityClose(t *testing.T, label string, got, want float64) {
	t.Helper()
	finite := !math.IsNaN(got) && !math.IsInf(got, 0) && !math.IsNaN(want) && !math.IsInf(want, 0)
	if !finite || !(math.Abs(got-want) <= 1e-12*math.Max(1, math.Abs(want))) {
		t.Errorf("%s = %.17g, want %.17g (1e-12 relative)", label, got, want)
	}
}

// AnalyzeDoubleCorrupt over every corpus case matches golden-doublecorrupt.json:
// exact on every field except the probability-derived floats.
//
// Those floats are non-deterministic today: doubleCorruptCellDistribution
// (doublecorrupt.go:299-330) accumulates probabilities over Go map iteration,
// and float addition is not associative, so Probability, EV, EVRaw, Profit,
// PricedProbability and UnpricedProbability differ in the last bits between
// runs. They are compared within 1e-12 relative (assertProbabilityClose), and
// outcomes are matched by (Name, Variant) rather than slice position, since the
// sort at doublecorrupt.go:491 can swap two mathematically equal
// probabilities. The golden's "note" records the same exception.
func TestDoubleCorruptCharacterisation(t *testing.T) {
	cases := loadClassificationCorpus(t)

	got := doubleCorruptGolden{Note: doubleCorruptGoldenNote, Cases: make(map[string]doubleCorruptCaseGolden, len(cases))}
	for _, c := range cases {
		got.Cases[c.Case] = characteriseDoubleCorrupt(c.Gems)
	}
	if *updateGolden {
		writeGolden(t, doubleCorruptGoldenPath, got)
		return
	}

	raw, err := os.ReadFile(doubleCorruptGoldenPath)
	if err != nil {
		t.Fatalf("read golden (regenerate with -update): %v", err)
	}
	var want doubleCorruptGolden
	if err := json.Unmarshal(raw, &want); err != nil {
		t.Fatalf("parse golden: %v", err)
	}
	if want.Note != doubleCorruptGoldenNote {
		t.Errorf("golden note = %q, want %q", want.Note, doubleCorruptGoldenNote)
	}
	if len(want.Cases) != len(cases) {
		t.Errorf("golden holds %d cases, corpus %d", len(want.Cases), len(cases))
	}
	for _, c := range cases {
		w, ok := want.Cases[c.Case]
		if !ok {
			t.Errorf("case %q: missing from golden", c.Case)
			continue
		}
		compareDoubleCorruptCase(t, c.Case, got.Cases[c.Case], w)
	}
}

func compareDoubleCorruptCase(t *testing.T, name string, got, want doubleCorruptCaseGolden) {
	t.Helper()
	if !equalStrings(got.NoResultInputs, want.NoResultInputs) {
		t.Errorf("case %q: inputs without a result = %v, want %v", name, got.NoResultInputs, want.NoResultInputs)
	}
	if len(got.Results) != len(want.Results) {
		t.Errorf("case %q: %d results, want %d", name, len(got.Results), len(want.Results))
		return
	}
	for i := range want.Results {
		g, w := got.Results[i], want.Results[i]
		label := name + " result[" + w.Name + " " + w.InputVariant + "]"
		if !g.Time.Equal(w.Time) || g.Name != w.Name || g.Color != w.Color || g.InputVariant != w.InputVariant ||
			g.InputCost != w.InputCost || g.TempleOverheadChaos != w.TempleOverheadChaos ||
			g.HasVaalVersion != w.HasVaalVersion || g.ThinOutcomeCells != w.ThinOutcomeCells ||
			g.LiquidityRisk != w.LiquidityRisk || g.Model != w.Model {
			t.Errorf("case %q: result %d exact fields\n got: %+v\nwant: %+v", name, i,
				exactResultFields(g), exactResultFields(w))
		}
		assertProbabilityClose(t, label+" EV", g.EV, w.EV)
		assertProbabilityClose(t, label+" EVRaw", g.EVRaw, w.EVRaw)
		assertProbabilityClose(t, label+" Profit", g.Profit, w.Profit)
		assertProbabilityClose(t, label+" PricedProbability", g.PricedProbability, w.PricedProbability)
		assertProbabilityClose(t, label+" UnpricedProbability", g.UnpricedProbability, w.UnpricedProbability)

		type cellKey struct{ name, variant string }
		gotCells := make(map[cellKey]DoubleCorruptOutcome, len(g.Outcomes))
		for _, o := range g.Outcomes {
			gotCells[cellKey{o.Name, o.Variant}] = o
		}
		if len(gotCells) != len(w.Outcomes) {
			t.Errorf("%s: %d outcome cells, want %d", label, len(gotCells), len(w.Outcomes))
		}
		for _, wo := range w.Outcomes {
			cell := label + " outcome[" + wo.Name + " " + wo.Variant + "]"
			goo, ok := gotCells[cellKey{wo.Name, wo.Variant}]
			if !ok {
				t.Errorf("%s: missing", cell)
				continue
			}
			if goo.Chaos != wo.Chaos || goo.Adjusted != wo.Adjusted || goo.Listings != wo.Listings ||
				goo.Priced != wo.Priced || goo.Thin != wo.Thin || goo.LowConfidence != wo.LowConfidence {
				t.Errorf("%s exact fields\n got: chaos=%v adjusted=%v listings=%d priced=%v thin=%v lowConfidence=%v"+
					"\nwant: chaos=%v adjusted=%v listings=%d priced=%v thin=%v lowConfidence=%v", cell,
					goo.Chaos, goo.Adjusted, goo.Listings, goo.Priced, goo.Thin, goo.LowConfidence,
					wo.Chaos, wo.Adjusted, wo.Listings, wo.Priced, wo.Thin, wo.LowConfidence)
			}
			assertProbabilityClose(t, cell+" Probability", goo.Probability, wo.Probability)
		}
	}
}

func exactResultFields(r DoubleCorruptResult) map[string]any {
	return map[string]any{
		"time": r.Time, "name": r.Name, "color": r.Color, "inputVariant": r.InputVariant,
		"inputCost": r.InputCost, "templeOverheadChaos": r.TempleOverheadChaos,
		"hasVaalVersion": r.HasVaalVersion, "thinOutcomeCells": r.ThinOutcomeCells,
		"liquidityRisk": r.LiquidityRisk, "model": r.Model,
	}
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
