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
	"profitofexile/internal/gemrule"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
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

	scope, err := league.Resolve(ctx, pool)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Resolve league: %v\n", err)
		os.Exit(1)
	}

	repo := lab.NewRepository(pool)
	cache := lab.NewCache(scope)
	throttler := lab.NewThrottler("", "", 1*time.Second, cache)
	analyzer := lab.NewAnalyzer(repo, throttler, cache, nil)

	if *backfill {
		if err := runBackfill(ctx, pool, repo, analyzer, scope, *hours); err != nil {
			fmt.Fprintf(os.Stderr, "Backfill failed: %v\n", err)
			os.Exit(1)
		}
	} else {
		fmt.Println("Running v2 analysis pipeline (latest snapshot)...")
		if err := analyzer.RunV2(ctx, scope); err != nil {
			fmt.Fprintf(os.Stderr, "RunV2 failed: %v\n", err)
			os.Exit(1)
		}
		// Font analysis needs features from RunV2.
		if err := analyzer.RunFont(ctx, scope); err != nil {
			fmt.Fprintf(os.Stderr, "RunFont failed: %v\n", err)
		}
		fmt.Println("Done!")
	}
}

func runBackfill(ctx context.Context, pool *pgxpool.Pool, repo *lab.Repository, analyzer *lab.Analyzer, scope league.Scope, hours int) error {
	// Get distinct snapshot times.
	q := fmt.Sprintf(`SELECT DISTINCT time FROM gem_snapshots
		WHERE league = $1 AND time > NOW() - make_interval(hours => %d)
		ORDER BY time`, hours)
	rows, err := pool.Query(ctx, q, scope.ID())
	if err != nil {
		return fmt.Errorf("query snapshot times: %w", err)
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
		gems, priceTime, err := repo.GemPricesAsOf(ctx, scope, snapTime)
		if err != nil {
			return fmt.Errorf("snapshot %s: %w", snapTime.Format(time.RFC3339), err)
		}
		gems, err = lab.LoadEvidenceHolds(ctx, repo, scope, gems, priceTime)
		if err != nil {
			return fmt.Errorf("snapshot %s: %w", snapTime.Format(time.RFC3339), err)
		}
		if len(gems) == 0 {
			continue
		}

		// Load history up to this snapshot time (168h lookback from this point).
		history, err := repo.GemPriceHistoryByVariant(ctx, scope, "", snapTime.Add(-168*time.Hour), snapTime)
		if err != nil {
			return fmt.Errorf("snapshot %s: %w", snapTime.Format(time.RFC3339), err)
		}

		// Step 0: Unified gem classification.
		classification := lab.ComputeGemClassification(gems)

		// Compute market context.
		mc := lab.ComputeMarketContext(snapTime, gems, history, classification)
		if err := repo.SaveMarketContext(ctx, scope, mc); err != nil {
			continue // skip on conflict
		}

		// Normalize history using temporal coefficients before computing features.
		depthMap := lab.PrecomputeMarketDepth(gems, mc)
		normalizedHistory := lab.NormalizeHistoryDepthGated(history, mc, depthMap)

		// Compute gem features.
		features := lab.ComputeGemFeatures(snapTime, gems, normalizedHistory, mc, classification.Gems, nil)
		repo.SaveGemFeatures(ctx, scope, features)

		// Compute gem signals (need base history).
		baseHistory := loadBaseHistoryUpTo(ctx, pool, scope, snapTime, 24)
		marketAvgBaseLst := computeMarketAvgBase(gems)
		signals := lab.ComputeGemSignals(snapTime, features, mc, gems, baseHistory, marketAvgBaseLst)
		repo.SaveGemSignals(ctx, scope, signals)

		// Compute font analysis (needs features for tier-based modes).
		fontAnalysis := lab.AnalyzeFont(snapTime, gems, features)
		allFontResults := make([]lab.FontResult, 0, len(fontAnalysis.Safe)+len(fontAnalysis.Premium)+len(fontAnalysis.Jackpot))
		allFontResults = append(allFontResults, fontAnalysis.Safe...)
		allFontResults = append(allFontResults, fontAnalysis.Premium...)
		allFontResults = append(allFontResults, fontAnalysis.Jackpot...)
		if len(allFontResults) > 0 {
			repo.SaveFontResults(ctx, scope, allFontResults)
		}
	}

	fmt.Fprintf(os.Stderr, "Backfill complete: %d snapshots processed\n", len(times))
	return nil
}

func loadBaseHistoryUpTo(ctx context.Context, pool *pgxpool.Pool, scope league.Scope, upTo time.Time, hours int) map[string][]lab.PricePoint {
	cutoff := upTo.Add(-time.Duration(hours) * time.Hour)
	q := `SELECT name, time, listings
	      FROM gem_snapshots
	      WHERE league = $1 AND NOT is_transfigured AND NOT is_corrupted AND chaos > 0
	        AND time >= $2 AND time <= $3
	        AND ` + gemrule.SQLNotHeistOnly + `
	      ORDER BY name, time
	      LIMIT 500000`
	rows, err := pool.Query(ctx, q, scope.ID(), cutoff, upTo)
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
