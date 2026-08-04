package handlers

import (
	"encoding/json"
	"net/http/httptest"
	"testing"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

// These tests follow the seam this package already uses for "served without
// touching the database": the handler is built with a nil *lab.Repository, so
// any surviving query panics on the nil pool (see collective_sparkline_test.go).
//
// The defect under test (POE-152) is that an empty search result read as a cold
// cache. Asserting only that the response is an empty list would pass with the
// bug present — the handler returned the same empty list after querying
// gem_snapshots. The panic is what separates the two.

var autocompleteScope = league.Historical("Mirage")

// warmGemNameCache returns a cache whose transfigured-name corpus is populated,
// which is what SetTransfigure does on every analysis run.
func warmGemNameCache(t *testing.T, names ...string) *lab.Cache {
	t.Helper()
	c := lab.NewCache(autocompleteScope)
	results := make([]lab.TransfigureResult, 0, len(names))
	for _, n := range names {
		results = append(results, lab.TransfigureResult{TransfiguredName: n})
	}
	c.For(autocompleteScope).SetTransfigure(results)
	return c
}

// decodeNames reads the autocomplete response body. A nil slice means the
// handler sent JSON null, which the desktop client cannot iterate.
func decodeNames(t *testing.T, w *httptest.ResponseRecorder) []string {
	t.Helper()
	if w.Code != 200 {
		t.Fatalf("status = %d, want 200; body: %s", w.Code, w.Body.String())
	}
	var resp struct {
		Names []string `json:"names"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp.Names == nil {
		t.Fatalf("names = null, want a JSON array; body: %s", w.Body.String())
	}
	return resp.Names
}

// The acceptance criterion for POE-152. A typed prefix that matches nothing is
// answered by the warm cache: the corpus it would search is the same corpus the
// database holds, so re-asking the database can only produce the same empty
// answer at the cost of a DISTINCT ... ILIKE over the gem_snapshots hypertable.
func TestGemNamesAutocomplete_WarmCacheAnswersAZeroMatchQueryWithoutQuerying(t *testing.T) {
	cache := warmGemNameCache(t, "Spark of Nova", "Ice Nova of Frostbolts")

	w := serveWithoutRepository(t, GemNamesAutocomplete(nil, cache, autocompleteScope),
		"/api/analysis/gems/names?q=zzq")

	if names := decodeNames(t, w); len(names) != 0 {
		t.Errorf("names = %v, want none", names)
	}
}

// The matching path must stay on the cache too — this is the request shape that
// was already fast, and it fixes the boundary the zero-match test sits on.
func TestGemNamesAutocomplete_WarmCacheServesMatchesWithoutQuerying(t *testing.T) {
	cache := warmGemNameCache(t, "Spark of Nova", "Ice Nova of Frostbolts")

	w := serveWithoutRepository(t, GemNamesAutocomplete(nil, cache, autocompleteScope),
		"/api/analysis/gems/names?q=frostbolts")

	names := decodeNames(t, w)
	if len(names) != 1 || names[0] != "Ice Nova of Frostbolts" {
		t.Errorf("names = %v, want only [Ice Nova of Frostbolts]", names)
	}
}

// The fallback stays reachable. Before the first analysis run there is no corpus
// to search, and answering "no matches" out of an empty cache would make
// autocomplete permanently dead on a fresh process.
func TestGemNamesAutocomplete_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(autocompleteScope)

	if !queriedRepository(GemNamesAutocomplete(nil, cold, autocompleteScope),
		"/api/analysis/gems/names?q=zzq") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// The Dedication (corrupted) branch carries the same defect and the same fix.
// Its fallback is worse than the transfigured one: it asks for limit*10 rows and
// filters them in Go.
func TestGemNamesAutocomplete_WarmCorruptedPoolAnswersAZeroMatchQueryWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(autocompleteScope)
	cache.For(autocompleteScope).SetCorruptedGemNamePool(true, []string{"Grace of the Vaal"})

	w := serveWithoutRepository(t, GemNamesAutocomplete(nil, cache, autocompleteScope),
		"/api/analysis/gems/names?q=zzq&corrupted=true")

	if names := decodeNames(t, w); len(names) != 0 {
		t.Errorf("names = %v, want none", names)
	}
}

// Warmth is per pool: the skill pool being empty must still reach the database
// even when the transfigured pool is warm, because the two are filled by two
// separate queries and only one of them may have succeeded.
func TestGemNamesAutocomplete_ColdCorruptedSkillPoolFallsBackToTheRepository(t *testing.T) {
	cache := lab.NewCache(autocompleteScope)
	cache.For(autocompleteScope).SetCorruptedGemNamePool(true, []string{"Grace of the Vaal"})

	if !queriedRepository(GemNamesAutocomplete(nil, cache, autocompleteScope),
		"/api/analysis/gems/names?q=zzq&corrupted=true&transfigured=false") {
		t.Fatal("cold skill pool did not reach the repository; the database fallback is gone")
	}
}
