package main

import (
	"context"
	"flag"
	"fmt"
	"math"
	"os"
	"sort"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/db"
	"profitofexile/internal/lab"
)

// Snapshot represents a single gem observation at a point in time.
type Snapshot struct {
	Time           time.Time
	Name           string
	Variant        string
	Chaos          float64
	Listings       int
	IsTransfigured bool
	GemColor       string
}

// gemKey uniquely identifies a gem across snapshots.
type gemKey struct {
	Name    string
	Variant string
}

// precomputed holds config-independent features for one (gem, time) evaluation point.
type precomputed struct {
	gem        gemKey
	chaos      float64
	listings   int
	priceVel   float64
	listingVel float64
	cv         float64
	futurePct  float64 // actual price change % (4 snapshots ahead)
}

// SweepResult holds the accuracy metrics for one parameter combination.
type SweepResult struct {
	Config     lab.SignalConfig
	OverallAcc float64
	TopAcc     float64
	MidAcc     float64
	LowAcc     float64
	Top15Acc   float64
}

func main() {
	topN := flag.Int("top", 15, "Show top N parameter combos")
	flag.Parse()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		dbURL = "postgresql://profitofexile:profitofexile@postgres:5432/profitofexile"
	}

	ctx := context.Background()
	pool, err := db.NewPool(ctx, dbURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "DB connect failed: %v\n", err)
		os.Exit(1)
	}
	defer pool.Close()

	fmt.Fprintln(os.Stderr, "Loading snapshots from database...")
	t0 := time.Now()
	snapshots := loadFromDB(ctx, pool)
	fmt.Fprintf(os.Stderr, "Loaded %d snapshots in %s\n", len(snapshots), time.Since(t0).Round(time.Millisecond))

	if len(snapshots) == 0 {
		fmt.Fprintln(os.Stderr, "No snapshots found")
		os.Exit(1)
	}

	fmt.Fprintln(os.Stderr, "Pre-computing features (velocities, CVs, future outcomes)...")
	t1 := time.Now()
	features, lastGems := precomputeFeatures(snapshots)
	fmt.Fprintf(os.Stderr, "Pre-computed %d evaluation points in %s\n", len(features), time.Since(t1).Round(time.Millisecond))

	grid := generateGrid()
	fmt.Fprintf(os.Stderr, "Sweeping %d parameter combos...\n", len(grid))

	t2 := time.Now()
	results := sweep(features, lastGems, grid)
	fmt.Fprintf(os.Stderr, "Sweep complete in %s\n", time.Since(t2).Round(time.Millisecond))

	// Sort by top-15 accuracy descending.
	sort.Slice(results, func(i, j int) bool {
		return results[i].Top15Acc > results[j].Top15Acc
	})

	n := *topN
	if n > len(results) {
		n = len(results)
	}

	fmt.Printf("\n%-6s %-8s %-8s %-8s %-8s %-6s %-6s %-6s %-6s %-6s %-6s\n",
		"Rank", "Top15%", "TOP%", "MID%", "Overall%", "H_pv", "H_lv", "S_pv", "B_pv", "T_top", "T_mid")
	fmt.Println("------ -------- -------- -------- -------- ------ ------ ------ ------ ------ ------")
	for i, r := range results[:n] {
		fmt.Printf("%-6d %-8.1f %-8.1f %-8.1f %-8.1f %-6.1f %-6.1f %-6.1f %-6.1f %-6.2f %-6.2f\n",
			i+1, r.Top15Acc, r.TopAcc, r.MidAcc, r.OverallAcc,
			r.Config.PreHERDPriceVel, r.Config.PreHERDListingVel,
			r.Config.StablePriceVel, r.Config.BrewingMinPVel,
			r.Config.TierTopMult, r.Config.TierMidMult)
	}

	// Print current defaults for comparison.
	def := lab.DefaultSignalConfig()
	fmt.Printf("\nCurrent defaults: H_pv=%.0f H_lv=%.0f S_pv=%.1f B_pv=%.1f T_top=%.2f T_mid=%.2f\n",
		def.PreHERDPriceVel, def.PreHERDListingVel,
		def.StablePriceVel, def.BrewingMinPVel,
		def.TierTopMult, def.TierMidMult)
}

