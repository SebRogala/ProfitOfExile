//go:build integration

package lab

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// collectiveTransfigureRow is one transfigure_results row. Zero values are
// filled by seedCollective with an eligible OK row: base 10, price 10+roi,
// 20 listings per side, colour BLUE.
type collectiveTransfigureRow struct {
	name, base, variant, color, confidence string
	basePrice, price, roi, roiPct          float64
	baseListings, listings                 int
}

type collectiveSignalRow struct {
	name, variant, signal                                          string
	sellability                                                    int
	tier, window, advanced, urgency, reason, label, sellConfidence string
}

type collectiveFeatureRow struct {
	name, variant                                 string
	velPrice, velListing, cv, hist, low7d, high7d float64
	depth                                         float64
	lowConfidence                                 bool
}

// collectiveLeague registers a fresh league and removes its corpus rows on
// cleanup (before the league itself, which registerLeague cleans up last).
func collectiveLeague(t *testing.T, pool *pgxpool.Pool, id string) league.Scope {
	t.Helper()
	registerLeague(t, pool, id)
	t.Cleanup(func() {
		for _, table := range []string{"transfigure_results", "gem_signals", "gem_features", "gem_snapshots"} {
			if _, err := pool.Exec(context.Background(), "DELETE FROM "+table+" WHERE league = $1", id); err != nil {
				t.Logf("cleanup warning: delete %s for %q: %v", table, id, err)
			}
		}
	})
	return league.Historical(id)
}

func seedCollective(t *testing.T, pool *pgxpool.Pool, scope league.Scope, at time.Time,
	trs []collectiveTransfigureRow, sigs []collectiveSignalRow, feats []collectiveFeatureRow) {
	t.Helper()
	ctx := context.Background()
	for _, r := range trs {
		if r.base == "" {
			r.base = "Base of " + r.name
		}
		if r.variant == "" {
			r.variant = "20/20"
		}
		if r.color == "" {
			r.color = "BLUE"
		}
		if r.confidence == "" {
			r.confidence = "OK"
		}
		if r.basePrice == 0 && r.confidence != ConfidenceNoBase {
			r.basePrice = 10
		}
		if r.price == 0 {
			r.price = r.basePrice + r.roi
		}
		if r.listings == 0 {
			r.listings = 20
		}
		if r.baseListings == 0 {
			r.baseListings = 20
		}
		if _, err := pool.Exec(ctx, `
			INSERT INTO transfigure_results
				(league, time, base_name, transfigured_name, variant, base_price, transfigured_price,
				 roi, roi_pct, base_listings, transfigured_listings, gem_color, confidence)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)`,
			scope.ID(), at, r.base, r.name, r.variant, r.basePrice, r.price,
			r.roi, r.roiPct, r.baseListings, r.listings, r.color, r.confidence); err != nil {
			t.Fatalf("seed transfigure row %q: %v", r.name, err)
		}
	}
	for _, s := range sigs {
		if s.variant == "" {
			s.variant = "20/20"
		}
		if s.signal == "" {
			s.signal = "STABLE"
		}
		if _, err := pool.Exec(ctx, `
			INSERT INTO gem_signals
				(league, time, name, variant, signal, sellability, tier, window_signal, advanced_signal,
				 sell_urgency, sell_reason, sellability_label, sell_confidence)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)`,
			scope.ID(), at, s.name, s.variant, s.signal, s.sellability, s.tier, s.window, s.advanced,
			s.urgency, s.reason, s.label, s.sellConfidence); err != nil {
			t.Fatalf("seed signal row %q: %v", s.name, err)
		}
	}
	for _, f := range feats {
		if f.variant == "" {
			f.variant = "20/20"
		}
		if _, err := pool.Exec(ctx, `
			INSERT INTO gem_features
				(league, time, name, variant, vel_long_price, vel_long_listing, cv, hist_position,
				 low_7d, high_7d, market_depth, low_confidence)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`,
			scope.ID(), at, f.name, f.variant, f.velPrice, f.velListing, f.cv, f.hist,
			f.low7d, f.high7d, f.depth, f.lowConfidence); err != nil {
			t.Fatalf("seed feature row %q: %v", f.name, err)
		}
	}
}

