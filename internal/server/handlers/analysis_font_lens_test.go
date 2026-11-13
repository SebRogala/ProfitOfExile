package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// The Font analysis carries the Facetor's Lens craft (POE-329 WI-3) as a
// top-level lens object built from the two fixed trade targets. Every case
// serves from a warm-empty lab corpus with a nil repository, so a case that
// reached the database would panic in serveWithoutRepository.

type lensQuoteWire struct {
	Chaos         *float64   `json:"chaos"`
	Listings      int        `json:"listings"`
	FetchedAt     *time.Time `json:"fetchedAt"`
	LowConfidence bool       `json:"lowConfidence"`
}

type lensWire struct {
	Outcome lensQuoteWire  `json:"outcome"`
	Feeder  lensFeederWire `json:"feeder"`
	EV      *float64       `json:"ev"`
}

type lensFeederWire struct {
	lensQuoteWire
	TradeURL string `json:"tradeUrl"`
}

func chaosLensListings(prices ...float64) []trade.TradeListingDetail {
	out := make([]trade.TradeListingDetail, len(prices))
	for i, p := range prices {
		out[i] = trade.TradeListingDetail{Price: p, Currency: "chaos"}
	}
	return out
}

func lensObservation(gem string, listings []trade.TradeListingDetail, divineRate float64, age time.Duration) *trade.TradeLookupResult {
	r := trade.BuildResult(gem, trade.FixedTargetVariantFont, corpusScope.ID(), trade.SearchResponse{}, listings, divineRate)
	r.FetchedAt = time.Now().Add(-age).UTC().Truncate(time.Second)
	return r
}

func setLens(c *trade.TradeCache, r *trade.TradeLookupResult) {
	c.For(corpusScope).Set(trade.CacheKey(r.Gem, r.Variant), r)
}

