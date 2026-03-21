// cmd/run-v2 triggers the v2 analysis pipeline manually.
// Use --backfill to compute features for all historical snapshots (for optimizer/validator).
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/db"
	"profitofexile/internal/lab"
)

func main() {
	backfill := flag.Bool("backfill", false, "Backfill gem_features for all historical snapshot times")
	hours := flag.Int("hours", 168, "Hours of history to backfill")
	flag.Parse()

	ctx := context.Background()
	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		dbURL = "postgresql://profitofexile:profitofexile@postgres:5432/profitofexile"
	}
	pool, err := db.NewPool(ctx, dbURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "DB: %v\n", err)
		os.Exit(1)
	}
	defer pool.Close()

	repo := lab.NewRepository(pool)
	cache := lab.NewCache()
	throttler := lab.NewThrottler("", "", 1*time.Second, cache)
	analyzer := lab.NewAnalyzer(repo, throttler, cache)

	if *backfill {
		runBackfill(ctx, pool, repo, analyzer, *hours)
	} else {
		fmt.Println("Running v2 analysis pipeline (latest snapshot)...")
		if err := analyzer.RunV2(ctx); err != nil {
			fmt.Fprintf(os.Stderr, "RunV2 failed: %v\n", err)
			os.Exit(1)
		}
		// Font analysis needs features from RunV2.
		if err := analyzer.RunFont(ctx); err != nil {
			fmt.Fprintf(os.Stderr, "RunFont failed: %v\n", err)
		}
		fmt.Println("Done!")
	}
}

func runBackfill(ctx context.Context, pool *pgxpool.Pool, repo *lab.Repository, analyzer *lab.Analyzer, hours int) {
	// Get distinct snapshot times.
	q := fmt.Sprintf(`SELECT DISTINCT time FROM gem_snapshots
		WHERE time > NOW() - make_interval(hours => %d)
		ORDER BY time`, hours)
	rows, err := pool.Query(ctx, q)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Query snapshot times: %v\n", err)
		os.Exit(1)
	}
	var times []time.Time
	for rows.Next() {
		var t time.Time
		if err := rows.Scan(&t); err != nil {
			continue
		}
		times = append(times, t)
	}
	rows.Close()

	fmt.Fprintf(os.Stderr, "Found %d snapshots to backfill over %dh\n", len(times), hours)

	// For each snapshot time, load gems at that time and compute features.
	// We use the full history up to that point for velocity/profile computation.
	for i, snapTime := range times {
		if i%10 == 0 {
			fmt.Fprintf(os.Stderr, "  Progress: %d/%d (%.0f%%)\n", i, len(times), float64(i)/float64(len(times))*100)
		}

		// Load gems at this exact snapshot time.
		gems, err := loadGemsAtTime(ctx, pool, snapTime)
		if err != nil || len(gems) == 0 {
			continue
		}

		// Load history up to this snapshot time (168h lookback from this point).
		history, err := loadHistoryUpTo(ctx, pool, snapTime, 168)
		if err != nil {
			continue
		}

		// Compute market context.
		mc := lab.ComputeMarketContext(snapTime, gems, history)
		if err := repo.SaveMarketContext(ctx, mc); err != nil {
			continue // skip on conflict
		}

		// Normalize history using temporal coefficients before computing features.
		normalizedHistory := lab.NormalizeHistoryFromMC(history, mc)

		// Compute gem features.
		features := lab.ComputeGemFeatures(snapTime, gems, normalizedHistory, mc)
		repo.SaveGemFeatures(ctx, features)

		// Compute gem signals (need base history).
		baseHistory := loadBaseHistoryUpTo(ctx, pool, snapTime, 24)
		marketAvgBaseLst := computeMarketAvgBase(gems)
		signals := lab.ComputeGemSignals(snapTime, features, mc, gems, baseHistory, marketAvgBaseLst)
		repo.SaveGemSignals(ctx, signals)

		// Compute font analysis (needs features for tier-based modes).
		fontAnalysis := lab.AnalyzeFont(snapTime, gems, features)
		allFontResults := make([]lab.FontResult, 0, len(fontAnalysis.Safe)+len(fontAnalysis.Premium)+len(fontAnalysis.Jackpot))
		allFontResults = append(allFontResults, fontAnalysis.Safe...)
		allFontResults = append(allFontResults, fontAnalysis.Premium...)
		allFontResults = append(allFontResults, fontAnalysis.Jackpot...)
		if len(allFontResults) > 0 {
			repo.SaveFontResults(ctx, allFontResults)
		}
	}

	fmt.Fprintf(os.Stderr, "Backfill complete: %d snapshots processed\n", len(times))
}

