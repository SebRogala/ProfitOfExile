package lab

import (
	"reflect"
	"testing"
	"time"
)

// The four leakage cases POE-142 names, asserted against the eligibility SSOT
// and against the two surfaces that were measured leaking.

func TestIsFontOutcome_ExcludesHeistOnlyGem(t *testing.T) {
	g := GemPrice{Name: "Wave of Conviction of Trarthus", Variant: "20/20", Chaos: 400, IsTransfigured: true, GemColor: "BLUE"}

	if isFontOutcome(g) {
		t.Error("a Trarthus gem drops only from Heist blueprints — no Font can hand one out")
	}
}

func TestIsFontOutcome_ExcludesWhiteGem(t *testing.T) {
	// A white gem has no attribute requirement, and the Font hands out a gem of
	// the same colour. The measured instances (Pact of Beidat and friends) are
	// not transfigured, so this pins the rule rather than an open row.
	g := GemPrice{Name: "Whitewash of Nothing", Variant: "20/20", Chaos: 400, IsTransfigured: true, GemColor: "WHITE"}

	if isFontOutcome(g) {
		t.Error("a white gem has no colour to be rerolled within — the Font cannot hand one out")
	}
}

func TestIsFontOutcome_ExcludesSupportGem(t *testing.T) {
	g := GemPrice{Name: "Awakened Empower Support", Variant: "20/20", Chaos: 400, IsTransfigured: true, GemColor: "RED"}

	if isFontOutcome(g) {
		t.Error("both lab gem crafts hand out skill gems only")
	}
}

// The support rule is a SUFFIX test, and only the non-terminal rows below hold
// it there. `strings.Contains(name, "Support")` — the form this replaced, and
// the form the Dedication picker's `LIKE '%Support%'` still carried at POE-142 —
// agrees on every real support gem, because every real support gem's name ends
// in " Support". The two forms part company only on a name that carries the word
// somewhere else, and a substring rule silently drops those: a skill gem whose
// name merely contains "Support" is not a support gem, and excluding it would
// take a sellable outcome out of the Font and Dedication pools without saying
// so.
//
// The equivalent SQL fragment is pinned against this predicate by
// TestSQLExclusionsMatchGoPredicates — but that test needs a database, so
// nothing in `make test` holds the Go side alone. This does.
func TestIsSupportGemName_MatchesTheTerminalWordOnly(t *testing.T) {
	tests := []struct {
		name string
		want bool
		why  string
	}{
		{"Empower Support", true, "the ordinary shape: every support gem's name ends in the word"},
		{"Awakened Added Fire Damage Support", true, "the awakened variants keep the suffix"},
		{"Cyclone", false, "an active skill gem carries the word nowhere"},
		{"Supportive", false, "the word is a prefix of a longer word, not the terminal word"},
		{"Support Cyclone", false, "the word leads the name; the gem it names is still a skill gem"},
		{"Cyclone Supported", false, "the terminal word merely starts with Support"},
		{"Support", false, "no separating space, so the name is the word rather than a name ending in it"},
		{"", false, "an empty name has no suffix to match"},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			if got := isSupportGemName(tc.name); got != tc.want {
				t.Errorf("isSupportGemName(%q) = %v, want %v — %s", tc.name, got, tc.want, tc.why)
			}
		})
	}
}

func TestIsFontOutcome_IncludesUncolouredTransfiguredGem(t *testing.T) {
	// The counterpart to TestIsFontOutcome_ExcludesWhiteGem: an unresolved colour
	// is the gemcolor resolver not having met the name, not a game rule. Dropping
	// it here would blind the analysis to exactly the new gems at a league start.
	g := GemPrice{Name: "Spark of Unpredictability", Variant: "20/20", Chaos: 400, IsTransfigured: true, GemColor: ""}

	if !isFontOutcome(g) {
		t.Error("an unresolved colour is a data gap, not a reason the Font cannot produce the gem")
	}
}

