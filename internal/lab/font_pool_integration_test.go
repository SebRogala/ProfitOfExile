//go:build integration

package lab

import (
	"context"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestRunFont_WarmEmptyFeaturesPersistCompletePools(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	leagueID := "font-pool-warm-empty"
	registerLeague(t, pool, leagueID)
	scope := league.Historical(leagueID)
	now := time.Date(2026, 11, 16, 13, 2, 0, 0, time.UTC)
	cleanupAtTime(t, pool, now, "font_snapshots", "gem_snapshots")
	for _, color := range []string{"RED", "GREEN", "BLUE"} {
		seedGemSnapshot(t, pool, leagueID, now, color+" Boundary", "20/20", true, false, 5, 50, color)
		seedGemSnapshot(t, pool, leagueID, now, color+" Unpriced", "1", true, false, 0, 0, color)
	}
	cache := NewCache(scope)
	cache.SetGemFeatures([]GemFeature{})
	analyzer := NewAnalyzer(NewRepository(pool), nil, cache, nil)
	if err := analyzer.RunFont(ctx, scope); err != nil {
		t.Fatalf("RunFont: %v", err)
	}
	analysis, warm := cache.Font()
	if !warm {
		t.Fatal("Font cache remains cold")
	}
	for mode, results := range map[string][]FontResult{"safe": analysis.Safe, "premium": analysis.Premium, "jackpot": analysis.Jackpot} {
		if len(results) != 12 {
			t.Fatalf("cached %s rows = %d, want 12", mode, len(results))
		}
		seen := make(map[string]bool)
		for _, r := range results {
			key := r.Color + "/" + r.Variant
			if seen[key] {
				t.Errorf("duplicate cached %s/%s", mode, key)
			}
			seen[key] = true
			if r.Pool != 2 || r.PricedGems != 0 || r.EV != 0 || r.EVRaw != 0 || r.Winners != 0 || !r.Time.Equal(now) {
				t.Errorf("cached %s/%s: %+v", mode, key, r)
			}
		}
	}
	rows, err := pool.Query(ctx, `SELECT color, variant, mode, pool, priced_gems, ev, profit + input_cost, winners
		FROM font_snapshots WHERE league = $1 AND time = $2`, leagueID, now)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	count := 0
	for rows.Next() {
		var color, variant, mode string
		var size, priced, winners int
		var ev, evRaw float64
		if err := rows.Scan(&color, &variant, &mode, &size, &priced, &ev, &evRaw, &winners); err != nil {
			t.Fatal(err)
		}
		count++
		if size != 2 || priced != 0 || ev != 0 || evRaw != 0 || winners != 0 {
			t.Errorf("persisted %s/%s/%s: pool=%d priced=%d ev=%g raw=%g winners=%d", color, variant, mode, size, priced, ev, evRaw, winners)
		}
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if count != 36 {
		t.Errorf("persisted rows = %d, want 36", count)
	}
}
