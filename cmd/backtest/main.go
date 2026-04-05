// cmd/backtest scores v2 and v3 (trade-enriched) signals against subsequent
// gem_snapshots to measure signal accuracy. The tool iterates historical
// snapshot times within the trade data overlap window (since 2026-03-16),
// recomputes signals for each, then checks whether the signal correctly
// described the market state in short-term (T+1h) and medium-term (T+4h)
// follow-up windows. Time-based, not snapshot-count-based.
//
// Usage:
//
//	go run ./cmd/backtest --hours 168 --sample 4
//	go run ./cmd/backtest --since 2026-03-16
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"os"
	"sort"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/db"
	"profitofexile/internal/lab"
	"profitofexile/internal/trade"
)

func main() {
	hours := flag.Int("hours", 168, "Hours of history to backtest (from now)")
	since := flag.String("since", "", "Start date (YYYY-MM-DD), overrides --hours")
	sample := flag.Int("sample", 4, "Process every Nth snapshot (1 = all)")
	jsonOut := flag.Bool("json", false, "Output results as JSON instead of human-readable")
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

	// Determine time range.
	var startTime time.Time
	if *since != "" {
		t, err := time.Parse("2006-01-02", *since)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Invalid --since format: %v\n", err)
			os.Exit(1)
		}
		startTime = t
	} else {
		startTime = time.Now().Add(-time.Duration(*hours) * time.Hour)
	}

	// We need at least 4h of future data after each snapshot for medium-term scoring.
	endTime := time.Now().Add(-4 * time.Hour)

	if startTime.After(endTime) {
		fmt.Fprintf(os.Stderr, "Time range too short: start %s > end %s\n", startTime.Format(time.RFC3339), endTime.Format(time.RFC3339))
		os.Exit(1)
	}

	fmt.Fprintf(os.Stderr, "Backtest: %s to %s (sample every %d)\n",
		startTime.Format("2006-01-02 15:04"), endTime.Format("2006-01-02 15:04"), *sample)

	// 1. Load distinct snapshot times in range.
	snapTimes, err := loadSnapshotTimes(ctx, pool, startTime, endTime)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Load snapshot times: %v\n", err)
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Found %d snapshots in range\n", len(snapTimes))

	// Subsample.
	if *sample > 1 {
		var sampled []time.Time
		for i, t := range snapTimes {
			if i%*sample == 0 {
				sampled = append(sampled, t)
			}
		}
		snapTimes = sampled
		fmt.Fprintf(os.Stderr, "After sampling: %d snapshots\n", len(snapTimes))
	}

	if len(snapTimes) == 0 {
		fmt.Fprintf(os.Stderr, "No snapshots to process\n")
		os.Exit(0)
	}

	// 2. Process each snapshot.
	var allResults []snapshotResult
	for i, snapTime := range snapTimes {
		if i%10 == 0 {
			fmt.Fprintf(os.Stderr, "  Progress: %d/%d (%.0f%%)\n", i, len(snapTimes), float64(i)/float64(len(snapTimes))*100)
		}

		res, err := processSnapshot(ctx, pool, snapTime)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  Skip %s: %v\n", snapTime.Format("2006-01-02 15:04"), err)
			continue
		}
		if res != nil {
			allResults = append(allResults, *res)
		}
	}

	fmt.Fprintf(os.Stderr, "Processed %d snapshots successfully\n\n", len(allResults))

	// 3. Aggregate and output.
	report := aggregateResults(allResults)

	if *jsonOut {
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		if err := enc.Encode(report); err != nil {
			fmt.Fprintf(os.Stderr, "JSON encode: %v\n", err)
			os.Exit(1)
		}
	} else {
		printReport(report)
	}
}

// ---------------------------------------------------------------------------
// Data loading
// ---------------------------------------------------------------------------

func loadSnapshotTimes(ctx context.Context, pool *pgxpool.Pool, start, end time.Time) ([]time.Time, error) {
	rows, err := pool.Query(ctx,
		`SELECT DISTINCT time FROM gem_snapshots
		 WHERE time >= $1 AND time <= $2
		 ORDER BY time`, start, end)
	if err != nil {
		return nil, fmt.Errorf("query snapshot times: %w", err)
	}
	defer rows.Close()

	var times []time.Time
	for rows.Next() {
		var t time.Time
		if err := rows.Scan(&t); err != nil {
			continue
		}
		times = append(times, t)
	}
	return times, rows.Err()
}