func TestIsDedicationOutcome_ExcludesWhitePactGem(t *testing.T) {
	// Observed 2026-08-05 on the local Allflame snapshot: Pact of Beidat is
	// corrupted, priced at every Dedication variant, and resolves to WHITE. At
	// 21/23c it was listed at 34,220c — enough to set the TOP-tier boundary for
	// every colour if it reaches classification.
	g := GemPrice{Name: "Pact of Beidat", Variant: "21/23c", Chaos: 34220, Listings: 1, IsCorrupted: true, GemColor: "WHITE"}

	if isDedicationOutcome(g) {
		t.Error("a Pact gem is a white Exceptional gem — the Dedication cannot hand one out")
	}
}

func TestIsDedicationOutcome_IncludesCorruptedColouredSkillGem(t *testing.T) {
	g := GemPrice{Name: "Cyclone", Variant: "21/23c", Chaos: 300, Listings: 12, IsCorrupted: true, GemColor: "GREEN"}

	if !isDedicationOutcome(g) {
		t.Error("a corrupted, coloured, non-Vaal skill gem is exactly what the Dedication produces")
	}
}

// Tier classification is the surface POE-142 was opened over: the pool loop had
// the colour rule and classification did not, so a white listing entered the
// tiers and took TOP away from the gems the craft can actually produce.
func TestComputeDedicationClassification_ExcludesWhiteGems(t *testing.T) {
	gems := []GemPrice{
		makeDedicationGem("Cyclone", "GREEN", 300, 12, false),
		makeDedicationGem("Fireball", "BLUE", 250, 20, false),
		makeDedicationGem("Cleave", "RED", 200, 15, false),
		makeDedicationGem("Arc", "BLUE", 180, 18, false),
		makeDedicationGem("Pact of Beidat", "WHITE", 34220, 20, false),
	}

	got := ComputeDedicationClassification(gems, false, "21/23c")

	want := map[GemClassificationKey]bool{
		{"Cyclone", "21/23c"}: true, {"Fireball", "21/23c"}: true,
		{"Cleave", "21/23c"}: true, {"Arc", "21/23c"}: true,
	}
	for key := range got.Gems {
		if !want[key] {
			t.Errorf("%v was classified — the Dedication cannot hand out a white gem, "+
				"and at 34,220c this one takes TOP from the gems it can", key)
		}
	}
	if len(got.Gems) != len(want) {
		t.Errorf("classified %d gems, want %d", len(got.Gems), len(want))
	}
}

// makeFontGem creates an uncorrupted transfigured 20/20 GemPrice for Font pool
// tests. Listings are uniform across a pool so that detectLowConfidence — which
// flags anything under 40% of the variant median — leaves every gem in and the
// tier assertion reads on the colour rule alone.
func makeFontGem(name, color string, chaos float64) GemPrice {
	return GemPrice{
		Name:           name,
		Variant:        "20/20",
		Chaos:          chaos,
		Listings:       50,
		IsTransfigured: true,
		GemColor:       color,
	}
}

