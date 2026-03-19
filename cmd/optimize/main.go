package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strings"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/lab"
)

// JSONOutput is the structured output for --json mode.
type JSONOutput struct {
	Context    JSONContext        `json:"context"`
	Results    []JSONResult       `json:"results"`
	BestDetail *JSONBestDetail   `json:"best_detail,omitempty"`
	Defaults   *JSONDefaults      `json:"defaults,omitempty"`
}

// JSONContext holds metadata about the optimization run.
type JSONContext struct {
	MarketTime      time.Time `json:"market_time"`
	TotalGems       int       `json:"total_gems"`
	VelocityMean    float64   `json:"velocity_mean"`
	VelocitySigma   float64   `json:"velocity_sigma"`
	ListingVelMean  float64   `json:"listing_vel_mean"`
	ListingVelSigma float64   `json:"listing_vel_sigma"`
	EvalPoints      int       `json:"eval_points"`
	DroppedPoints   int       `json:"dropped_points"`
	GridSize        int       `json:"grid_size"`
	Hours           int       `json:"hours"`
	Horizon         string    `json:"horizon"`
}

// JSONResult holds one sweep result row.
type JSONResult struct {
	Rank          int                `json:"rank"`
	WeightedScore float64           `json:"weighted_score"`
	TopAcc        float64           `json:"top_acc"`
	HighAcc       float64           `json:"high_acc"`
	MidAcc        float64           `json:"mid_acc"`
	LowAcc        float64           `json:"low_acc"`
	OverallAcc    float64           `json:"overall_acc"`
	SweetSpot     int               `json:"sweet_spot"`
	Sigma         lab.SigmaConfig   `json:"sigma"`
	TemporalAcc   map[string]float64 `json:"temporal_acc,omitempty"`
}

// JSONBestDetail holds extended info about the best result.
type JSONBestDetail struct {
	ConfBands   []lab.ConfidenceBand `json:"confidence_bands"`
	TemporalAcc map[string]float64   `json:"temporal_acc"`
}

// JSONDefaults holds the approximate sigma equivalents of current defaults.
type JSONDefaults struct {
	ApproxHERDPriceMult    float64 `json:"approx_herd_price_mult"`
	ApproxHERDListingMult  float64 `json:"approx_herd_listing_mult"`
	ApproxStablePriceMult  float64 `json:"approx_stable_price_mult"`
	ApproxBrewingPriceMult float64 `json:"approx_brewing_price_mult"`
}

func main() {
	topN := flag.Int("top", 15, "Show top N results")
	hours := flag.Int("hours", 168, "Backtest time range in hours")
	horizon := flag.String("horizon", "2h", "Ground truth forward horizon (e.g. 2h, 90m)")
	jsonMode := flag.Bool("json", false, "Output JSON instead of console table")
	flag.Parse()

	horizonDur, err := time.ParseDuration(*horizon)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Invalid horizon %q: %v\n", *horizon, err)
		os.Exit(1)
	}

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

	repo := lab.NewRepository(pool)

	// Load market context.
	fmt.Fprintln(os.Stderr, "Loading market context...")
	mc, err := repo.LatestMarketContext(ctx)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to load market context: %v\n", err)
		os.Exit(1)
	}
	if mc == nil {
		fmt.Fprintln(os.Stderr, "No market context found — run the analyzer pipeline first")
		os.Exit(1)
	}

	// Load gem features.
	fmt.Fprintf(os.Stderr, "Loading gem features (%dh range)...\n", *hours)
	t0 := time.Now()
	features, err := repo.AllGemFeaturesInRange(ctx, *hours)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to load gem features: %v\n", err)
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Loaded %d features in %s\n", len(features), time.Since(t0).Round(time.Millisecond))

	// Load snapshot prices for ground truth.
	fmt.Fprintf(os.Stderr, "Loading snapshot prices (%dh range)...\n", *hours)
	t1 := time.Now()
	prices, err := repo.SnapshotPricesInRange(ctx, *hours)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to load snapshot prices: %v\n", err)
		os.Exit(1)
	}
	fmt.Fprintf(os.Stderr, "Loaded %d prices in %s\n", len(prices), time.Since(t1).Round(time.Millisecond))

	// Build evaluation points.
	fmt.Fprintf(os.Stderr, "Building eval points (horizon=%s)...\n", horizonDur)
	evals, dropped := lab.BuildEvalPoints(features, prices, horizonDur)
	fmt.Fprintf(os.Stderr, "Built %d eval points, dropped %d (no valid future price)\n", len(evals), dropped)

	if len(evals) < 1000 {
		fmt.Fprintf(os.Stderr, "WARNING: Only %d eval points — results may be unreliable (recommend >= 1000)\n", len(evals))
	}

	if len(evals) == 0 {
		fmt.Fprintln(os.Stderr, "No eval points — cannot sweep. Check time range and data freshness.")
		os.Exit(1)
	}

	// Generate grid and sweep.
	grid := lab.GenerateSigmaGrid()
	fmt.Fprintf(os.Stderr, "Sweeping %d sigma combos over %d eval points...\n", len(grid), len(evals))
	t2 := time.Now()
	results := lab.SweepV2(evals, *mc, grid)
	fmt.Fprintf(os.Stderr, "Sweep complete in %s\n", time.Since(t2).Round(time.Millisecond))

	n := *topN
	if n > len(results) {
		n = len(results)
	}

	if *jsonMode {
		printJSON(results[:n], mc, len(evals), dropped, len(grid), *hours, *horizon)
	} else {
		printConsole(results[:n], mc, len(evals), dropped, len(grid), *hours, *horizon)
	}
}

