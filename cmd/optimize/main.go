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

// JSONOutput is the structured output for --json mode (sweep).
type JSONOutput struct {
	Context    JSONContext        `json:"context"`
	Results    []JSONResult       `json:"results"`
	BestDetail *JSONBestDetail   `json:"best_detail,omitempty"`
	Defaults   *JSONDefaults      `json:"defaults,omitempty"`
}

// JSONValidateOutput is the structured output for --validate --json mode.
type JSONValidateOutput struct {
	Context         JSONContext                        `json:"context"`
	PerSignal       map[string]lab.SignalAccuracy      `json:"per_signal"`
	ConfusionMatrix map[string]map[string]int          `json:"confusion_matrix"`
	ConfBands       []lab.ConfidenceBand               `json:"confidence_bands"`
	PerTier         map[string]float64                 `json:"per_tier"`
	PerPhase        map[string]float64                 `json:"per_phase"`
	SweetSpot       int                                `json:"sweet_spot"`
	TotalEvals      int                                `json:"total_evals"`
	OverallAcc      float64                            `json:"overall_acc"`
}

// JSONSellabilityOutput is the structured output for --validate-sellability --json mode.
type JSONSellabilityOutput struct {
	Context               JSONContext                           `json:"context"`
	PerSignalCapture      map[string]lab.ValueCapture           `json:"per_signal_capture"`
	FloorHoldRate         map[string]lab.FloorHoldResult        `json:"floor_hold_rate"`
	ConfidenceCalibration map[string]lab.ConfidenceCalResult    `json:"confidence_calibration"`
	PerTierCapture        map[string]lab.ValueCapture           `json:"per_tier_capture"`
	TotalEvals            int                                   `json:"total_evals"`
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
	validate := flag.Bool("validate", false, "Validate current defaults (skip grid sweep)")
	validateSellability := flag.Bool("validate-sellability", false, "Validate risk-adjusted value scoring")
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
		fmt.Fprintln(os.Stderr, "No eval points — cannot run. Check time range and data freshness.")
		os.Exit(1)
	}

	if *validate {
		// Validate current defaults — skip grid sweep.
		fmt.Fprintf(os.Stderr, "Validating defaults over %d eval points...\n", len(evals))
		t2 := time.Now()
		report := lab.ValidateDefaults(evals, *mc)
		fmt.Fprintf(os.Stderr, "Validation complete in %s\n", time.Since(t2).Round(time.Millisecond))

		if *jsonMode {
			printValidateJSON(report, mc, len(evals), dropped, *hours, *horizon)
		} else {
			printValidateConsole(report, mc, len(evals), dropped, *hours, *horizon)
		}
		return
	}

	if *validateSellability {
		// Validate risk-adjusted value scoring — skip grid sweep.
		fmt.Fprintf(os.Stderr, "Validating sellability over %d eval points...\n", len(evals))
		t2 := time.Now()
		report := lab.ValidateSellability(evals, *mc)
		fmt.Fprintf(os.Stderr, "Sellability validation complete in %s\n", time.Since(t2).Round(time.Millisecond))

		if *jsonMode {
			printSellabilityJSON(report, mc, len(evals), dropped, *hours, *horizon)
		} else {
			printSellabilityConsole(report, mc, len(evals), dropped, *hours, *horizon)
		}
		return
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

// printValidateConsole outputs the multi-section human-readable validation report.
func printValidateConsole(report lab.ValidationReport, mc *lab.MarketContext, evalCount, droppedCount, hours int, horizon string) {
	// Section 1: Context
	fmt.Println()
	fmt.Println("=== SIGNAL VALIDATION — CURRENT DEFAULTS ===")
	fmt.Printf("  Market time:       %s\n", mc.Time.Format(time.RFC3339))
	fmt.Printf("  Total gems:        %d\n", mc.TotalGems)
	fmt.Printf("  Velocity:          mean=%.2f sigma=%.2f\n", mc.VelocityMean, mc.VelocitySigma)
	fmt.Printf("  Listing velocity:  mean=%.2f sigma=%.2f\n", mc.ListingVelMean, mc.ListingVelSigma)
	fmt.Printf("  Eval points:       %d (dropped %d)\n", evalCount, droppedCount)
	fmt.Printf("  Time range:        %dh, horizon=%s\n", hours, horizon)
	fmt.Printf("  Overall accuracy:  %.1f%%\n", report.OverallAcc)
	sweetStr := fmt.Sprintf("%d", report.SweetSpot)
	if report.SweetSpot < 0 {
		sweetStr = "none"
	}
	fmt.Printf("  Sweet spot:        %s\n", sweetStr)
	fmt.Println()

	// Section 2: Per-signal accuracy scorecard
	fmt.Println("=== PER-SIGNAL ACCURACY ===")
	fmt.Println()
	fmt.Printf("  %-12s %-10s %-8s %-8s %-10s %-10s\n",
		"Signal", "Predicts", "Count", "Correct", "Accuracy", "AvgConf")
	fmt.Printf("  %s\n", strings.Repeat("-", 62))

	// Sort signals for deterministic output.
	signalOrder := []string{"HERD", "DUMPING", "UNCERTAIN", "RECOVERY", "STABLE", "TRAP"}
	for _, sig := range signalOrder {
		sa, ok := report.PerSignal[sig]
		if !ok {
			continue
		}
		fmt.Printf("  %-12s %-10s %-8d %-8d %-10.1f %-10.1f\n",
			sa.Signal, sa.Predicted, sa.Count, sa.Correct, sa.Accuracy, sa.AvgConfidence)
	}
	// Print any signals not in the standard order.
	for sig, sa := range report.PerSignal {
		found := false
		for _, s := range signalOrder {
			if s == sig {
				found = true
				break
			}
		}
		if !found {
			fmt.Printf("  %-12s %-10s %-8d %-8d %-10.1f %-10.1f\n",
				sa.Signal, sa.Predicted, sa.Count, sa.Correct, sa.Accuracy, sa.AvgConfidence)
		}
	}
	fmt.Println()

	// Section 3: Confusion matrix
	fmt.Println("=== CONFUSION MATRIX (predicted vs actual) ===")
	fmt.Println()
	dirs := []string{"UP", "DOWN", "FLAT"}
	fmt.Printf("  %-12s", "Predicted\\Actual")
	for _, d := range dirs {
		fmt.Printf(" %-8s", d)
	}
	fmt.Println()
	fmt.Printf("  %s\n", strings.Repeat("-", 38))
	for _, predicted := range dirs {
		fmt.Printf("  %-12s", predicted)
		row, ok := report.ConfusionMatrix[predicted]
		for _, actual := range dirs {
			count := 0
			if ok {
				count = row[actual]
			}
			fmt.Printf(" %-8d", count)
		}
		fmt.Println()
	}
	fmt.Println()

	// Section 4: Confidence bands
	fmt.Println("=== CONFIDENCE BANDS ===")
	fmt.Println()
	fmt.Printf("  %-12s %-10s %-8s\n", "Conf Range", "Accuracy", "Count")
	fmt.Printf("  %s\n", strings.Repeat("-", 32))
	for _, b := range report.ConfBands {
		fmt.Printf("  %-12s %-10.1f %-8d\n",
			fmt.Sprintf("%d-%d", b.MinConf, b.MaxConf), b.Accuracy, b.Count)
	}
	fmt.Println()

	// Section 5: Per-tier accuracy
	fmt.Println("=== PER-TIER ACCURACY ===")
	fmt.Println()
	tierOrder := []string{"TOP", "HIGH", "MID", "LOW"}
	for _, tier := range tierOrder {
		acc, ok := report.PerTier[tier]
		if ok {
			fmt.Printf("  %-12s %.1f%%\n", tier, acc)
		} else {
			fmt.Printf("  %-12s (no data)\n", tier)
		}
	}
	fmt.Println()

	// Section 6: Temporal accuracy
	fmt.Println("=== TEMPORAL ACCURACY ===")
	fmt.Println()
	phases := []string{"weekday-peak", "weekday-offpeak", "weekend"}
	for _, phase := range phases {
		acc, ok := report.PerPhase[phase]
		if ok {
			fmt.Printf("  %-20s %.1f%%\n", phase, acc)
		} else {
			fmt.Printf("  %-20s (no data)\n", phase)
		}
	}
	fmt.Println()
}

// printValidateJSON outputs the structured JSON validation report.
func printValidateJSON(report lab.ValidationReport, mc *lab.MarketContext, evalCount, droppedCount, hours int, horizon string) {
	out := JSONValidateOutput{
		Context: JSONContext{
			MarketTime:      mc.Time,
			TotalGems:       mc.TotalGems,
			VelocityMean:    mc.VelocityMean,
			VelocitySigma:   mc.VelocitySigma,
			ListingVelMean:  mc.ListingVelMean,
			ListingVelSigma: mc.ListingVelSigma,
			EvalPoints:      evalCount,
			DroppedPoints:   droppedCount,
			Hours:           hours,
			Horizon:         horizon,
		},
		PerSignal:       report.PerSignal,
		ConfusionMatrix: report.ConfusionMatrix,
		ConfBands:       report.ConfBands,
		PerTier:         report.PerTier,
		PerPhase:        report.PerPhase,
		SweetSpot:       report.SweetSpot,
		TotalEvals:      report.TotalEvals,
		OverallAcc:      report.OverallAcc,
	}

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(out); err != nil {
		fmt.Fprintf(os.Stderr, "JSON encode error: %v\n", err)
		os.Exit(1)
	}
}

// printSellabilityConsole outputs the human-readable sellability validation report.
func printSellabilityConsole(report lab.SellabilityReport, mc *lab.MarketContext, evalCount, droppedCount, hours int, horizon string) {
	// Section 1: Context
	fmt.Println()
	fmt.Println("=== Sellability Validation ===")
	fmt.Printf("  Market time:       %s\n", mc.Time.Format(time.RFC3339))
	fmt.Printf("  Total gems:        %d\n", mc.TotalGems)
	fmt.Printf("  Velocity:          mean=%.2f sigma=%.2f\n", mc.VelocityMean, mc.VelocitySigma)
	fmt.Printf("  Listing velocity:  mean=%.2f sigma=%.2f\n", mc.ListingVelMean, mc.ListingVelSigma)
	fmt.Printf("  Eval points:       %d (dropped %d)\n", evalCount, droppedCount)
	fmt.Printf("  Time range:        %dh, horizon=%s\n", hours, horizon)
	fmt.Printf("  Scored evals:      %d\n", report.TotalEvals)
	fmt.Println()

	// Section 2: Per-signal value capture
	fmt.Println("Per-Signal Value Capture (actual / risk-adjusted):")
	signalOrder := []string{"HERD", "STABLE", "DUMPING", "UNCERTAIN", "RECOVERY", "TRAP"}
	printedSignals := make(map[string]bool)
	for _, sig := range signalOrder {
		vc, ok := report.PerSignalCapture[sig]
		if !ok {
			continue
		}
		printedSignals[sig] = true
		fmt.Printf("  %-12s avg %.2f  median %.2f  [p25: %.2f  p75: %.2f]  (n=%d)\n",
			sig+":", vc.AvgCapture, vc.MedianCapture, vc.P25Capture, vc.P75Capture, vc.Count)
	}
	// Print any signals not in the standard order.
	for sig, vc := range report.PerSignalCapture {
		if printedSignals[sig] {
			continue
		}
		fmt.Printf("  %-12s avg %.2f  median %.2f  [p25: %.2f  p75: %.2f]  (n=%d)\n",
			sig+":", vc.AvgCapture, vc.MedianCapture, vc.P25Capture, vc.P75Capture, vc.Count)
	}
	fmt.Println()

	// Section 3: Floor hold rate
	fmt.Println("Floor Hold Rate (price stayed above 7d floor):")
	tierOrder := []string{"TOP", "HIGH", "MID", "LOW"}
	for _, tier := range tierOrder {
		fh, ok := report.FloorHoldRate[tier]
		if !ok {
			continue
		}
		fmt.Printf("  %-6s %.1f%%  (n=%d)\n", tier+":", fh.HeldRate, fh.Count)
	}
	fmt.Println()

	// Section 4: Sell confidence calibration
	fmt.Println("Sell Confidence Calibration:")
	confOrder := []string{"GREEN", "YELLOW", "RED"}
	for _, conf := range confOrder {
		cal, ok := report.ConfidenceCalibration[conf]
		if !ok {
			continue
		}
		fmt.Printf("  %-8s price held %.1f%% of time  avg change %.1f%%  (n=%d)\n",
			conf+":", cal.HeldRate, cal.AvgChange, cal.Count)
	}
	fmt.Println()

	// Section 5: Per-tier value capture
	fmt.Println("Per-Tier Value Capture:")
	for _, tier := range tierOrder {
		vc, ok := report.PerTierCapture[tier]
		if !ok {
			continue
		}
		fmt.Printf("  %-6s avg %.2f  median %.2f  [p25: %.2f  p75: %.2f]  (n=%d)\n",
			tier+":", vc.AvgCapture, vc.MedianCapture, vc.P25Capture, vc.P75Capture, vc.Count)
	}
	fmt.Println()
}

// printSellabilityJSON outputs the structured JSON sellability validation report.
func printSellabilityJSON(report lab.SellabilityReport, mc *lab.MarketContext, evalCount, droppedCount, hours int, horizon string) {
	out := JSONSellabilityOutput{
		Context: JSONContext{
			MarketTime:      mc.Time,
			TotalGems:       mc.TotalGems,
			VelocityMean:    mc.VelocityMean,
			VelocitySigma:   mc.VelocitySigma,
			ListingVelMean:  mc.ListingVelMean,
			ListingVelSigma: mc.ListingVelSigma,
			EvalPoints:      evalCount,
			DroppedPoints:   droppedCount,
			Hours:           hours,
			Horizon:         horizon,
		},
		PerSignalCapture:      report.PerSignalCapture,
		FloorHoldRate:         report.FloorHoldRate,
		ConfidenceCalibration: report.ConfidenceCalibration,
		PerTierCapture:        report.PerTierCapture,
		TotalEvals:            report.TotalEvals,
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