// loadFromDB fetches all gem snapshots ordered by time.
func loadFromDB(ctx context.Context, pool *pgxpool.Pool) []Snapshot {
	q := `SELECT time, name, variant, chaos, listings, is_transfigured, gem_color
	      FROM gem_snapshots ORDER BY time`

	rows, err := pool.Query(ctx, q)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Query failed: %v\n", err)
		return nil
	}
	defer rows.Close()

	var out []Snapshot
	for rows.Next() {
		var s Snapshot
		var gemColor *string
		if err := rows.Scan(&s.Time, &s.Name, &s.Variant, &s.Chaos, &s.Listings, &s.IsTransfigured, &gemColor); err != nil {
			fmt.Fprintf(os.Stderr, "Scan error: %v\n", err)
			continue
		}
		if gemColor != nil {
			s.GemColor = *gemColor
		}
		out = append(out, s)
	}
	if err := rows.Err(); err != nil {
		fmt.Fprintf(os.Stderr, "Rows error: %v\n", err)
	}
	return out
}

// precomputeFeatures builds all config-independent evaluation points in one pass.
// Returns the feature slice and the last snapshot's gem prices (for tier computation).
func precomputeFeatures(snapshots []Snapshot) ([]precomputed, []lab.GemPrice) {
	// Group snapshots by time.
	timeMap := make(map[time.Time]map[gemKey]Snapshot)
	var times []time.Time

	for _, s := range snapshots {
		t := s.Time.Truncate(time.Minute)
		if _, ok := timeMap[t]; !ok {
			timeMap[t] = make(map[gemKey]Snapshot)
			times = append(times, t)
		}
		timeMap[t][gemKey{s.Name, s.Variant}] = s
	}
	sort.Slice(times, func(i, j int) bool { return times[i].Before(times[j]) })

	// Build last snapshot gem prices for tier computation.
	var lastGems []lab.GemPrice
	if len(times) > 0 {
		for _, s := range timeMap[times[len(times)-1]] {
			lastGems = append(lastGems, lab.GemPrice{
				Name:           s.Name,
				Variant:        s.Variant,
				Chaos:          s.Chaos,
				Listings:       s.Listings,
				IsTransfigured: s.IsTransfigured,
				GemColor:       s.GemColor,
			})
		}
	}

	// Rolling history per gem.
	type histEntry struct {
		points []lab.PricePoint
	}
	gemHist := make(map[gemKey]*histEntry)

	var features []precomputed

	for ti, t := range times {
		gems := timeMap[t]

		// Update history.
		for gk, s := range gems {
			h, ok := gemHist[gk]
			if !ok {
				h = &histEntry{}
				gemHist[gk] = h
			}
			h.points = append(h.points, lab.PricePoint{
				Time:     s.Time,
				Chaos:    s.Chaos,
				Listings: s.Listings,
			})
			if len(h.points) > 8 {
				h.points = h.points[len(h.points)-8:]
			}
		}

		// Need history and future.
		if ti < 1 || ti+4 >= len(times) {
			continue
		}

		futureGems := timeMap[times[ti+4]]

		for gk, s := range gems {
			if !s.IsTransfigured || s.Chaos <= 5 {
				continue
			}

			h := gemHist[gk]
			if h == nil || len(h.points) < 2 {
				continue
			}

			futureSnap, ok := futureGems[gk]
			if !ok {
				continue
			}

			priceVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return p.Chaos })
			listingVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return float64(p.Listings) })
			cv := cvFromPoints(h.points)

			var futurePct float64
			if s.Chaos > 0 {
				futurePct = (futureSnap.Chaos - s.Chaos) / s.Chaos * 100
			}

			features = append(features, precomputed{
				gem:        gk,
				chaos:      s.Chaos,
				listings:   s.Listings,
				priceVel:   priceVel,
				listingVel: listingVel,
				cv:         cv,
				futurePct:  futurePct,
			})
		}
	}

	return features, lastGems
}