// fullSellability gives each named gem a STABLE signal at sellability 100, so
// its weighted ROI equals its raw ROI.
func fullSellability(rows []collectiveTransfigureRow) []collectiveSignalRow {
	out := make([]collectiveSignalRow, 0, len(rows))
	for _, r := range rows {
		out = append(out, collectiveSignalRow{name: r.name, variant: r.variant, sellability: 100})
	}
	return out
}

func numberedRows(n int) []collectiveTransfigureRow {
	rows := make([]collectiveTransfigureRow, 0, n)
	for i := 0; i < n; i++ {
		rows = append(rows, collectiveTransfigureRow{name: fmt.Sprintf("Gem %02d", i), roi: float64(1000 - i)})
	}
	return rows
}

func pagedQuery(page, size int, sort CollectiveSort) CollectiveQuery {
	return CollectiveQuery{Page: page, PageSize: size, Sort: sort}
}

func collectivePage(t *testing.T, repo *Repository, scope league.Scope, q CollectiveQuery) CollectivePage {
	t.Helper()
	got, err := repo.CollectivePage(context.Background(), scope, q)
	if err != nil {
		t.Fatalf("CollectivePage(%+v): %v", q, err)
	}
	return got
}

func pageNames(page CollectivePage) []string {
	names := make([]string, 0, len(page.Rows))
	for _, r := range page.Rows {
		names = append(names, r.TransfiguredName)
	}
	return names
}

func rowNamed(t *testing.T, page CollectivePage, name string) CollectiveResult {
	t.Helper()
	for _, r := range page.Rows {
		if r.TransfiguredName == name {
			return r
		}
	}
	t.Fatalf("page has no row %q: %v", name, pageNames(page))
	return CollectiveResult{}
}

func assertNames(t *testing.T, got []string, want ...string) {
	t.Helper()
	if strings.Join(got, "|") != strings.Join(want, "|") {
		t.Errorf("rows = %v, want %v", got, want)
	}
}

func TestCollectivePage_TotalCountsEveryMatchBeyondThePage(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Total")
	rows := numberedRows(25)
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))

	if len(got.Rows) != 10 || got.Total != 25 {
		t.Errorf("rows = %d, total = %d; want 10 rows and total 25", len(got.Rows), got.Total)
	}
}

func TestCollectivePage_SecondPageStartsAfterTheFirst(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Page2")
	rows := numberedRows(25)
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(2, 10, CollectiveSortPrice))

	want := make([]string, 0, 10)
	for i := 10; i < 20; i++ {
		want = append(want, fmt.Sprintf("Gem %02d", i))
	}
	assertNames(t, pageNames(got), want...)
}

func TestCollectivePage_PagePastTheEndIsEmptyWithTotal(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-PastEnd")
	rows := numberedRows(5)
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(3, 10, CollectiveSortPrice))

	if len(got.Rows) != 0 || got.Total != 5 {
		t.Errorf("rows = %v, total = %d; want no rows and total 5", pageNames(got), got.Total)
	}
}

// Each gem leads under exactly one key. "Pct Gem" has the highest raw roi_pct
// but the lowest sellability, so a weighted-pct sort would not put it first.
func TestCollectivePage_SortsByEachClientKey(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Sorts")
	rows := []collectiveTransfigureRow{
		{name: "Price Gem", basePrice: 900, roi: 100, roiPct: 11},
		{name: "Roi Gem", basePrice: 10, roi: 500, roiPct: 5000},
		{name: "Weighted Gem", basePrice: 10, roi: 400, roiPct: 4000},
		{name: "Pct Gem", basePrice: 1, roi: 90, roiPct: 9000},
	}
	sigs := []collectiveSignalRow{
		{name: "Price Gem", sellability: 50},
		{name: "Roi Gem", sellability: 20},
		{name: "Weighted Gem", sellability: 100},
		{name: "Pct Gem", sellability: 1},
	}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)
	repo := NewRepository(pool)

	for _, tt := range []struct {
		sort CollectiveSort
		want string
	}{
		{CollectiveSortPrice, "Price Gem"},
		{CollectiveSortROI, "Roi Gem"},
		{CollectiveSortWeightedROI, "Weighted Gem"},
		{CollectiveSortROIPct, "Pct Gem"},
	} {
		t.Run(string(tt.sort), func(t *testing.T) {
			got := collectivePage(t, repo, scope, pagedQuery(1, 10, tt.sort))
			if len(got.Rows) == 0 || got.Rows[0].TransfiguredName != tt.want {
				t.Errorf("first row under %s = %v, want %s", tt.sort, pageNames(got), tt.want)
			}
		})
	}
}

