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

// The signal-history and gem-dictionary endpoints are the two POE-149 measured
// as the desktop's in-game critical path: three history reads per gem scan, and
// a blocking dictionary fetch before the scan loop. Both are served from the
// tick's own output, so a warm cache must not reach the database — and the
// assertions are on whether it did, since the response body is identical either
// way. The seam is a nil *lab.Repository.

var cacheContractScope = league.Historical("Mirage")

// --- SignalHistory ---------------------------------------------------------

func signalHistoryCache(t *testing.T) *lab.Cache {
	t.Helper()
	now := time.Date(2026, 7, 20, 12, 0, 0, 0, time.UTC)
	cache := lab.NewCache(cacheContractScope)
	cache.For(cacheContractScope).SetSignalHistoryByName(map[string]map[string][]lab.SignalChange{
		"Spark of Nova": {"20/20": {
			{Time: now, Signal: "DEMAND", Price: 120},
			{Time: now.Add(-time.Hour), Signal: "STABLE", Price: 100},
			{Time: now.Add(-2 * time.Hour), Signal: "CAUTION", Price: 90},
		}},
	})
	return cache
}

type historyResponse struct {
	Count   int                `json:"count"`
	History []lab.SignalChange `json:"history"`
}

func decodeHistory(t *testing.T, w *httptest.ResponseRecorder) historyResponse {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body %s)", w.Code, w.Body.String())
	}
	var resp historyResponse
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	return resp
}

func TestSignalHistory_WarmCacheServesTheRingWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, SignalHistory(nil, signalHistoryCache(t), cacheContractScope),
		"/api/analysis/history?name=Spark+of+Nova&variant=20/20&limit=3")

	resp := decodeHistory(t, w)
	if len(resp.History) != 3 {
		t.Fatalf("got %d transitions, want 3: %+v", len(resp.History), resp.History)
	}
	if resp.History[0].Signal != "DEMAND" {
		t.Errorf("first transition = %q, want DEMAND — the ring is served newest first",
			resp.History[0].Signal)
	}
}

// The ring holds up to SignalHistoryDepth entries and the request asks for its
// own count, so the newest N are served rather than the whole ring.
func TestSignalHistory_WarmCacheTruncatesTheRingToTheRequestedLimit(t *testing.T) {
	w := serveWithoutRepository(t, SignalHistory(nil, signalHistoryCache(t), cacheContractScope),
		"/api/analysis/history?name=Spark+of+Nova&variant=20/20&limit=2")

	resp := decodeHistory(t, w)
	if len(resp.History) != 2 {
		t.Fatalf("got %d transitions, want 2: %+v", len(resp.History), resp.History)
	}
	if resp.History[0].Signal != "DEMAND" || resp.History[1].Signal != "STABLE" {
		t.Errorf("transitions = %+v, want the two newest (DEMAND, STABLE)", resp.History)
	}
}

// The keyed-read half of the cache-state contract: a gem with no ring on a warm
// cache genuinely has no transitions, and reading that as cold would put one
// query per gem back on every scan — three per scan, from the in-game path.
func TestSignalHistory_WarmCacheAnswersAGemWithNoRingWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, SignalHistory(nil, signalHistoryCache(t), cacheContractScope),
		"/api/analysis/history?name=Unknown+Gem&variant=20/20")

	if resp := decodeHistory(t, w); len(resp.History) != 0 {
		t.Errorf("got %d transitions, want none: %+v", len(resp.History), resp.History)
	}
}

// Warmth is per corpus, not per variant: the rings are filled for every gem the
// tick computed signals for, in one assignment.
func TestSignalHistory_WarmCacheAnswersAnUnseenVariantWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, SignalHistory(nil, signalHistoryCache(t), cacheContractScope),
		"/api/analysis/history?name=Spark+of+Nova&variant=1/0")

	if resp := decodeHistory(t, w); len(resp.History) != 0 {
		t.Errorf("got %d transitions, want none — the 20/20 ring must not answer for 1/0: %+v",
			len(resp.History), resp.History)
	}
}

// Before the first RunV2 there are no rings, and answering "no transitions" out
// of an empty cache would leave the endpoint permanently blank on a fresh
// process.
func TestSignalHistory_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(cacheContractScope)

	if !queriedRepository(SignalHistory(nil, cold, cacheContractScope),
		"/api/analysis/history?name=Spark+of+Nova&variant=20/20") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}

// --- GemDictionary ---------------------------------------------------------

func gemDictionaryCache(t *testing.T) *lab.Cache {
	t.Helper()
	cache := lab.NewCache(cacheContractScope)
	cache.For(cacheContractScope).SetGemDictionary(
		[]string{"Cyclone", "Spark"},
		[]string{"Spark of Nova"},
	)
	return cache
}

// The desktop blocks on this before its gem scan loop, so a query here lands as
// OCR that does not start.
func TestGemDictionary_WarmCacheServesTheTransfiguredPoolWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, GemDictionary(nil, gemDictionaryCache(t), cacheContractScope),
		"/api/analysis/gems/dictionary")

	names := decodeNames(t, w)
	if len(names) != 1 || names[0] != "Spark of Nova" {
		t.Errorf("names = %v, want the transfigured pool — that is the default", names)
	}
}

func TestGemDictionary_WarmCacheServesTheSkillPoolWithoutQuerying(t *testing.T) {
	w := serveWithoutRepository(t, GemDictionary(nil, gemDictionaryCache(t), cacheContractScope),
		"/api/analysis/gems/dictionary?transfigured=false")

	names := decodeNames(t, w)
	if len(names) != 2 || names[0] != "Cyclone" || names[1] != "Spark" {
		t.Errorf("names = %v, want the skill pool [Cyclone Spark]", names)
	}
}

// An empty dictionary is a legitimate answer for a league with no gem_colors
// rows and no snapshots; reading it as cold would query twice per request
// forever for an answer that cannot change before the next tick.
func TestGemDictionary_WarmButEmptyCacheAnswersWithoutQuerying(t *testing.T) {
	cache := lab.NewCache(cacheContractScope)
	cache.For(cacheContractScope).SetGemDictionary(nil, nil)

	w := serveWithoutRepository(t, GemDictionary(nil, cache, cacheContractScope),
		"/api/analysis/gems/dictionary")

	if names := decodeNames(t, w); len(names) != 0 {
		t.Errorf("names = %v, want none", names)
	}
}

// Before the first RunV2 the dictionary is unpopulated, and the OCR matcher
// cannot work from an empty one.
func TestGemDictionary_ColdCacheFallsBackToTheRepository(t *testing.T) {
	cold := lab.NewCache(cacheContractScope)

	if !queriedRepository(GemDictionary(nil, cold, cacheContractScope),
		"/api/analysis/gems/dictionary") {
		t.Fatal("cold cache did not reach the repository; the database fallback is gone")
	}
}
