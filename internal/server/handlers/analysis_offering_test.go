package handlers

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

var offeringScope = league.Historical("Mirage")

// unusableRepo is the database seam for MarketOverview: a repository over a
// zero-value pool has no puddle behind it, so the first query panics. It is the
// same observable the nil-repository handlers use — "this request reached the
// database" — for a handler whose nil repository means "never compute".
//
// MarketOverview touches the repository in exactly one place, the
// offering-timing block, so a panic here can only have come from that block.
func unusableRepo() *lab.Repository { return lab.NewRepository(&pgxpool.Pool{}) }

// The dormant half of POE-152. OfferingTimings drops an offering whose
// current-price query returns NULL, so an offering with no fragment_snapshots
// rows produces an empty result. Judging the cache by the number of offerings in
// it made that empty result a permanent miss: ten queries per request, forever.
//
// An assertion on the response alone would pass with the bug present — the
// response is offering-less either way. The panic is what distinguishes served
// from recomputed.
func TestMarketOverview_EmptyCachedOfferingTimingIsServedNotRecomputed(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	cache.For(offeringScope).SetOfferingTiming(json.RawMessage(`[]`))

	req := httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil)
	w := httptest.NewRecorder()
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("handler recomputed offering timings against a cache that already held an empty answer (pool touched: %v)", r)
		}
	}()

	MarketOverview(cache, unusableRepo(), nil, offeringScope).ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
}

// A populated cache is served as-is. This is the boundary the empty-answer test
// sits on: both must avoid the pool, for the same reason.
func TestMarketOverview_PopulatedCachedOfferingTimingIsServedFromTheCache(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	cache.For(offeringScope).SetOfferingTiming(json.RawMessage(
		`[{"name":"Gift to the Goddess","fragmentId":"offer-gift","currentPrice":42}]`))

	req := httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil)
	w := httptest.NewRecorder()
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("handler queried the pool despite a populated offering-timing cache: %v", r)
		}
	}()

	MarketOverview(cache, unusableRepo(), nil, offeringScope).ServeHTTP(w, req)

	var resp struct {
		Offerings []struct {
			Name         string  `json:"name"`
			CurrentPrice float64 `json:"currentPrice"`
		} `json:"offerings"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if len(resp.Offerings) != 1 || resp.Offerings[0].Name != "Gift to the Goddess" || resp.Offerings[0].CurrentPrice != 42 {
		t.Errorf("offerings = %+v, want the cached Gift entry at 42", resp.Offerings)
	}
}

// A cache that has never held an answer must still reach the pool — the compute
// path is what fills it in the first place.
func TestMarketOverview_ColdOfferingTimingCacheReachesThePool(t *testing.T) {
	cache := lab.NewCache(offeringScope)

	reached := func() (reached bool) {
		defer func() {
			if recover() != nil {
				reached = true
			}
		}()
		MarketOverview(cache, unusableRepo(), nil, offeringScope).
			ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
		return false
	}()

	if !reached {
		t.Fatal("cold cache did not reach the pool; offering timings would never be computed")
	}
}

func TestMarketOverview_divineRateIsTheAppWideRateRounded(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	cache.For(offeringScope).SetOfferingTiming(json.RawMessage(`[]`))
	w := httptest.NewRecorder()

	MarketOverview(cache, unusableRepo(), readerWithDivineRate(199.6), offeringScope).
		ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))

	var resp struct {
		DivineRate float64 `json:"divineRate"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if resp.DivineRate != 200 {
		t.Errorf("divineRate = %v, want 200 (the app-wide 199.6, rounded)", resp.DivineRate)
	}
}

// failingRepo is a repository whose every query fails: its pool is closed.
func failingRepo(t *testing.T) *lab.Repository {
	t.Helper()
	cfg, err := pgxpool.ParseConfig("postgres://u:p@127.0.0.1:1/none")
	if err != nil {
		t.Fatalf("parse pool config: %v", err)
	}
	pool, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatalf("open pool: %v", err)
	}
	pool.Close()
	return lab.NewRepository(pool)
}

func serveOverview(t *testing.T, cache *lab.Cache, repo *lab.Repository) (*httptest.ResponseRecorder, map[string]json.RawMessage) {
	t.Helper()
	w := httptest.NewRecorder()
	MarketOverview(cache, repo, readerWithDivineRate(150), offeringScope).
		ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
	var body map[string]json.RawMessage
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v; body: %s", err, w.Body.String())
	}
	return w, body
}

// POE-410 C1: a cold fill that fails keeps today's partial response — normal
// status, offerings omitted, every other overview field as before — marks it
// stale and caches nothing, so the next request queries again.
func TestKeepCriticalMarketOverviewFailedColdFillServesPartialStaleUncached(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	w, body := serveOverview(t, cache, failingRepo(t))

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	if got := w.Header().Get("X-Data-Stale"); got != "1" {
		t.Errorf("X-Data-Stale = %q, want 1 on the partial response", got)
	}
	if _, ok := body["offerings"]; ok {
		t.Errorf("offerings = %s, want the key omitted as today's partial response does", body["offerings"])
	}
	if string(body["divineRate"]) != "150" || string(body["temporalMode"]) != `"none"` {
		t.Errorf("divineRate %s, temporalMode %s; want 150 and \"none\" unchanged", body["divineRate"], body["temporalMode"])
	}
	if stored := cache.For(offeringScope).OfferingTiming(); stored != nil {
		t.Errorf("a failed fill cached %s; want nothing so the next request queries again", stored)
	}
}

// A failed background refresh keeps the last complete offerings and serves
// them marked stale.
func TestKeepCriticalMarketOverviewFailedRefreshServesLastCompleteValueStale(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	last := `[{"name":"Gift to the Goddess","fragmentId":"offer-gift","currentPrice":42}]`
	cache.For(offeringScope).SetOfferingTiming(json.RawMessage(last))

	if _, err := lab.RefreshOfferingTimings(context.Background(), failingRepo(t), cache, offeringScope); err == nil {
		t.Error("RefreshOfferingTimings over a failing repository returned no error")
	}
	if got := string(cache.For(offeringScope).OfferingTiming()); got != last {
		t.Fatalf("cached offering timing = %s after a failed refresh, want the last complete %s", got, last)
	}

	w, body := serveOverview(t, cache, unusableRepo())
	if got := w.Header().Get("X-Data-Stale"); got != "1" {
		t.Errorf("X-Data-Stale = %q, want 1 while the last value is stale", got)
	}
	if string(body["offerings"]) != last {
		t.Errorf("offerings = %s, want the last complete %s", body["offerings"], last)
	}
}

// A fresh value carries no stale header.
func TestKeepHappyMarketOverviewFreshOfferingsCarryNoStaleHeader(t *testing.T) {
	cache := lab.NewCache(offeringScope)
	cache.For(offeringScope).SetOfferingTiming(json.RawMessage(`[]`))
	w, _ := serveOverview(t, cache, unusableRepo())
	if got := w.Header().Get("X-Data-Stale"); got != "" {
		t.Errorf("X-Data-Stale = %q on a fresh value, want absent", got)
	}
}