func TestCollectivePage_FiltersByVariant(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Variant")
	rows := []collectiveTransfigureRow{
		{name: "Twenty Gem", variant: "20/20", roi: 50},
		{name: "One Gem", variant: "1/20", roi: 60},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	q := pagedQuery(1, 10, CollectiveSortPrice)
	q.Variant = "20/20"
	got := collectivePage(t, NewRepository(pool), scope, q)

	assertNames(t, pageNames(got), "Twenty Gem")
	if got.Total != 1 {
		t.Errorf("total = %d, want 1", got.Total)
	}
}

func TestCollectivePage_FiltersByColor(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Color")
	rows := []collectiveTransfigureRow{
		{name: "Red Gem", color: "RED", roi: 50},
		{name: "Blue Gem", color: "BLUE", roi: 60},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	q := pagedQuery(1, 10, CollectiveSortPrice)
	q.Color = "RED"
	got := collectivePage(t, NewRepository(pool), scope, q)

	assertNames(t, pageNames(got), "Red Gem")
	if got.Total != 1 {
		t.Errorf("total = %d, want 1", got.Total)
	}
}

func TestCollectivePage_BudgetDropsNoBaseAndPricierBases(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Budget")
	rows := []collectiveTransfigureRow{
		{name: "Cheap Gem", basePrice: 40, roi: 50},
		{name: "At Budget Gem", basePrice: 100, roi: 50},
		{name: "Pricey Gem", basePrice: 101, roi: 500},
		{name: "No Base Gem", confidence: ConfidenceNoBase, price: 900},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	q := pagedQuery(1, 10, CollectiveSortPrice)
	q.Budget = 100
	got := collectivePage(t, NewRepository(pool), scope, q)

	assertNames(t, pageNames(got), "At Budget Gem", "Cheap Gem")
	if got.Total != 2 {
		t.Errorf("total = %d, want 2", got.Total)
	}
}

func TestCollectivePage_HideLowConfidenceDropsFeatureAndLowConfidenceRows(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-HideLow")
	rows := []collectiveTransfigureRow{
		{name: "Solid Gem", roi: 50},
		{name: "Thin Transfigure Gem", confidence: "LOW", roi: 60},
		{name: "Thin Feature Gem", roi: 70},
	}
	feats := []collectiveFeatureRow{{name: "Thin Feature Gem", lowConfidence: true}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), feats)
	repo := NewRepository(pool)

	shown := collectivePage(t, repo, scope, pagedQuery(1, 10, CollectiveSortPrice))
	if shown.Total != 3 {
		t.Fatalf("total without the filter = %d, want 3 (the filter must be what removes rows)", shown.Total)
	}
	q := pagedQuery(1, 10, CollectiveSortPrice)
	q.HideLowConfidence = true
	got := collectivePage(t, repo, scope, q)

	assertNames(t, pageNames(got), "Solid Gem")
	if got.Total != 1 {
		t.Errorf("total = %d, want 1", got.Total)
	}
}

func TestCollectivePage_SearchMatchesSubstringCaseInsensitivelyAndIgnoresPageSize(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Search")
	rows := []collectiveTransfigureRow{
		{name: "Spark of Nova", roi: 50},
		{name: "Spark of the Nova", roi: 40},
		{name: "Ice Spark Ball", roi: 30},
		{name: "Cleave of Rage", roi: 900},
		{name: "Sparkle_Gem 100%", roi: 20},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)
	repo := NewRepository(pool)

	q := pagedQuery(1, 1, CollectiveSortPrice)
	q.Search = "sPaRk"
	got := collectivePage(t, repo, scope, q)
	assertNames(t, pageNames(got), "Spark of Nova", "Spark of the Nova", "Ice Spark Ball", "Sparkle_Gem 100%")
	if got.Total != 4 {
		t.Errorf("total = %d, want 4", got.Total)
	}

	// LIKE metacharacters in the search are literal text.
	q.Search = "e_G"
	assertNames(t, pageNames(collectivePage(t, repo, scope, q)), "Sparkle_Gem 100%")
	q.Search = "0%"
	assertNames(t, pageNames(collectivePage(t, repo, scope, q)), "Sparkle_Gem 100%")
}

func TestCollectivePage_ExcludesCaution(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Caution")
	rows := []collectiveTransfigureRow{{name: "Spark of Nova", roi: 50}, {name: "Cleave of Rage", roi: 100}}
	sigs := []collectiveSignalRow{
		{name: "Spark of Nova", sellability: 60},
		{name: "Cleave of Rage", signal: "CAUTION", sellability: 60},
	}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	assertNames(t, pageNames(got), "Spark of Nova")
}

func TestCollectivePage_ExcludesNonPositiveRoi(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-NonPositive")
	rows := []collectiveTransfigureRow{
		{name: "Profit Gem", roi: 0.01},
		{name: "Zero Gem", basePrice: 50, price: 50, roi: 0},
		{name: "Loss Gem", basePrice: 50, price: 40, roi: -10},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	assertNames(t, pageNames(got), "Profit Gem")
}

func TestCollectivePage_KeepsPricedNoBaseRow(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-NoBaseKept")
	rows := []collectiveTransfigureRow{{name: "Orphan Gem", confidence: ConfidenceNoBase, price: 300}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	row := rowNamed(t, got, "Orphan Gem")
	if row.Confidence != ConfidenceNoBase || row.TransfiguredPrice != 300 || row.ROI != 0 {
		t.Errorf("row = (%s, price %v, roi %v), want NO_BASE at price 300 with ROI 0", row.Confidence, row.TransfiguredPrice, row.ROI)
	}
}

func TestCollectivePage_DropsUnpricedNoBaseRow(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-NoBaseDropped")
	rows := []collectiveTransfigureRow{
		{name: "Unpriced Orphan", confidence: ConfidenceNoBase}, // price stays 0
		{name: "Kept Gem", roi: 5},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	assertNames(t, pageNames(got), "Kept Gem")
}

func TestCollectivePage_LowTransfigureConfidenceMarksLowConfidence(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-LowTransfigure")
	rows := []collectiveTransfigureRow{{name: "Thin Gem", confidence: "LOW", roi: 80}, {name: "Thick Gem", roi: 70}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	if !rowNamed(t, got, "Thin Gem").LowConfidence {
		t.Error("LOW transfigure row LowConfidence = false, want true")
	}
	if rowNamed(t, got, "Thick Gem").LowConfidence {
		t.Error("OK transfigure row LowConfidence = true, want false")
	}
}

// A feature row with low_confidence=false must not clear the transfigure flag.
func TestCollectivePage_LowConfidenceFlagSurvivesFeatureJoin(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-LowSurvives")
	rows := []collectiveTransfigureRow{{name: "Thin Gem", confidence: "LOW", roi: 80}}
	feats := []collectiveFeatureRow{{name: "Thin Gem", lowConfidence: false, depth: 1}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), feats)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	if !rowNamed(t, got, "Thin Gem").LowConfidence {
		t.Error("LowConfidence = false after the feature join, want the LOW transfigure flag kept")
	}
}

func TestCollectivePage_WeightedRoiIsRoiTimesSellability(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Weighted")
	rows := []collectiveTransfigureRow{{name: "Low Sell Gem", roi: 100}, {name: "High Sell Gem", roi: 100}}
	sigs := []collectiveSignalRow{{name: "Low Sell Gem", sellability: 30}, {name: "High Sell Gem", sellability: 80}}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortWeightedROI))
	assertNames(t, pageNames(got), "High Sell Gem", "Low Sell Gem")
	if got.Rows[0].WeightedROI != 80 || got.Rows[1].WeightedROI != 30 {
		t.Errorf("weighted ROI = (%v, %v), want (80, 30)", got.Rows[0].WeightedROI, got.Rows[1].WeightedROI)
	}
}

// D5: the faithful port keeps today's latent bug — a gem with no signal row
// is emitted as STABLE and scores sellability 0.
func TestCollectivePage_MissingSignalEmitsStableWithZeroWeightedRoi(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-NoSignal")
	seedCollective(t, pool, scope, time.Now(), []collectiveTransfigureRow{{name: "Spark of Nova", roi: 50}}, nil, nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	row := rowNamed(t, got, "Spark of Nova")
	if row.Signal != "STABLE" || row.WeightedROI != 0 {
		t.Errorf("signal = %q, weightedRoi = %v; want STABLE and 0", row.Signal, row.WeightedROI)
	}
}

func TestCollectivePage_SignalJoinCarriesTierAndSellFields(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-SignalFields")
	rows := []collectiveTransfigureRow{{name: "Top Gem", roi: 200}, {name: "High Gem", roi: 150}, {name: "Mid Gem", roi: 50}}
	sigs := []collectiveSignalRow{
		{name: "Top Gem", signal: "DEMAND", tier: "TOP", sellability: 80, window: "OPEN", advanced: "SURGE",
			urgency: "SELL_NOW", reason: "demand spike", label: "GOOD", sellConfidence: "SAFE"},
		{name: "High Gem", tier: "HIGH", sellability: 70, sellConfidence: "FAIR"},
		{name: "Mid Gem", tier: "MID", sellability: 60, sellConfidence: "RISKY"},
	}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	for name, tier := range map[string]string{"Top Gem": "TOP", "High Gem": "HIGH", "Mid Gem": "MID"} {
		if row := rowNamed(t, got, name); row.PriceTier != tier {
			t.Errorf("%s PriceTier = %q, want %q", name, row.PriceTier, tier)
		}
	}
	top := rowNamed(t, got, "Top Gem")
	want := CollectiveResult{
		Signal: "DEMAND", WindowSignal: "OPEN", AdvancedSignal: "SURGE", SellUrgency: "SELL_NOW",
		SellReason: "demand spike", SellabilityLabel: "GOOD", SellConfidence: "SAFE", Sellability: 80,
		TierAction: tierAction("DEMAND", "OPEN", "TOP"),
	}
	gotFields := CollectiveResult{
		Signal: top.Signal, WindowSignal: top.WindowSignal, AdvancedSignal: top.AdvancedSignal, SellUrgency: top.SellUrgency,
		SellReason: top.SellReason, SellabilityLabel: top.SellabilityLabel, SellConfidence: top.SellConfidence,
		Sellability: top.Sellability, TierAction: top.TierAction,
	}
	if gotFields != want {
		t.Errorf("signal fields = %+v, want %+v", gotFields, want)
	}
	if rowNamed(t, got, "Mid Gem").SellConfidence != "RISKY" {
		t.Errorf("Mid Gem SellConfidence = %q, want RISKY", rowNamed(t, got, "Mid Gem").SellConfidence)
	}
}

func TestCollectivePage_FeatureJoinPopulatesVelocityCvHistPosition(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Features")
	rows := []collectiveTransfigureRow{{name: "Spark of Nova", roi: 50}}
	feats := []collectiveFeatureRow{{name: "Spark of Nova", velPrice: 1.5, velListing: -2.25, cv: 0.31,
		hist: 72.5, low7d: 40, high7d: 95, depth: 0.5}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), feats)

	row := rowNamed(t, collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice)), "Spark of Nova")
	got := [7]any{row.PriceVelocity, row.ListingVelocity, row.CV, row.HistPosition, row.Low7Days, row.High7Days, row.LiquidityTier}
	want := [7]any{1.5, -2.25, 0.31, 72.5, 40.0, 95.0, "MED"}
	if got != want {
		t.Errorf("feature fields = %v, want %v", got, want)
	}
}

func TestCollectivePage_RoiPctPopulated(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-RoiPct")
	rows := []collectiveTransfigureRow{{name: "Spark of Nova", basePrice: 5, roi: 495, roiPct: 9900}}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	row := rowNamed(t, collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice)), "Spark of Nova")
	if row.ROIPct != 9900 || row.WeightedROIPct != 9900 {
		t.Errorf("roiPct = %v, weightedRoiPct = %v; want 9900 and 9900 (sellability 100)", row.ROIPct, row.WeightedROIPct)
	}
}

