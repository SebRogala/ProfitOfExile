package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"profitofexile/internal/lab"
)

// The double-corruption HTTP surface (POE-125): the ranking endpoint and the
// compare path's tiebreaker join.
//
// The seam is this package's usual one, a nil *lab.Repository — every query
// method dereferences r.pool, so a surviving query panics. Asserting on the
// response body cannot see a warm-cache read that queried anyway: the body is
// identical either way.

func dcCache(t *testing.T, results ...lab.DoubleCorruptResult) *lab.Cache {
	t.Helper()
	c := lab.NewCache(corpusScope)
	c.For(corpusScope).SetDoubleCorrupt(lab.BuildDoubleCorruptCorpus(results))
	return c
}

func dcRow(name, inputVariant string, profit float64) lab.DoubleCorruptResult {
	return lab.DoubleCorruptResult{
		Time: time.Now(), Name: name, InputVariant: inputVariant, Color: "BLUE",
		InputCost: 10, EV: profit + 10, EVRaw: profit + 10, Profit: profit,
		LiquidityRisk: "LOW", Model: lab.DoubleCorruptModelEstimated,
	}
}

type dcResponseRow struct {
	Name         string  `json:"name"`
	InputVariant string  `json:"inputVariant"`
	Profit       float64 `json:"profit"`
	Model        string  `json:"model"`
}

func decodeDCRows(t *testing.T, w *httptest.ResponseRecorder) []dcResponseRow {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var resp struct {
		Data []dcResponseRow `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return resp.Data
}

func TestDoubleCorruptAnalysis_WarmCacheServesWithoutQuerying(t *testing.T) {
	cache := dcCache(t, dcRow("Arc of Surging", "20/20", 500))

	w := serveWithoutRepository(t, DoubleCorruptAnalysis(nil, cache, corpusScope),
		"/api/analysis/double-corrupt?variant=20/20")

	rows := decodeDCRows(t, w)
	if len(rows) != 1 || rows[0].Name != "Arc of Surging" {
		t.Fatalf("rows = %+v, want the one cached result", rows)
	}
	if rows[0].Model != lab.DoubleCorruptModelEstimated {
		t.Errorf("model = %q, want %q — a consumer badges these odds as estimates off this field",
			rows[0].Model, lab.DoubleCorruptModelEstimated)
	}
}

// A tick that priced nothing still ran, and its empty answer is authoritative.
// Reading warmth from the row count instead puts every request back on the
// hypertable for the life of the process.
func TestDoubleCorruptAnalysis_WarmButEmptyCorpusAnswersWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, DoubleCorruptAnalysis(nil, dcCache(t), corpusScope),
		"/api/analysis/double-corrupt?variant=20/20")

	if rows := decodeDCRows(t, w); len(rows) != 0 {
		t.Errorf("rows = %+v, want none", rows)
	}
}

// The other half of the same contract: a corpus nothing has stored must still
// reach the database.
func TestDoubleCorruptAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	if !queriedRepository(DoubleCorruptAnalysis(nil, lab.NewCache(corpusScope), corpusScope),
		"/api/analysis/double-corrupt?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

func TestDoubleCorruptAnalysis_RejectsAnUnmodelledInputVariant(t *testing.T) {
	// Answering with every variant would serve a different market than the one
	// asked for, silently.
	req := httptest.NewRequest(http.MethodGet, "/api/analysis/double-corrupt?variant=1/20", nil)
	w := httptest.NewRecorder()
	DoubleCorruptAnalysis(nil, dcCache(t, dcRow("Arc of Surging", "20/20", 500)), corpusScope).
		ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Errorf("status = %d, want 400 for a variant the calculator does not model (body %s)",
			w.Code, w.Body.String())
	}
}

func TestDoubleCorruptAnalysis_NarrowsToTheRequestedInputVariant(t *testing.T) {
	cache := dcCache(t,
		dcRow("Arc of Surging", "20/20", 500),
		dcRow("Arc of Surging", "1/20", 9000),
	)

	w := serveWithoutRepository(t, DoubleCorruptAnalysis(nil, cache, corpusScope),
		"/api/analysis/double-corrupt?variant=20/20")

	rows := decodeDCRows(t, w)
	if len(rows) != 1 || rows[0].InputVariant != "20/20" {
		t.Fatalf("rows = %+v, want only the 20/20 market", rows)
	}
}

// --- compare tiebreaker join ------------------------------------------------

type dcCompareRow struct {
	TransfiguredName      string  `json:"transfiguredName"`
	Recommendation        string  `json:"recommendation"`
	DoubleCorruptProfit   float64 `json:"doubleCorruptProfit"`
	DoubleCorruptModel    string  `json:"doubleCorruptModel"`
	DoubleCorruptTiebreak bool    `json:"doubleCorruptTiebreak"`
}

func decodeDCCompareRows(t *testing.T, w *httptest.ResponseRecorder) map[string]dcCompareRow {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var resp struct {
		Data []dcCompareRow `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	out := make(map[string]dcCompareRow, len(resp.Data))
	for _, row := range resp.Data {
		out[row.TransfiguredName] = row
	}
	return out
}

// The compare path reaches the double-corruption corpus, and the numbers survive
// the handler's row mapping into JSON.
func TestCompareAnalysis_ServesDoubleCorruptFieldsFromTheWarmCorpus(t *testing.T) {
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	warmSparklines(t, cache, nil, nil)
	cache.For(sparklineScope).SetDoubleCorrupt(lab.BuildDoubleCorruptCorpus(
		[]lab.DoubleCorruptResult{dcRow("Spark of Nova", "20/20", 750)}))

	w := serveWithoutRepository(t, CompareAnalysis(nil, cache, nil, sparklineScope),
		"/api/analysis/compare?gems=Spark+of+Nova&variant=20/20")

	row, ok := decodeDCCompareRows(t, w)["Spark of Nova"]
	if !ok {
		t.Fatal("no compare row for the requested gem")
	}
	if row.DoubleCorruptProfit != 750 {
		t.Errorf("doubleCorruptProfit = %v, want 750", row.DoubleCorruptProfit)
	}
	if row.DoubleCorruptModel != lab.DoubleCorruptModelEstimated {
		t.Errorf("doubleCorruptModel = %q, want %q", row.DoubleCorruptModel, lab.DoubleCorruptModelEstimated)
	}
}

// The corpus is only consulted for the input variants the calculator models.
// DoubleCorruptVariants is what makes an EV meaningful — an outcome
// distribution is computed per input variant — so a row filed under any other
// one must not reach a comparison even when the corpus happens to hold it.
func TestCompareAnalysis_UnmodelledVariantCarriesNoDoubleCorruptFields(t *testing.T) {
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "1/20", roi: 40})
	warmSparklines(t, cache, nil, nil)
	cache.For(sparklineScope).SetDoubleCorrupt(lab.BuildDoubleCorruptCorpus(
		[]lab.DoubleCorruptResult{dcRow("Spark of Nova", "1/20", 750)}))

	w := serveWithoutRepository(t, CompareAnalysis(nil, cache, nil, sparklineScope),
		"/api/analysis/compare?gems=Spark+of+Nova&variant=1/20")

	row, ok := decodeDCCompareRows(t, w)["Spark of Nova"]
	if !ok {
		t.Fatal("no compare row for the requested gem")
	}
	if row.DoubleCorruptProfit != 0 || row.DoubleCorruptModel != "" || row.DoubleCorruptTiebreak {
		t.Errorf("1/20 request carried 20/20 double-corruption data: %+v", row)
	}
}
