package handlers

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
)

func TestAssembleTrendRows_EnrichesSignalWithMatchingFeatureAndTrend(t *testing.T) {
	when := time.Date(2026, time.September, 24, 12, 34, 56, 0, time.UTC)
	query := trendQueryResult{
		filtered: []trendSignalWithFeature{{
			signal: &lab.GemSignal{
				Time:             when,
				Name:             "Spark of Nova",
				Variant:          "20/20",
				Signal:           "DEMAND",
				WindowSignal:     "OPEN",
				AdvancedSignal:   "SURGE",
				Tier:             "TOP",
				SellUrgency:      "NOW",
				SellReason:       "buyers active",
				Sellability:      91,
				SellabilityLabel: "SAFE",
			},
			feature: &lab.GemFeature{
				Name:             "Spark of Nova",
				Variant:          "20/20",
				Chaos:            123.5,
				Listings:         17,
				GemColor:         "BLUE",
				VelLongPrice:     2.5,
				VelLongListing:   -1.25,
				CV:               0.18,
				HistPosition:     0.72,
				High7Days:        150,
				Low7Days:         90,
				RelativeListings: 0.2,
				MarketDepth:      0.2,
			},
		}},
		trends: map[trendGemKey]trendData{
			{name: "Spark of Nova", variant: "20/20"}: {
				prices:       []int{110, 120, 123},
				listings:     []int{22, 19, 17},
				baseListings: []int{30, 28, 25},
			},
		},
	}

	rows := assembleTrendRows(query)
	if len(rows) != 1 {
		t.Fatalf("rows = %d, want 1", len(rows))
	}
	got := rows[0]
	if got.Time != "2026-09-24T12:34:56Z" || got.Name != "Spark of Nova" || got.Variant != "20/20" {
		t.Errorf("identity = (%q, %q, %q), want the signal identity", got.Time, got.Name, got.Variant)
	}
	if got.GemColor != "BLUE" || got.CurrentPrice != 123.5 || got.CurrentListings != 17 {
		t.Errorf("market enrichment = (%q, %.1f, %d), want (BLUE, 123.5, 17)", got.GemColor, got.CurrentPrice, got.CurrentListings)
	}
	if got.PriceVelocity != 2.5 || got.ListingVelocity != -1.25 || got.CV != 0.18 || got.HistPosition != 0.72 {
		t.Errorf("feature metrics = (%v, %v, %v, %v), want literal feature values", got.PriceVelocity, got.ListingVelocity, got.CV, got.HistPosition)
	}
	if got.PriceHigh7Days != 150 || got.PriceLow7Days != 90 || got.RelativeLiquidity != 0.2 || got.LiquidityTier != "LOW" {
		t.Errorf("range/liquidity = (%v, %v, %v, %q), want (150, 90, 0.2, LOW)", got.PriceHigh7Days, got.PriceLow7Days, got.RelativeLiquidity, got.LiquidityTier)
	}
	if got.Signal != "DEMAND" || got.WindowSignal != "OPEN" || got.AdvancedSignal != "SURGE" || got.PriceTier != "TOP" || got.TierAction != "SELL — active demand, list at market price" {
		t.Errorf("signal enrichment = (%q, %q, %q, %q, %q), want the signal values and TOP/DEMAND action", got.Signal, got.WindowSignal, got.AdvancedSignal, got.PriceTier, got.TierAction)
	}
	if got.SellUrgency != "NOW" || got.SellReason != "buyers active" || got.Sellability != 91 || got.SellabilityLabel != "SAFE" {
		t.Errorf("sellability = (%q, %q, %d, %q), want literal signal values", got.SellUrgency, got.SellReason, got.Sellability, got.SellabilityLabel)
	}
	if len(got.PriceTrend) != 3 || got.PriceTrend[2] != 123 || len(got.ListingsTrend) != 3 || got.ListingsTrend[0] != 22 || len(got.BaseListingsTrend) != 3 || got.BaseListingsTrend[1] != 28 {
		t.Errorf("trends = (%v, %v, %v), want the assembled trend series", got.PriceTrend, got.ListingsTrend, got.BaseListingsTrend)
	}
}