func TestCollectivePage_DumpingAt14ListingsHalvesWeightedRoi(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Dump14")
	rows := []collectiveTransfigureRow{{name: "Dumping Gem", roi: 100, listings: 14}}
	sigs := []collectiveSignalRow{{name: "Dumping Gem", signal: "DUMPING", sellability: 80}}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)

	row := rowNamed(t, collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice)), "Dumping Gem")
	if want := 100.0 * 0.8 * 0.5; row.WeightedROI != want {
		t.Errorf("weighted ROI = %v, want %v", row.WeightedROI, want)
	}
}

func TestCollectivePage_DumpingAt15ListingsTakes015Penalty(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Dump15")
	rows := []collectiveTransfigureRow{{name: "Dumping Gem", roi: 100, listings: 15}}
	sigs := []collectiveSignalRow{{name: "Dumping Gem", signal: "DUMPING", sellability: 80}}
	seedCollective(t, pool, scope, time.Now(), rows, sigs, nil)

	row := rowNamed(t, collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice)), "Dumping Gem")
	if want := 100.0 * 0.8 * (1.0 - 0.15); row.WeightedROI != want {
		t.Errorf("weighted ROI = %v, want %v", row.WeightedROI, want)
	}
}

func TestCollectivePage_UsesEachCorpusAtItsOwnLatestTime(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-LatestTime")
	now := time.Now().UTC().Truncate(time.Second)
	// An old run carried a CAUTION signal and a different ROI for the gem.
	seedCollective(t, pool, scope, now.Add(-2*time.Hour),
		[]collectiveTransfigureRow{{name: "Spark of Nova", roi: 999}},
		[]collectiveSignalRow{{name: "Spark of Nova", signal: "CAUTION", sellability: 10}}, nil)
	// The latest transfigure run, then a later signal run for another gem only.
	seedCollective(t, pool, scope, now.Add(-time.Hour), []collectiveTransfigureRow{{name: "Spark of Nova", roi: 50}}, nil, nil)
	seedCollective(t, pool, scope, now, nil, []collectiveSignalRow{{name: "Other Gem", sellability: 90}}, nil)

	got := collectivePage(t, NewRepository(pool), scope, pagedQuery(1, 10, CollectiveSortPrice))
	row := rowNamed(t, got, "Spark of Nova")
	if row.ROI != 50 || row.Signal != "STABLE" || row.Sellability != 0 {
		t.Errorf("row = (roi %v, signal %q, sellability %d), want the latest ROI 50 and no stale signal join", row.ROI, row.Signal, row.Sellability)
	}
}