// The Font mirror of TestComputeDedicationClassification_ExcludesWhiteGems.
// isFontOutcome gained the same WHITE rule in POE-142 and feeds six surfaces
// (tier classification, gem features, market-context percentiles, the temporal
// depth gate, the low-confidence pass and TOP detection), but only the predicate
// itself was pinned — the surface that actually carried the measured bug was
// asserted on the Dedication side and nowhere on the Font side.
//
// The white gem here is priced to move a boundary rather than merely to sit
// outside one. Admitted, it is the only gem past TOP detection's gap test, and
// the 1300c/1200c pair that should hold TOP is demoted to HIGH — the exact
// shape of the 34,220c listing that took TOP from every Dedication colour. So
// the tier assertion is the load-bearing half: absence alone would still pass
// against a pipeline that let the gem set the boundary and then dropped it from
// the output map.
func TestComputeGemClassification_ExcludesWhiteGems(t *testing.T) {
	gems := []GemPrice{
		makeFontGem("Spark of Unpredictability", "BLUE", 1300),
		makeFontGem("Arc of Oscillating", "BLUE", 1200),
		makeFontGem("Cleave of Rage", "RED", 400),
		makeFontGem("Slam of Magnitude", "RED", 350),
		makeFontGem("Ice Shot of Shattering", "GREEN", 300),
		makeFontGem("Fireball of Volatility", "RED", 100),
		makeFontGem("Bash of Impact", "RED", 80),
		makeFontGem("Strike of Swiping", "GREEN", 50),
		makeFontGem("Pact of Beidat", "WHITE", 34220),
	}

	got := ComputeGemClassification(gems)

	if _, classified := got.Gems[GemClassificationKey{"Pact of Beidat", "20/20"}]; classified {
		t.Error("a white gem has no colour to be rerolled within — the Font cannot hand one out, " +
			"so it must not reach classification")
	}
	if len(got.Gems) != 8 {
		t.Errorf("classified %d gems, want the 8 the Font can produce; got %v", len(got.Gems), got.Gems)
	}

	for _, name := range []string{"Spark of Unpredictability", "Arc of Oscillating"} {
		if tier := got.Gems[GemClassificationKey{name, "20/20"}].Tier; tier != "TOP" {
			t.Errorf("%s is %s, want TOP — at 34,220c the white gem takes the TOP boundary "+
				"away from the two gems the Font can actually hand out", name, tier)
		}
	}
}

// The verified OCR-dictionary leak. gem_colors carries 12 seeded "of Trarthus"
// names whose base gem is also seeded, so every one of them used to land in the
// transfigured half the desktop matcher scores against — a name nothing can
// price, which POE-145 showed does not stay harmless in a fuzzy matcher.
func TestFilterGemDictionary_ExcludesHeistOnlyGems(t *testing.T) {
	all := []string{
		"Wave of Conviction",
		"Wave of Conviction of Trarthus",
		"Wave of Conviction of Judgement",
	}

	got := FilterGemDictionary(all, true)

	if len(got) != 1 || got[0] != "Wave of Conviction of Judgement" {
		t.Errorf("transfigured dictionary = %v, want only [Wave of Conviction of Judgement] — "+
			"a Trarthus gem drops from Heist and the collector never stores one", got)
	}
}

func TestGemDictionaryNames_ExcludesHeistOnlyGemsFromBothHalves(t *testing.T) {
	gems := []GemPrice{
		{Name: "Wave of Conviction", IsTransfigured: false},
		{Name: "Bloodlust Support", IsTransfigured: false},
		{Name: "Wave of Conviction of Judgement", IsTransfigured: true},
		{Name: "Wave of Conviction of Trarthus", IsTransfigured: true},
		{Name: "Trarthus Ire", IsTransfigured: false},
	}

	skills, transfigured := gemDictionaryNames(gems)

	if len(skills) != 1 || skills[0] != "Wave of Conviction" {
		t.Errorf("skills = %v, want only [Wave of Conviction]", skills)
	}
	if len(transfigured) != 1 || transfigured[0] != "Wave of Conviction of Judgement" {
		t.Errorf("transfigured = %v, want only [Wave of Conviction of Judgement]", transfigured)
	}
}

