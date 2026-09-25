//go:build integration

package handlers

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

type collectiveSparklineSeed struct {
	time     time.Time
	variant  string
	price    float64
	listings int
}

type collectiveSparklinePoint struct {
	ago      time.Duration
	price    float64
	listings int
}

func registerCollectiveSparklineFixture(t *testing.T, pool *pgxpool.Pool, scope league.Scope, name string, seeds []collectiveSparklineSeed) {
	t.Helper()
	ctx := context.Background()
	if _, err := pool.Exec(ctx,
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, scope.ID()); err != nil {
		t.Fatalf("register league %q: %v", scope.ID(), err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM gem_snapshots WHERE league = $1 AND name = $2`, scope.ID(), name); err != nil {
			t.Logf("cleanup warning: delete gem_snapshots for league %q: %v", scope.ID(), err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id = $1`, scope.ID()); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", scope.ID(), err)
		}
	})

	for _, seed := range seeds {
		if _, err := pool.Exec(ctx, `
			INSERT INTO gem_snapshots
				(league, time, name, variant, is_corrupted, is_transfigured, chaos, listings, gem_color)
			VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`,
			scope.ID(), seed.time, name, seed.variant, false, true, seed.price, seed.listings, "BLUE"); err != nil {
			t.Fatalf("seed collective sparkline (league %q, variant %q): %v", scope.ID(), seed.variant, err)
		}
	}
}

func collectiveSparklinePoints(now time.Time, variant string, pricesAndListings ...collectiveSparklinePoint) []collectiveSparklineSeed {
	seeds := make([]collectiveSparklineSeed, 0, len(pricesAndListings))
	for _, point := range pricesAndListings {
		seeds = append(seeds, collectiveSparklineSeed{
			time:     now.Add(-point.ago),
			variant:  variant,
			price:    point.price,
			listings: point.listings,
		})
	}
	return seeds
}

func TestCollectiveAnalysis_ColdDatabaseKeepsSameNameVariantSeriesSeparate(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := league.Historical("POE133-Collective-Separate")
	name := "Spark of Nova"
	now := time.Now().UTC().Truncate(time.Second)
	twentyTwenty := []lab.SparklinePoint{
		sparkAt(now, 2*time.Hour, 125, 26),
		sparkAt(now, time.Hour, 140, 22),
	}
	oneTwenty := []lab.SparklinePoint{
		sparkAt(now, 2*time.Hour, 9, 90),
		sparkAt(now, time.Hour, 11, 84),
	}
	registerCollectiveSparklineFixture(t, pool, scope, name, append(
		collectiveSparklinePoints(now, "20/20",
			collectiveSparklinePoint{2 * time.Hour, 125, 26},
			collectiveSparklinePoint{time.Hour, 140, 22},
		),
		collectiveSparklinePoints(now, "1/20",
			collectiveSparklinePoint{2 * time.Hour, 9, 90},
			collectiveSparklinePoint{time.Hour, 11, 84},
		)...),
	)
	cache := sameNameVariantRankingCacheForScope(t, scope,
		sameNameVariantFixture{variant: "20/20", roi: 100},
		sameNameVariantFixture{variant: "1/20", roi: 10},
	)

	w := httptest.NewRecorder()
	CollectiveAnalysis(lab.NewRepository(pool), cache, scope).ServeHTTP(w,
		httptest.NewRequest(http.MethodGet, "/api/analysis/collective", nil))
	rows := decodeCollectiveSparklineRows(t, w)
	if len(rows) != 2 {
		t.Fatalf("response rows = %d, want two same-name variants: %+v", len(rows), rows)
	}
	if rows[0].Variant != "20/20" || rows[1].Variant != "1/20" {
		t.Fatalf("response variants = (%q, %q), want (20/20, 1/20)", rows[0].Variant, rows[1].Variant)
	}
	assertSparkline(t, rows[0].Sparkline, twentyTwenty)
	assertSparkline(t, rows[1].Sparkline, oneTwenty)
}

func TestCollectiveAnalysis_ColdDatabaseLeavesMissingVariantSeriesEmpty(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := league.Historical("POE133-Collective-Missing")
	name := "Spark of Nova"
	now := time.Now().UTC().Truncate(time.Second)
	want := []lab.SparklinePoint{
		sparkAt(now, 2*time.Hour, 125, 26),
		sparkAt(now, time.Hour, 140, 22),
	}
	registerCollectiveSparklineFixture(t, pool, scope, name, collectiveSparklinePoints(now, "20/20",
		collectiveSparklinePoint{2 * time.Hour, 125, 26},
		collectiveSparklinePoint{time.Hour, 140, 22},
	))
	cache := sameNameVariantRankingCacheForScope(t, scope,
		sameNameVariantFixture{variant: "20/20", roi: 100},
		sameNameVariantFixture{variant: "1/20", roi: 10},
	)

	w := httptest.NewRecorder()
	CollectiveAnalysis(lab.NewRepository(pool), cache, scope).ServeHTTP(w,
		httptest.NewRequest(http.MethodGet, "/api/analysis/collective", nil))
	rows := decodeCollectiveSparklineRows(t, w)
	if len(rows) != 2 {
		t.Fatalf("response rows = %d, want two same-name variants: %+v", len(rows), rows)
	}
	assertSparkline(t, rows[0].Sparkline, want)
	if rows[1].Sparkline == nil {
		t.Fatal("missing 1/20 sparkline = null, want an empty array")
	}
	if len(rows[1].Sparkline) != 0 {
		t.Errorf("missing 1/20 sparkline = %+v, want empty", rows[1].Sparkline)
	}
}

func TestCollectiveAnalysis_ColdDatabaseExplicitVariantUsesRequestedMarketSeries(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := league.Historical("POE133-Collective-Explicit")
	name := "Spark of Nova"
	now := time.Now().UTC().Truncate(time.Second)
	twentyTwenty := []lab.SparklinePoint{sparkAt(now, time.Hour, 125, 26)}
	oneTwenty := []lab.SparklinePoint{sparkAt(now, time.Hour, 9, 90)}
	registerCollectiveSparklineFixture(t, pool, scope, name, append(
		collectiveSparklinePoints(now, "20/20", collectiveSparklinePoint{time.Hour, 125, 26}),
		collectiveSparklinePoints(now, "1/20", collectiveSparklinePoint{time.Hour, 9, 90})...,
	))
	cache := sameNameVariantRankingCacheForScope(t, scope,
		sameNameVariantFixture{variant: "20/20", roi: 100},
		sameNameVariantFixture{variant: "1/20", roi: 10},
	)

	for _, tt := range []struct {
		variant string
		want    []lab.SparklinePoint
	}{
		{variant: "20/20", want: twentyTwenty},
		{variant: "1/20", want: oneTwenty},
	} {
		t.Run(tt.variant, func(t *testing.T) {
			w := httptest.NewRecorder()
			CollectiveAnalysis(lab.NewRepository(pool), cache, scope).ServeHTTP(w,
				httptest.NewRequest(http.MethodGet, "/api/analysis/collective?variant="+tt.variant, nil))
			rows := decodeCollectiveSparklineRows(t, w)
			if len(rows) != 1 {
				t.Fatalf("response rows = %d, want one: %+v", len(rows), rows)
			}
			if rows[0].Variant != tt.variant {
				t.Fatalf("response variant = %q, want %q", rows[0].Variant, tt.variant)
			}
			assertSparkline(t, rows[0].Sparkline, tt.want)
		})
	}
}
