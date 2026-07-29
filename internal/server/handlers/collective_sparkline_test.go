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

// The sparkline handlers are constructed with a nil *lab.Repository throughout
// this file. A nil repository is not an error value: every query method
// dereferences r.pool, so a surviving database call panics. That panic is the
// observable this file asserts on — "served without touching gem_snapshots" is
// the acceptance criterion for the sparkline cache, and a nil repository is the
// only seam the current handler signatures offer to observe it.

var sparklineScope = league.Historical("Mirage")

// sparkAt builds one point ago before now, so cached series land inside the
// 12-hour response window without depending on wall-clock alignment.
func sparkAt(now time.Time, ago time.Duration, price float64, listings int) lab.SparklinePoint {
	return lab.SparklinePoint{
		Time:     now.Add(-ago).UTC().Format(time.RFC3339),
		Price:    price,
		Listings: listings,
	}
}

// sparklineGem is one gem's worth of ranking input: enough transfigure, signal
// and feature data for RankCollective to emit a row for it.
type sparklineGem struct {
	name         string
	variant      string
	roi          float64
	windowSignal string
}

// warmAnalysisCache returns a cache warm enough that CollectiveAnalysis,
// CompareAnalysis and TrendAnalysis all take their cache-first path for
// rankings. Sparklines are populated separately so a test can hold the ranking
// data warm while leaving the sparkline cache cold.
func warmAnalysisCache(t *testing.T, gems ...sparklineGem) *lab.Cache {
	t.Helper()
	c := lab.NewCache(sparklineScope)
	now := time.Now()

	transfigure := make([]lab.TransfigureResult, 0, len(gems))
	signals := make([]lab.GemSignal, 0, len(gems))
	features := make([]lab.GemFeature, 0, len(gems))
	for _, g := range gems {
		transfigure = append(transfigure, lab.TransfigureResult{
			Time:                 now,
			BaseName:             "Spark",
			TransfiguredName:     g.name,
			Variant:              g.variant,
			BasePrice:            10,
			TransfiguredPrice:    10 + g.roi,
			ROI:                  g.roi,
			ROIPct:               g.roi * 10,
			BaseListings:         25,
			TransfiguredListings: 25,
			GemColor:             "BLUE",
			Confidence:           "OK",
		})
		signals = append(signals, lab.GemSignal{
			Time:             now,
			Name:             g.name,
			Variant:          g.variant,
			Signal:           "STABLE",
			Confidence:       80,
			Sellability:      70,
			SellabilityLabel: "GOOD",
			WindowSignal:     g.windowSignal,
			Tier:             "MID",
		})
		features = append(features, lab.GemFeature{
			Time:     now,
			Name:     g.name,
			Variant:  g.variant,
			Chaos:    10 + g.roi,
			Listings: 25,
			GemColor: "BLUE",
		})
	}

	c.For(sparklineScope).SetTransfigure(transfigure)
	c.For(sparklineScope).SetGemSignals(signals)
	c.For(sparklineScope).SetGemFeatures(features)
	return c
}

// warmSparklines installs series into the cache keyed by gem name and variant.
func warmSparklines(t *testing.T, c *lab.Cache, series, corrupted map[string]map[string][]lab.SparklinePoint) {
	t.Helper()
	c.For(sparklineScope).SetSparklinesByName(series, corrupted, time.Now())
}

// serveWithoutRepository serves target and fails the test if the handler
// dereferenced the nil repository, naming that as a database call.
func serveWithoutRepository(t *testing.T, h http.HandlerFunc, target string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(http.MethodGet, target, nil)
	w := httptest.NewRecorder()
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("handler queried the repository while serving %s (nil repository dereferenced: %v)", target, r)
		}
	}()
	h.ServeHTTP(w, req)
	return w
}

// queriedRepository reports whether serving target reached the repository.
func queriedRepository(h http.HandlerFunc, target string) (queried bool) {
	defer func() {
		if recover() != nil {
			queried = true
		}
	}()
	h.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, target, nil))
	return false
}