// sweep evaluates every config against pre-computed features.
func sweep(features []precomputed, lastGems []lab.GemPrice, grid []lab.SignalConfig) []SweepResult {
	// Pre-compute gem average ROIs for top-15 set (config-independent).
	gemROIs := make(map[gemKey]struct{ sum float64; count int })
	for _, f := range features {
		entry := gemROIs[f.gem]
		entry.sum += f.futurePct
		entry.count++
		gemROIs[f.gem] = entry
	}

	type gemAvgROI struct {
		key gemKey
		avg float64
	}
	var sorted []gemAvgROI
	for gk, entry := range gemROIs {
		sorted = append(sorted, gemAvgROI{gk, entry.sum / float64(entry.count)})
	}
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].avg > sorted[j].avg })

	top15Count := 15
	if top15Count > len(sorted) {
		top15Count = len(sorted)
	}
	top15Set := make(map[gemKey]bool, top15Count)
	for _, g := range sorted[:top15Count] {
		top15Set[g.key] = true
	}

	results := make([]SweepResult, len(grid))

	for i, cfg := range grid {
		if i%1000 == 0 {
			fmt.Fprintf(os.Stderr, "  Progress: %d/%d\n", i, len(grid))
		}

		topThresh, midThresh := lab.ComputePriceTiersWithConfig(lastGems, cfg)

		var totalCorrect, totalCount int
		var topCorrect, topCount int
		var midCorrect, midCount int
		var lowCorrect, lowCount int
		var top15Correct, top15Total int

		for _, f := range features {
			signal := lab.ClassifySignalWithConfig(f.priceVel, f.listingVel, f.cv, f.listings, cfg)
			predicted := predictedDirection(signal)
			actual := directionFromChange(f.futurePct)
			correct := predicted == actual

			tier := lab.ClassifyPriceTier(f.chaos, topThresh, midThresh)

			totalCount++
			if correct {
				totalCorrect++
			}

			switch tier {
			case "TOP":
				topCount++
				if correct {
					topCorrect++
				}
			case "MID":
				midCount++
				if correct {
					midCorrect++
				}
			case "LOW":
				lowCount++
				if correct {
					lowCorrect++
				}
			}

			if top15Set[f.gem] {
				top15Total++
				if correct {
					top15Correct++
				}
			}
		}

		results[i] = SweepResult{
			Config:     cfg,
			OverallAcc: pct(totalCorrect, totalCount),
			TopAcc:     pct(topCorrect, topCount),
			MidAcc:     pct(midCorrect, midCount),
			LowAcc:     pct(lowCorrect, lowCount),
			Top15Acc:   pct(top15Correct, top15Total),
		}
	}

	return results
}

// generateGrid produces a focused parameter grid for sweeping.
func generateGrid() []lab.SignalConfig {
	base := lab.DefaultSignalConfig()
	var grid []lab.SignalConfig

	for _, herdPV := range []float64{20, 25, 30, 35, 40} {
		for _, herdLV := range []float64{2, 3, 5, 7} {
			for _, stablePV := range []float64{1.0, 1.5, 2.0, 2.5} {
				for _, brewPV := range []float64{1, 2, 3, 5} {
					for _, tierTop := range []float64{0.5, 0.6, 0.7, 0.8} {
						for _, tierMid := range []float64{0.10, 0.15, 0.20, 0.25} {
							cfg := base
							cfg.PreHERDPriceVel = herdPV
							cfg.PreHERDListingVel = herdLV
							cfg.StablePriceVel = stablePV
							cfg.BrewingMinPVel = brewPV
							cfg.TierTopMult = tierTop
							cfg.TierMidMult = tierMid
							grid = append(grid, cfg)
						}
					}
				}
			}
		}
	}
	return grid // 5x4x4x4x4x4 = 5,120 combos
}

// velocityFromPoints computes rate of change per hour using last 4 points.
func velocityFromPoints(points []lab.PricePoint, extract func(lab.PricePoint) float64) float64 {
	n := len(points)
	if n < 2 {
		return 0
	}
	start := 0
	if n > 4 {
		start = n - 4
	}
	first := points[start]
	last := points[n-1]
	hours := last.Time.Sub(first.Time).Hours()
	if hours <= 0 {
		return 0
	}
	return (extract(last) - extract(first)) / hours
}

// cvFromPoints computes coefficient of variation from price history.
func cvFromPoints(points []lab.PricePoint) float64 {
	if len(points) < 2 {
		return 0
	}
	prices := make([]float64, len(points))
	for i, p := range points {
		prices[i] = p.Chaos
	}
	var sum float64
	for _, p := range prices {
		sum += p
	}
	mean := sum / float64(len(prices))
	if mean == 0 {
		return 0
	}
	var variance float64
	for _, p := range prices {
		d := p - mean
		variance += d * d
	}
	variance /= float64(len(prices))
	cv := (math.Sqrt(variance) / math.Abs(mean)) * 100
	if math.IsNaN(cv) || math.IsInf(cv, 0) {
		return 0
	}
	return cv
}

// directionFromChange maps a price change % into UP, DOWN, or FLAT.
func directionFromChange(pctChange float64) string {
	if pctChange > 2 {
		return "UP"
	}
	if pctChange < -2 {
		return "DOWN"
	}
	return "FLAT"
}

// predictedDirection maps a signal to an expected price direction.
func predictedDirection(signal string) string {
	switch signal {
	case "HERD", "RISING", "RECOVERY":
		return "UP"
	case "DUMPING", "FALLING", "TRAP":
		return "DOWN"
	case "STABLE":
		return "FLAT"
	default:
		return "FLAT"
	}
}

// pct returns a percentage, handling division by zero.
func pct(correct, total int) float64 {
	if total == 0 {
		return 0
	}
	return float64(correct) / float64(total) * 100
}
