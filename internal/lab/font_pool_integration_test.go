//go:build integration

package lab

import (
	"context"
	"math"
	"reflect"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

// keep:critical — silent: Font silently classifies held gems differently from RunV2.
func TestKeepCritical_RunFontClassifiesHeldGemsWithTierInputs(t *testing.T) {
	checkRunFontTierInputs(t, true)
}

// keep:critical — silent: Font discards a stored HIGH after a history gap and moves a held gem down.
func TestKeepCritical_RunFontClampsHighFromTheStoredPreviousBoundary(t *testing.T) {
	checkRunFontTierInputs(t, false)
}

func checkRunFontTierInputs(t *testing.T, recent bool) {
	t.Helper()
	pool := labIntegrationPool(t)
	ctx := context.Background()
	id := "font-tier-stored-boundary"
	if recent {
		id = "font-tier-consecutive"
	}
	scope := tierInputIntegrationLeague(t, pool, id)
	repo := NewRepository(pool)
	asOf := time.Date(2026, 11, 21, 12, 0, 0, 0, time.UTC)
	gems := tierClampGems()
	const target = "Held Clamp of Test"
	gems[2].Name, gems[2].GemColor = target, "GREEN"
	gems[2].Chaos = 49
	seed := func(at time.Time, g GemPrice, price float64, listings int, sample *int) {
		t.Helper()
		seedGemSnapshot(t, pool, id, at, g.Name, g.Variant, true, false, price, listings, g.GemColor)
		if _, err := pool.Exec(ctx, `UPDATE gem_snapshots SET sample_count=$1 WHERE league=$2 AND time=$3 AND name=$4 AND variant=$5`, sample, id, at, g.Name, g.Variant); err != nil {
			t.Fatal(err)
		}
	}
	var features []GemFeature
	for i, g := range gems {
		features = append(features, GemFeature{Time: asOf, Name: g.Name, Variant: g.Variant, GemColor: g.GemColor, Chaos: g.Chaos, Listings: g.Listings, Tier: tier.MidHigh})
		if recent {
			seed(asOf.Add(-60*time.Minute), g, g.Chaos, 20, solidSample())
			seed(asOf.Add(-30*time.Minute), g, g.Chaos, 20, solidSample())
		}
		price, listings, sample := g.Chaos, g.Listings, g.SampleCount
		if i == 2 {
			price, listings, sample = 60, 1, nil
		}
		seed(asOf, g, price, listings, sample)
	}
	priorAt := asOf.Add(-90 * time.Minute)
	if recent {
		priorAt = asOf.Add(-30 * time.Minute)
	} else {
		seed(asOf.Add(-2*time.Hour), gems[2], 49, 20, solidSample())
	}
	mc := testMarketContext()
	mc.Time = priorAt
	mc.VariantStats = map[string]VariantBaseline{"20/20": {Tiers: tier.Boundaries{Names: []string{tier.High}, Boundaries: []float64{38.8234}}}}
	if err := repo.SaveMarketContext(ctx, scope, mc); err != nil {
		t.Fatal(err)
	}
	cache := NewCache(scope)
	analyzer := NewAnalyzer(repo, nil, cache, nil)
	if recent {
		prior := append([]GemFeature(nil), features...)
		for i := range prior {
			prior[i].Time = priorAt
			prior[i].Tier = tier.High
		}
		if _, err := repo.SaveGemFeatures(ctx, scope, prior); err != nil {
			t.Fatal(err)
		}
		if err := analyzer.RunV2(ctx, scope); err != nil {
			t.Fatalf("RunV2: %v", err)
		}
		v2, _ := cache.GemFeatures()
		found := false
		for _, f := range v2 {
			if f.Name == target {
				found = true
				if f.Tier != tier.High {
					t.Errorf("RunV2 held tier=%q, want HIGH", f.Tier)
				}
			}
		}
		if !found {
			t.Error("RunV2 omitted held gem")
		}
	} else {
		cache.SetGemFeatures(features)
	}
	if err := analyzer.RunFont(ctx, scope); err != nil {
		t.Fatalf("RunFont: %v", err)
	}
	analysis, warm := cache.Font()
	if !warm {
		t.Fatal("Font remains cold")
	}
	found := false
	for _, r := range analysis.Safe {
		if r.Color != "GREEN" || r.Variant != "20/20" {
			continue
		}
		found = true
		var count int
		for _, b := range r.PoolBreakdown {
			if b.Tier == tier.High {
				count += b.Count
				if b.MinPrice != 49 || b.MaxPrice != 49 {
					t.Errorf("held HIGH price range=%g..%g, want 49", b.MinPrice, b.MaxPrice)
				}
			}
		}
		if count != 1 {
			t.Errorf("served held HIGH count=%d, want 1 against clamped HIGH 48.52925; breakdown=%+v", count, r.PoolBreakdown)
		}
	}
	if !found {
		t.Error("GREEN/20/20 safe row missing")
	}
}

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
			if r.Pool != 2 || r.PricedGems != 0 || r.Winners != 0 || !r.Time.Equal(now) {
				t.Errorf("cached %s/%s: %+v", mode, key, r)
			}
			assertFontMissingEV(t, r)
		}
	}
	rows, err := pool.Query(ctx, `SELECT color, variant, mode, pool, priced_gems, ev, profit, winners
		FROM font_snapshots WHERE league = $1 AND time = $2`, leagueID, now)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	count := 0
	for rows.Next() {
		var color, variant, mode string
		var size, priced, winners int
		var ev *float64
		var profit float64
		if err := rows.Scan(&color, &variant, &mode, &size, &priced, &ev, &profit, &winners); err != nil {
			t.Fatal(err)
		}
		count++
		if size != 2 || priced != 0 || ev != nil || profit != 0 || winners != 0 {
			t.Errorf("persisted %s/%s/%s: pool=%d priced=%d ev=%v profit=%g winners=%d, want 2/0/NULL/0/0", color, variant, mode, size, priced, ev, profit, winners)
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
			if r.Pool != 3 || r.PricedGems != 3 || math.Abs(fontNumber(r.EVRaw)-100) > 1e-9 || math.Abs(fontNumber(r.EV)-36) > 1e-9 || len(r.LowConfidenceGems) != 0 {
				t.Errorf("cached %s held Font: pool=%d priced=%d EVRaw=%v EV=%v exclusions=%+v, want 3/3/100/36/none", mode, r.Pool, r.PricedGems, r.EVRaw, r.EV, r.LowConfidenceGems)
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

// keep:critical — silent: an existing feature keeps an uncapped price above the
// cutoff, so Font EV and coverage silently count a held key at a price the hold forbids.
func TestKeepCritical_RunFontExcludesKeyHeldBelowTierCutoff(t *testing.T) {
	pool := labIntegrationPool(t)
	ctx := context.Background()
	const leagueID = "font-held-below-cutoff"
	registerLeague(t, pool, leagueID)
	scope := league.Historical(leagueID)
	now := futureTime(104)
	lastSolid := now.Add(-2 * time.Hour)
	cleanupAtTime(t, pool, now, "font_snapshots", "gem_snapshots")
	cleanupAtTime(t, pool, lastSolid, "gem_snapshots")
	// 8c thin now against a 4c floor-SOLID point: the inclusive 2x edge holds at 4c.
	seedGemSnapshot(t, pool, leagueID, lastSolid, heldName, "20/20", true, false, 4, 9, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, heldName, "20/20", true, false, 8, 1, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, "Font Peer A", "20/20", true, false, 6, 50, "BLUE")
	seedGemSnapshot(t, pool, leagueID, now, "Font Peer B", "20/20", true, false, 6, 50, "BLUE")
	if _, err := pool.Exec(ctx, `UPDATE gem_snapshots SET sample_count = listings
		WHERE league = $1 AND (time = $2 OR time = $3)`, leagueID, now, lastSolid); err != nil {
		t.Fatal(err)
	}
	cache := NewCache(scope)
	cache.SetGemFeatures([]GemFeature{
		{Time: lastSolid, Name: heldName, Variant: "20/20", GemColor: "BLUE", Chaos: 8, Listings: 9, Tier: "LOW"},
		{Time: now, Name: "Font Peer A", Variant: "20/20", GemColor: "BLUE", Chaos: 6, Listings: 50, Tier: "LOW"},
		{Time: now, Name: "Font Peer B", Variant: "20/20", GemColor: "BLUE", Chaos: 6, Listings: 50, Tier: "LOW"},
	})
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
			if r.Pool != 3 || r.PricedGems != 3 || math.Abs(fontNumber(r.EVRaw)-6) > 1e-9 {
				t.Errorf("cached %s Font: pool=%d priced=%d EVRaw=%v, want 3/3/6 (held 4c key counts as SOLID below the 5c cutoff)", mode, r.Pool, r.PricedGems, r.EVRaw)
			}
		}
		if !found {
			t.Errorf("cached %s BLUE/20/20 row missing", mode)
		}
	}
	rows, err := pool.Query(ctx, `SELECT mode, pool, priced_gems, profit + input_cost
		FROM font_snapshots WHERE league = $1 AND time = $2 AND color = 'BLUE' AND variant = '20/20'`, leagueID, now)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	count := 0
	for rows.Next() {
		var mode string
		var size, priced int
		var raw float64
		if err := rows.Scan(&mode, &size, &priced, &raw); err != nil {
			t.Fatal(err)
		}
		count++
		if size != 3 || priced != 3 || math.Abs(raw-6) > 1e-9 {
			t.Errorf("persisted %s Font: pool=%d priced=%d EVRaw=%g, want 3/3/6", mode, size, priced, raw)
		}
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if count != 3 {
		t.Errorf("persisted BLUE/20/20 rows = %d, want 3", count)
	}
}

// keep:critical — silent: ignoring a held key makes a covered market appear
// under-covered, withholding the EV consumed by the overview and BEST.
func TestKeepCritical_RunFontCountsAHeldKeyAsCovered(t *testing.T) {
	for _, held := range []bool{false, true} {
		t.Run(map[bool]string{false: "unheld", true: "held"}[held], func(t *testing.T) {
			pool := labIntegrationPool(t)
			ctx := context.Background()
			leagueID := "font-coverage-unheld"
			if held {
				leagueID = "font-coverage-held"
			}
			registerLeague(t, pool, leagueID)
			scope := league.Historical(leagueID)
			now := futureTime(105)
			lastSolid := now.Add(-time.Hour)
			cleanupAtTime(t, pool, now, "font_snapshots", "gem_snapshots")
			cleanupAtTime(t, pool, lastSolid, "gem_snapshots")
			seed := func(at time.Time, name, variant string, chaos float64, listings, sample int) {
				t.Helper()
				_, err := pool.Exec(ctx, `INSERT INTO gem_snapshots
					(league, time, name, variant, is_transfigured, is_corrupted, chaos, listings, sample_count, gem_color)
					VALUES ($1, $2, $3, $4, true, false, $5, $6, $7, 'BLUE')`, leagueID, at, name, variant, chaos, listings, sample)
				if err != nil {
					t.Fatal(err)
				}
			}
			seed(now, "Coverage Solid of Test", "20/20", 10, 20, 20)
			seed(now, "Coverage Thin of Test", "20/20", 12, 1, 2)
			for _, name := range []string{"Coverage A of Test", "Coverage B of Test", "Coverage C of Test"} {
				seed(now, name, "1", 3, 20, 20)
			}
			if held {
				seed(lastSolid, "Coverage Thin of Test", "20/20", 10, 20, 20)
			}
			cache := NewCache(scope)
			cache.SetGemFeatures([]GemFeature{})
			if err := NewAnalyzer(NewRepository(pool), nil, cache, nil).RunFont(ctx, scope); err != nil {
				t.Fatalf("RunFont: %v", err)
			}
			analysis, warm := cache.Font()
			if !warm {
				t.Fatal("Font cache remains cold")
			}
			wantPriced := 1
			if held {
				wantPriced = 2
			}
			for _, results := range [][]FontResult{analysis.Safe, analysis.Premium, analysis.Jackpot} {
				found := false
				for _, r := range results {
					if r.Color == "BLUE" && r.Variant == "20/20" {
						found = true
						assertFontCoverage(t, r, 5, wantPriced, float64(wantPriced)/5, held)
					}
				}
				if !found {
					t.Error("cached BLUE 20/20 row missing")
				}
			}
			rows, err := pool.Query(ctx, `SELECT mode, pool, priced_gems, ev FROM font_snapshots
				WHERE league = $1 AND time = $2 AND color = 'BLUE' AND variant = '20/20'`, leagueID, now)
			if err != nil {
				t.Fatal(err)
			}
			defer rows.Close()
			count := 0
			for rows.Next() {
				var mode string
				var size, priced int
				var ev *float64
				if err := rows.Scan(&mode, &size, &priced, &ev); err != nil {
					t.Fatal(err)
				}
				count++
				if size != 5 || priced != wantPriced || (ev != nil) != held {
					t.Errorf("persisted %s pool/priced/ev = %d/%d/%v, want 5/%d/served=%v", mode, size, priced, ev, wantPriced, held)
				}
			}
			if err := rows.Err(); err != nil {
				t.Fatal(err)
			}
			if count != 3 {
				t.Errorf("persisted BLUE 20/20 modes = %d, want 3", count)
			}
		})
	}
}
