package lab

import (
	"reflect"
	"testing"

	"profitofexile/internal/league"
)

// The cache-state contract for the double-corruption corpus (POE-125): warmth is
// the tick's own answer, never inferred from how many rows it produced. These
// tests are the reader/writer halves of that rule, plus the corpus partition the
// keyed read is served from.

func dcResult(name, inputVariant string, profit float64) DoubleCorruptResult {
	return DoubleCorruptResult{
		Name: name, InputVariant: inputVariant, Profit: profit,
		Model: DoubleCorruptModelEstimated,
	}
}

func TestBuildDoubleCorruptCorpus_KeysEveryAnalyzedInputVariant(t *testing.T) {
	// A keyed read must be able to tell "this snapshot priced no gems here" from
	// "not computed". That only works if every analyzed variant is addressable,
	// including one no result landed on.
	corpus := BuildDoubleCorruptCorpus(nil)

	for _, v := range DoubleCorruptVariants {
		if _, addressable := corpus.ByVariant[v]; !addressable {
			t.Errorf("input variant %q is not a key of an empty corpus — a keyed read cannot distinguish no gems from not computed", v)
		}
	}
}

func TestBuildDoubleCorruptCorpus_PartitionsResultsByInputVariant(t *testing.T) {
	results := []DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
		dcResult("Spark of the Nova", "20/20", 100),
		dcResult("Arc of Surging", "1/20", 9000),
	}

	corpus := BuildDoubleCorruptCorpus(results)

	if got := len(corpus.ByVariant["20/20"]); got != 2 {
		t.Errorf("20/20 partition holds %d results, want 2", got)
	}
	if got := corpus.ByVariant["1/20"]; len(got) != 1 || got[0].Profit != 9000 {
		t.Errorf("1/20 partition = %+v, want the one 1/20 result — input variants are separate markets", got)
	}
	if !reflect.DeepEqual(corpus.Results, results) {
		t.Errorf("Results = %+v, want the whole unpartitioned set", corpus.Results)
	}
}

func TestCache_DoubleCorrupt_ColdCacheReportsNotOK(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)

	results, ok := c.For(scope).DoubleCorrupt()

	if ok {
		t.Fatal("DoubleCorrupt on a cold cache: ok = true, want false (the database fallback must stay reachable before the first tick)")
	}
	if len(results) != 0 {
		t.Errorf("results = %+v, want none", results)
	}
}

func TestCache_DoubleCorrupt_WarmButEmptyCorpusReportsOK(t *testing.T) {
	// The writer half of the contract: a tick that priced nothing still ran, and
	// storing that answer is what stops every request re-querying for it.
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	c.For(scope).SetDoubleCorrupt(BuildDoubleCorruptCorpus(nil))

	results, ok := c.For(scope).DoubleCorrupt()

	if !ok {
		t.Fatal("DoubleCorrupt after a tick that priced nothing: ok = false, want true (a warm cache answers, it does not defer to the database)")
	}
	if len(results) != 0 {
		t.Errorf("results = %+v, want none", results)
	}
	if !c.For(scope).HasDoubleCorrupt() {
		t.Error("HasDoubleCorrupt = false after an empty store — the keyed read's warmth predicate must agree with DoubleCorrupt's ok")
	}
}

func TestCache_DoubleCorrupt_RoundTripsTheStoredCorpus(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	results := []DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
		dcResult("Spark of the Nova", "20/20", 100),
	}
	c.For(scope).SetDoubleCorrupt(BuildDoubleCorruptCorpus(results))

	got, ok := c.For(scope).DoubleCorrupt()

	if !ok || !reflect.DeepEqual(got, results) {
		t.Errorf("DoubleCorrupt = (%+v, %v), want the stored corpus and true", got, ok)
	}
}

func TestCache_DoubleCorruptByVariant_ServesOnlyTheRequestedInputVariant(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	c.For(scope).SetDoubleCorrupt(BuildDoubleCorruptCorpus([]DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
		dcResult("Arc of Surging", "1/20", 9000),
	}))

	got := c.For(scope).DoubleCorruptByVariant("20/20")

	if len(got) != 1 || got[0].InputVariant != "20/20" {
		t.Errorf("DoubleCorruptByVariant(\"20/20\") = %+v, want only the 20/20 result", got)
	}
}

func TestCache_DoubleCorrupt_RejectsForeignLeague(t *testing.T) {
	scope := league.Historical("LeagueA")
	c := NewCache(scope)
	c.For(scope).SetDoubleCorrupt(BuildDoubleCorruptCorpus([]DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
	}))

	requirePanic(t, func() { c.For(league.Historical("LeagueB")).DoubleCorrupt() })
}

func TestSelectDoubleCorruptByNames_KeepsOnlyTheNamedGems(t *testing.T) {
	results := []DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
		dcResult("Spark of the Nova", "20/20", 100),
		dcResult("Rolling Magma of Nothing", "20/20", 20),
	}

	got := SelectDoubleCorruptByNames(results, []string{"Arc of Surging", "Rolling Magma of Nothing"})

	if len(got) != 2 {
		t.Fatalf("got %d entries %+v, want 2", len(got), got)
	}
	if got["Arc of Surging"].Profit != 500 || got["Rolling Magma of Nothing"].Profit != 20 {
		t.Errorf("selected the wrong rows: %+v", got)
	}
	if _, unwanted := got["Spark of the Nova"]; unwanted {
		t.Error("an unrequested gem is in the selection")
	}
}

func TestSelectDoubleCorruptByNames_NoMatchYieldsAnEmptySelection(t *testing.T) {
	got := SelectDoubleCorruptByNames([]DoubleCorruptResult{
		dcResult("Arc of Surging", "20/20", 500),
	}, []string{"Ice Nova of Frostbolts"})

	if len(got) != 0 {
		t.Errorf("got %+v, want no entries for a name the corpus does not carry", got)
	}
}
