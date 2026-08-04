package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// The handler is constructed with a nil *lab.Repository: with a warm cache the
// database path is never taken, and a surviving query would panic rather than
// quietly serve the wrong pool.

var dedicationScope = league.Historical("Mirage")

// warmDedicationCache fills the cache with one BLUE skill row per Dedication
// variant, priced far enough apart that the response says which pool it served.
func warmDedicationCache(t *testing.T) *lab.Cache {
	t.Helper()
	now := time.Now()
	cache := lab.NewCache(dedicationScope)
	cache.For(dedicationScope).SetDedication(lab.DedicationCorpus{Analysis: lab.DedicationAnalysis{
		Skills: []lab.DedicationResult{
			{Time: now, Variant: "21/23c", Color: "BLUE", GemType: "skill", Mode: "safe", Pool: 3, EVRaw: 900, InputCost: 100, Profit: 800},
			{Time: now, Variant: "21/20c", Color: "BLUE", GemType: "skill", Mode: "safe", Pool: 40, EVRaw: 90, InputCost: 10, Profit: 80},
		},
		// Both pools are populated: the handler filters them separately, so a
		// fixture with only one leaves the other filter free to be dropped.
		Transfigured: []lab.DedicationResult{
			{Time: now, Variant: "21/23c", Color: "BLUE", GemType: "transfigured", Mode: "safe", Pool: 2, EVRaw: 500, InputCost: 200, Profit: 300},
			{Time: now, Variant: "21/20c", Color: "BLUE", GemType: "transfigured", Mode: "safe", Pool: 25, EVRaw: 50, InputCost: 20, Profit: 30},
		},
	}})
	return cache
}

type dedicationPool struct {
	Safe []struct {
		Variant string  `json:"variant"`
		Pool    int     `json:"pool"`
		Profit  float64 `json:"profit"`
	} `json:"safe"`
}

type dedicationResponse struct {
	Variant      string         `json:"variant"`
	Skills       dedicationPool `json:"skills"`
	Transfigured dedicationPool `json:"transfigured"`
}

func getDedication(t *testing.T, cache *lab.Cache, query string) (*httptest.ResponseRecorder, dedicationResponse) {
	t.Helper()
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/api/analysis/dedication"+query, nil)
	DedicationAnalysis(nil, cache, dedicationScope)(rec, req)

	var body dedicationResponse
	if rec.Code == http.StatusOK {
		if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
			t.Fatalf("decode response: %v (body %s)", err, rec.Body.String())
		}
	}
	return rec, body
}

func TestDedicationAnalysis_ServesTheRequestedVariant(t *testing.T) {
	rec, body := getDedication(t, warmDedicationCache(t), "?variant=21/20")

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", rec.Code, rec.Body.String())
	}
	if body.Variant != "21/20c" {
		t.Errorf("response variant = %q, want 21/20c", body.Variant)
	}
	if len(body.Skills.Safe) != 1 {
		t.Fatalf("got %d safe skill rows, want 1 (the other variant must not leak)", len(body.Skills.Safe))
	}
	row := body.Skills.Safe[0]
	if row.Variant != "21/20c" || row.Pool != 40 || row.Profit != 80 {
		t.Errorf("row = %+v, want the 21/20c pool (variant 21/20c, pool 40, profit 80)", row)
	}
}

// The transfigured pool is filtered by a separate call in the handler, so it
// gets its own assertion: dropping that filter is a plausible edit that the
// skills-only check cannot see.
func TestDedicationAnalysis_FiltersTheTransfiguredPoolToo(t *testing.T) {
	_, body := getDedication(t, warmDedicationCache(t), "?variant=21/20")

	if len(body.Transfigured.Safe) != 1 {
		t.Fatalf("got %d safe transfigured rows, want 1 (the 21/23c row must not leak)", len(body.Transfigured.Safe))
	}
	row := body.Transfigured.Safe[0]
	if row.Variant != "21/20c" || row.Pool != 25 {
		t.Errorf("row = %+v, want the 21/20c transfigured pool (variant 21/20c, pool 25)", row)
	}
}

// Clients built before the pool became selectable send no variant, and must
// keep getting the pool they have always been shown.
func TestDedicationAnalysis_DefaultsToTheOriginalVariant(t *testing.T) {
	rec, body := getDedication(t, warmDedicationCache(t), "")

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", rec.Code, rec.Body.String())
	}
	if body.Variant != "21/23c" {
		t.Errorf("response variant = %q, want 21/23c", body.Variant)
	}
	if len(body.Skills.Safe) != 1 || body.Skills.Safe[0].Variant != "21/23c" {
		t.Errorf("safe skill rows = %+v, want only the 21/23c row", body.Skills.Safe)
	}
}