func TestAssembleCollectiveRows_CalculatesTwentyTwentyGCPRecipe(t *testing.T) {
	query := collectiveQueryResult{
		results: []lab.CollectiveResult{{
			TransfiguredName:  "Spark of Nova",
			BaseName:          "Spark",
			Variant:           "20/20",
			GemColor:          "BLUE",
			ROI:               80,
			ROIPct:            400,
			BasePrice:         100,
			TransfiguredPrice: 180,
			Confidence:        "OK",
			Signal:            "STABLE",
			Sellability:       75,
			SellabilityLabel:  "FAIR",
		}},
		sparklines: map[string][]lab.SparklinePoint{
			"Spark of Nova": {{Time: "2026-09-24T12:00:00Z", Price: 180, Listings: 7}},
		},
		basePriceIndex: map[collectiveBaseKey]float64{
			{name: "Spark", variant: "20"}: 10,
		},
		gcpPrice: 2.5,
	}

	rows := assembleCollectiveRows(query)
	if len(rows) != 1 {
		t.Fatalf("rows = %d, want 1", len(rows))
	}
	got := rows[0]
	if got.TransfiguredName != "Spark of Nova" || got.BaseName != "Spark" || got.Variant != "20/20" || got.GemColor != "BLUE" {
		t.Errorf("identity = (%q, %q, %q, %q), want the ranked result identity", got.TransfiguredName, got.BaseName, got.Variant, got.GemColor)
	}
	if got.ROI != 80 || got.ROIPct != 400 || got.BasePrice != 100 || got.TransfiguredPrice != 180 {
		t.Errorf("pricing = (%v, %v, %v, %v), want the ranked result values", got.ROI, got.ROIPct, got.BasePrice, got.TransfiguredPrice)
	}
	if got.GCPRecipeCost != 60 || got.GCPRecipeBase != 10 || got.GCPRecipeSaves != 40 {
		t.Errorf("GCP recipe = (%.1f, %.1f, %.1f), want (60, 10, 40)", got.GCPRecipeCost, got.GCPRecipeBase, got.GCPRecipeSaves)
	}
	if len(got.Sparkline) != 1 || got.Sparkline[0].Price != 180 || got.Sparkline[0].Listings != 7 {
		t.Errorf("sparkline = %+v, want the supplied raw point", got.Sparkline)
	}
}

func collectiveLimitCache(t *testing.T) *lab.Cache {
	t.Helper()
	cache := warmAnalysisCache(t,
		sparklineGem{name: "Spark of Nova", variant: "20/20", roi: 40},
		sparklineGem{name: "Spark of Storms", variant: "20/20", roi: 30},
		sparklineGem{name: "Flame of Cinders", variant: "20/20", roi: 50},
	)
	warmSparklines(t, cache, nil, nil)
	return cache
}

func TestCollectiveAnalysis_LimitCapsRankedResults(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, collectiveLimitCache(t), sparklineScope),
		"/api/analysis/collective?variant=20/20&limit=1")
	names := decodeNormalCollectiveRankedNames(t, w)
	if len(names) != 1 || names[0] != "Flame of Cinders" {
		t.Errorf("names = %v, want the highest-ranked row only", names)
	}
}

func TestCollectiveAnalysis_SearchReturnsAllMatchingResults(t *testing.T) {
	w := serveWithoutRepository(t, CollectiveAnalysis(nil, collectiveLimitCache(t), sparklineScope),
		"/api/analysis/collective?variant=20/20&limit=1&search=Spark")
	names := decodeNormalCollectiveRankedNames(t, w)
	want := []string{"Spark of Nova", "Spark of Storms"}
	if len(names) != len(want) {
		t.Fatalf("names = %v, want both matching rows despite limit 1", names)
	}
	for i := range want {
		if names[i] != want[i] {
			t.Errorf("names[%d] = %q, want %q; full order = %v", i, names[i], want[i], names)
		}
	}
}

