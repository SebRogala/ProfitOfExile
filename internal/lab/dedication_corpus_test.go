package lab

import (
	"testing"
)

// corpusGem is a corrupted listing at an explicit Dedication variant.
func corpusGem(name, variant string, chaos float64) GemPrice {
	return GemPrice{
		Name: name, Variant: variant, Chaos: chaos, Listings: 20,
		IsCorrupted: true, GemColor: "BLUE",
	}
}

func rankedNames(results []CollectiveResult) []string {
	out := make([]string, len(results))
	for i, r := range results {
		out[i] = r.TransfiguredName
	}
	return out
}

func priceNames(gems []GemPrice) []string {
	out := make([]string, len(gems))
	for i, g := range gems {
		out[i] = g.Name
	}
	return out
}

func dedicationInput(variant string, inputCost float64) []DedicationResult {
	return []DedicationResult{
		{Variant: variant, Color: "BLUE", GemType: "skill", Mode: "safe", InputCost: inputCost},
	}
}

// --- BuildDedicationCorpus -------------------------------------------------

// The rankings table is served straight out of this, so its order is the order
// the request sees.
func TestBuildDedicationCorpus_RanksEachVariantByPriceDescending(t *testing.T) {
	gems := []GemPrice{
		corpusGem("Arc", "21/20c", 40),
		corpusGem("Spark", "21/20c", 900),
		corpusGem("Ball Lightning", "21/20c", 120),
	}

	corpus := BuildDedicationCorpus(gems, DedicationAnalysis{Skills: dedicationInput("21/20c", 10)})

	got := rankedNames(corpus.Rankings["21/20c"])
	want := []string{"Spark", "Ball Lightning", "Arc"}
	if len(got) != len(want) {
		t.Fatalf("ranking = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("ranking = %v, want %v", got, want)
		}
	}
}

// A variant's ROI is measured against that variant's input cost. Handing the
// ranking the other variant's analysis would price a gem against a pool nobody
// bought it from.
func TestBuildDedicationCorpus_PricesEachVariantAgainstItsOwnInputCost(t *testing.T) {
	gems := []GemPrice{
		corpusGem("Arc", "21/23c", 500),
		corpusGem("Spark", "21/20c", 40),
	}
	analysis := DedicationAnalysis{Skills: []DedicationResult{
		{Variant: "21/23c", Color: "BLUE", GemType: "skill", Mode: "safe", InputCost: 100},
		{Variant: "21/20c", Color: "BLUE", GemType: "skill", Mode: "safe", InputCost: 10},
	}}

	corpus := BuildDedicationCorpus(gems, analysis)

	if got := corpus.Rankings["21/23c"]; len(got) != 1 || got[0].ROI != 400 {
		t.Errorf("21/23c ranking = %+v, want one row with ROI 400 (500c against a 100c input)", got)
	}
	if got := corpus.Rankings["21/20c"]; len(got) != 1 || got[0].ROI != 30 {
		t.Errorf("21/20c ranking = %+v, want one row with ROI 30 (40c against a 10c input)", got)
	}
}

// A variant the snapshot holds nothing for still gets a key. An absent key and
// an empty one are the same answer, and the caller reads warmth from the cache
// rather than from whether the key exists.
func TestBuildDedicationCorpus_EmitsAKeyForAVariantWithNoGems(t *testing.T) {
	corpus := BuildDedicationCorpus([]GemPrice{corpusGem("Arc", "21/23c", 500)}, DedicationAnalysis{})

	for _, variant := range DedicationVariants {
		if _, ok := corpus.Rankings[variant]; !ok {
			t.Errorf("no rankings key for variant %q", variant)
		}
		if _, ok := corpus.GemPrices[variant]; !ok {
			t.Errorf("no prices key for variant %q", variant)
		}
	}
}

// The compare path answers for names the font can never hand out and marks them
// as not an outcome. Narrowing the price half to rankable gems would turn those
// rows into "no price found" instead.
func TestBuildDedicationCorpus_PricesCarryAGemTheFontCannotProduce(t *testing.T) {
	gems := []GemPrice{
		corpusGem("Vaal Arc", "21/20c", 60),
		corpusGem("Arc", "21/20c", 40),
	}

	corpus := BuildDedicationCorpus(gems, DedicationAnalysis{Skills: dedicationInput("21/20c", 10)})

	if got, want := len(corpus.GemPrices["21/20c"]), 2; got != want {
		t.Errorf("prices = %v, want both gems including the Vaal one", priceNames(corpus.GemPrices["21/20c"]))
	}
	if got := rankedNames(corpus.Rankings["21/20c"]); len(got) != 1 || got[0] != "Arc" {
		t.Errorf("ranking = %v, want only Arc — a Vaal gem is never a font outcome", got)
	}
}

