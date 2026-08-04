package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"profitofexile/internal/lab"
)

// The Dedication collective and compare paths are served from the corpus the
// tick builds, so neither should reach the database on a warm cache. The seam is
// this package's usual one: a nil *lab.Repository panics if a query survives.
//
// Response-shape assertions cannot see the defect these guard — the body is the
// same whether the rows came from the cache or from a query — which is why every
// case here asserts on whether the repository was reached.

// dedicationCorpusOnlyCache warms the Dedication corpus the way RunDedication
// does — one corpus built from a snapshot and the analysis computed over it —
// and nothing else. The sparkline corpus is filled by a different tick stage and
// reports its warmth separately, so this leaves it cold.
func dedicationCorpusOnlyCache(t *testing.T) *lab.Cache {
	t.Helper()
	gems := []lab.GemPrice{
		{Name: "Arc", Variant: "21/23c", Chaos: 500, Listings: 20, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Spark", Variant: "21/23c", Chaos: 120, Listings: 20, IsCorrupted: true, GemColor: "BLUE"},
		{Name: "Vaal Arc", Variant: "21/23c", Chaos: 60, Listings: 12, IsCorrupted: true, GemColor: "BLUE"},
	}
	analysis := lab.DedicationAnalysis{Skills: []lab.DedicationResult{
		{Time: time.Now(), Variant: "21/23c", Color: "BLUE", GemType: "skill", Mode: "safe", InputCost: 100},
	}}

	cache := lab.NewCache(dedicationScope)
	cache.For(dedicationScope).SetDedication(lab.BuildDedicationCorpus(gems, analysis))
	return cache
}

// dedicationCorpusCache adds the sparkline stage, so the cases below isolate the
// Dedication corpus instead of tripping over the compare path's series read.
func dedicationCorpusCache(t *testing.T) *lab.Cache {
	t.Helper()
	cache := dedicationCorpusOnlyCache(t)
	now := time.Now().UTC().Format(time.RFC3339)
	cache.For(dedicationScope).SetSparklinesByName(nil, map[string]map[string][]lab.SparklinePoint{
		"Arc": {
			"21/23c": {{Time: now, Price: 500, Listings: 20}},
			"21/20c": {{Time: now, Price: 20, Listings: 8}},
		},
	}, time.Now())
	return cache
}

type collectiveNameRow struct {
	TransfiguredName string  `json:"transfiguredName"`
	ROI              float64 `json:"roi"`
}

func decodeCollectiveNames(t *testing.T, w *httptest.ResponseRecorder) []collectiveNameRow {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var resp struct {
		Data []collectiveNameRow `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return resp.Data
}

// The route this replaces materialised all 7,325 rows of the latest snapshot on
// every request to re-derive a ranking whose other input was already cached.
func TestDedicationCollective_WarmCacheServesTheRankingWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, dedicationCorpusCache(t), dedicationScope),
		"/api/analysis/collective?mode=dedication&variant=21/23c")

	rows := decodeCollectiveNames(t, w)
	if len(rows) != 2 {
		t.Fatalf("got %d rows %+v, want Arc and Spark (a Vaal gem is never a font outcome)", len(rows), rows)
	}
	if rows[0].TransfiguredName != "Arc" || rows[0].ROI != 400 {
		t.Errorf("top row = %+v, want Arc with ROI 400 (500c against a 100c input cost)", rows[0])
	}
}

// The whole point of pre-ranking: search and limit narrow the cached list in
// memory rather than sending the request back to the snapshot.
func TestDedicationCollective_WarmCacheAppliesSearchWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, dedicationCorpusCache(t), dedicationScope),
		"/api/analysis/collective?mode=dedication&variant=21/23c&search=spark")

	rows := decodeCollectiveNames(t, w)
	if len(rows) != 1 || rows[0].TransfiguredName != "Spark" {
		t.Errorf("rows = %+v, want only Spark", rows)
	}
}

func TestDedicationCollective_WarmCacheAppliesLimitWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, dedicationCorpusCache(t), dedicationScope),
		"/api/analysis/collective?mode=dedication&variant=21/23c&limit=1")

	rows := decodeCollectiveNames(t, w)
	if len(rows) != 1 || rows[0].TransfiguredName != "Arc" {
		t.Errorf("rows = %+v, want only the highest-priced row, Arc", rows)
	}
}

// Both variants are ranked in the same pass, so a warm cache holding nothing for
// one of them is authoritative — the database has nothing either, and re-asking
// it would query on every poll of a dashboard endpoint forever.
func TestDedicationCollective_WarmCacheAnswersAnUnpopulatedVariantWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, dedicationCorpusCache(t), dedicationScope),
		"/api/analysis/collective?mode=dedication&variant=21/20c")

	if rows := decodeCollectiveNames(t, w); len(rows) != 0 {
		t.Errorf("rows = %+v, want none — the 21/23c gems must not leak into a 21/20c answer", rows)
	}
}

// The fallback stays reachable. Without it a restart leaves no input cost at
// all and every row's ROI silently becomes the gem's full listed price.
func TestDedicationCollective_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(dedicationScope)

	if !queriedRepository(CollectiveAnalysis(nil, cold, dedicationScope),
		"/api/analysis/collective?mode=dedication&variant=21/23c") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

type compareNameRow struct {
	TransfiguredName  string  `json:"transfiguredName"`
	TransfiguredPrice float64 `json:"transfiguredPrice"`
	BasePrice         float64 `json:"basePrice"`
}

func decodeCompareNames(t *testing.T, w *httptest.ResponseRecorder) map[string]compareNameRow {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var resp struct {
		Data []compareNameRow `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	out := make(map[string]compareNameRow, len(resp.Data))
	for _, row := range resp.Data {
		out[row.TransfiguredName] = row
	}
	return out
}

// The prices this path used to query for per request are a name-narrowing of the
// same snapshot the corpus already holds.
func TestDedicationCompare_WarmCacheServesPricesWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CompareAnalysis(nil, dedicationCorpusCache(t), nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Arc")

	rows := decodeCompareNames(t, w)
	row, ok := rows["Arc"]
	if !ok {
		t.Fatalf("no row for Arc: %+v", rows)
	}
	if row.TransfiguredPrice != 500 {
		t.Errorf("price = %.0f, want 500 from the cached snapshot", row.TransfiguredPrice)
	}
	if row.BasePrice != 100 {
		t.Errorf("input cost = %.0f, want 100 from the cached analysis", row.BasePrice)
	}
}

// The compare path answers for gems the font can never hand out, marking them as
// not an outcome. Serving prices from a corpus narrowed to rankable gems would
// turn those rows into "no price found" instead.
func TestDedicationCompare_WarmCacheStillPricesAGemTheFontCannotProduce(t *testing.T) {
	w := serveWithoutRepository(t, CompareAnalysis(nil, dedicationCorpusCache(t), nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Vaal+Arc")

	rows := decodeCompareNames(t, w)
	row, ok := rows["Vaal Arc"]
	if !ok {
		t.Fatalf("no row for Vaal Arc: %+v", rows)
	}
	if row.TransfiguredPrice != 60 {
		t.Errorf("price = %.0f, want 60 — the gem is a legal feed and has a listed price", row.TransfiguredPrice)
	}
}

// A name the snapshot holds no corrupted listing for is answered out of the warm
// corpus, not re-asked of the database.
func TestDedicationCompare_WarmCacheAnswersAnUnpricedGemWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CompareAnalysis(nil, dedicationCorpusCache(t), nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Nonexistent+Gem")

	rows := decodeCompareNames(t, w)
	row, ok := rows["Nonexistent Gem"]
	if !ok {
		t.Fatalf("no row for the requested gem: %+v", rows)
	}
	if row.TransfiguredPrice != 0 {
		t.Errorf("price = %.0f, want 0 — the corpus has no listing for this name", row.TransfiguredPrice)
	}
}

// The sparkline stage is separate from the Dedication corpus, so its fallback
// has to stay reachable on its own: a corpus warmed before the first sparkline
// population must still query for series rather than serve none.
func TestDedicationCompare_ColdSparklineCacheFallsBackToTheRepository(t *testing.T) {
	if !queriedRepository(CompareAnalysis(nil, dedicationCorpusOnlyCache(t), nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Arc") {
		t.Fatal("cold sparkline cache served without a repository query, want the database fallback")
	}
}

// The other half of that decision. A sparkline population that retained no
// corrupted series stores an empty map, and judging warmth by its contents sends
// this path back to gem_snapshots on every request for the life of the process —
// for series the tick already established do not exist.
func TestDedicationCompare_SparklinePopulationThatRetainedNothingServesWithoutQuerying(t *testing.T) {
	cache := dedicationCorpusOnlyCache(t)
	cache.For(dedicationScope).SetSparklinesByName(nil, nil, time.Now())

	w := serveWithoutRepository(t, CompareAnalysis(nil, cache, nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Arc")

	rows := decodeCompareNames(t, w)
	if row, ok := rows["Arc"]; !ok || row.TransfiguredPrice != 500 {
		t.Errorf("row for Arc = %+v (present: %t), want the cached 500c price", row, ok)
	}
}

func TestDedicationCompare_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(dedicationScope)

	if !queriedRepository(CompareAnalysis(nil, cold, nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/23c&gems=Arc") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// A variant the snapshot held no corrupted gems at is answered out of the warm
// corpus. Judging warmth on that variant's price list instead would send the
// three queries this path replaces back to the database on every request,
// forever — the corpus is filled for both variants in one pass, so an empty one
// is the answer and the database has nothing to add.
func TestDedicationCompare_WarmCacheAnswersAnUnpopulatedVariantWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, CompareAnalysis(nil, dedicationCorpusCache(t), nil, dedicationScope),
		"/api/analysis/compare?mode=dedication&variant=21/20c&gems=Arc")

	rows := decodeCompareNames(t, w)
	row, ok := rows["Arc"]
	if !ok {
		t.Fatalf("no row for Arc: %+v", rows)
	}
	if row.TransfiguredPrice != 0 {
		t.Errorf("price = %.0f, want 0 — the 21/23c price must not answer for 21/20c", row.TransfiguredPrice)
	}
}