// Falling back to 21/23c numbers under a 21/20 heading would misprice a farm;
// an unknown pool is refused instead.
func TestDedicationAnalysis_RejectsAVariantItDoesNotAnalyze(t *testing.T) {
	rec, _ := getDedication(t, warmDedicationCache(t), "?variant=20/20")

	if rec.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want 400 for variant 20/20 (body %s)", rec.Code, rec.Body.String())
	}
	// The body has to name what is allowed — a bare 400 is indistinguishable
	// from any other rejection this handler can emit.
	var errBody struct {
		Error string `json:"error"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &errBody); err != nil {
		t.Fatalf("decode error body: %v (body %s)", err, rec.Body.String())
	}
	if !strings.Contains(errBody.Error, "21/20c") {
		t.Errorf("error = %q, want it to name the analyzed variants", errBody.Error)
	}
}

// The sixth instance of the POE-152 defect, closed by POE-158: warmth was read
// off the variant-filtered result, so a variant the warm cache holds no rows
// for queried the database on every request. Both variants are analyzed in one
// pass, so a warm cache with nothing for one of them is authoritative — the
// database has nothing either.
//
// The assertion is on whether the repository was reached, not on the response:
// the body is an empty pool either way, so a shape check passes with the bug
// present. The seam is this package's usual one, a nil *lab.Repository.
func TestDedicationAnalysis_WarmCacheAnswersAnUnpopulatedVariantWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(dedicationScope)
	cache.For(dedicationScope).SetDedication(lab.DedicationCorpus{Analysis: lab.DedicationAnalysis{
		Skills: []lab.DedicationResult{
			{Time: time.Now(), Variant: "21/23c", Color: "BLUE", GemType: "skill", Mode: "safe", Pool: 3, Profit: 800},
		},
	}})

	w := serveWithoutRepository(t, DedicationAnalysis(nil, cache, dedicationScope),
		"/api/analysis/dedication?variant=21/20")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var body dedicationResponse
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v (body %s)", err, w.Body.String())
	}
	if len(body.Skills.Safe) != 0 || len(body.Transfigured.Safe) != 0 {
		t.Errorf("got %d skill and %d transfigured rows, want none — the 21/23c rows must not leak into a 21/20c answer",
			len(body.Skills.Safe), len(body.Transfigured.Safe))
	}
}

// The fallback stays reachable. Before the first Dedication run there is nothing
// cached, and answering "no rows" out of an empty cache would leave the endpoint
// permanently empty on a fresh process.
func TestDedicationAnalysis_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(dedicationScope)

	if !queriedRepository(DedicationAnalysis(nil, cold, dedicationScope),
		"/api/analysis/dedication?variant=21/23c") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// Warmth is judged on the whole analysis, so a run that produced transfigured
// rows and no skill rows must not send the skill pool back to the database: both
// pools come out of the same pass over the same snapshot.
func TestDedicationAnalysis_WarmCacheWithOnlyTransfiguredRowsDoesNotQueryForTheSkillPool(t *testing.T) {
	cache := lab.NewCache(dedicationScope)
	cache.For(dedicationScope).SetDedication(lab.DedicationCorpus{Analysis: lab.DedicationAnalysis{
		Transfigured: []lab.DedicationResult{
			{Time: time.Now(), Variant: "21/23c", Color: "BLUE", GemType: "transfigured", Mode: "safe", Pool: 2, Profit: 300},
		},
	}})

	w := serveWithoutRepository(t, DedicationAnalysis(nil, cache, dedicationScope),
		"/api/analysis/dedication?variant=21/23c")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var body dedicationResponse
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v (body %s)", err, w.Body.String())
	}
	if len(body.Skills.Safe) != 0 {
		t.Errorf("got %d safe skill rows, want none", len(body.Skills.Safe))
	}
	if len(body.Transfigured.Safe) != 1 {
		t.Fatalf("got %d safe transfigured rows, want the one cached row", len(body.Transfigured.Safe))
	}
}

func TestDedicationAnalysis_AcceptsTheVariantWithItsCorruptedSuffix(t *testing.T) {
	rec, body := getDedication(t, warmDedicationCache(t), "?variant=21/20c")

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", rec.Code, rec.Body.String())
	}
	if body.Variant != "21/20c" {
		t.Errorf("response variant = %q, want 21/20c", body.Variant)
	}
}

// A league whose snapshot holds no corrupted gems produces an analysis with no
// rows at all, and that is an answer: the tick computed it from the same data
// the query would read. Judging warmth on the rows instead of on the cache would
// run both LatestDedicationResults queries on every request, forever, for an
// answer that cannot change before the next tick.
func TestDedicationAnalysis_WarmCacheWithAnEmptyAnalysisDoesNotQuery(t *testing.T) {
	cache := lab.NewCache(dedicationScope)
	cache.For(dedicationScope).SetDedication(lab.BuildDedicationCorpus(nil, lab.DedicationAnalysis{}))

	w := serveWithoutRepository(t, DedicationAnalysis(nil, cache, dedicationScope),
		"/api/analysis/dedication?variant=21/23c")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var body dedicationResponse
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v (body %s)", err, w.Body.String())
	}
	if len(body.Skills.Safe) != 0 || len(body.Transfigured.Safe) != 0 {
		t.Errorf("got %d skill and %d transfigured rows, want none", len(body.Skills.Safe), len(body.Transfigured.Safe))
	}
}