func decodeNormalCollectiveRankedNames(t *testing.T, w *httptest.ResponseRecorder) []string {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var response struct {
		Data []struct {
			Name string `json:"transfiguredName"`
		} `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&response); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	names := make([]string, 0, len(response.Data))
	for _, row := range response.Data {
		names = append(names, row.Name)
	}
	return names
}

type trendSelectionFixture struct {
	name     string
	signal   string
	window   string
	advanced string
	tier     string
}

func trendSelectionCache(t *testing.T, target, other trendSelectionFixture) *lab.Cache {
	t.Helper()
	now := time.Now()
	cache := lab.NewCache(sparklineScope)
	signals := make([]lab.GemSignal, 0, 2)
	features := make([]lab.GemFeature, 0, 2)
	for _, fixture := range []trendSelectionFixture{target, other} {
		signals = append(signals, lab.GemSignal{
			Time:           now,
			Name:           fixture.name,
			Variant:        "20/20",
			Signal:         fixture.signal,
			WindowSignal:   fixture.window,
			AdvancedSignal: fixture.advanced,
			Tier:           fixture.tier,
		})
		features = append(features, lab.GemFeature{
			Time:    now,
			Name:    fixture.name,
			Variant: "20/20",
			Chaos:   100,
		})
	}
	cache.For(sparklineScope).SetGemSignals(signals)
	cache.For(sparklineScope).SetGemFeatures(features)
	cache.For(sparklineScope).SetSparklinesByName(nil, nil, now)
	return cache
}

func assertOnlyTrendName(t *testing.T, w *httptest.ResponseRecorder, want string) {
	t.Helper()
	rows := decodeTrendJSONRows(t, w)
	if len(rows) != 1 {
		t.Fatalf("trend rows = %d, want one: %+v", len(rows), rows)
	}
	var name string
	if err := json.Unmarshal(rows[0]["name"], &name); err != nil {
		t.Fatalf("decode trend name: %v", err)
	}
	if name != want {
		t.Errorf("trend name = %q, want %q", name, want)
	}
}

func TestTrendAnalysis_FiltersBySignal(t *testing.T) {
	cache := trendSelectionCache(t,
		trendSelectionFixture{name: "Demand Gem", signal: "DEMAND", window: "OPEN", advanced: "SURGE", tier: "TOP"},
		trendSelectionFixture{name: "Stable Gem", signal: "STABLE", window: "OPEN", advanced: "SURGE", tier: "TOP"})
	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20&signal=DEMAND")
	assertOnlyTrendName(t, w, "Demand Gem")
}

func TestTrendAnalysis_FiltersByWindow(t *testing.T) {
	cache := trendSelectionCache(t,
		trendSelectionFixture{name: "Open Gem", signal: "DEMAND", window: "OPEN", advanced: "SURGE", tier: "TOP"},
		trendSelectionFixture{name: "Closing Gem", signal: "DEMAND", window: "CLOSING", advanced: "SURGE", tier: "TOP"})
	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20&window=OPEN")
	assertOnlyTrendName(t, w, "Open Gem")
}

func TestTrendAnalysis_FiltersByAdvancedSignal(t *testing.T) {
	cache := trendSelectionCache(t,
		trendSelectionFixture{name: "Surging Gem", signal: "DEMAND", window: "OPEN", advanced: "SURGE", tier: "TOP"},
		trendSelectionFixture{name: "Recovering Gem", signal: "DEMAND", window: "OPEN", advanced: "RECOVERY", tier: "TOP"})
	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20&advanced=SURGE")
	assertOnlyTrendName(t, w, "Surging Gem")
}

func TestTrendAnalysis_FiltersByTier(t *testing.T) {
	cache := trendSelectionCache(t,
		trendSelectionFixture{name: "Top Gem", signal: "DEMAND", window: "OPEN", advanced: "SURGE", tier: "TOP"},
		trendSelectionFixture{name: "Mid Gem", signal: "DEMAND", window: "OPEN", advanced: "SURGE", tier: "MID"})
	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20&tier=TOP")
	assertOnlyTrendName(t, w, "Top Gem")
}

func decodeTrendJSONRows(t *testing.T, w *httptest.ResponseRecorder) []map[string]json.RawMessage {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var response struct {
		Data []map[string]json.RawMessage `json:"data"`
	}
	if err := json.NewDecoder(w.Body).Decode(&response); err != nil {
		t.Fatalf("decode trend response: %v", err)
	}
	return response.Data
}

func TestTrendAnalysis_JSONIncludesCompatibilityKeysForPopulatedTrend(t *testing.T) {
	now := time.Now()
	cache := warmAnalysisCache(t, sparklineGem{
		name: "Spark of Nova", variant: "20/20", roi: 40, windowSignal: "OPEN",
	})
	warmSparklines(t, cache, map[string]map[string][]lab.SparklinePoint{
		"Spark of Nova": {"20/20": {
			sparkAt(now, 3*time.Hour, 100, 20),
			sparkAt(now, 2*time.Hour, 110, 18),
		}},
		"Spark": {"20/20": {
			sparkAt(now, 3*time.Hour, 100, 30),
			sparkAt(now, 2*time.Hour, 100, 28),
		}},
	}, nil)

	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20")
	rows := decodeTrendJSONRows(t, w)
	if len(rows) != 1 {
		t.Fatalf("trend rows = %d, want one", len(rows))
	}
	for _, key := range []string{
		"time", "name", "variant", "gemColor", "currentPrice", "currentListings",
		"priceVelocity", "priceHigh7d", "priceLow7d", "windowSignal", "advancedSignal",
		"priceTier", "sellability", "priceTrend", "listingsTrend", "baseListingsTrend",
	} {
		if _, ok := rows[0][key]; !ok {
			t.Errorf("populated Trend JSON missing compatibility key %q: %s", key, w.Body.String())
		}
	}
}

func TestTrendAnalysis_JSONOmitsEmptyTrendSeries(t *testing.T) {
	cache := trendSelectionCache(t,
		trendSelectionFixture{name: "Stable Gem", signal: "STABLE", window: "CLOSED", advanced: "NONE", tier: "MID"},
		trendSelectionFixture{name: "Other Gem", signal: "DEMAND", window: "CLOSED", advanced: "NONE", tier: "MID"})
	w := serveWithoutRepository(t, TrendAnalysis(nil, cache, sparklineScope),
		"/api/analysis/trends?variant=20/20&signal=STABLE")
	rows := decodeTrendJSONRows(t, w)
	if len(rows) != 1 {
		t.Fatalf("trend rows = %d, want one", len(rows))
	}
	for _, key := range []string{"priceTrend", "listingsTrend", "baseListingsTrend"} {
		if _, ok := rows[0][key]; ok {
			t.Errorf("empty Trend JSON should omit %q: %s", key, w.Body.String())
		}
	}
}

func canceledLabRepository(t *testing.T) *lab.Repository {
	t.Helper()
	config, err := pgxpool.ParseConfig("postgres://test:test@127.0.0.1:1/test")
	if err != nil {
		t.Fatalf("parse lazy pgx config: %v", err)
	}
	config.MinConns = 0
	config.MaxConns = 1
	pool, err := pgxpool.NewWithConfig(context.Background(), config)
	if err != nil {
		t.Fatalf("create lazy pgx pool: %v", err)
	}
	t.Cleanup(pool.Close)
	return lab.NewRepository(pool)
}

func canceledRequest(target string) *http.Request {
	req := httptest.NewRequest(http.MethodGet, target, nil)
	ctx, cancel := context.WithCancel(req.Context())
	cancel()
	return req.WithContext(ctx)
}

func captureHandlerLogs(t *testing.T) *bytes.Buffer {
	t.Helper()
	previous := slog.Default()
	var logs bytes.Buffer
	slog.SetDefault(slog.New(slog.NewJSONHandler(&logs, nil)))
	t.Cleanup(func() { slog.SetDefault(previous) })
	return &logs
}

func TestTrendAnalysis_MandatoryCorpusErrorReturns500AndOperationLog(t *testing.T) {
	logs := captureHandlerLogs(t)
	req := canceledRequest("/api/analysis/trends?variant=20/20")
	w := httptest.NewRecorder()
	TrendAnalysis(canceledLabRepository(t), lab.NewCache(corpusScope), corpusScope).ServeHTTP(w, req)
	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500; body: %s", w.Code, w.Body.String())
	}
	if strings.TrimSpace(w.Body.String()) != `{"error":"query failed"}` {
		t.Errorf("body = %q, want query failed response", w.Body.String())
	}
	if !strings.Contains(logs.String(), "gem signals query failed") {
		t.Errorf("logs = %s, want gem signals operation context", logs.String())
	}
}

func TestTrendAnalysis_OptionalSparklineErrorReturnsRowWithoutTrendSeries(t *testing.T) {
	logs := captureHandlerLogs(t)
	cache := warmAnalysisCache(t, sparklineGem{
		name: "Spark of Nova", variant: "20/20", roi: 40, windowSignal: "OPEN",
	})
	req := canceledRequest("/api/analysis/trends?variant=20/20")
	w := httptest.NewRecorder()
	TrendAnalysis(canceledLabRepository(t), cache, sparklineScope).ServeHTTP(w, req)
	rows := decodeTrendJSONRows(t, w)
	if len(rows) != 1 {
		t.Fatalf("trend rows = %d, want one: %s", len(rows), w.Body.String())
	}
	var name string
	if err := json.Unmarshal(rows[0]["name"], &name); err != nil {
		t.Fatalf("decode trend name: %v", err)
	}
	if name != "Spark of Nova" || rows[0]["currentPrice"] == nil {
		t.Errorf("row = %s, want non-sparkline content for Spark of Nova", w.Body.String())
	}
	for _, key := range []string{"priceTrend", "listingsTrend", "baseListingsTrend"} {
		if _, ok := rows[0][key]; ok {
			t.Errorf("optional error response should omit %q: %s", key, w.Body.String())
		}
	}
	if !strings.Contains(logs.String(), "trans sparkline batch failed") || !strings.Contains(logs.String(), "base sparkline batch failed") {
		t.Errorf("logs = %s, want both optional Trend operation logs", logs.String())
	}
}

func TestCollectiveAnalysis_MandatoryCorpusErrorReturns500AndOperationLog(t *testing.T) {
	logs := captureHandlerLogs(t)
	req := canceledRequest("/api/analysis/collective?variant=20/20")
	w := httptest.NewRecorder()
	CollectiveAnalysis(canceledLabRepository(t), lab.NewCache(corpusScope), corpusScope).ServeHTTP(w, req)
	if w.Code != http.StatusInternalServerError {
		t.Fatalf("status = %d, want 500; body: %s", w.Code, w.Body.String())
	}
	if strings.TrimSpace(w.Body.String()) != `{"error":"query failed"}` {
		t.Errorf("body = %q, want query failed response", w.Body.String())
	}
	if !strings.Contains(logs.String(), "transfigure query failed") {
		t.Errorf("logs = %s, want transfigure operation context", logs.String())
	}
}

func TestCollectiveAnalysis_OptionalSparklineErrorReturnsRowWithEmptySparkline(t *testing.T) {
	logs := captureHandlerLogs(t)
	cache := warmAnalysisCache(t, sparklineGem{
		name: "Spark of Nova", variant: "20/20", roi: 40,
	})
	req := canceledRequest("/api/analysis/collective?variant=20/20")
	w := httptest.NewRecorder()
	CollectiveAnalysis(canceledLabRepository(t), cache, sparklineScope).ServeHTTP(w, req)
	rows := decodeSparklineRows(t, w)
	row, ok := rows["Spark of Nova"]
	if !ok {
		t.Fatalf("rows = %+v, want Spark of Nova", rows)
	}
	if len(row.Sparkline) != 0 {
		t.Errorf("sparkline = %+v, want empty array after optional error", row.Sparkline)
	}
	if !strings.Contains(logs.String(), "collective analysis: sparkline query failed") {
		t.Errorf("logs = %s, want optional sparkline operation context", logs.String())
	}
	if !strings.Contains(logs.String(), "collective: GCP price not cached, using fallback") {
		t.Errorf("logs = %s, want fallback warning at the query decision", logs.String())
	}
}
