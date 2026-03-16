package mercure

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// Publisher is the interface for publishing Mercure events. Implementations
// include HubPublisher (real hub) and test doubles.
type Publisher interface {
	Publish(ctx context.Context, topic, payload string) error
}

// HubPublisher publishes events to a Mercure hub over HTTP.
type HubPublisher struct {
	URL    string
	Secret string
}

// Publish sends an event to the Mercure hub on the given topic.
func (p *HubPublisher) Publish(ctx context.Context, topic, payload string) error {
	return PublishMercureEvent(ctx, p.URL, p.Secret, topic, payload)
}

// mercureClient is a shared HTTP client for Mercure publish calls.
// Reused across calls to benefit from connection pooling.
// http.Client is safe for concurrent use by multiple goroutines.
var mercureClient = &http.Client{Timeout: 5 * time.Second}

// signMercureJWT creates an HS256-signed JWT with publish permissions for the
// Mercure hub. The secret is used as the HMAC-SHA256 signing key. A short-lived
// expiry (60s) is included as defense-in-depth; a fresh token is generated per
// publish call so the short TTL has no operational impact.
func signMercureJWT(secret string) (string, error) {
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`))

	now := time.Now()
	claims := map[string]any{
		"mercure": map[string]any{
			"publish": []string{"*"},
		},
		"iat": now.Unix(),
		"exp": now.Add(60 * time.Second).Unix(),
	}
	claimsJSON, err := json.Marshal(claims)
	if err != nil {
		return "", fmt.Errorf("marshal JWT claims: %w", err)
	}
	payload := base64.RawURLEncoding.EncodeToString(claimsJSON)

	unsigned := header + "." + payload
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(unsigned))
	signature := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))

	return unsigned + "." + signature, nil
}

// PublishMercureEvent posts an event to the Mercure hub.
// If mercureURL or publisherSecret is empty, the publish is silently skipped
// (Mercure not configured). The publisherSecret is the HMAC key used to sign a
// JWT with publish permissions. Publish failures are returned as errors but
// callers should treat them as non-fatal — the snapshot was already stored
// successfully.
func PublishMercureEvent(ctx context.Context, mercureURL, publisherSecret, topic, payload string) error {
	if mercureURL == "" || publisherSecret == "" {
		return nil
	}

	token, err := signMercureJWT(publisherSecret)
	if err != nil {
		return fmt.Errorf("mercure: sign JWT: %w", err)
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
	req.Header.Set("Authorization", "Bearer "+token)

	resp, err := mercureClient.Do(req)
	if err != nil {
		return fmt.Errorf("mercure: publish to %s: %w", mercureURL, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		return fmt.Errorf("mercure: publish to %s: status %d: %s", mercureURL, resp.StatusCode, string(body))
	}

	return nil
}