type sparklineRow struct {
	TransfiguredName string               `json:"transfiguredName"`
	Variant          string               `json:"variant"`
	Sparkline        []lab.SparklinePoint `json:"sparkline"`
}

// decodeSparklineRows decodes a collective or compare response into rows keyed
// by gem name.
func decodeSparklineRows(t *testing.T, w *httptest.ResponseRecorder) map[string]sparklineRow {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var resp struct {
		Count int            `json:"count"`
		Data  []sparklineRow `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	out := make(map[string]sparklineRow, len(resp.Data))
	for _, row := range resp.Data {
		out[row.TransfiguredName] = row
	}
	return out
}

func assertSparkline(t *testing.T, got, want []lab.SparklinePoint) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("sparkline has %d points, want %d: %+v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("sparkline[%d] = %+v, want %+v", i, got[i], want[i])
		}
	}
}

// This is the acceptance criterion for POE-133: a warm cache serves the
// sparkline series with no gem_snapshots query. Routing any one call site back
// through the repository makes it fail on the nil-pool panic.
func TestCollectiveAnalysis_WarmCacheServesSparklinesWithoutQuerying(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	want := []lab.SparklinePoint{
		sparkAt(now, 4*time.Hour, 100, 30),
		sparkAt(now, 2*time.Hour, 110, 28),
		sparkAt(now, time.Hour, 125, 26),
	}
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": want},
	}, nil)

	w := serveWithoutRepository(t, CollectiveAnalysis(nil, cache, sparklineScope),
		"/api/analysis/collective?variant=20/20")

	rows := decodeSparklineRows(t, w)
	row, ok := rows["Spark of Nova"]
	if !ok {
		t.Fatalf("response has no row for Spark of Nova: %v", rows)
	}
	assertSparkline(t, row.Sparkline, want)
}

// A gem the warm cache has no series for is a gem with no recent points, not a
// cache miss: it gets an empty series and still no query.
func TestCollectiveAnalysis_WarmCacheGemWithoutSeriesServesEmptySparkline(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t,
		sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40},
		sparklineGem{name: "Ice Nova of Frostbolts", variant: "20/20", roi: 30},
	)
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {sparkAt(now, time.Hour, 125, 26)}},
	}, nil)

	w := serveWithoutRepository(t, CollectiveAnalysis(nil, cache, sparklineScope),
		"/api/analysis/collective?variant=20/20")

	rows := decodeSparklineRows(t, w)
	row, ok := rows["Ice Nova of Frostbolts"]
	if !ok {
		t.Fatalf("response has no row for Ice Nova of Frostbolts: %v", rows)
	}
	if row.Sparkline == nil {
		t.Fatal("sparkline = null, want an empty array")
	}
	if len(row.Sparkline) != 0 {
		t.Errorf("sparkline = %+v, want empty", row.Sparkline)
	}
}

// The cache holds a longer series than a response shows — up to a 168-hour tail
// for a gem that stopped appearing in snapshots. The 12-hour window is applied
// when the response is built.
func TestCollectiveAnalysis_WarmCacheDropsPointsOlderThanTheResponseWindow(t *testing.T) {
	now := time.Now()
	recent := sparkAt(now, time.Hour, 125, 26)
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {
			sparkAt(now, 100*time.Hour, 60, 40),
			sparkAt(now, 30*time.Hour, 80, 35),
			recent,
		}},
	}, nil)

	w := serveWithoutRepository(t, CollectiveAnalysis(nil, cache, sparklineScope),
		"/api/analysis/collective?variant=20/20")

	rows := decodeSparklineRows(t, w)
	assertSparkline(t, rows["Spark of Nova"].Sparkline, []lab.SparklinePoint{recent})
}

// Without a variant parameter each gem's series must come from its own variant.
// 20/20 and 1/20 are different markets: reading both rows at one variant, or at
// the empty request variant, serves the wrong prices.
//
// The two gems carry different names because the response map is keyed by gem
// name alone — see the note on the response-shape limitation at the bottom of
// this file.
func TestCollectiveAnalysis_AllVariantsWarmCacheUsesEachGemsOwnVariant(t *testing.T) {
	now := time.Now()
	twentyTwenty := []lab.SparklinePoint{sparkAt(now, time.Hour, 125, 26)}
	oneTwenty := []lab.SparklinePoint{sparkAt(now, time.Hour, 9, 90)}
	cache := warmAnalysisCache(t,
		sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40},
		sparklineGem{name: "Ice Nova of Frostbolts", variant: "1/20", roi: 30},
	)
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova":          {"20/20": twentyTwenty, "1/20": oneTwenty},
		"Ice Nova of Frostbolts": {"20/20": twentyTwenty, "1/20": oneTwenty},
	}, nil)

	w := serveWithoutRepository(t, CollectiveAnalysis(nil, cache, sparklineScope),
		"/api/analysis/collective")

	rows := decodeSparklineRows(t, w)
	if rows["Spark of Nova"].Variant != "20/20" || rows["Ice Nova of Frostbolts"].Variant != "1/20" {
		t.Fatalf("unexpected variants in response: %+v", rows)
	}
	assertSparkline(t, rows["Spark of Nova"].Sparkline, twentyTwenty)
	assertSparkline(t, rows["Ice Nova of Frostbolts"].Sparkline, oneTwenty)
}

// A cold sparkline cache must still reach the database, even when the rest of
// the cache is warm — otherwise a restarted server serves empty sparklines
// until the first tick lands.
func TestCollectiveAnalysis_ColdSparklineCacheFallsBackToRepository(t *testing.T) {
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	// Rankings warm, sparklines never populated.

	if !queriedRepository(CollectiveAnalysis(nil, cache, sparklineScope),
		"/api/analysis/collective?variant=20/20") {
		t.Error("cold sparkline cache served without a repository query, want the database fallback")
	}
}

func TestCompareAnalysis_WarmCacheServesSparklinesWithoutQuerying(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	want := []lab.SparklinePoint{
		sparkAt(now, 3*time.Hour, 100, 30),
		sparkAt(now, time.Hour, 125, 26),
	}
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": want},
	}, nil)

	w := serveWithoutRepository(t, CompareAnalysis(nil, cache, nil, sparklineScope),
		"/api/analysis/compare?gems=Spark+of+Nova&variant=20/20")

	rows := decodeSparklineRows(t, w)
	row, ok := rows["Spark of Nova"]
	if !ok {
		t.Fatalf("response has no row for Spark of Nova: %v", rows)
	}
	assertSparkline(t, row.Sparkline, want)
}

// The cache keys a series by name AND variant, so it cannot reproduce the
// mixed-variant series an empty variant returns. That request stays on the
// query even against a warm cache.
func TestCompareAnalysis_EmptyVariantQueriesRepositoryDespiteWarmCache(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t, sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40})
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {sparkAt(now, time.Hour, 125, 26)}},
	}, nil)

	if !queriedRepository(CompareAnalysis(nil, cache, nil, sparklineScope),
		"/api/analysis/compare?gems=Spark+of+Nova") {
		t.Error("empty variant served from cache, want the mixed-variant query")
	}
}

// TrendAnalysis shows the last four points of the full cached series; it is the
// one consumer whose window is 168 hours rather than 12, so the cached series
// must not be trimmed to the shorter window before it is cut to four.
func TestTrendAnalysis_WarmCacheServesLastFourPointsWithoutQuerying(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t, sparklineGem{
		name: "Spark of Nova", variant: "20/20", roi: 40, windowSignal: "OPEN",
	})
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {
			sparkAt(now, 120*time.Hour, 50, 60),
			sparkAt(now, 96*time.Hour, 60, 55),
			sparkAt(now, 72*time.Hour, 70, 50),
			sparkAt(now, 48*time.Hour, 80, 45),
			sparkAt(now, 24*time.Hour, 90, 40),
			sparkAt(now, time.Hour, 125, 26),
		}},
	}, nil)

	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var resp struct {
		Data []struct {
			Name          string `json:"name"`
			PriceTrend    []int  `json:"priceTrend"`
			ListingsTrend []int  `json:"listingsTrend"`
		} `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&resp); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if len(resp.Data) != 1 {
		t.Fatalf("response has %d rows, want 1", len(resp.Data))
	}
	got := resp.Data[0]
	wantPrices := []int{70, 80, 90, 125}
	wantListings := []int{50, 45, 40, 26}
	if len(got.PriceTrend) != len(wantPrices) {
		t.Fatalf("priceTrend = %v, want %v", got.PriceTrend, wantPrices)
	}
	for i := range wantPrices {
		if got.PriceTrend[i] != wantPrices[i] {
			t.Errorf("priceTrend[%d] = %d, want %d", i, got.PriceTrend[i], wantPrices[i])
		}
	}
	for i := range wantListings {
		if got.ListingsTrend[i] != wantListings[i] {
			t.Errorf("listingsTrend[%d] = %d, want %d", i, got.ListingsTrend[i], wantListings[i])
		}
	}
}

