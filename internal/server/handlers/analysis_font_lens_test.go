package handlers

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"reflect"
	"strings"
	"sync"
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
	r := trade.BuildResult(gem, trade.FixedTargetVariantFont, corpusScope.ID(), trade.SearchResponse{}, listings, 0, divineRate)
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
		r := trade.BuildResult(gem, trade.FixedTargetVariantFont, settlersScope.ID(), trade.SearchResponse{}, chaosLensListings(prices...), 0, 0)
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

// keep:happy AC-5: the Lens Gives row buys from the registry's stored
// experience search in the scope league, served even without a lens price.
func TestKeepHappyFontAnalysis_LensOutcomeTradeURLSearchesStoredExperienceUnpriced(t *testing.T) {
	lens := serveLensChoices(t, nil)

	if lens.Outcome.Chaos != nil {
		t.Fatalf("outcome chaos = %v, want null", *lens.Outcome.Chaos)
	}
	q := decodeTradeSearch(t, lens.Outcome.TradeURL)
	if got := q.query["type"]; got != "Facetor's Lens" {
		t.Errorf("outcome search type = %v, want Facetor's Lens", got)
	}
	if got := jsonPath(q.query, "filters", "misc_filters", "filters", "stored_experience", "min"); got != float64(999627000) {
		t.Errorf("outcome stored_experience min = %v, want 999627000", got)
	}
	outcome, _ := trade.LookupFixedTarget(trade.FixedTargetLens60Outcome, trade.FixedTargetVariantFont)
	if lens.Outcome.TradeURL != outcome.TradeURL(corpusScope.ID()) {
		t.Errorf("outcome tradeUrl = %q, want the registry lens search", lens.Outcome.TradeURL)
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

// The Font body served from a warm corpus, pinned whole: per-mode rows
// narrowed to the variant in profit order and cut at the limit, best colors,
// and the all-null lens of a nil trade cache and nil repository, carrying
// every search with four unpriced feeders.
func TestKeepHappyFontAnalysis_WarmBodyIsPinned(t *testing.T) {
	at := time.Date(2026, 11, 19, 12, 0, 0, 0, time.UTC)
	c := lab.NewCache(corpusScope)
	c.For(corpusScope).SetFont(lab.FontAnalysis{
		Safe: []lab.FontResult{
			{Time: at, Color: "RED", Variant: "20/20", Pool: 9, PricedGems: 8, Winners: 2, PWin: 0.5, EV: 3, InputCost: 1, Profit: 2, FontsToHit: 2, LiquidityRisk: "LOW"},
			{Time: at, Color: "BLUE", Variant: "20/20", Pool: 7, PricedGems: 7, Winners: 1, PWin: 0.25, EV: 9, InputCost: 1, Profit: 8, FontsToHit: 4, LiquidityRisk: "HIGH"},
			{Time: at, Color: "GREEN", Variant: "1", Pool: 5, Profit: 99, LiquidityRisk: "LOW"},
		},
		Jackpot: []lab.FontResult{{Time: at, Color: "GREEN", Variant: "20/20", Pool: 3, EV: 1, Profit: 1, ThinPoolGems: 1, LiquidityRisk: "MEDIUM"}},
	})
	w := serveWithoutRepository(t, FontAnalysis(nil, c, nil, corpusScope), "/api/analysis/font?variant=20/20&limit=1")
	var got map[string]any
	if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
		t.Fatalf("decode: %v", err)
	}
	wantLens := unpricedLensBody(t, got)
	var want map[string]any
	if err := json.Unmarshal([]byte(`{
		"safe": [{"time":"2026-11-19T12:00:00Z","color":"BLUE","variant":"20/20","pool":7,"pricedGems":7,"winners":1,"pWin":0.25,"avgWin":0,"avgWinRaw":0,"ev":9,"evRaw":0,"inputCost":1,"profit":8,"fontsToHit":4,"thinPoolGems":0,"liquidityRisk":"HIGH"}],
		"premium": [],
		"jackpot": [{"time":"2026-11-19T12:00:00Z","color":"GREEN","variant":"20/20","pool":3,"pricedGems":0,"winners":0,"pWin":0,"avgWin":0,"avgWinRaw":0,"ev":1,"evRaw":0,"inputCost":0,"profit":1,"fontsToHit":0,"thinPoolGems":1,"liquidityRisk":"MEDIUM"}],
		"bestColorSafe": "BLUE", "bestColorPremium": "", "bestColorJackpot": "GREEN"}`), &want); err != nil {
		t.Fatal(err)
	}
	want["lens"] = wantLens
	if !reflect.DeepEqual(got, want) {
		t.Errorf("body = %v\nwant %v", got, want)
	}
}

// --- POE-474: four Lens feeders, the cheapest EV and their trade searches ---

// lensCraftWithFeeders is the seam the Lens craft is built through: it reads
// the level-3 exceptional feeder snapshot from feeders (FontAnalysis passes
// its retained lab.ExceptionalSupportCache, or a zero snapshot without a
// repository) and returns the lens object the handler encodes.
type lensCraftWithFeeders = func(context.Context, *trade.TradeCache, exceptionalSupportPricer, league.Scope, time.Time) (lensCraftResponse, error)

func lensCraftSeam(t *testing.T) lensCraftWithFeeders {
	t.Helper()
	craft, ok := any(lensCraft).(lensCraftWithFeeders)
	if !ok {
		var want lensCraftWithFeeders
		t.Fatalf("lensCraft is %T, want %T: the Lens craft does not read the level-3 exceptional feeders", lensCraft, want)
	}
	return craft
}

type lensChoiceWire struct {
	Source    string     `json:"source"`
	Name      string     `json:"name"`
	Chaos     *float64   `json:"chaos"`
	FetchedAt *time.Time `json:"fetchedAt"`
	TradeURL  string     `json:"tradeUrl"`
}

type lensChoicesWire struct {
	raw     map[string]json.RawMessage
	Outcome struct {
		Chaos    *float64 `json:"chaos"`
		TradeURL string   `json:"tradeUrl"`
	} `json:"outcome"`
	Feeder     lensFeederWire   `json:"feeder"`
	EV         *float64         `json:"ev"`
	Feeders    []lensChoiceWire `json:"feeders"`
	Cheapest   *int             `json:"cheapest"`
	EVCheapest *float64         `json:"evCheapest"`
}

func decodeLensChoices(t *testing.T, body []byte) lensChoicesWire {
	t.Helper()
	var lens lensChoicesWire
	if err := json.Unmarshal(body, &lens); err != nil {
		t.Fatalf("decode lens: %v", err)
	}
	if err := json.Unmarshal(body, &lens.raw); err != nil {
		t.Fatalf("decode lens fields: %v", err)
	}
	return lens
}

// serveLensChoices serves the Font analysis with a nil repository, so the
// exceptional feeders carry no price.
func serveLensChoices(t *testing.T, tc *trade.TradeCache) lensChoicesWire {
	t.Helper()
	w := serveWithoutRepository(t, FontAnalysis(nil, warmEmptyCorpora(t), tc, corpusScope), "/api/analysis/font")
	if w.Code != 200 {
		t.Fatalf("status = %d, body %s", w.Code, w.Body.String())
	}
	var body struct {
		Lens json.RawMessage `json:"lens"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode: %v", err)
	}
	return decodeLensChoices(t, body.Lens)
}

// craftLensChoices builds the lens object through the seam and decodes the
// JSON the handler would write.
func craftLensChoices(t *testing.T, tc *trade.TradeCache, feeders exceptionalSupportPricer) (lensChoicesWire, error) {
	t.Helper()
	resp, err := lensCraftSeam(t)(context.Background(), tc, feeders, corpusScope, time.Now())
	if err != nil {
		return lensChoicesWire{}, err
	}
	body, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("encode lens: %v", err)
	}
	return decodeLensChoices(t, body), nil
}

// assertRawField pins a field's exact JSON, so null is told apart from absent.
func assertRawField(t *testing.T, lens lensChoicesWire, key, want string) {
	t.Helper()
	got, ok := lens.raw[key]
	if !ok {
		t.Errorf("lens has no %q field, want %s", key, want)
		return
	}
	if string(got) != want {
		t.Errorf("lens %s = %s, want %s", key, got, want)
	}
}

var lensFeederAsOf = time.Date(2026, 11, 24, 15, 30, 0, 0, time.UTC)

// lensFeederSnapshot is WI-1's level-3 snapshot: Enhance 12, Enlighten 20,
// Empower 7; a negative price leaves that gem out.
func lensFeederSnapshot(enhance, enlighten, empower float64) lab.ExceptionalSupportSnapshot {
	chaos := map[string]float64{}
	for name, p := range map[string]float64{"Enhance Support": enhance, "Enlighten Support": enlighten, "Empower Support": empower} {
		if p >= 0 {
			chaos[name] = p
		}
	}
	return lab.ExceptionalSupportSnapshot{AsOf: lensFeederAsOf, Chaos: chaos}
}

var lensFeederOrder = [4]struct{ source, name string }{
	{"trade", "Exceptional Support Gem lv 3+"},
	{"poe.ninja", "Enhance Support"},
	{"poe.ninja", "Enlighten Support"},
	{"poe.ninja", "Empower Support"},
}

func chaosText(p *float64) string {
	if p == nil {
		return "null"
	}
	return fmt.Sprint(*p)
}

// keep:critical silent: a wrong cheapest index or EV is a plausible Lens EV
// the Lens card shows and the player buys on.
func TestKeepCriticalFontAnalysis_LensCheapestFeederPricesTheEV(t *testing.T) {
	cases := []struct {
		name           string
		lens, support  []float64
		snap           lab.ExceptionalSupportSnapshot
		wantChaos      [4]string
		wantCheapest   string
		wantEVCheapest string
	}{
		{"an Empower below the trade feeder is cheapest", []float64{400, 412, 450}, []float64{29, 30, 31}, lensFeederSnapshot(12, 20, 7),
			[4]string{"30", "12", "20", "7"}, "3", "405"},
		{"a trade feeder below every exceptional gem is cheapest", []float64{400, 412, 450}, []float64{4, 5, 6}, lensFeederSnapshot(12, 20, 7),
			[4]string{"5", "12", "20", "7"}, "0", "407"},
		{"a tie goes to the first feeder in fixed order", []float64{400, 412, 450}, []float64{6, 7, 8}, lensFeederSnapshot(12, 20, 7),
			[4]string{"7", "12", "20", "7"}, "0", "405"},
		{"no priced feeder has no cheapest", []float64{400, 412, 450}, nil, lab.ExceptionalSupportSnapshot{},
			[4]string{"null", "null", "null", "null"}, "null", "null"},
		{"an unpriced lens keeps the cheapest without an EV", nil, []float64{29, 30, 31}, lensFeederSnapshot(12, 20, 7),
			[4]string{"30", "12", "20", "7"}, "3", "null"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			lens, err := craftLensChoices(t, pricedLensCache(t, tc.lens, tc.support), &fakeExceptionalPricer{snap: tc.snap})
			if err != nil {
				t.Fatalf("lens craft: %v", err)
			}

			assertRawField(t, lens, "cheapest", tc.wantCheapest)
			assertRawField(t, lens, "evCheapest", tc.wantEVCheapest)
			if len(lens.Feeders) != 4 {
				t.Fatalf("feeders = %+v, want 4 entries", lens.Feeders)
			}
			for i, f := range lens.Feeders {
				if f.Source != lensFeederOrder[i].source || f.Name != lensFeederOrder[i].name || chaosText(f.Chaos) != tc.wantChaos[i] {
					t.Errorf("feeders[%d] = %s %q %s, want %s %q %s", i, f.Source, f.Name, chaosText(f.Chaos),
						lensFeederOrder[i].source, lensFeederOrder[i].name, tc.wantChaos[i])
				}
				if f.TradeURL == "" {
					t.Errorf("feeders[%d] tradeUrl is empty, want its search", i)
				}
				if i > 0 && f.Chaos != nil && (f.FetchedAt == nil || !f.FetchedAt.Equal(lensFeederAsOf)) {
					t.Errorf("feeders[%d] fetchedAt = %v, want the snapshot time %v", i, f.FetchedAt, lensFeederAsOf)
				}
			}
			if lens.Feeders[0].Chaos != nil && (lens.Feeders[0].FetchedAt == nil || lens.Feeder.FetchedAt == nil || !lens.Feeders[0].FetchedAt.Equal(*lens.Feeder.FetchedAt)) {
				t.Errorf("trade feeder fetchedAt = %v, want the trade quote's %v", lens.Feeders[0].FetchedAt, lens.Feeder.FetchedAt)
			}
			if lens.Outcome.TradeURL == "" {
				t.Error("outcome tradeUrl is empty, want the lens search")
			}
		})
	}
}

type tradeSearch struct {
	path  string
	query map[string]any
	sort  map[string]any
}

func decodeTradeSearch(t *testing.T, raw string) tradeSearch {
	t.Helper()
	u, err := url.Parse(raw)
	if err != nil || u.Scheme != "https" || u.Host != "www.pathofexile.com" {
		t.Fatalf("tradeUrl = %q, want a pathofexile.com trade search", raw)
	}
	var body struct {
		Query map[string]any `json:"query"`
		Sort  map[string]any `json:"sort"`
	}
	if err := json.Unmarshal([]byte(u.Query().Get("q")), &body); err != nil {
		t.Fatalf("tradeUrl %q: decode q: %v", raw, err)
	}
	return tradeSearch{path: u.Path, query: body.Query, sort: body.Sort}
}

// jsonPath walks nested JSON objects; nil when any step is missing.
func jsonPath(v any, keys ...string) any {
	for _, k := range keys {
		m, ok := v.(map[string]any)
		if !ok {
			return nil
		}
		v = m[k]
	}
	return v
}

func containsKey(v any, key string) bool {
	switch x := v.(type) {
	case map[string]any:
		for k, child := range x {
			if k == key || containsKey(child, key) {
				return true
			}
		}
	case []any:
		for _, child := range x {
			if containsKey(child, key) {
				return true
			}
		}
	}
	return false
}

// keep:critical irreversible: a wrong filter opens a search that buys the
// wrong gem, level, corruption or league with real currency.
func TestKeepCriticalFontAnalysis_ExceptionalFeederSearchesBuyTheLevel3GemInTheScopeLeague(t *testing.T) {
	lens := serveLensChoices(t, nil)
	if len(lens.Feeders) != 4 {
		t.Fatalf("feeders = %+v, want 4 entries", lens.Feeders)
	}
	for i, name := range lab.ExceptionalSupportNames {
		t.Run(name, func(t *testing.T) {
			q := decodeTradeSearch(t, lens.Feeders[i+1].TradeURL)

			if q.path != "/trade/search/"+corpusScope.ID() {
				t.Errorf("path = %q, want the %s search", q.path, corpusScope.ID())
			}
			if q.query["type"] != name {
				t.Errorf("type = %v, want %q", q.query["type"], name)
			}
			level := jsonPath(q.query, "filters", "misc_filters", "filters", "gem_level")
			if jsonPath(level, "min") != float64(3) || jsonPath(level, "max") != float64(3) {
				t.Errorf("gem_level = %v, want min 3 max 3", level)
			}
			if containsKey(q.query, "corrupted") {
				t.Errorf("query = %v, want no corrupted filter (any corruption)", q.query)
			}
			if got := jsonPath(q.query, "status", "option"); got != "securable" {
				t.Errorf("status = %v, want securable", got)
			}
			if got := jsonPath(q.query, "filters", "trade_filters", "filters", "sale_type", "option"); got != "priced" {
				t.Errorf("sale_type = %v, want priced", got)
			}
			if q.sort["price"] != "asc" {
				t.Errorf("sort = %v, want price asc", q.sort)
			}
		})
	}
}

// keep:regression: an older desktop reads feeder and ev, so they stay the
// trade quote and its EV even when an exceptional gem is cheaper.
func TestKeepRegressionFontAnalysis_LegacyFeederAndEVStayOnTheTradeQuote(t *testing.T) {
	lens, err := craftLensChoices(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{90, 95, 99}), &fakeExceptionalPricer{snap: lensFeederSnapshot(12, 20, 7)})
	if err != nil {
		t.Fatalf("lens craft: %v", err)
	}

	if lens.Feeder.Chaos == nil || *lens.Feeder.Chaos != 95 {
		t.Errorf("feeder chaos = %s, want the trade quote 95", chaosText(lens.Feeder.Chaos))
	}
	if lens.EV == nil || *lens.EV != 317 {
		t.Errorf("ev = %s, want lens - trade feeder = 317", chaosText(lens.EV))
	}
	support, _ := trade.LookupFixedTarget(trade.FixedTargetLens60Feeder, trade.FixedTargetVariantFont)
	if lens.Feeder.TradeURL != support.TradeURL(corpusScope.ID()) {
		t.Errorf("feeder tradeUrl = %q, want the registry support search", lens.Feeder.TradeURL)
	}
	if len(lens.Feeders) == 0 || lens.Feeders[0].TradeURL != support.TradeURL(corpusScope.ID()) {
		t.Errorf("feeders = %+v, want the trade feeder first with the registry support search", lens.Feeders)
	}
}

// lensFeederSource serves the snapshot it holds and counts its reads.
type lensFeederSource struct {
	mu    sync.Mutex
	snap  lab.ExceptionalSupportSnapshot
	reads int
}

func (s *lensFeederSource) read(context.Context, league.Scope) (lab.ExceptionalSupportSnapshot, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.reads++
	return s.snap, nil
}

func (s *lensFeederSource) set(snap lab.ExceptionalSupportSnapshot) {
	s.mu.Lock()
	s.snap = snap
	s.mu.Unlock()
}

func (s *lensFeederSource) readCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.reads
}

func TestKeepHappyFontAnalysis_RetainedLensFeederSnapshotFollowsTheGemGeneration(t *testing.T) {
	// retained polls once through a retained cache at Empower 7, then swaps
	// the source to Enhance 6.
	retained := func(t *testing.T) (*lab.Cache, *lab.ExceptionalSupportCache, *lensFeederSource) {
		src := &lensFeederSource{snap: lensFeederSnapshot(12, 20, 7)}
		labCache := lab.NewCache(corpusScope)
		feeders := lab.NewExceptionalSupportCache(src.read, labCache)
		if _, err := craftLensChoices(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{29, 30, 31}), feeders); err != nil {
			t.Fatalf("first poll: %v", err)
		}
		src.set(lensFeederSnapshot(6, 20, 7))
		return labCache, feeders, src
	}
	poll := func(t *testing.T, feeders *lab.ExceptionalSupportCache) lensChoicesWire {
		lens, err := craftLensChoices(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{29, 30, 31}), feeders)
		if err != nil {
			t.Fatalf("poll: %v", err)
		}
		return lens
	}

	t.Run("a repeat poll serves the retained snapshot's cheapest", func(t *testing.T) {
		_, feeders, src := retained(t)

		lens := poll(t, feeders)
		assertRawField(t, lens, "cheapest", "3")
		if n := src.readCount(); n != 1 {
			t.Errorf("snapshot reads = %d, want 1", n)
		}
	})

	t.Run("an accepted gem snapshot serves the new cheapest", func(t *testing.T) {
		labCache, feeders, _ := retained(t)
		labCache.Rankings().Publish(lab.RankingInputGemSnapshots)

		lens := poll(t, feeders)
		assertRawField(t, lens, "cheapest", "1")
		assertRawField(t, lens, "evCheapest", "406")
	})

	t.Run("a failed snapshot read fails the lens craft", func(t *testing.T) {
		failing := lab.NewExceptionalSupportCache(func(context.Context, league.Scope) (lab.ExceptionalSupportSnapshot, error) {
			return lab.ExceptionalSupportSnapshot{}, errors.New("gem_snapshots unavailable")
		}, lab.NewCache(corpusScope))

		if _, err := craftLensChoices(t, pricedLensCache(t, []float64{400, 412, 450}, []float64{29, 30, 31}), failing); err == nil {
			t.Error("lens craft error = nil, want the snapshot read failure")
		}
	})
}

// unpricedLensBody is the lens object of a nil trade cache and nil
// repository: registry searches for the lens and the trade feeder, four
// unpriced feeders and no cheapest. The exceptional searches are copied from
// got; their filters are pinned by the search test.
func unpricedLensBody(t *testing.T, got map[string]any) any {
	t.Helper()
	outcome, _ := trade.LookupFixedTarget(trade.FixedTargetLens60Outcome, trade.FixedTargetVariantFont)
	support, _ := trade.LookupFixedTarget(trade.FixedTargetLens60Feeder, trade.FixedTargetVariantFont)
	quote := func(tradeURL string) map[string]any {
		return map[string]any{"chaos": nil, "listings": float64(0), "fetchedAt": nil, "lowConfidence": false, "tradeUrl": tradeURL}
	}
	feeders := []any{map[string]any{"source": "trade", "name": "Exceptional Support Gem lv 3+", "chaos": nil, "fetchedAt": nil, "tradeUrl": support.TradeURL(corpusScope.ID())}}
	for i, name := range lab.ExceptionalSupportNames {
		served, _ := jsonPath(got, "lens", "feeders").([]any)
		search := ""
		if i+1 < len(served) {
			search, _ = jsonPath(served[i+1], "tradeUrl").(string)
		}
		feeders = append(feeders, map[string]any{"source": "poe.ninja", "name": name, "chaos": nil, "fetchedAt": nil, "tradeUrl": search})
	}
	return map[string]any{
		"outcome":    quote(outcome.TradeURL(corpusScope.ID())),
		"feeder":     quote(support.TradeURL(corpusScope.ID())),
		"ev":         nil,
		"feeders":    feeders,
		"cheapest":   nil,
		"evCheapest": nil,
	}
}
