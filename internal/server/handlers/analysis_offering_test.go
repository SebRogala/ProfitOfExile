package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

var offeringScope = league.Historical("Mirage")

// unusablePool is the database seam for MarketOverview, which takes a pool
// rather than a repository: a zero-value pool has no puddle behind it, so the
// first query panics. It is the same observable the nil-repository handlers use
// — "this request reached the database" — expressed for a pool argument.
//
// MarketOverview touches the pool in exactly one place, the offering-timing
// block, so a panic here can only have come from that block.
func unusablePool() *pgxpool.Pool { return &pgxpool.Pool{} }

// The dormant half of POE-152. ComputeOfferingTimings drops an offering whose
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

	MarketOverview(cache, unusablePool(), offeringScope).ServeHTTP(w, req)

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

	MarketOverview(cache, unusablePool(), offeringScope).ServeHTTP(w, req)

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
		MarketOverview(cache, unusablePool(), offeringScope).
			ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
		return false
	}()

	if !reached {
		t.Fatal("cold cache did not reach the pool; offering timings would never be computed")
	}
}