// The per-surface difference the SSOT exists to keep explicit. The quality font
// enhances the gem you feed it rather than handing you a random one, so a
// support gem is a legitimate subject here even though no lab craft produces
// one. Folding isLabOutcomeName into this surface would break this test.
func TestAnalyzeQuality_IncludesSupportGems(t *testing.T) {
	gems := []GemPrice{
		{Name: "Awakened Empower Support", Variant: "1", Chaos: 100, Listings: 19, GemColor: "RED"},
		{Name: "Awakened Empower Support", Variant: "1/20", Chaos: 500, Listings: 8, GemColor: "RED"},
	}

	results := AnalyzeQuality(time.Now(), gems, 1)

	if len(results) != 1 {
		t.Fatalf("got %d quality results, want 1 — the quality roll enhances whatever gem you feed it", len(results))
	}
	if results[0].Name != "Awakened Empower Support" || results[0].ROI20 != 400 {
		t.Errorf("result = %q ROI20 %.0f, want Awakened Empower Support at 400", results[0].Name, results[0].ROI20)
	}
}

func TestAnalyzeQuality_ExcludesHeistOnlyGems(t *testing.T) {
	gems := []GemPrice{
		{Name: "Sunder of Trarthus", Variant: "1", Chaos: 100, Listings: 19, GemColor: "RED"},
		{Name: "Sunder of Trarthus", Variant: "1/20", Chaos: 500, Listings: 8, GemColor: "RED"},
	}

	if results := AnalyzeQuality(time.Now(), gems, 1); len(results) != 0 {
		t.Errorf("got %d quality results, want 0 — a Heist-only gem is not obtainable in standard play", len(results))
	}
}

// --- the unresolved-colour counter ----------------------------------------
//
// hasAttributeColor carries one exclusion that is a data gap rather than a game
// rule, and until this counter existed the gap was undetectable: the gem left
// the Dedication pool silently, EV and pWin and input cost were computed over
// the shorter pool, and the autocomplete returned a shorter list at HTTP 200.

func TestUnresolvedDedicationColorNames_CountsCorruptedGemDroppedOnlyForTheColourGap(t *testing.T) {
	// Everything isDedicationFeed asks for except a colour the resolver has
	// placed: this gem belongs in the pool and is not in it.
	gems := []GemPrice{
		{Name: "Mana-Infused Staff", Variant: "21/23c", Chaos: 90, Listings: 4, IsCorrupted: true, GemColor: ""},
	}

	got := unresolvedDedicationColorNames(gems)

	if len(got) != 1 || got[0] != "Mana-Infused Staff" {
		t.Errorf("unresolvedDedicationColorNames = %v, want [Mana-Infused Staff] — a corrupted "+
			"skill gem the resolver has not met is a data gap, and the pool is short by it", got)
	}
}

func TestUnresolvedDedicationColorNames_IgnoresGemsWithAResolvedColour(t *testing.T) {
	// The healthy state, and the one measured on the latest local Allflame
	// snapshot: every name the market carries resolves, so the count is zero and
	// the tick prints no warning.
	gems := []GemPrice{
		{Name: "Cyclone", Variant: "21/23c", Chaos: 300, Listings: 12, IsCorrupted: true, GemColor: "GREEN"},
		{Name: "Arc", Variant: "21/20c", Chaos: 180, Listings: 18, IsCorrupted: true, GemColor: "BLUE"},
	}

	if got := unresolvedDedicationColorNames(gems); got != nil {
		t.Errorf("unresolvedDedicationColorNames = %v, want nil — every gem here carries a colour, "+
			"so nothing was dropped for the gap", got)
	}
}

// WHITE is the discrimination the whole counter rests on. A white gem is dropped
// by isColorlessByRule, a game rule that holds with the resolver fully warm, so
// counting it would report a permanent non-zero floor and teach the reader that
// the number means nothing.
func TestUnresolvedDedicationColorNames_ExcludesWhiteGem(t *testing.T) {
	gems := []GemPrice{
		{Name: "Pact of Beidat", Variant: "21/23c", Chaos: 34220, Listings: 1, IsCorrupted: true, GemColor: "WHITE"},
	}

	if got := unresolvedDedicationColorNames(gems); got != nil {
		t.Errorf("unresolvedDedicationColorNames = %v, want nil — a white gem is excluded by a "+
			"game rule, not by a gap in gem_colors", got)
	}
}

