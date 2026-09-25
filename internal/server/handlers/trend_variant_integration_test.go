//go:build integration

package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

func registerTrendIntegrationLeague(t *testing.T, pool *pgxpool.Pool, id string) {
	t.Helper()
	ctx := context.Background()
	if _, err := pool.Exec(ctx,
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
}

func cleanupTrendSnapshots(t *testing.T, pool *pgxpool.Pool, leagueID string, names []string, first, last time.Time) {
	t.Helper()
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(),
			`DELETE FROM gem_snapshots WHERE league = $1 AND name = ANY($2) AND time >= $3 AND time <= $4`,
			leagueID, names, first, last); err != nil {
			t.Logf("cleanup warning: delete Trend snapshots for league %q: %v", leagueID, err)
		}
	})
}

func seedTrendSnapshot(t *testing.T, pool *pgxpool.Pool, leagueID string, tm time.Time,
	name, variant string, isTransfigured bool, chaos float64, listings int, color string) {
	t.Helper()
	_, err := pool.Exec(context.Background(), `
		INSERT INTO gem_snapshots
			(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
		VALUES ($1, $2, $3, $4, false, $5, $6, $7, $8)`,
		leagueID, tm, name, variant, isTransfigured, chaos, listings, color)
	if err != nil {
		t.Fatalf("seed Trend snapshot (league %q, %s, %s): %v", leagueID, name, variant, err)
	}
}

func trendIntegrationCache(scope league.Scope, name string) *lab.Cache {
	now := time.Now().UTC()
	cache := lab.NewCache(scope)
	cache.For(scope).SetGemSignals([]lab.GemSignal{
		{Time: now, Name: name, Variant: "20/20", WindowSignal: "OPEN", Tier: "MID"},
		{Time: now, Name: name, Variant: "1/20", WindowSignal: "OPEN", Tier: "MID"},
	})
	cache.For(scope).SetGemFeatures([]lab.GemFeature{
		{Time: now, Name: name, Variant: "20/20", Chaos: 110, Listings: 31, GemColor: "BLUE"},
		{Time: now, Name: name, Variant: "1/20", Chaos: 8, Listings: 91, GemColor: "BLUE"},
	})
	return cache
}

func serveTrendIntegration(t *testing.T, repo *lab.Repository, cache *lab.Cache, scope league.Scope) []map[string]json.RawMessage {
	t.Helper()
	w := httptest.NewRecorder()
	TrendAnalysis(repo, cache, scope).ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/trends", nil))
	return decodeTrendJSONRows(t, w)
}

func TestTrendAnalysis_ColdRepositoryPreservesSameNameVariantSeries(t *testing.T) {
	pool := offeringIntegrationPool(t)
	leagueID := fmt.Sprintf("POE-WI2-trend-series-%d", time.Now().UnixNano())
	scope := league.Historical(leagueID)
	registerTrendIntegrationLeague(t, pool, leagueID)

	const (
		transfiguredName = "POE-WI2 Trend of Nova"
		baseName         = "POE-WI2 Trend"
	)
	now := time.Now().UTC().Truncate(time.Second)
	first := now.Add(-2 * time.Hour)
	last := now.Add(-time.Hour)
	cleanupTrendSnapshots(t, pool, leagueID, []string{transfiguredName, baseName}, first, last)
	seedTrendSnapshot(t, pool, leagueID, first, transfiguredName, "20/20", true, 101, 31, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, transfiguredName, "20/20", true, 112, 29, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, first, baseName, "20/20", false, 1, 51, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, baseName, "20/20", false, 1, 47, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, first, transfiguredName, "1/20", true, 7, 91, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, transfiguredName, "1/20", true, 9, 84, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, first, baseName, "1/20", false, 1, 13, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, baseName, "1/20", false, 1, 11, "BLUE")

	rows := serveTrendIntegration(t, lab.NewRepository(pool), trendIntegrationCache(scope, transfiguredName), scope)
	if len(rows) != 2 {
		t.Fatalf("response rows = %d, want two same-name variants: %+v", len(rows), rows)
	}
	for i, expected := range []struct {
		variant  string
		prices   []int
		listings []int
		base     []int
	}{
		{variant: "20/20", prices: []int{101, 112}, listings: []int{31, 29}, base: []int{51, 47}},
		{variant: "1/20", prices: []int{7, 9}, listings: []int{91, 84}, base: []int{13, 11}},
	} {
		var variant string
		if err := json.Unmarshal(rows[i]["variant"], &variant); err != nil {
			t.Fatalf("decode row %d variant: %v", i, err)
		}
		if variant != expected.variant {
			t.Fatalf("row %d variant = %q, want %q", i, variant, expected.variant)
		}
		if got := decodeTrendSeries(t, rows[i], "priceTrend"); !equalIntSlices(got, expected.prices) {
			t.Errorf("%s priceTrend = %v, want %v", variant, got, expected.prices)
		}
		if got := decodeTrendSeries(t, rows[i], "listingsTrend"); !equalIntSlices(got, expected.listings) {
			t.Errorf("%s listingsTrend = %v, want %v", variant, got, expected.listings)
		}
		if got := decodeTrendSeries(t, rows[i], "baseListingsTrend"); !equalIntSlices(got, expected.base) {
			t.Errorf("%s baseListingsTrend = %v, want %v", variant, got, expected.base)
		}
	}
}

func TestTrendAnalysis_ColdRepositoryLeavesMissingVariantSeriesAbsent(t *testing.T) {
	pool := offeringIntegrationPool(t)
	leagueID := fmt.Sprintf("POE-WI2-trend-missing-%d", time.Now().UnixNano())
	scope := league.Historical(leagueID)
	registerTrendIntegrationLeague(t, pool, leagueID)

	const (
		transfiguredName = "POE-WI2 Trend Missing of Nova"
		baseName         = "POE-WI2 Trend Missing"
	)
	now := time.Now().UTC().Truncate(time.Second)
	first := now.Add(-2 * time.Hour)
	last := now.Add(-time.Hour)
	cleanupTrendSnapshots(t, pool, leagueID, []string{transfiguredName, baseName}, first, last)
	// The first market has only transformed history; the second has only base
	// history. Each row must keep its available series without borrowing the
	// other market's points.
	seedTrendSnapshot(t, pool, leagueID, first, transfiguredName, "20/20", true, 101, 31, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, transfiguredName, "20/20", true, 112, 29, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, first, baseName, "1/20", false, 1, 13, "BLUE")
	seedTrendSnapshot(t, pool, leagueID, last, baseName, "1/20", false, 1, 11, "BLUE")

	rows := serveTrendIntegration(t, lab.NewRepository(pool), trendIntegrationCache(scope, transfiguredName), scope)
	if len(rows) != 2 {
		t.Fatalf("response rows = %d, want two same-name variants: %+v", len(rows), rows)
	}
	if got := decodeTrendSeries(t, rows[0], "priceTrend"); !equalIntSlices(got, []int{101, 112}) {
		t.Errorf("20/20 priceTrend = %v, want [101 112]", got)
	}
	if _, ok := rows[0]["baseListingsTrend"]; ok {
		t.Errorf("20/20 unexpectedly has base series: %s", rows[0]["baseListingsTrend"])
	}
	if got := decodeTrendSeries(t, rows[1], "baseListingsTrend"); !equalIntSlices(got, []int{13, 11}) {
		t.Errorf("1/20 baseListingsTrend = %v, want [13 11]", got)
	}
	if _, ok := rows[1]["priceTrend"]; ok {
		t.Errorf("1/20 unexpectedly has transformed series: %s", rows[1]["priceTrend"])
	}
}