// The price half stands in for `is_corrupted = true AND variant = $3`, so an
// uncorrupted listing of the same name must not enter it.
func TestBuildDedicationCorpus_PricesExcludeUncorruptedListings(t *testing.T) {
	gems := []GemPrice{
		corpusGem("Arc", "21/20c", 40),
		{Name: "Arc", Variant: "21/20c", Chaos: 5, Listings: 90, GemColor: "BLUE"},
	}

	corpus := BuildDedicationCorpus(gems, DedicationAnalysis{Skills: dedicationInput("21/20c", 10)})

	prices := corpus.GemPrices["21/20c"]
	if len(prices) != 1 {
		t.Fatalf("prices = %+v, want only the corrupted listing", prices)
	}
	if prices[0].Chaos != 40 {
		t.Errorf("price = %.0f, want 40 — the uncorrupted listing leaked in", prices[0].Chaos)
	}
}

// --- FilterDedicationRankings ----------------------------------------------

// Search then limit, in that order: a search must return its top matches, not
// the matches that happen to fall inside the top N.
func TestFilterDedicationRankings_SearchesTheWholeListBeforeApplyingTheLimit(t *testing.T) {
	ranked := []CollectiveResult{
		{TransfiguredName: "Spark"},
		{TransfiguredName: "Ball Lightning"},
		{TransfiguredName: "Arc"},
	}

	got := FilterDedicationRankings(ranked, "arc", 2)

	if len(got) != 1 || got[0].TransfiguredName != "Arc" {
		t.Errorf("filtered = %v, want [Arc] — the limit must not truncate before the search runs",
			rankedNames(got))
	}
}

func TestFilterDedicationRankings_LimitKeepsTheHighestRanked(t *testing.T) {
	ranked := []CollectiveResult{
		{TransfiguredName: "Spark"},
		{TransfiguredName: "Arc"},
	}

	got := FilterDedicationRankings(ranked, "", 1)

	if len(got) != 1 || got[0].TransfiguredName != "Spark" {
		t.Errorf("filtered = %v, want [Spark] — the list is already ranked", rankedNames(got))
	}
}

// The cached ranking is read by every request; a filter that wrote into it would
// corrupt the corpus for the next caller.
func TestFilterDedicationRankings_DoesNotModifyTheRankingItFilters(t *testing.T) {
	ranked := []CollectiveResult{
		{TransfiguredName: "Spark"},
		{TransfiguredName: "Arc"},
	}

	got := FilterDedicationRankings(ranked, "arc", 0)
	got[0].TransfiguredName = "mutated"

	if ranked[0].TransfiguredName != "Spark" || ranked[1].TransfiguredName != "Arc" {
		t.Errorf("ranking = %v, want [Spark Arc] — the filter wrote through to the cached list",
			rankedNames(ranked))
	}
}

// A limit of zero is how the tick asks for the whole ranking.
func TestFilterDedicationRankings_ZeroLimitKeepsEverything(t *testing.T) {
	ranked := []CollectiveResult{{TransfiguredName: "Spark"}, {TransfiguredName: "Arc"}}

	if got := FilterDedicationRankings(ranked, "", 0); len(got) != 2 {
		t.Errorf("filtered = %v, want both rows", rankedNames(got))
	}
}

// --- SelectGemPricesByNames ------------------------------------------------

func TestSelectGemPricesByNames_KeepsOnlyTheRequestedNames(t *testing.T) {
	gems := []GemPrice{
		corpusGem("Arc", "21/20c", 40),
		corpusGem("Spark", "21/20c", 900),
		corpusGem("Ball Lightning", "21/20c", 120),
	}

	got := SelectGemPricesByNames(gems, []string{"Spark", "Arc"})

	if len(got) != 2 {
		t.Fatalf("selected = %v, want Arc and Spark", priceNames(got))
	}
	for _, g := range got {
		if g.Name == "Ball Lightning" {
			t.Errorf("selected = %v, want no Ball Lightning", priceNames(got))
		}
	}
}

func TestSelectGemPricesByNames_UnknownNameSelectsNothing(t *testing.T) {
	gems := []GemPrice{corpusGem("Arc", "21/20c", 40)}

	if got := SelectGemPricesByNames(gems, []string{"Nonexistent"}); len(got) != 0 {
		t.Errorf("selected = %v, want nothing", priceNames(got))
	}
}

// The cache answers "am I warm"; the rows do not. A tick that produced no
// Dedication results at all still leaves an authoritative corpus, and a reader
// deriving warmth from the analysis it just read would fall back forever.
func TestCache_Dedication_EmptyCorpusStillReportsWarm(t *testing.T) {
	cache := NewCache(gemDictScope)
	cache.For(gemDictScope).SetDedication(BuildDedicationCorpus(nil, DedicationAnalysis{}))

	analysis, ok := cache.For(gemDictScope).Dedication()
	if !ok {
		t.Fatal("ok = false after a stored corpus; the reader would take the database fallback forever")
	}
	if len(analysis.Skills) != 0 || len(analysis.Transfigured) != 0 {
		t.Errorf("analysis = %+v, want empty", analysis)
	}
}

// Nothing stored means only the database can answer.
func TestCache_Dedication_ColdCacheReportsNotOK(t *testing.T) {
	if _, ok := NewCache(gemDictScope).For(gemDictScope).Dedication(); ok {
		t.Error("ok = true before any Dedication tick; the fallback would be skipped")
	}
}
