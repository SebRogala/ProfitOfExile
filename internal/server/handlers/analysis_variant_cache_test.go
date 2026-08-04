package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// Found while auditing lab.Cache's accessors for POE-152's nil-means-cold
// ambiguity: these two handlers derived cache warmth from the size of the
// *filtered* result, so a variant the warm cache holds no rows for read as a
// cold cache and queried the database on every request. variant is an
// unvalidated query parameter (normalizeVariant only strips a "/0" suffix), so
// any value at all reaches this path.
//
// The seam is the same one the rest of this package uses: a nil repository
// panics if a query survives.

var variantCacheScope = league.Historical("Mirage")

func decodeCount(t *testing.T, w *httptest.ResponseRecorder) int {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var resp struct {
		Count int `json:"count"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return resp.Count
}

// The analyzer computes every variant in one pass, so a warm cache with no rows
// for the requested variant is authoritative — the database has none either.
func TestTransfigureAnalysis_WarmCacheUnknownVariantAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(variantCacheScope)
	cache.For(variantCacheScope).SetTransfigure([]lab.TransfigureResult{
		{Time: time.Now(), TransfiguredName: "Spark of Nova", Variant: "20/20", ROI: 40},
	})

	w := serveWithoutRepository(t, TransfigureAnalysis(nil, cache, variantCacheScope),
		"/api/analysis/transfigure?variant=1/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

// The fallback stays reachable before the first analysis run.
func TestTransfigureAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(variantCacheScope)

	if !queriedRepository(TransfigureAnalysis(nil, cold, variantCacheScope),
		"/api/analysis/transfigure?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// Same defect, same fix, on the quality endpoint. Its variant maps to a gem
// level, so "1/20" asks for level 1 out of a cache holding only level 20.
func TestQualityAnalysis_WarmCacheUnknownVariantAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(variantCacheScope)
	cache.For(variantCacheScope).SetQuality([]lab.QualityResult{
		{Time: time.Now(), Name: "Spark", Level: 20, AvgROI: 12},
	})

	w := serveWithoutRepository(t, QualityAnalysis(nil, cache, variantCacheScope),
		"/api/analysis/quality?variant=1/20")

	if count := decodeCount(t, w); count != 0 {
		t.Errorf("count = %d, want 0", count)
	}
}

func TestQualityAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(variantCacheScope)

	if !queriedRepository(QualityAnalysis(nil, cold, variantCacheScope),
		"/api/analysis/quality?variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}