func loadGemsAtTime(ctx context.Context, pool *pgxpool.Pool, t time.Time) ([]lab.GemPrice, error) {
	rows, err := pool.Query(ctx,
		`SELECT name, variant, chaos, listings, is_transfigured, COALESCE(gem_color, '') as gem_color, is_corrupted
		 FROM gem_snapshots WHERE time = $1`, t)
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
	return gems, rows.Err()
}

func loadHistoryUpTo(ctx context.Context, pool *pgxpool.Pool, upTo time.Time, hours int) ([]lab.GemPriceHistory, error) {
	cutoff := upTo.Add(-time.Duration(hours) * time.Hour)
	rows, err := pool.Query(ctx,
		`SELECT name, variant, COALESCE(gem_color, '') as gem_color, time, chaos, listings
		 FROM gem_snapshots
		 WHERE is_transfigured = true AND NOT is_corrupted AND chaos > 5
		   AND time >= $1 AND time <= $2
		   AND name NOT LIKE '%Trarthus%'
		 ORDER BY name, variant, time
		 LIMIT 500000`, cutoff, upTo)
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
	return result, rows.Err()
}

func loadBaseHistoryUpTo(ctx context.Context, pool *pgxpool.Pool, upTo time.Time, hours int) map[string][]lab.PricePoint {
	cutoff := upTo.Add(-time.Duration(hours) * time.Hour)
	rows, err := pool.Query(ctx,
		`SELECT name, time, listings
		 FROM gem_snapshots
		 WHERE NOT is_transfigured AND NOT is_corrupted AND chaos > 0
		   AND time >= $1 AND time <= $2
		   AND name NOT LIKE '%Trarthus%'
		 ORDER BY name, time
		 LIMIT 500000`, cutoff, upTo)
	if err != nil {
		fmt.Fprintf(os.Stderr, "  loadBaseHistoryUpTo: query error: %v\n", err)
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
	if err := rows.Err(); err != nil {
		fmt.Fprintf(os.Stderr, "  loadBaseHistoryUpTo: rows error: %v\n", err)
	}
	return result
}

// loadTradeDataNear loads the nearest trade_lookups row per gem+variant within
// a +-90min window of the target time. Returns a populated TradeCache.
func loadTradeDataNear(ctx context.Context, pool *pgxpool.Pool, target time.Time) *trade.TradeCache {
	lo := target.Add(-90 * time.Minute)
	hi := target.Add(90 * time.Minute)

	rows, err := pool.Query(ctx,
		`SELECT DISTINCT ON (gem, variant)
		        time, gem, variant,
		        COALESCE(total_listings, 0),
		        COALESCE(price_floor, 0),
		        COALESCE(price_ceiling, 0),
		        COALESCE(median_top10, 0),
		        COALESCE(divine_rate, 0),
		        COALESCE(listings, '[]'::jsonb)
		 FROM trade_lookups
		 WHERE time >= $1 AND time <= $2
		 ORDER BY gem, variant, ABS(EXTRACT(EPOCH FROM time - $3)) ASC`,
		lo, hi, target)
	if err != nil {
		return nil
	}
	defer rows.Close()

	var results []trade.TradeLookupResult
	for rows.Next() {
		var r trade.TradeLookupResult
		var listingsJSON []byte
		if err := rows.Scan(
			&r.FetchedAt, &r.Gem, &r.Variant, &r.Total,
			&r.PriceFloor, &r.PriceCeiling, &r.MedianTop10,
			&r.DivinePrice, &listingsJSON,
		); err != nil {
			continue
		}
		if err := json.Unmarshal(listingsJSON, &r.Listings); err != nil {
			r.Listings = nil
		}
		if r.Listings != nil {
			r.Signals = trade.ComputeSignals(r.Listings)
		}
		results = append(results, r)
	}

	if err := rows.Err(); err != nil {
		fmt.Fprintf(os.Stderr, "  loadTradeDataNear: rows error: %v\n", err)
	}

	if len(results) == 0 {
		return nil
	}

	tc := trade.NewTradeCache(len(results))
	tc.Warm(results)
	return tc
}

// loadFutureSnapshots loads gem prices at specific future time offsets (durations)
// following the target time. For each duration, finds the nearest snapshot within
// a 15-minute tolerance window. Returns a map from offset index to gem prices.
// durations maps an arbitrary key to a time offset (e.g., 0 → 1h, 1 → 4h).
func loadFutureSnapshots(ctx context.Context, pool *pgxpool.Pool, after time.Time, durations map[int]time.Duration) (map[int][]lab.GemPrice, error) {
	// Find the nearest snapshot time to each target time.
	const tolerance = 15 * time.Minute

	result := make(map[int][]lab.GemPrice)
	for key, dur := range durations {
		target := after.Add(dur)
		// Find closest snapshot within tolerance.
		rows, err := pool.Query(ctx,
			`SELECT time FROM (
			   SELECT DISTINCT time FROM gem_snapshots
			   WHERE time BETWEEN $1 AND $2
			 ) t
			 ORDER BY ABS(EXTRACT(EPOCH FROM time - $3))
			 LIMIT 1`, target.Add(-tolerance), target.Add(tolerance), target)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  WARN: future snapshot query failed for offset %v: %v\n", dur, err)
			continue
		}

		var snapTime time.Time
		found := false
		for rows.Next() {
			if err := rows.Scan(&snapTime); err != nil {
				continue
			}
			found = true
		}
		rows.Close()
		if !found {
			continue
		}
		gems, err := loadGemsAtTime(ctx, pool, snapTime)
		if err != nil {
			fmt.Fprintf(os.Stderr, "  WARN: load gems at %v failed: %v\n", snapTime, err)
			continue
		}
		result[key] = gems
	}
	return result, nil
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

// ---------------------------------------------------------------------------
// Snapshot processing
// ---------------------------------------------------------------------------

// gemOutcome holds the price/listing changes at future snapshot offsets.
type gemOutcome struct {
	name    string
	variant string
	// Price changes as fraction of current price.
	shortPriceDelta  float64 // avg of T+1, T+2
	mediumPriceDelta float64 // avg of T+6, T+7, T+8
	// Listing changes as fraction of current listings.
	shortListingDelta  float64
	mediumListingDelta float64
	hasShort           bool
	hasMedium          bool
}

// signalScore records whether a signal prediction was correct.
type signalScore struct {
	signal    string
	tier      string
	correct   bool
	hasTrade  bool
	timeframe string // "short" or "medium"
}

type snapshotResult struct {
	time       time.Time
	scores     []signalScore
	gemCount   int
	tradeCount int // gems with trade data
}

func processSnapshot(ctx context.Context, pool *pgxpool.Pool, snapTime time.Time) (*snapshotResult, error) {
	// Load data at T.
	gems, err := loadGemsAtTime(ctx, pool, snapTime)
	if err != nil || len(gems) == 0 {
		return nil, fmt.Errorf("no gems at %s", snapTime)
	}

	history, err := loadHistoryUpTo(ctx, pool, snapTime, 168)
	if err != nil {
		return nil, fmt.Errorf("load history: %w", err)
	}

	// Load future snapshots for scoring: short-term (T+1h) and medium-term (T+4h).
	futureGems, err := loadFutureSnapshots(ctx, pool, snapTime, map[int]time.Duration{
		0: 1 * time.Hour, // short-term
		1: 4 * time.Hour, // medium-term
	})
	if err != nil || len(futureGems) == 0 {
		return nil, fmt.Errorf("no future data for %s", snapTime)
	}

	// Build outcome map.
	outcomes := buildOutcomes(gems, futureGems)
	if len(outcomes) == 0 {
		return nil, fmt.Errorf("no outcomes for %s", snapTime)
	}

	// Classification + market context.
	classification := lab.ComputeGemClassification(gems)
	mc := lab.ComputeMarketContext(snapTime, gems, history, classification)
	depthMap := lab.PrecomputeMarketDepth(gems, mc)
	normalizedHistory := lab.NormalizeHistoryDepthGated(history, mc, depthMap)

	baseHistory := loadBaseHistoryUpTo(ctx, pool, snapTime, 24)
	marketAvgBaseLst := computeMarketAvgBase(gems)

	// === V2 signals (no trade) ===
	v2Features := lab.ComputeGemFeatures(snapTime, gems, normalizedHistory, mc, classification.Gems, nil)
	v2Signals := lab.ComputeGemSignals(snapTime, v2Features, mc, gems, baseHistory, marketAvgBaseLst)

	// === V3 signals (with trade) ===
	tradeCache := loadTradeDataNear(ctx, pool, snapTime)
	v3Features := lab.ComputeGemFeatures(snapTime, gems, normalizedHistory, mc, classification.Gems, tradeCache)
	v3Signals := lab.ComputeGemSignals(snapTime, v3Features, mc, gems, baseHistory, marketAvgBaseLst)

	// Score both.
	result := &snapshotResult{
		time:     snapTime,
		gemCount: len(v2Signals),
	}

	// Count gems with trade data.
	for _, f := range v3Features {
		if f.TradeDataAvailable {
			result.tradeCount++
		}
	}

	// Score v2 signals.
	for _, sig := range v2Signals {
		key := sig.Name + "|" + sig.Variant
		outcome, ok := outcomes[key]
		if !ok {
			continue
		}
		result.scores = append(result.scores, scoreSignal(sig.Signal, sig.Tier, outcome, false)...)
	}

	// Score v3 signals.
	for i, sig := range v3Signals {
		key := sig.Name + "|" + sig.Variant
		outcome, ok := outcomes[key]
		if !ok {
			continue
		}
		hasTrade := i < len(v3Features) && v3Features[i].TradeDataAvailable
		result.scores = append(result.scores, scoreSignal(sig.Signal, sig.Tier, outcome, hasTrade)...)
	}

	return result, nil
}

// buildOutcomes computes price/listing deltas for each gem from T to future snapshots.
func buildOutcomes(currentGems []lab.GemPrice, futureGems map[int][]lab.GemPrice) map[string]*gemOutcome {
	// Index current transfigured gems.
	current := make(map[string]*lab.GemPrice)
	for i := range currentGems {
		g := &currentGems[i]
		if !g.IsTransfigured || g.IsCorrupted || g.Chaos <= 5 {
			continue
		}
		current[g.Name+"|"+g.Variant] = g
	}

	// Index future snapshots.
	type futurePrice struct {
		chaos    float64
		listings int
	}
	futureIndex := make(map[string]map[int]futurePrice) // key -> offset -> price
	for offset, fGems := range futureGems {
		for _, g := range fGems {
			if !g.IsTransfigured || g.IsCorrupted {
				continue
			}
			key := g.Name + "|" + g.Variant
			if _, ok := futureIndex[key]; !ok {
				futureIndex[key] = make(map[int]futurePrice)
			}
			futureIndex[key][offset] = futurePrice{g.Chaos, g.Listings}
		}
	}

	outcomes := make(map[string]*gemOutcome)
	for key, cur := range current {
		fIdx, ok := futureIndex[key]
		if !ok {
			continue
		}

		out := &gemOutcome{
			name:    cur.Name,
			variant: cur.Variant,
		}

		// Short-term: T+1h (key 0).
		if fp, ok := fIdx[0]; ok && cur.Chaos > 0 {
			out.shortPriceDelta = (fp.chaos - cur.Chaos) / cur.Chaos
			if cur.Listings > 0 {
				out.shortListingDelta = (float64(fp.listings) - float64(cur.Listings)) / float64(cur.Listings)
			}
			out.hasShort = true
		}

		// Medium-term: T+4h (key 1).
		if fp, ok := fIdx[1]; ok && cur.Chaos > 0 {
			out.mediumPriceDelta = (fp.chaos - cur.Chaos) / cur.Chaos
			if cur.Listings > 0 {
				out.mediumListingDelta = (float64(fp.listings) - float64(cur.Listings)) / float64(cur.Listings)
			}
			out.hasMedium = true
		}

		if out.hasShort || out.hasMedium {
			outcomes[key] = out
		}
	}

	return outcomes
}

// scoreSignal evaluates whether a signal's prediction was correct for both timeframes.
// UNCERTAIN signals are excluded entirely — they have no directional prediction to evaluate.
func scoreSignal(signal, tier string, outcome *gemOutcome, hasTrade bool) []signalScore {
	if signal == "UNCERTAIN" {
		return nil
	}

	var scores []signalScore

	if outcome.hasShort {
		correct := evaluateSignal(signal, outcome.shortPriceDelta, outcome.shortListingDelta, "short")
		scores = append(scores, signalScore{
			signal:    signal,
			tier:      tier,
			correct:   correct,
			hasTrade:  hasTrade,
			timeframe: "short",
		})
	}

	if outcome.hasMedium {
		correct := evaluateSignal(signal, outcome.mediumPriceDelta, outcome.mediumListingDelta, "medium")
		scores = append(scores, signalScore{
			signal:    signal,
			tier:      tier,
			correct:   correct,
			hasTrade:  hasTrade,
			timeframe: "medium",
		})
	}

	return scores
}

// evaluateSignal checks if the signal correctly described the market state.
// UNCERTAIN is handled upstream in scoreSignal (excluded entirely, not passed here).
//
// Trend signals (predict continuation):
//   - BREWING:  price didn't crash within 1h = signal was correct (opportunity existed)
//   - HERD:     listings kept rising = crowd behavior confirmed
//   - RECOVERY: price didn't crash further = bottom-forming confirmed
//   - STABLE:   small changes = calm market confirmed
//
// State signals (describe current conditions, not predictions):
//   - DEMAND:   price didn't crash within 1h = selling at market price was safe
//   - DUMPING:  price didn't recover within 1h = avoiding was correct
//   - TRAP:     price swung significantly = volatility warning was justified
func evaluateSignal(signal string, priceDelta, listingDelta float64, timeframe string) bool {
	const smallThresh = 0.05
	const sigThresh = 0.10

	switch signal {
	case "BREWING":
		// Opportunity signal: price didn't crash = the window was real.
		return priceDelta >= -smallThresh

	case "DEMAND":
		// State signal: "buyers are absorbing supply, safe to sell at market price."
		// Correct if price didn't crash — the demand was real, you could sell.
		return priceDelta >= -smallThresh

	case "DUMPING":
		// State signal: "sellers flooding, avoid this gem."
		// Correct if price didn't meaningfully recover — avoiding was right.
		if timeframe == "short" {
			return priceDelta < smallThresh // didn't bounce back >5%
		}
		return priceDelta < sigThresh // didn't recover >10% medium-term

	case "HERD":
		// Trend signal: crowd FOMO, listings should keep rising.
		if timeframe == "short" {
			return listingDelta > 0 // any listing increase confirms
		}
		return listingDelta >= -smallThresh

	case "RECOVERY":
		// Trend signal: bottom forming, price should stabilize or rise.
		return priceDelta >= -smallThresh

	case "STABLE":
		// State signal: calm market. Both should stay within range.
		return math.Abs(priceDelta) < sigThresh && math.Abs(listingDelta) < sigThresh

	case "CAUTION":
		// Caution signal: "don't farm — you'd lose money."
		// Correct if price dropped AND/OR listings flooded (supply dumped).
		// This is what matters to a farmer: did the gem become unprofitable?
		if timeframe == "short" {
			return priceDelta < -smallThresh || listingDelta > smallThresh
		}
		return priceDelta < -smallThresh || listingDelta > smallThresh

	default:
		return true // unknown signals not penalized
	}
}

// ---------------------------------------------------------------------------
// Aggregation and reporting
// ---------------------------------------------------------------------------

// BacktestReport is the top-level output of the backtest.
type BacktestReport struct {
	SnapshotsProcessed int                      `json:"snapshots_processed"`
	TotalScores        int                      `json:"total_scores"`
	GemsWithTrade      int                      `json:"gems_with_trade_data"`
	BySignal           map[string]*SignalReport `json:"by_signal"`
	ByTier             map[string]*TierReport   `json:"by_tier"`
	BySignalTier       map[string]*SignalReport `json:"by_signal_tier"` // key: "SIGNAL|TIER"
	V2vsV3             *ComparisonReport        `json:"v2_vs_v3"`
}

// SignalReport holds accuracy stats for a single signal type.
type SignalReport struct {
	ShortTotal   int     `json:"short_total"`
	ShortCorrect int     `json:"short_correct"`
	ShortAccPct  float64 `json:"short_accuracy_pct"`
	MedTotal     int     `json:"med_total"`
	MedCorrect   int     `json:"med_correct"`
	MedAccPct    float64 `json:"med_accuracy_pct"`
}

// TierReport holds accuracy stats for a single tier (same shape as SignalReport).
type TierReport = SignalReport

// ComparisonReport compares v2 (no trade) vs v3 (trade-enriched) accuracy.
type ComparisonReport struct {
	V2ShortTotal   int     `json:"v2_short_total"`
	V2ShortCorrect int     `json:"v2_short_correct"`
	V2ShortAccPct  float64 `json:"v2_short_accuracy_pct"`
	V3ShortTotal   int     `json:"v3_short_total"`
	V3ShortCorrect int     `json:"v3_short_correct"`
	V3ShortAccPct  float64 `json:"v3_short_accuracy_pct"`
	V2MedTotal     int     `json:"v2_med_total"`
	V2MedCorrect   int     `json:"v2_med_correct"`
	V2MedAccPct    float64 `json:"v2_med_accuracy_pct"`
	V3MedTotal     int     `json:"v3_med_total"`
	V3MedCorrect   int     `json:"v3_med_correct"`
	V3MedAccPct    float64 `json:"v3_med_accuracy_pct"`
}

func aggregateResults(results []snapshotResult) *BacktestReport {
	report := &BacktestReport{
		SnapshotsProcessed: len(results),
		BySignal:           make(map[string]*SignalReport),
		ByTier:             make(map[string]*TierReport),
		BySignalTier:       make(map[string]*SignalReport),
		V2vsV3:             &ComparisonReport{},
	}

	for _, res := range results {
		report.GemsWithTrade += res.tradeCount

		for _, s := range res.scores {
			report.TotalScores++

			// By signal.
			sr, ok := report.BySignal[s.signal]
			if !ok {
				sr = &SignalReport{}
				report.BySignal[s.signal] = sr
			}
			if s.timeframe == "short" {
				sr.ShortTotal++
				if s.correct {
					sr.ShortCorrect++
				}
			} else {
				sr.MedTotal++
				if s.correct {
					sr.MedCorrect++
				}
			}

			// By tier.
			tr, ok := report.ByTier[s.tier]
			if !ok {
				tr = &TierReport{}
				report.ByTier[s.tier] = tr
			}
			if s.timeframe == "short" {
				tr.ShortTotal++
				if s.correct {
					tr.ShortCorrect++
				}
			} else {
				tr.MedTotal++
				if s.correct {
					tr.MedCorrect++
				}
			}

			// By signal x tier.
			stKey := s.signal + "|" + s.tier
			st, ok := report.BySignalTier[stKey]
			if !ok {
				st = &SignalReport{}
				report.BySignalTier[stKey] = st
			}
			if s.timeframe == "short" {
				st.ShortTotal++
				if s.correct {
					st.ShortCorrect++
				}
			} else {
				st.MedTotal++
				if s.correct {
					st.MedCorrect++
				}
			}

			// V2 vs V3 comparison.
			cmp := report.V2vsV3
			if s.hasTrade {
				if s.timeframe == "short" {
					cmp.V3ShortTotal++
					if s.correct {
						cmp.V3ShortCorrect++
					}
				} else {
					cmp.V3MedTotal++
					if s.correct {
						cmp.V3MedCorrect++
					}
				}
			} else {
				if s.timeframe == "short" {
					cmp.V2ShortTotal++
					if s.correct {
						cmp.V2ShortCorrect++
					}
				} else {
					cmp.V2MedTotal++
					if s.correct {
						cmp.V2MedCorrect++
					}
				}
			}
		}
	}

	// Compute percentages.
	for _, sr := range report.BySignal {
		sr.ShortAccPct = pct(sr.ShortCorrect, sr.ShortTotal)
		sr.MedAccPct = pct(sr.MedCorrect, sr.MedTotal)
	}
	for _, tr := range report.ByTier {
		tr.ShortAccPct = pct(tr.ShortCorrect, tr.ShortTotal)
		tr.MedAccPct = pct(tr.MedCorrect, tr.MedTotal)
	}
	for _, st := range report.BySignalTier {
		st.ShortAccPct = pct(st.ShortCorrect, st.ShortTotal)
		st.MedAccPct = pct(st.MedCorrect, st.MedTotal)
	}
	cmp := report.V2vsV3
	cmp.V2ShortAccPct = pct(cmp.V2ShortCorrect, cmp.V2ShortTotal)
	cmp.V3ShortAccPct = pct(cmp.V3ShortCorrect, cmp.V3ShortTotal)
	cmp.V2MedAccPct = pct(cmp.V2MedCorrect, cmp.V2MedTotal)
	cmp.V3MedAccPct = pct(cmp.V3MedCorrect, cmp.V3MedTotal)

	return report
}

func printReport(r *BacktestReport) {
	fmt.Println("=== SIGNAL BACKTEST REPORT ===")
	fmt.Printf("Snapshots processed: %d\n", r.SnapshotsProcessed)
	fmt.Printf("Total signal scores: %d\n", r.TotalScores)
	fmt.Printf("Gems with trade data: %d\n\n", r.GemsWithTrade)

	// Signal accuracy.
	fmt.Println("--- Per-Signal Accuracy ---")
	signalOrder := sortedKeys(r.BySignal)
	fmt.Printf("%-16s %6s %6s %8s    %6s %6s %8s\n", "Signal", "S.Tot", "S.Hit", "S.Acc%", "M.Tot", "M.Hit", "M.Acc%")
	for _, sig := range signalOrder {
		sr := r.BySignal[sig]
		fmt.Printf("%-16s %6d %6d %7.1f%%    %6d %6d %7.1f%%\n",
			sig, sr.ShortTotal, sr.ShortCorrect, sr.ShortAccPct,
			sr.MedTotal, sr.MedCorrect, sr.MedAccPct)
	}

	// Tier accuracy.
	fmt.Println("\n--- Per-Tier Accuracy ---")
	tierOrder := []string{"TOP", "HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"}
	fmt.Printf("%-12s %6s %6s %8s    %6s %6s %8s\n", "Tier", "S.Tot", "S.Hit", "S.Acc%", "M.Tot", "M.Hit", "M.Acc%")
	for _, tier := range tierOrder {
		tr, ok := r.ByTier[tier]
		if !ok {
			continue
		}
		fmt.Printf("%-12s %6d %6d %7.1f%%    %6d %6d %7.1f%%\n",
			tier, tr.ShortTotal, tr.ShortCorrect, tr.ShortAccPct,
			tr.MedTotal, tr.MedCorrect, tr.MedAccPct)
	}

	// Signal x Tier cross-tabulation.
	fmt.Println("\n--- Signal × Tier Accuracy ---")
	signalOrderST := sortedKeys(r.BySignal) // reuse same signal sort
	tierOrderST := []string{"TOP", "HIGH", "MID-HIGH", "MID", "LOW", "FLOOR"}
	fmt.Printf("%-16s %-12s %6s %6s %8s    %6s %6s %8s\n", "Signal", "Tier", "S.Tot", "S.Hit", "S.Acc%", "M.Tot", "M.Hit", "M.Acc%")
	for _, sig := range signalOrderST {
		for _, tier := range tierOrderST {
			key := sig + "|" + tier
			st, ok := r.BySignalTier[key]
			if !ok {
				continue
			}
			fmt.Printf("%-16s %-12s %6d %6d %7.1f%%    %6d %6d %7.1f%%\n",
				sig, tier, st.ShortTotal, st.ShortCorrect, st.ShortAccPct,
				st.MedTotal, st.MedCorrect, st.MedAccPct)
		}
	}

	// V2 vs V3 comparison.
	fmt.Println("\n--- V2 (no trade) vs V3 (trade-enriched) ---")
	cmp := r.V2vsV3
	fmt.Printf("V2 Short: %d/%d = %.1f%%\n", cmp.V2ShortCorrect, cmp.V2ShortTotal, cmp.V2ShortAccPct)
	fmt.Printf("V3 Short: %d/%d = %.1f%%\n", cmp.V3ShortCorrect, cmp.V3ShortTotal, cmp.V3ShortAccPct)
	fmt.Printf("V2 Med:   %d/%d = %.1f%%\n", cmp.V2MedCorrect, cmp.V2MedTotal, cmp.V2MedAccPct)
	fmt.Printf("V3 Med:   %d/%d = %.1f%%\n", cmp.V3MedCorrect, cmp.V3MedTotal, cmp.V3MedAccPct)
	if cmp.V2ShortTotal > 0 && cmp.V3ShortTotal > 0 {
		fmt.Printf("\nShort-term delta: %+.1fpp (v3 - v2)\n", cmp.V3ShortAccPct-cmp.V2ShortAccPct)
	}
	if cmp.V2MedTotal > 0 && cmp.V3MedTotal > 0 {
		fmt.Printf("Med-term delta:   %+.1fpp (v3 - v2)\n", cmp.V3MedAccPct-cmp.V2MedAccPct)
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

func avg(vals []float64) float64 {
	if len(vals) == 0 {
		return 0
	}
	sum := 0.0
	for _, v := range vals {
		sum += v
	}
	return sum / float64(len(vals))
}

func pct(correct, total int) float64 {
	if total == 0 {
		return 0
	}
	return math.Round(float64(correct)/float64(total)*1000) / 10 // one decimal place
}

func sortedKeys(m map[string]*SignalReport) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