func TestCollectivePage_LegacyOrderingMatchesBudgetAwareDefault(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Legacy")
	// Chaos Gem leads on weighted chaos, Pct Gem on weighted pct; both fit budget 40.
	rows := []collectiveTransfigureRow{
		{name: "Chaos Gem", basePrice: 40, roi: 400, roiPct: 1000},
		{name: "Pct Gem", basePrice: 2, roi: 100, roiPct: 5000},
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)
	repo := NewRepository(pool)

	for _, tt := range []struct {
		name   string
		sort   CollectiveSort
		budget float64
		want   []string
	}{
		{"budget 40 resolves to weighted pct", CollectiveSortLegacyWeightedPct, 40, []string{"Pct Gem", "Chaos Gem"}},
		{"no budget resolves to weighted chaos", CollectiveSortLegacyWeightedChaos, 0, []string{"Chaos Gem", "Pct Gem"}},
		{"sort=pct", CollectiveSortLegacyWeightedPct, 0, []string{"Pct Gem", "Chaos Gem"}},
	} {
		t.Run(tt.name, func(t *testing.T) {
			q := CollectiveQuery{Page: 1, PageSize: 20, Sort: tt.sort, Budget: tt.budget, Legacy: true}
			assertNames(t, pageNames(collectivePage(t, repo, scope, q)), tt.want...)
		})
	}
	if got := LegacyCollectiveSort("", 40); got != CollectiveSortLegacyWeightedPct {
		t.Errorf("LegacyCollectiveSort(\"\", 40) = %q, want weighted pct", got)
	}
	if got := LegacyCollectiveSort("", 51); got != CollectiveSortLegacyWeightedChaos {
		t.Errorf("LegacyCollectiveSort(\"\", 51) = %q, want weighted chaos", got)
	}
	if got := LegacyCollectiveSort("pct", 500); got != CollectiveSortLegacyWeightedPct {
		t.Errorf("LegacyCollectiveSort(pct, 500) = %q, want weighted pct", got)
	}
}

