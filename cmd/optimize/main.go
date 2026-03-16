package main

import (
	"encoding/csv"
	"flag"
	"fmt"
	"io"
	"math"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"

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

// SweepResult holds the accuracy metrics for one parameter combination.
type SweepResult struct {
	Config     lab.SignalConfig
	OverallAcc float64
	TopAcc     float64
	MidAcc     float64
	LowAcc    float64
	Top15Acc   float64
}

func main() {
	csvPath := flag.String("csv", "data/backtest-dump.csv", "Path to CSV dump")
	dump := flag.Bool("dump", false, "Dump prod data to CSV (requires DATABASE_URL)")
	topN := flag.Int("top", 10, "Show top N parameter combos")
	flag.Parse()

	if *dump {
		fmt.Println("Dump mode not yet implemented — use SSH + psql COPY")
		os.Exit(0)
	}

	snapshots := loadCSV(*csvPath)
	if len(snapshots) == 0 {
		fmt.Fprintln(os.Stderr, "No snapshots loaded — check CSV path and format")
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Loaded %d snapshots\n", len(snapshots))

	grid := generateGrid()
	fmt.Fprintf(os.Stderr, "Grid size: %d combos\n", len(grid))

	results := sweep(snapshots, grid)

	// Sort by top-15 accuracy descending.
	sort.Slice(results, func(i, j int) bool {
		return results[i].Top15Acc > results[j].Top15Acc
	})

	n := *topN
	if n > len(results) {
		n = len(results)
	}

	fmt.Printf("%-6s %-8s %-8s %-8s %-8s %-6s %-6s %-6s %-6s %-6s %-6s\n",
		"Rank", "Top15%", "TOP%", "MID%", "Overall%", "H_pv", "H_lv", "S_pv", "B_pv", "T_top", "T_mid")
	for i, r := range results[:n] {
		fmt.Printf("%-6d %-8.1f %-8.1f %-8.1f %-8.1f %-6.1f %-6.1f %-6.1f %-6.1f %-6.2f %-6.2f\n",
			i+1, r.Top15Acc, r.TopAcc, r.MidAcc, r.OverallAcc,
			r.Config.PreHERDPriceVel, r.Config.PreHERDListingVel,
			r.Config.StablePriceVel, r.Config.BrewingMinPVel,
			r.Config.TierTopMult, r.Config.TierMidMult)
	}
}

// loadCSV parses the backtest dump CSV into snapshots.
// Expected columns: time,name,variant,chaos,listings,gem_color
func loadCSV(path string) []Snapshot {
	f, err := os.Open(path)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Cannot open CSV: %v\n", err)
		return nil
	}
	defer f.Close()

	r := csv.NewReader(f)
	r.TrimLeadingSpace = true

	// Read header.
	header, err := r.Read()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Cannot read CSV header: %v\n", err)
		return nil
	}

	// Map column names to indices.
	colIdx := make(map[string]int, len(header))
	for i, h := range header {
		colIdx[strings.TrimSpace(strings.ToLower(h))] = i
	}

	required := []string{"time", "name", "variant", "chaos", "listings", "is_transfigured", "gem_color"}
	for _, col := range required {
		if _, ok := colIdx[col]; !ok {
			fmt.Fprintf(os.Stderr, "Missing required column: %s\n", col)
			return nil
		}
	}

	var snapshots []Snapshot
	var skipped int
	for {
		record, err := r.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			fmt.Fprintf(os.Stderr, "CSV read error: %v\n", err)
			skipped++
			continue
		}

		timeStr := strings.TrimSpace(record[colIdx["time"]])
		t, err := time.Parse(time.RFC3339, timeStr)
		if err != nil {
			t, err = time.Parse("2006-01-02 15:04:05.999999-07", timeStr)
		}
		if err != nil {
			t, err = time.Parse("2006-01-02 15:04:05.999999+00", timeStr)
		}
		if err != nil {
			t, err = time.Parse("2006-01-02 15:04:05", timeStr)
			if err != nil {
				skipped++
				continue
			}
		}

		chaos, err := strconv.ParseFloat(strings.TrimSpace(record[colIdx["chaos"]]), 64)
		if err != nil {
			skipped++
			continue
		}

		listings, err := strconv.Atoi(strings.TrimSpace(record[colIdx["listings"]]))
		if err != nil {
			skipped++
			continue
		}

		isTransStr := strings.TrimSpace(strings.ToLower(record[colIdx["is_transfigured"]]))
		isTrans := isTransStr == "t" || isTransStr == "true" || isTransStr == "1"

		snapshots = append(snapshots, Snapshot{
			Time:           t,
			Name:           strings.TrimSpace(record[colIdx["name"]]),
			Variant:        strings.TrimSpace(record[colIdx["variant"]]),
			Chaos:          chaos,
			Listings:       listings,
			IsTransfigured: isTrans,
			GemColor:       strings.TrimSpace(record[colIdx["gem_color"]]),
		})
	}

	if skipped > 0 {
		fmt.Fprintf(os.Stderr, "Skipped %d rows due to parse errors\n", skipped)
	}

	return snapshots
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

