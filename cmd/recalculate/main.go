// cmd/recalculate triggers a full recomputation of all analysis pipelines.
// Runs: Transfigure, Quality, V2 (recompute), Font — in correct order.
// V2 runs before Font so Font reads fresh tier classification.
//
// Usage (on prod via docker exec):
//
//	docker exec <server-container> /app/recalculate
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/lab"
)

func main() {
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
	analyzer := lab.NewAnalyzer(repo, throttler, cache, nil)

	fmt.Println("Recalculating all analysis pipelines...")

	start := time.Now()

	fmt.Print("  Transfigure... ")
	if err := analyzer.RunTransfigure(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "FAILED: %v\n", err)
	} else {
		fmt.Println("OK")
	}

	fmt.Print("  Quality... ")
	if err := analyzer.RunQuality(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "FAILED: %v\n", err)
	} else {
		fmt.Println("OK")
	}

	fmt.Print("  V2 (recompute)... ")
	if err := analyzer.RecomputeLatestV2(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "FAILED: %v\n", err)
		os.Exit(1)
	}
	fmt.Println("OK")

	fmt.Print("  Font... ")
	if err := analyzer.RunFont(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "FAILED: %v\n", err)
	} else {
		fmt.Println("OK")
	}

	fmt.Printf("Done in %s\n", time.Since(start).Round(time.Millisecond))
}