func TestCollectivePage_RejectsInvalidScope(t *testing.T) {
	pool := labIntegrationPool(t)
	_, err := NewRepository(pool).CollectivePage(context.Background(), league.Scope{}, pagedQuery(1, 10, CollectiveSortPrice))
	if err == nil {
		t.Fatal("CollectivePage with a zero scope returned no error, want the scope validation error")
	}
}

// statementCounter counts every SQL statement the pool sends, BEGIN and
// COMMIT included.
type statementCounter struct {
	mu    sync.Mutex
	count int
}

func (c *statementCounter) TraceQueryStart(ctx context.Context, _ *pgx.Conn, _ pgx.TraceQueryStartData) context.Context {
	c.mu.Lock()
	c.count++
	c.mu.Unlock()
	return ctx
}

func (c *statementCounter) TraceQueryEnd(context.Context, *pgx.Conn, pgx.TraceQueryEndData) {}

func (c *statementCounter) reset() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	n := c.count
	c.count = 0
	return n
}

func countingPool(t *testing.T, dbURL string) (*pgxpool.Pool, *statementCounter) {
	t.Helper()
	cfg, err := pgxpool.ParseConfig(dbURL)
	if err != nil {
		t.Fatalf("parse pool config: %v", err)
	}
	counter := &statementCounter{}
	cfg.ConnConfig.Tracer = counter
	pool, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatalf("open counting pool: %v", err)
	}
	t.Cleanup(pool.Close)
	return pool, counter
}