func TestTrimSparkline(t *testing.T) {
	now := time.Now()
	old := sparkAt(now, 20*time.Hour, 60, 40)
	recent := sparkAt(now, time.Hour, 125, 26)

	tests := []struct {
		name  string
		pts   []lab.SparklinePoint
		hours int
		want  []lab.SparklinePoint
	}{
		{"keeps points inside the window", []lab.SparklinePoint{old, recent}, 12, []lab.SparklinePoint{recent}},
		{"drops everything older than the window", []lab.SparklinePoint{old}, 12, nil},
		{"no window keeps the whole series", []lab.SparklinePoint{old, recent}, 0, []lab.SparklinePoint{old, recent}},
		{"unparseable times are skipped, not kept", []lab.SparklinePoint{{Time: "not-a-time", Price: 1}, recent}, 12, []lab.SparklinePoint{recent}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := trimSparkline(tt.pts, tt.hours)
			if len(got) != len(tt.want) {
				t.Fatalf("got %d points %+v, want %d", len(got), got, len(tt.want))
			}
			for i := range tt.want {
				if got[i] != tt.want[i] {
					t.Errorf("point[%d] = %+v, want %+v", i, got[i], tt.want[i])
				}
			}
		})
	}
}

// serveDedicationCompare queries corrupted gem prices before it ever reaches a
// sparkline, so its cache read is covered here at the function it delegates to.
func TestCachedCorruptedSparklines_DedicationVariantReadsTheCorruptedCache(t *testing.T) {
	now := time.Now()
	want := []lab.SparklinePoint{sparkAt(now, time.Hour, 900, 5)}
	cache := lab.NewCache(sparklineScope)
	warmSparklines(t, cache, nil, map[string]map[string][]lab.SparklinePoint{
		"Vaal Grace": {"21/23c": want},
	})

	got, cached := cachedCorruptedSparklines(cache, sparklineScope, []string{"Vaal Grace"}, "21/23c", sparklineWindowHours)
	if !cached {
		t.Fatal("cached = false, want the warm corrupted cache to serve the request")
	}
	assertSparkline(t, got["Vaal Grace"], want)
}