// printConsole outputs the multi-section human-readable report.
func printConsole(results []lab.SweepResultV2, mc *lab.MarketContext, evalCount, droppedCount, gridSize, hours int, horizon string) {
	// Section 1: Context
	fmt.Println()
	fmt.Println("=== OPTIMIZER v2 CONTEXT ===")
	fmt.Printf("  Market time:       %s\n", mc.Time.Format(time.RFC3339))
	fmt.Printf("  Total gems:        %d\n", mc.TotalGems)
	fmt.Printf("  Velocity:          mean=%.2f sigma=%.2f\n", mc.VelocityMean, mc.VelocitySigma)
	fmt.Printf("  Listing velocity:  mean=%.2f sigma=%.2f\n", mc.ListingVelMean, mc.ListingVelSigma)
	fmt.Printf("  Eval points:       %d (dropped %d)\n", evalCount, droppedCount)
	fmt.Printf("  Grid size:         %d combos\n", gridSize)
	fmt.Printf("  Time range:        %dh, horizon=%s\n", hours, horizon)
	fmt.Println()

	// Section 2: Top N table
	fmt.Println("=== TOP RESULTS (sorted by weighted score) ===")
	fmt.Println()
	fmt.Printf("%-5s %-9s %-7s %-7s %-7s %-7s %-9s %-6s  %-7s %-7s %-7s %-7s %-7s\n",
		"Rank", "WtScore", "TOP%", "HIGH%", "MID%", "LOW%", "Overall%", "Sweet",
		"HP_m", "HL_m", "SP_m", "BP_m", "DP_m")
	fmt.Println(strings.Repeat("-", 105))

	for i, r := range results {
		sweetStr := fmt.Sprintf("%d", r.SweetSpot)
		if r.SweetSpot < 0 {
			sweetStr = "  -"
		}
		fmt.Printf("%-5d %-9.1f %-7.1f %-7.1f %-7.1f %-7.1f %-9.1f %-6s  %-7.2f %-7.2f %-7.2f %-7.2f %-7.2f\n",
			i+1, r.WeightedScore, r.TopAcc, r.HighAcc, r.MidAcc, r.LowAcc, r.OverallAcc, sweetStr,
			r.Sigma.HERDPriceMult, r.Sigma.HERDListingMult,
			r.Sigma.StablePriceMult, r.Sigma.BrewingPriceMult, r.Sigma.DumpPriceMult)
	}

	// Section 3: Best result confidence breakdown
	if len(results) > 0 {
		best := results[0]
		fmt.Println()
		fmt.Println("=== BEST RESULT — CONFIDENCE BANDS ===")
		fmt.Printf("  Config: HP_m=%.2f  HL_m=%.2f  SP_m=%.2f  BP_m=%.2f  DP_m=%.2f\n",
			best.Sigma.HERDPriceMult, best.Sigma.HERDListingMult,
			best.Sigma.StablePriceMult, best.Sigma.BrewingPriceMult, best.Sigma.DumpPriceMult)
		fmt.Printf("  Total evals: %d  High-conf evals (>=70): %d\n", best.TotalEvals, best.HighConfEvals)
		fmt.Println()
		fmt.Printf("  %-12s %-10s %-8s\n", "Conf Range", "Accuracy", "Count")
		fmt.Printf("  %s\n", strings.Repeat("-", 32))
		for _, b := range best.ConfBands {
			fmt.Printf("  %-12s %-10.1f %-8d\n",
				fmt.Sprintf("%d-%d", b.MinConf, b.MaxConf), b.Accuracy, b.Count)
		}

		// Section 4: Best result temporal accuracy
		fmt.Println()
		fmt.Println("=== BEST RESULT — TEMPORAL ACCURACY ===")
		phases := []string{"weekday-peak", "weekday-offpeak", "weekend"}
		for _, phase := range phases {
			acc, ok := best.TemporalAcc[phase]
			if ok {
				fmt.Printf("  %-20s %.1f%%\n", phase, acc)
			} else {
				fmt.Printf("  %-20s (no data)\n", phase)
			}
		}
	}

	// Section 5: Current defaults comparison
	fmt.Println()
	fmt.Println("=== CURRENT DEFAULTS (approximate sigma equivalents) ===")
	def := lab.DefaultSignalConfig()
	approxHP, approxHL, approxSP, approxBP := approxSigmaEquivalents(def, mc)
	fmt.Printf("  PreHERDPriceVel=%.0f   -> approx HP_m=%.2f\n", def.PreHERDPriceVel, approxHP)
	fmt.Printf("  PreHERDListingVel=%.0f  -> approx HL_m=%.2f\n", def.PreHERDListingVel, approxHL)
	fmt.Printf("  StablePriceVel=%.1f     -> approx SP_m=%.2f\n", def.StablePriceVel, approxSP)
	fmt.Printf("  BrewingMinPVel=%.1f     -> approx BP_m=%.2f\n", def.BrewingMinPVel, approxBP)
	fmt.Println()
}