// The page's statement budget: BEGIN + count + page + COMMIT, one sparkline
// read per distinct variant on the page (4), one GCP base lookup.
const collectivePageStatementBudget = 4 + 4 + 1

func TestQueryCollective_StatementCountIsIndependentOfPageSize(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "ADR030-Statements")
	variants := []string{"1", "1/20", "20", "20/20"}
	rows := make([]collectiveTransfigureRow, 0, 60)
	for i := 0; i < 60; i++ {
		rows = append(rows, collectiveTransfigureRow{
			name: fmt.Sprintf("Gem %02d", i), base: fmt.Sprintf("Base %02d", i), variant: variants[i%4], roi: float64(1000 - i),
		})
	}
	seedCollective(t, pool, scope, time.Now(), rows, fullSellability(rows), nil)

	counted, counter := countingPool(t, pool.Config().ConnString())
	repo := NewRepository(counted)
	run := func(size int) (int, CollectiveQueryResult) {
		counter.reset()
		got, err := QueryCollective(context.Background(), repo, nil, scope, pagedQuery(1, size, CollectiveSortPrice))
		if err != nil {
			t.Fatalf("QueryCollective(pageSize %d): %v", size, err)
		}
		return counter.reset(), got
	}
	run(10) // warm the connection so connect-time statements are not counted

	small, smallPage := run(10)
	large, largePage := run(50)
	for _, page := range []CollectiveQueryResult{smallPage, largePage} {
		seen := map[string]bool{}
		for _, r := range page.Results {
			seen[r.Variant] = true
		}
		if len(seen) != 4 {
			t.Fatalf("page of %d rows covers variants %v, want all 4", len(page.Results), seen)
		}
	}
	if small != collectivePageStatementBudget || large != collectivePageStatementBudget {
		t.Errorf("statements = %d (pageSize 10) and %d (pageSize 50), want %d for both", small, large, collectivePageStatementBudget)
	}
}
