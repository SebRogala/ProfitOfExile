package handlers

import (
	"encoding/json"
	"net/http/httptest"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// WARM-AND-EMPTY on the whole-corpus reads.
//
// Every handler here used to judge the cache by the length of the corpus it read
// — len(cached) > 0 — which reads a tick that legitimately produced no rows as a
// cold cache and re-runs its queries on every request for the life of the
// process. The corpora now report their own warmth, and these cases pin that: a
// stored-but-empty corpus answers with no query, a corpus nothing has stored
// still falls back.
//
// The seam is this package's usual one, a nil *lab.Repository: every query method
// dereferences r.pool, so a surviving query panics. Asserting on the response
// body cannot see the defect — the body is byte-identical whether the empty
// answer came from the cache or from the database — which is exactly why these
// shipped in the first place.

var corpusScope = league.Historical("Mirage")

// warmEmptyCorpora marks every whole-corpus analysis field warm with nothing in
// it: the state after a tick that ran against a snapshot holding no gem it could
// price, classify or rank.
func warmEmptyCorpora(t *testing.T) *lab.Cache {
	t.Helper()
	c := lab.NewCache(corpusScope)
	x := c.For(corpusScope)
	x.SetTransfigure(nil)
	x.SetFont(lab.FontAnalysis{})
	x.SetQuality(nil)
	x.SetGemFeatures(nil)
	x.SetGemSignals(nil)
	return c
}

// warmUnrelatedSparklines marks the sparkline corpus warm with a series for a
// gem none of these cases ask about.
//
// The sparkline map is a separate corpus with its own predicate, and the
// collective and compare paths read it independently of the ranking corpora.
// Leaving it cold makes every case here reach the database through the sparkline
// query whatever the ranking corpora report — which is a test that passes for the
// wrong reason in both directions.
func warmUnrelatedSparklines(t *testing.T, c *lab.Cache) *lab.Cache {
	t.Helper()
	c.For(corpusScope).SetSparklinesByName(map[string]map[string][]lab.SparklinePoint{
		"Some Other Gem": {"20/20": {{
			Time:     time.Now().UTC().Format(time.RFC3339),
			Price:    10,
			Listings: 5,
		}}},
	}, nil, time.Now())
	return c
}

func warmEmptyCorporaWithSparklines(t *testing.T) *lab.Cache {
	t.Helper()
	return warmUnrelatedSparklines(t, warmEmptyCorpora(t))
}

// --- transfigure -----------------------------------------------------------

func TestTransfigureAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, TransfigureAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/transfigure?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

// --- font ------------------------------------------------------------------

type fontModes struct {
	Safe    []json.RawMessage `json:"safe"`
	Premium []json.RawMessage `json:"premium"`
	Jackpot []json.RawMessage `json:"jackpot"`
}

func decodeFontModes(t *testing.T, w *httptest.ResponseRecorder) fontModes {
	t.Helper()
	if w.Code != 200 {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	var resp fontModes
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return resp
}

// One AnalyzeFont pass fills all three modes, so three empty modes is an answer:
// no colour cleared the tier threshold at this snapshot. Reading it as cold cost
// three LatestFontResults queries per request.
func TestFontAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, FontAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/font?variant=20/20")

	modes := decodeFontModes(t, w)
	if len(modes.Safe) != 0 || len(modes.Premium) != 0 || len(modes.Jackpot) != 0 {
		t.Errorf("modes = %+v, want all three empty", modes)
	}
}

func TestFontAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(corpusScope)

	if !queriedRepository(FontAnalysis(nil, cold, corpusScope),
		"/api/analysis/font?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// The three modes share one flag because one call stores them. A cache warmed
// with winners in Safe alone must not send the other two modes to the database.
func TestFontAnalysis_WarmCorpusWithOneEmptyModeAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(corpusScope)
	cache.For(corpusScope).SetFont(lab.FontAnalysis{
		Safe: []lab.FontResult{{Time: time.Now(), Color: "BLUE", Variant: "20/20", EV: 12, Profit: 5}},
	})

	w := serveWithoutRepository(t, FontAnalysis(nil, cache, corpusScope),
		"/api/analysis/font?variant=20/20")

	modes := decodeFontModes(t, w)
	if len(modes.Safe) != 1 {
		t.Errorf("safe = %+v, want the one stored row", modes.Safe)
	}
	if len(modes.Jackpot) != 0 {
		t.Errorf("jackpot = %+v, want none", modes.Jackpot)
	}
}

// --- quality ---------------------------------------------------------------

func TestQualityAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, QualityAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/quality?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

// --- trends ----------------------------------------------------------------

func TestTrendAnalysis_WarmButEmptyCorporaAnswerWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, TrendAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/trends?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestTrendAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(corpusScope)

	if !queriedRepository(TrendAnalysis(nil, cold, corpusScope),
		"/api/analysis/trends?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// RunV2 stores features and signals in two calls with a persist between them
// that can fail the run, so they report warmth separately. A run that stored
// features and never reached the signals must still fall back.
func TestTrendAnalysis_WarmFeaturesWithColdSignalsFallsBackToTheRepository(t *testing.T) {
	half := lab.NewCache(corpusScope)
	half.For(corpusScope).SetGemFeatures(nil)

	if !queriedRepository(TrendAnalysis(nil, half, corpusScope),
		"/api/analysis/trends?variant=20/20") {
		t.Fatal("a cache holding features but no signals answered without querying; " +
			"the two corpora must report warmth separately")
	}
}

// --- gem features / gem signals --------------------------------------------

func TestGemFeaturesAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, GemFeaturesAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/gem-features?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestGemFeaturesAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(corpusScope)

	if !queriedRepository(GemFeaturesAnalysis(nil, cold, corpusScope),
		"/api/analysis/gem-features?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

func TestGemSignalsAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, GemSignalsAnalysis(nil, warmEmptyCorpora(t), corpusScope),
		"/api/analysis/gem-signals?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestGemSignalsAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(corpusScope)

	if !queriedRepository(GemSignalsAnalysis(nil, cold, corpusScope),
		"/api/analysis/gem-signals?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// --- collective (normal mode) ----------------------------------------------

// The dashboard's endpoint. Every poll used to re-run three queries against a
// corpus the tick had already answered for.
func TestCollectiveAnalysis_WarmButEmptyCorporaAnswerWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, warmEmptyCorporaWithSparklines(t), corpusScope),
		"/api/analysis/collective?variant=20/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestCollectiveAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(corpusScope)

	if !queriedRepository(CollectiveAnalysis(nil, cold, corpusScope),
		"/api/analysis/collective?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// Transfigure comes from its own tick, so a warm v2 pipeline says nothing about
// it: the rows this endpoint ranks would all be missing their ROI.
func TestCollectiveAnalysis_WarmSignalsWithColdTransfigureFallsBackToTheRepository(t *testing.T) {
	half := lab.NewCache(corpusScope)
	half.For(corpusScope).SetGemSignals(nil)
	half.For(corpusScope).SetGemFeatures(nil)

	if !queriedRepository(CollectiveAnalysis(nil, half, corpusScope),
		"/api/analysis/collective?variant=20/20") {
		t.Fatal("a cache with no transfigure corpus answered without querying; " +
			"the transfigure tick reports warmth on its own")
	}
}

// --- compare (normal mode) -------------------------------------------------

// The desktop's per-gem path: one call per gem in an in-game scan.
func TestCompareAnalysis_WarmButEmptyCorporaAnswerWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CompareAnalysis(nil, warmEmptyCorporaWithSparklines(t), nil, corpusScope),
		"/api/analysis/compare?gems=Spark+of+Nova&variant=20/20")

	rows := decodeCompareNames(t, w)
	row, ok := rows["Spark of Nova"]
	if !ok {
		t.Fatalf("no row for the requested gem: %+v", rows)
	}
	if row.TransfiguredPrice != 0 {
		t.Errorf("price = %.0f, want 0 — the corpus holds nothing for this gem", row.TransfiguredPrice)
	}
}

// Only the ranking corpora are cold here. Every gem this endpoint is asked about
// is also a sparkline read, so a wholly cold cache reaches the database through
// that query alone and the case would pass with the ranking fallback deleted.
func TestCompareAnalysis_ColdRankingCorporaFallBackToTheRepository(t *testing.T) {
	cold := warmUnrelatedSparklines(t, lab.NewCache(corpusScope))

	if !queriedRepository(CompareAnalysis(nil, cold, nil, corpusScope),
		"/api/analysis/compare?gems=Spark+of+Nova&variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// --- autocomplete over the Font name pool ----------------------------------

// The pool is warm as soon as the tick's query has stored it — including when
// that query found no eligible gem to name. Deriving warmth from the corpus
// instead put every keystroke back on a DISTINCT ... ILIKE over gem_snapshots.
func TestGemNamesAutocomplete_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(corpusScope)
	cache.For(corpusScope).SetGemNamePool(nil)

	w := serveWithoutRepository(t, GemNamesAutocomplete(nil, cache, corpusScope),
		"/api/analysis/gems/names?q=spark")

	if names := decodeNames(t, w); len(names) != 0 {
		t.Errorf("names = %v, want none", names)
	}
}

// Same for the Dedication pools, which are stored one at a time.
func TestGemNamesAutocomplete_WarmButEmptyCorruptedPoolAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(corpusScope)
	cache.For(corpusScope).SetCorruptedGemNamePool(true, nil)

	w := serveWithoutRepository(t, GemNamesAutocomplete(nil, cache, corpusScope),
		"/api/analysis/gems/names?q=grace&corrupted=true")

	if names := decodeNames(t, w); len(names) != 0 {
		t.Errorf("names = %v, want none", names)
	}
}