func TestCachedCorruptedSparklines_NonDedicationVariantDefersToTheQuery(t *testing.T) {
	now := time.Now()
	cache := lab.NewCache(sparklineScope)
	warmSparklines(t, cache, nil, map[string]map[string][]lab.SparklinePoint{
		"Vaal Grace": {"21/23c": {sparkAt(now, time.Hour, 900, 5)}},
	})

	// The cache populates corrupted series for the Dedication variant only, so
	// any other variant must reach the database rather than read an empty
	// series out of a warm cache.
	if _, cached := cachedCorruptedSparklines(cache, sparklineScope, []string{"Vaal Grace"}, "20/20", sparklineWindowHours); cached {
		t.Error("cached = true for variant 20/20, want the query")
	}
}

// The corrupted map is the one this call reads. A cache warmed only on the
// non-corrupted side is cold for this request, and reporting it warm would hand
// back an empty series for every Dedication gem with no fallback and no log.
func TestCachedCorruptedSparklines_NonCorruptedOnlyCacheDefersToTheQuery(t *testing.T) {
	now := time.Now()
	cache := lab.NewCache(sparklineScope)
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {sparkAt(now, time.Hour, 125, 26)}},
	}, nil)

	if _, cached := cachedCorruptedSparklines(cache, sparklineScope, []string{"Vaal Grace"}, "21/23c", sparklineWindowHours); cached {
		t.Error("cached = true with only the non-corrupted map populated, want the query")
	}
}

