// cmd/recalculate triggers a full recomputation by publishing a Mercure
// event. The running server's subscriber receives poe/admin/recompute,
// runs the analysis pipeline in-process (so its in-memory cache stays
// consistent), and emits its own update events to the frontend.
//
// Usage on prod (run inside the server container so MERCURE_URL resolves
// to the docker-network hostname):
//
//	docker exec <server-container> /recalculate
//
// Required env vars (already present in the server container's env): MERCURE_URL,
// MERCURE_JWT_SECRET. No additional secret is required — Mercure JWT is the
// only auth boundary, and it's already configured for the existing
// collector→server pipeline.
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	"profitofexile/internal/mercure"
)

func main() {
	mercureURL := os.Getenv("MERCURE_URL")
	mercureSecret := os.Getenv("MERCURE_JWT_SECRET")
	if mercureURL == "" || mercureSecret == "" {
		fmt.Fprintln(os.Stderr, "recalculate: MERCURE_URL and MERCURE_JWT_SECRET must be set")
		os.Exit(1)
	}

	pub := &mercure.HubPublisher{URL: mercureURL, Secret: mercureSecret}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	if err := pub.Publish(ctx, "poe/admin/recompute", "{}"); err != nil {
		fmt.Fprintf(os.Stderr, "recalculate: publish failed: %v\n", err)
		os.Exit(1)
	}

	fmt.Println("recalculate: trigger published — server is running the pipeline.")
	fmt.Println("Watch server logs for 'admin recompute: complete' (~10–30s).")
}