func serveLens(t *testing.T, tc *trade.TradeCache, target string) lensWire {
	t.Helper()
	w := serveWithoutRepository(t, FontAnalysis(nil, warmEmptyCorpora(t), tc, corpusScope), target)
	if w.Code != 200 {
		t.Fatalf("status = %d, body %s", w.Code, w.Body.String())
	}
	var body struct {
		Lens *lensWire `json:"lens"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if body.Lens == nil {
		t.Fatalf("response has no lens object")
	}
	return *body.Lens
}

func pricedLensCache(t *testing.T, lens, support []float64) *trade.TradeCache {
	t.Helper()
	tc := trade.NewTradeCache(10, corpusScope)
	setLens(tc, lensObservation(trade.FixedTargetLens60Outcome, chaosLensListings(lens...), 0, time.Hour))
	setLens(tc, lensObservation(trade.FixedTargetLens60Feeder, chaosLensListings(support...), 0, time.Hour))
	return tc
}

func TestFontAnalysis_LensAboveSupportServesAPositiveEV(t *testing.T) {
	lens := serveLens(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{90, 95, 99}), "/api/analysis/font")

	if lens.Outcome.Chaos == nil || *lens.Outcome.Chaos != 412 || lens.Feeder.Chaos == nil || *lens.Feeder.Chaos != 95 {
		t.Fatalf("outcome %v feeder %v, want 412 and 95", lens.Outcome.Chaos, lens.Feeder.Chaos)
	}
	if lens.EV == nil || *lens.EV != 317 {
		t.Fatalf("ev = %v, want 317", lens.EV)
	}
}

func TestFontAnalysis_LensBelowSupportServesANegativeEV(t *testing.T) {
	lens := serveLens(t, pricedLensCache(t, []float64{50, 60, 70}, []float64{90, 95, 99}), "/api/analysis/font")

	if lens.EV == nil || *lens.EV != -35 {
		t.Fatalf("ev = %v, want -35", lens.EV)
	}
}

func TestFontAnalysis_MissingSupportQuoteHasNoEVButKeepsTheLens(t *testing.T) {
	tc := trade.NewTradeCache(10, corpusScope)
	setLens(tc, lensObservation(trade.FixedTargetLens60Outcome, chaosLensListings(400, 412, 450), 0, time.Hour))

	lens := serveLens(t, tc, "/api/analysis/font")

	if lens.EV != nil {
		t.Errorf("ev = %v, want null", *lens.EV)
	}
	if lens.Outcome.Chaos == nil || lens.Outcome.FetchedAt == nil {
		t.Errorf("outcome = %+v, want the lens price and its fetchedAt", lens.Outcome)
	}
	if lens.Feeder.Chaos != nil || lens.Feeder.FetchedAt != nil {
		t.Errorf("feeder = %+v, want chaos and fetchedAt null", lens.Feeder)
	}
}

func TestFontAnalysis_EmptyLensSearchHasNoEVButKeepsItsFetchedAt(t *testing.T) {
	tc := pricedLensCache(t, nil, []float64{90, 95, 99})

	lens := serveLens(t, tc, "/api/analysis/font")

	if lens.EV != nil || lens.Outcome.Chaos != nil {
		t.Fatalf("ev %v outcome chaos %v, want both null", lens.EV, lens.Outcome.Chaos)
	}
	if lens.Outcome.FetchedAt == nil {
		t.Error("outcome fetchedAt = null, want the empty check's time")
	}
}

func TestFontAnalysis_SupportOlderThanADayHasNoEVButKeepsItsFetchedAt(t *testing.T) {
	tc := trade.NewTradeCache(10, corpusScope)
	setLens(tc, lensObservation(trade.FixedTargetLens60Outcome, chaosLensListings(400, 412, 450), 0, time.Hour))
	stale := lensObservation(trade.FixedTargetLens60Feeder, chaosLensListings(90, 95, 99), 0, 25*time.Hour)
	setLens(tc, stale)

	lens := serveLens(t, tc, "/api/analysis/font")

	if lens.EV != nil || lens.Feeder.Chaos != nil {
		t.Fatalf("ev %v feeder chaos %v, want both null", lens.EV, lens.Feeder.Chaos)
	}
	if lens.Feeder.FetchedAt == nil || !lens.Feeder.FetchedAt.Equal(stale.FetchedAt) {
		t.Errorf("feeder fetchedAt = %v, want %v", lens.Feeder.FetchedAt, stale.FetchedAt)
	}
}

func TestFontAnalysis_ThinLensMarketKeepsTheMedianAsLowConfidence(t *testing.T) {
	lens := serveLens(t, pricedLensCache(t, []float64{400, 420}, []float64{90, 95, 99}), "/api/analysis/font")

	if lens.Outcome.Chaos == nil || *lens.Outcome.Chaos != 410 || !lens.Outcome.LowConfidence || lens.Outcome.Listings != 2 {
		t.Fatalf("outcome = %+v (chaos %v), want 410 from 2 listings, low confidence", lens.Outcome, lens.Outcome.Chaos)
	}
	if lens.EV == nil || *lens.EV != 315 {
		t.Errorf("ev = %v, want 315", lens.EV)
	}
}

func TestFontAnalysis_VariantFilterKeepsTheLensObject(t *testing.T) {
	lens := serveLens(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{90, 95, 99}), "/api/analysis/font?variant=1/20")

	if lens.EV == nil || *lens.EV != 317 {
		t.Fatalf("ev = %v, want 317 under a variant filter", lens.EV)
	}
}

func TestFontAnalysis_NilTradeCacheServesAnAllNullLens(t *testing.T) {
	lens := serveLens(t, nil, "/api/analysis/font")

	if lens.EV != nil || lens.Outcome.Chaos != nil || lens.Outcome.FetchedAt != nil || lens.Feeder.Chaos != nil || lens.Feeder.FetchedAt != nil {
		t.Fatalf("lens = %+v, want all null", lens)
	}
}

var settlersScope = league.Historical("Settlers")

// settlersLensCache is a trade cache bound to Settlers holding priced lens and
// support observations tagged with Settlers.
func settlersLensCache(t *testing.T) *trade.TradeCache {
	t.Helper()
	tc := trade.NewTradeCache(10, settlersScope)
	for gem, prices := range map[string][]float64{
		trade.FixedTargetLens60Outcome: {400, 412, 450},
		trade.FixedTargetLens60Feeder:  {90, 95, 99},
	} {
		r := trade.BuildResult(gem, trade.FixedTargetVariantFont, settlersScope.ID(), trade.SearchResponse{}, chaosLensListings(prices...), 0)
		tc.For(settlersScope).Set(trade.CacheKey(gem, trade.FixedTargetVariantFont), r)
	}
	return tc
}

func warmEmptyFont(scope league.Scope) *lab.Cache {
	c := lab.NewCache(scope)
	c.For(scope).SetFont(lab.FontAnalysis{})
	return c
}

// A trade cache is bound to one league and For rejects any other scope by
// panicking, so a foreign cache reaching the handler must never price it.
func TestFontAnalysis_AnotherLeaguesObservationNeverFillsTheLens(t *testing.T) {
	h := FontAnalysis(nil, warmEmptyFont(corpusScope), settlersLensCache(t), corpusScope)
	w := httptest.NewRecorder()

	rejected := func() (panicked bool) {
		defer func() { panicked = recover() != nil }()
		h.ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/font", nil))
		return false
	}()

	if !rejected {
		t.Fatalf("a Settlers trade cache served the %s Font analysis: body %s", corpusScope.ID(), w.Body.String())
	}
	if strings.Contains(w.Body.String(), "412") {
		t.Errorf("body carries the Settlers lens price: %s", w.Body.String())
	}
}

func TestFontAnalysis_OwningLeaguesCachePricesTheLens(t *testing.T) {
	w := serveWithoutRepository(t, FontAnalysis(nil, warmEmptyFont(settlersScope), settlersLensCache(t), settlersScope),
		"/api/analysis/font")

	var body struct {
		Lens lensWire `json:"lens"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if body.Lens.EV == nil || *body.Lens.EV != 317 {
		t.Fatalf("ev = %v, want 317 from the owning league's cache", body.Lens.EV)
	}
}

func TestFontAnalysis_SupportTradeURLSearchesTheScopeLeagueWithANullPrice(t *testing.T) {
	lens := serveLens(t, nil, "/api/analysis/font")

	if lens.Feeder.Chaos != nil {
		t.Fatalf("feeder chaos = %v, want null", *lens.Feeder.Chaos)
	}
	u, err := url.Parse(lens.Feeder.TradeURL)
	if err != nil || u.Path != "/trade/search/"+corpusScope.ID() || u.Query().Get("q") == "" {
		t.Fatalf("feeder tradeUrl = %q, want a search in %s carrying q", lens.Feeder.TradeURL, corpusScope.ID())
	}
	support, _ := trade.LookupFixedTarget(trade.FixedTargetLens60Feeder, trade.FixedTargetVariantFont)
	if lens.Feeder.TradeURL != support.TradeURL(corpusScope.ID()) {
		t.Errorf("feeder tradeUrl = %q, want the registry support search", lens.Feeder.TradeURL)
	}
}

func TestFontAnalysis_LensOutcomeCarriesNoTradeURL(t *testing.T) {
	w := serveWithoutRepository(t, FontAnalysis(nil, warmEmptyCorpora(t), nil, corpusScope), "/api/analysis/font")

	var body struct {
		Lens struct {
			Outcome map[string]json.RawMessage `json:"outcome"`
		} `json:"lens"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if _, ok := body.Lens.Outcome["tradeUrl"]; ok {
		t.Errorf("outcome = %v, want no tradeUrl", body.Lens.Outcome)
	}
}

func divineOnlyLensListings() []trade.TradeListingDetail {
	return []trade.TradeListingDetail{
		{Price: 5, Currency: "divine"}, {Price: 5, Currency: "divine"}, {Price: 6, Currency: "divine"},
	}
}

func assertUnconvertibleLens(t *testing.T, lens lensWire, fetchedAt time.Time) {
	t.Helper()
	if lens.Outcome.Chaos != nil {
		t.Errorf("outcome chaos = %v, want null: divines without a rate are not chaos", *lens.Outcome.Chaos)
	}
	if lens.Outcome.FetchedAt == nil || !lens.Outcome.FetchedAt.Equal(fetchedAt) {
		t.Errorf("outcome fetchedAt = %v, want %v", lens.Outcome.FetchedAt, fetchedAt)
	}
	if lens.EV != nil {
		t.Errorf("ev = %v, want null", *lens.EV)
	}
}

func TestFontAnalysis_DivineLensWithoutARateSetInTheCacheHasNoPrice(t *testing.T) {
	tc := pricedLensCache(t, nil, []float64{90, 95, 99})
	r := lensObservation(trade.FixedTargetLens60Outcome, divineOnlyLensListings(), 0, time.Hour)
	setLens(tc, r)

	assertUnconvertibleLens(t, serveLens(t, tc, "/api/analysis/font"), r.FetchedAt)
}

// LatestLookups restores listings from JSONB with DivinePrice from divine_rate
// and no MedianTop3; Warm re-derives the median from the raw listings.
func TestFontAnalysis_DivineLensWithoutARateWarmedFromTheDatabaseHasNoPrice(t *testing.T) {
	built := lensObservation(trade.FixedTargetLens60Outcome, divineOnlyLensListings(), 0, time.Hour)
	listingsJSON, err := json.Marshal(built.Listings)
	if err != nil {
		t.Fatal(err)
	}
	restored := trade.TradeLookupResult{
		FetchedAt: built.FetchedAt, Gem: built.Gem, Variant: built.Variant,
		Total: 3, DivinePrice: 0,
	}
	if err := json.Unmarshal(listingsJSON, &restored.Listings); err != nil {
		t.Fatal(err)
	}
	tc := pricedLensCache(t, nil, []float64{90, 95, 99})
	tc.For(corpusScope).Warm([]trade.TradeLookupResult{restored})

	assertUnconvertibleLens(t, serveLens(t, tc, "/api/analysis/font"), built.FetchedAt)
}

func TestFontAnalysis_MixedChaosAndDivineLensWithoutARateHasNoPrice(t *testing.T) {
	tc := pricedLensCache(t, nil, []float64{90, 95, 99})
	r := lensObservation(trade.FixedTargetLens60Outcome, []trade.TradeListingDetail{
		{Price: 400, Currency: "chaos"}, {Price: 412, Currency: "chaos"}, {Price: 2, Currency: "divine"},
	}, 0, time.Hour)
	setLens(tc, r)

	assertUnconvertibleLens(t, serveLens(t, tc, "/api/analysis/font"), r.FetchedAt)
}