func TestCachedCorruptedSparklines_ColdCacheDefersToTheQuery(t *testing.T) {
	cache := lab.NewCache(sparklineScope)

	if _, cached := cachedCorruptedSparklines(cache, sparklineScope, []string{"Vaal Grace"}, "21/23c", sparklineWindowHours); cached {
		t.Error("cached = true on a cold cache, want the query")
	}
}

func TestCachedCorruptedSparklines_NilCacheDefersToTheQuery(t *testing.T) {
	if _, cached := cachedCorruptedSparklines(nil, sparklineScope, []string{"Vaal Grace"}, "21/23c", sparklineWindowHours); cached {
		t.Error("cached = true with no cache, want the query")
	}
}

// collectiveRow.Sparkline carries no omitempty, so a row left with a nil series
// marshals as `null` while a populated one marshals as an array. Both collective
// modes share this row type, and the Dedication mode never populates the field —
// leaving it unset there makes the two endpoints disagree on response shape.
func TestCollectiveRow_SparklineWithoutPointsMarshalsAsEmptyArray(t *testing.T) {
	encoded, err := json.Marshal(collectiveRow{Sparkline: nonNilSparkline(nil)})
	if err != nil {
		t.Fatalf("marshal collectiveRow: %v", err)
	}

	var fields map[string]json.RawMessage
	if err := json.Unmarshal(encoded, &fields); err != nil {
		t.Fatalf("unmarshal collectiveRow: %v", err)
	}
	got, ok := fields["sparkline"]
	if !ok {
		t.Fatalf("response has no sparkline field: %s", encoded)
	}
	if string(got) != "[]" {
		t.Errorf("sparkline = %s, want []", got)
	}
}

// Response-shape limitation, pre-existing and not introduced by the sparkline
// cache: CollectiveAnalysis collects series into a map keyed by gem name alone,
// then reads it back per row. When one gem appears at two variants in the same
// response — the default "all variants" request — the second lookup overwrites
// the first and both rows render the same series. The cache path and the query
// path share the defect, because both fill the same name-keyed map. Fixing it
// means keying that map by name and variant; no test here asserts the current
// behaviour, so a fix will not have to break one.

// Warmth is per variant, not per corpus. Once the Dedication pool became
// selectable, a cache holding only 21/23c series would otherwise report itself
// able to serve a 21/20c request and hand back empty series with no query and
// no warning — the same silent-empty-market failure the 20/20 case above exists
// to prevent, one level down.
func TestCachedCorruptedSparklines_UnpopulatedDedicationVariantDefersToTheQuery(t *testing.T) {
	now := time.Now()
	cache := lab.NewCache(sparklineScope)
	warmSparklines(t, cache, nil, map[string]map[string][]lab.SparklinePoint{
		"Vaal Grace": {"21/23c": {sparkAt(now, time.Hour, 900, 5)}},
	})

	if _, cached := cachedCorruptedSparklines(cache, sparklineScope, []string{"Vaal Grace"}, "21/20c", sparklineWindowHours); cached {
		t.Error("cached = true for 21/20c against a cache warmed only on 21/23c, want the query")
	}
}
