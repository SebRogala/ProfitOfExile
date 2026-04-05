// cmd/recalculate triggers a full recomputation via the running server process.
// Calls the server's /api/internal/recalculate endpoint on localhost, which
// updates the in-memory cache AND triggers Mercure events to the frontend.
//
// Usage (on prod via docker exec):
//
//	docker exec <server-container> /recalculate
package main

import (
	"fmt"
	"io"
	"net/http"
	"os"
	"time"
)

func main() {
	secret := os.Getenv("INTERNAL_SECRET")
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}
	url := fmt.Sprintf("http://localhost:%s/api/internal/recalculate", port)

	fmt.Printf("Triggering recalculation on %s...\n", url)

	req, err := http.NewRequest("POST", url, nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to create request: %v\n", err)
		os.Exit(1)
	}
	if secret != "" {
		req.Header.Set("X-Internal-Token", secret)
	}

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to reach server: %v\n", err)
		fmt.Fprintf(os.Stderr, "Is the server process running in this container?\n")
		os.Exit(1)
	}
	defer resp.Body.Close()

	body, _ := io.ReadAll(resp.Body)

	if resp.StatusCode != http.StatusOK {
		fmt.Fprintf(os.Stderr, "Server returned %d: %s\n", resp.StatusCode, string(body))
		os.Exit(1)
	}

	fmt.Printf("OK: %s\n", string(body))
	fmt.Println("Recalculation started on server — cache + Mercure will update.")
}