// printJSON outputs the structured JSON report.
func printJSON(results []lab.SweepResultV2, mc *lab.MarketContext, evalCount, droppedCount, gridSize, hours int, horizon string) {
	out := JSONOutput{
		Context: JSONContext{
			MarketTime:      mc.Time,
			TotalGems:       mc.TotalGems,
			VelocityMean:    mc.VelocityMean,
			VelocitySigma:   mc.VelocitySigma,
			ListingVelMean:  mc.ListingVelMean,
			ListingVelSigma: mc.ListingVelSigma,
			EvalPoints:      evalCount,
			DroppedPoints:   droppedCount,
			GridSize:        gridSize,
			Hours:           hours,
			Horizon:         horizon,
		},
		Results: make([]JSONResult, 0, len(results)),
	}

	for i, r := range results {
		out.Results = append(out.Results, JSONResult{
			Rank:          i + 1,
			WeightedScore: r.WeightedScore,
			TopAcc:        r.TopAcc,
			HighAcc:       r.HighAcc,
			MidAcc:        r.MidAcc,
			LowAcc:        r.LowAcc,
			OverallAcc:    r.OverallAcc,
			SweetSpot:     r.SweetSpot,
			Sigma:         r.Sigma,
			TemporalAcc:   r.TemporalAcc,
		})
	}

	if len(results) > 0 {
		best := results[0]
		out.BestDetail = &JSONBestDetail{
			ConfBands:   best.ConfBands,
			TemporalAcc: best.TemporalAcc,
		}
	}

	def := lab.DefaultSignalConfig()
	approxHP, approxHL, approxSP, approxBP := approxSigmaEquivalents(def, mc)
	out.Defaults = &JSONDefaults{
		ApproxHERDPriceMult:    approxHP,
		ApproxHERDListingMult:  approxHL,
		ApproxStablePriceMult:  approxSP,
		ApproxBrewingPriceMult: approxBP,
	}

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(out); err != nil {
		fmt.Fprintf(os.Stderr, "JSON encode error: %v\n", err)
		os.Exit(1)
	}
}

// approxSigmaEquivalents computes approximate sigma multipliers for the current
// default absolute thresholds by dividing by the market context's sigma values.
// Returns 0 for any field where the corresponding sigma is zero (avoids NaN in JSON output).
// This is a simple division for reference, not an exact inversion.
func approxSigmaEquivalents(def lab.SignalConfig, mc *lab.MarketContext) (herdP, herdL, stableP, brewP float64) {
	if mc.VelocitySigma > 0 {
		herdP = (def.PreHERDPriceVel - mc.VelocityMean) / mc.VelocitySigma
		stableP = def.StablePriceVel / mc.VelocitySigma
		brewP = def.BrewingMinPVel / mc.VelocitySigma
	}
	if mc.ListingVelSigma > 0 {
		herdL = (def.PreHERDListingVel - mc.ListingVelMean) / mc.ListingVelSigma
	}
	return
}
