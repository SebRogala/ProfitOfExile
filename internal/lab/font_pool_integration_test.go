//go:build integration

package lab

import (
	"context"
	"math"
	"reflect"
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

// keep:critical — silent: Font coverage and EV trust stale features instead of held valuations.
func TestKeepCritical_RunFontUsesHeldPrices(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	const leagueID = "font-held-price"
	registerLeague(t, pool, leagueID)
	scope := league.Historical(leagueID)
	now := futureTime(103)
	lastSolid := now.Add(-2 * time.Hour)
	cleanupAtTime(t, pool, now, "font_snapshots", "gem_snapshots")
	cleanupAtTime(t, pool, lastSolid, "gem_snapshots")
	seedGemSnapshot(t, pool, leagueID, lastSolid, heldName, "20/20", true, false, 100, 9, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, heldName, "20/20", true, false, 120, 1, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, "Font Peer A", "20/20", true, false, 10, 50, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, "Font Peer B", "20/20", true, false, 8, 50, "BLUE")
	if _, err := pool.Exec(ctx, `UPDATE gem_snapshots SET sample_count = listings
		WHERE league = $1 AND (time = $2 OR time = $3)`, leagueID, now, lastSolid); err != nil {
		t.Fatal(err)
	}
	features := []GemFeature{
		{Time: lastSolid, Name: heldName, Variant: "20/20", GemColor: "BLUE", Chaos: 120, Listings: 1,
			Tier: "CHAOTIC", LowConfidence: true, Low7Days: 80, CVShort: 40, VelShortPrice: 17},
		{Time: now, Name: "Font Peer A", Variant: "20/20", GemColor: "BLUE", Chaos: 10, Listings: 50, Tier: "LOW"},
		{Time: now, Name: "Font Peer B", Variant: "20/20", GemColor: "BLUE", Chaos: 8, Listings: 50, Tier: "LOW"},
	}
	before := append([]GemFeature(nil), features...)
	cache := NewCache(scope)
	cache.SetGemFeatures(features)
	if err := NewAnalyzer(NewRepository(pool), nil, cache, nil).RunFont(ctx, scope); err != nil {
		t.Fatalf("RunFont: %v", err)
	}
	analysis, warm := cache.Font()
	if !warm {
		t.Fatal("Font cache remains cold")
	}
	for mode, results := range map[string][]FontResult{"safe": analysis.Safe, "premium": analysis.Premium, "jackpot": analysis.Jackpot} {
		found := false
		for _, r := range results {
			if r.Color != "BLUE" || r.Variant != "20/20" {
				continue
			}
			found = true
			// Three draws from three outcomes always include the 100c held key;
			// its retained risk inputs yield 100 × 0.45 × 0.8 = 36c.
			if r.Pool != 3 || r.PricedGems != 3 || math.Abs(r.EVRaw-100) > 1e-9 || math.Abs(r.EV-36) > 1e-9 || len(r.LowConfidenceGems) != 0 {
				t.Errorf("cached %s held Font: pool=%d priced=%d EVRaw=%g EV=%g exclusions=%+v, want 3/3/100/36/none", mode, r.Pool, r.PricedGems, r.EVRaw, r.EV, r.LowConfidenceGems)
			}
		}
		if !found {
			t.Errorf("cached %s BLUE/20/20 row missing", mode)
		}
	}
	rows, err := pool.Query(ctx, `SELECT mode, pool, priced_gems, ev, profit + input_cost
		FROM font_snapshots WHERE league = $1 AND time = $2 AND color = 'BLUE' AND variant = '20/20'`, leagueID, now)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	count := 0
	for rows.Next() {
		var mode string
		var size, priced int
		var ev, raw float64
		if err := rows.Scan(&mode, &size, &priced, &ev, &raw); err != nil {
			t.Fatal(err)
		}
		count++
		if size != 3 || priced != 3 || math.Abs(raw-100) > 1e-9 || math.Abs(ev-36) > 1e-9 {
			t.Errorf("persisted %s held Font: pool=%d priced=%d EVRaw=%g EV=%g, want 3/3/100/36", mode, size, priced, raw, ev)
		}
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if count != 3 {
		t.Errorf("persisted BLUE/20/20 rows = %d, want 3", count)
	}
	after, _ := cache.GemFeatures()
	if !reflect.DeepEqual(after, before) || !reflect.DeepEqual(features, before) {
		t.Errorf("RunFont mutated cached features: got %+v, want %+v", after, before)
	}
	var observed float64
	if err := pool.QueryRow(ctx, `SELECT chaos FROM gem_snapshots WHERE league = $1 AND time = $2 AND name = $3 AND variant = '20/20'`, leagueID, now, heldName).Scan(&observed); err != nil {
		t.Fatal(err)
	}
	if observed != 120 {
		t.Errorf("stored observed price = %g, want 120", observed)
	}
}