func loadGemsAtTime(ctx context.Context, pool *pgxpool.Pool, t time.Time) ([]lab.GemPrice, error) {
	q := `SELECT name, variant, chaos, listings, is_transfigured, COALESCE(gem_color, '') as gem_color, is_corrupted
	      FROM gem_snapshots WHERE time = $1`
	rows, err := pool.Query(ctx, q, t)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var gems []lab.GemPrice
	for rows.Next() {
		var g lab.GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings, &g.IsTransfigured, &g.GemColor, &g.IsCorrupted); err != nil {
			continue
		}
		gems = append(gems, g)
	}
	return gems, nil
}

func loadHistoryUpTo(ctx context.Context, pool *pgxpool.Pool, upTo time.Time, hours int) ([]lab.GemPriceHistory, error) {
	cutoff := upTo.Add(-time.Duration(hours) * time.Hour)
	q := `SELECT name, variant, COALESCE(gem_color, '') as gem_color, time, chaos, listings
	      FROM gem_snapshots
	      WHERE is_transfigured = true AND NOT is_corrupted AND chaos > 5
	        AND time >= $1 AND time <= $2
	        AND name NOT LIKE '%Trarthus%'
	      ORDER BY name, variant, time
	      LIMIT 500000`
	rows, err := pool.Query(ctx, q, cutoff, upTo)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	histMap := make(map[string]*lab.GemPriceHistory)
	for rows.Next() {
		var name, variant, color string
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&name, &variant, &color, &t, &chaos, &listings); err != nil {
			continue
		}
		key := name + "|" + variant
		h, ok := histMap[key]
		if !ok {
			h = &lab.GemPriceHistory{Name: name, Variant: variant, GemColor: color}
			histMap[key] = h
		}
		h.Points = append(h.Points, lab.PricePoint{Time: t, Chaos: chaos, Listings: listings})
	}

	result := make([]lab.GemPriceHistory, 0, len(histMap))
	for _, h := range histMap {
		result = append(result, *h)
	}
	return result, nil
}

func loadBaseHistoryUpTo(ctx context.Context, pool *pgxpool.Pool, upTo time.Time, hours int) map[string][]lab.PricePoint {
	cutoff := upTo.Add(-time.Duration(hours) * time.Hour)
	q := `SELECT name, time, listings
	      FROM gem_snapshots
	      WHERE NOT is_transfigured AND NOT is_corrupted AND chaos > 0
	        AND time >= $1 AND time <= $2
	        AND name NOT LIKE '%Trarthus%'
	      ORDER BY name, time
	      LIMIT 500000`
	rows, err := pool.Query(ctx, q, cutoff, upTo)
	if err != nil {
		return nil
	}
	defer rows.Close()

	result := make(map[string][]lab.PricePoint)
	for rows.Next() {
		var name string
		var t time.Time
		var listings int
		if err := rows.Scan(&name, &t, &listings); err != nil {
			continue
		}
		result[name] = append(result[name], lab.PricePoint{Time: t, Listings: listings})
	}
	return result
}

func computeMarketAvgBase(gems []lab.GemPrice) float64 {
	var sum, count float64
	for _, g := range gems {
		if !g.IsTransfigured && !g.IsCorrupted && g.Listings > 0 {
			sum += float64(g.Listings)
			count++
		}
	}
	if count == 0 {
		return 0
	}
	return sum / count
}