// sweep evaluates every config in the grid against the snapshot data.
func sweep(snapshots []Snapshot, grid []lab.SignalConfig) []SweepResult {
	var results []SweepResult
	for i, cfg := range grid {
		if i%500 == 0 {
			fmt.Fprintf(os.Stderr, "Progress: %d/%d\n", i, len(grid))
		}
		r := backtest(snapshots, cfg)
		results = append(results, r)
	}
	return results
}

// gemKey uniquely identifies a gem across snapshots.
type gemKey struct {
	Name    string
	Variant string
}

// backtest evaluates a single SignalConfig against the snapshot data.
// It groups snapshots by time, computes signals at each time step using rolling
// history, looks forward 4 snapshots, and scores accuracy by tier.
func backtest(snapshots []Snapshot, cfg lab.SignalConfig) SweepResult {
	// Group snapshots by time.
	timeMap := make(map[time.Time]map[gemKey]Snapshot)
	var times []time.Time

	for _, s := range snapshots {
		t := s.Time.Truncate(time.Minute) // normalize
		if _, ok := timeMap[t]; !ok {
			timeMap[t] = make(map[gemKey]Snapshot)
			times = append(times, t)
		}
		timeMap[t][gemKey{s.Name, s.Variant}] = s
	}

	sort.Slice(times, func(i, j int) bool { return times[i].Before(times[j]) })

	if len(times) < 5 {
		return SweepResult{Config: cfg}
	}

	// Build rolling history per gem.
	type histEntry struct {
		points []lab.PricePoint
	}
	gemHist := make(map[gemKey]*histEntry)

	// Compute tiers from the last snapshot (representative of the market).
	lastGems := buildGemPrices(timeMap[times[len(times)-1]])
	topThresh, midThresh := lab.ComputePriceTiersWithConfig(lastGems, cfg)

	var totalCorrect, totalCount int
	var topCorrect, topCount int
	var midCorrect, midCount int
	var lowCorrect, lowCount int

	// Track top-15 gems by ROI across the whole dataset.
	type gemROI struct {
		Key gemKey
		ROI float64
	}
	gemROIs := make(map[gemKey][]float64)

	for ti, t := range times {
		gems := timeMap[t]

		// Update history for each gem.
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
			// Keep at most 8 points of history.
			if len(h.points) > 8 {
				h.points = h.points[len(h.points)-8:]
			}
		}

		// Need at least 2 history points and 4 future snapshots.
		if ti < 1 || ti+4 >= len(times) {
			continue
		}

		for gk, s := range gems {
			h := gemHist[gk]
			if h == nil || len(h.points) < 2 {
				continue
			}

			// Compute velocities.
			priceVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return p.Chaos })
			listingVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return float64(p.Listings) })
			cv := cvFromPoints(h.points)

			signal := lab.ClassifySignalWithConfig(priceVel, listingVel, cv, s.Listings, cfg)

			// Look forward 4 snapshots to determine actual outcome.
			futureIdx := ti + 4
			if futureIdx >= len(times) {
				futureIdx = len(times) - 1
			}
			futureGems := timeMap[times[futureIdx]]
			futureSnap, ok := futureGems[gk]
			if !ok {
				continue
			}

			actualPriceChange := futureSnap.Chaos - s.Chaos
			actualDirection := directionFromChange(actualPriceChange, s.Chaos)

			predicted := predictedDirection(signal)
			correct := predicted == actualDirection

			// Determine tier.
			tier := lab.ClassifyPriceTier(s.Chaos, topThresh, midThresh)

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

			// Track ROI for top-15 computation.
			if s.Chaos > 0 {
				roi := (futureSnap.Chaos - s.Chaos) / s.Chaos * 100
				gemROIs[gk] = append(gemROIs[gk], roi)
			}
		}
	}

	// Compute top-15 accuracy: among the 15 gems with highest average ROI,
	// what fraction of their signals were correctly predicted?
	var allGemROIs []gemROI
	for gk, rois := range gemROIs {
		avg := 0.0
		for _, r := range rois {
			avg += r
		}
		avg /= float64(len(rois))
		allGemROIs = append(allGemROIs, gemROI{Key: gk, ROI: avg})
	}
	sort.Slice(allGemROIs, func(i, j int) bool {
		return allGemROIs[i].ROI > allGemROIs[j].ROI
	})

	top15Count := 15
	if top15Count > len(allGemROIs) {
		top15Count = len(allGemROIs)
	}
	top15Set := make(map[gemKey]bool, top15Count)
	for _, gr := range allGemROIs[:top15Count] {
		top15Set[gr.Key] = true
	}

	// Re-sweep for top-15 accuracy.
	var top15Correct, top15Total int
	// Reset history for second pass.
	gemHist2 := make(map[gemKey]*histEntry)

	for ti, t := range times {
		gems := timeMap[t]
		for gk, s := range gems {
			h, ok := gemHist2[gk]
			if !ok {
				h = &histEntry{}
				gemHist2[gk] = h
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

		if ti < 1 || ti+4 >= len(times) {
			continue
		}

		for gk, s := range gems {
			if !top15Set[gk] {
				continue
			}
			h := gemHist2[gk]
			if h == nil || len(h.points) < 2 {
				continue
			}

			priceVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return p.Chaos })
			listingVel := velocityFromPoints(h.points, func(p lab.PricePoint) float64 { return float64(p.Listings) })
			cv := cvFromPoints(h.points)

			signal := lab.ClassifySignalWithConfig(priceVel, listingVel, cv, s.Listings, cfg)

			futureIdx := ti + 4
			if futureIdx >= len(times) {
				futureIdx = len(times) - 1
			}
			futureGems := timeMap[times[futureIdx]]
			futureSnap, ok := futureGems[gk]
			if !ok {
				continue
			}

			actualPriceChange := futureSnap.Chaos - s.Chaos
			actualDirection := directionFromChange(actualPriceChange, s.Chaos)
			predicted := predictedDirection(signal)

			top15Total++
			if predicted == actualDirection {
				top15Correct++
			}
		}
	}

	return SweepResult{
		Config:     cfg,
		OverallAcc: pct(totalCorrect, totalCount),
		TopAcc:     pct(topCorrect, topCount),
		MidAcc:     pct(midCorrect, midCount),
		LowAcc:    pct(lowCorrect, lowCount),
		Top15Acc:   pct(top15Correct, top15Total),
	}
}

// buildGemPrices converts a time slice into GemPrice slice for tier computation.
func buildGemPrices(gems map[gemKey]Snapshot) []lab.GemPrice {
	prices := make([]lab.GemPrice, 0, len(gems))
	for _, s := range gems {
		prices = append(prices, lab.GemPrice{
			Name:           s.Name,
			Variant:        s.Variant,
			Chaos:          s.Chaos,
			Listings:       s.Listings,
			IsTransfigured: s.IsTransfigured,
			GemColor:       s.GemColor,
		})
	}
	return prices
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

// directionFromChange maps a price change into UP, DOWN, or FLAT.
func directionFromChange(change, basePrice float64) string {
	if basePrice == 0 {
		return "FLAT"
	}
	pctChange := (change / basePrice) * 100
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
