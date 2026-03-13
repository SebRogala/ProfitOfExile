package collector

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// PublishMercureEvent posts an event to the Mercure hub.
// If mercureURL or jwtSecret is empty, the publish is silently skipped (Mercure
// not configured). Publish failures are returned as errors but callers should
// treat them as non-fatal — the snapshot was already stored successfully.
func PublishMercureEvent(ctx context.Context, mercureURL, jwtSecret, topic, payload string) error {
	if mercureURL == "" || jwtSecret == "" {
		return nil
	}

	form := url.Values{
		"topic": {topic},
		"data":  {payload},
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, mercureURL, strings.NewReader(form.Encode()))
	if err != nil {
		return fmt.Errorf("mercure: create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	req.Header.Set("Authorization", "Bearer "+jwtSecret)

	client := &http.Client{Timeout: 5 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("mercure: publish to %s: %w", mercureURL, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("mercure: publish to %s: unexpected status %d", mercureURL, resp.StatusCode)
	}

	return nil
}