// A gem another feed rule already rejects is not an instance of the colour gap:
// seeding its colour would not put it back in the pool, so counting it would send
// an operator to fix something that is working.
func TestUnresolvedDedicationColorNames_ExcludesGemsAnotherFeedRuleRejects(t *testing.T) {
	tests := []struct {
		scenario string
		gem      GemPrice
		why      string
	}{
		{
			"support gem",
			GemPrice{Name: "Communion Support", Variant: "1/23c", Chaos: 1711, Listings: 1, IsCorrupted: true, GemColor: ""},
			"measured 2026-08-05, the two unresolved names in the latest Allflame snapshot are " +
				"both support gems — isLabOutcomeName rejects them whatever their colour",
		},
		{
			"Heist-only gem",
			GemPrice{Name: "Sunder of Trarthus", Variant: "21/23c", Chaos: 400, Listings: 2, IsCorrupted: true, GemColor: ""},
			"a Trarthus gem drops only from Heist blueprints; the lab can never be fed one",
		},
		{
			"uncorrupted gem",
			GemPrice{Name: "Spark of Nova", Variant: "20/20", Chaos: 400, Listings: 9, IsCorrupted: false, GemColor: ""},
			"the Dedication takes corrupted gems only, so an uncorrupted one is out on its own rule",
		},
		{
			"Vaal gem at 21/23c",
			GemPrice{Name: "Vaal Arc", Variant: "21/23c", Chaos: 900, Listings: 1, IsCorrupted: true, GemColor: ""},
			"21/23c would take three corruption outcomes and the Temple grants two — the listing cannot exist",
		},
	}

	for _, tc := range tests {
		t.Run(tc.scenario, func(t *testing.T) {
			if got := unresolvedDedicationColorNames([]GemPrice{tc.gem}); got != nil {
				t.Errorf("unresolvedDedicationColorNames(%q) = %v, want nil — %s", tc.gem.Name, got, tc.why)
			}
		})
	}
}

// A gem is priced at up to nine Dedication variants. Counting rows would report
// the variant fan-out rather than how many gems are missing, and the number an
// operator acts on is names.
func TestUnresolvedDedicationColorNames_ReturnsEachNameOnceAcrossVariants(t *testing.T) {
	gems := []GemPrice{
		{Name: "Dark Bargain", Variant: "21/23c", Chaos: 500, Listings: 2, IsCorrupted: true, GemColor: ""},
		{Name: "Dark Bargain", Variant: "21/20c", Chaos: 300, Listings: 5, IsCorrupted: true, GemColor: ""},
		{Name: "Dark Bargain", Variant: "20/23c", Chaos: 120, Listings: 9, IsCorrupted: true, GemColor: ""},
	}

	got := unresolvedDedicationColorNames(gems)

	if len(got) != 1 {
		t.Errorf("unresolvedDedicationColorNames = %v, want one entry — three variants of one gem "+
			"are one missing name, not three", got)
	}
}

// The caller logs this slice and the reader compares it between ticks, so map
// iteration order would churn the line while nothing changed.
func TestUnresolvedDedicationColorNames_ReturnsNamesSorted(t *testing.T) {
	gems := []GemPrice{
		{Name: "Mana-Infused Staff", Variant: "21/23c", Chaos: 90, Listings: 4, IsCorrupted: true, GemColor: ""},
		{Name: "Dark Bargain", Variant: "21/23c", Chaos: 500, Listings: 2, IsCorrupted: true, GemColor: ""},
		{Name: "Absolution", Variant: "21/23c", Chaos: 60, Listings: 7, IsCorrupted: true, GemColor: ""},
	}

	got := unresolvedDedicationColorNames(gems)

	want := []string{"Absolution", "Dark Bargain", "Mana-Infused Staff"}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("unresolvedDedicationColorNames = %v, want %v", got, want)
	}
}
