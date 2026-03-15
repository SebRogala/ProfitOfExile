package server

import (
	"bufio"
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// errStreamClosed is returned when the Mercure hub closes the SSE stream
// gracefully (clean EOF). Distinguished from network errors so the reconnect
// loop can log at Info level instead of Warn.
var errStreamClosed = errors.New("stream closed by hub")

// MercureEvent represents a parsed SSE event from the Mercure hub.
type MercureEvent struct {
	Topic string
	Data  string
}

// MercureSubscriber connects to a Mercure hub and dispatches events to a handler.
type MercureSubscriber struct {
	hubURL        string
	topics        []string
	subscriberKey string
	handler       func(MercureEvent)
	logger        *slog.Logger
	client        *http.Client
}

// NewMercureSubscriber creates a subscriber that listens to the given topics.
// subscriberKey is the HMAC secret for signing subscriber JWTs. If empty, connects without auth.
func NewMercureSubscriber(hubURL string, topics []string, subscriberKey string, handler func(MercureEvent)) *MercureSubscriber {
	return &MercureSubscriber{
		hubURL:        hubURL,
		topics:        topics,
		subscriberKey: subscriberKey,
		handler:       handler,
		logger:  slog.Default(),
		client: &http.Client{
			Transport: &http.Transport{
				DialContext:           (&net.Dialer{Timeout: 10 * time.Second}).DialContext,
				TLSHandshakeTimeout:  10 * time.Second,
				ResponseHeaderTimeout: 15 * time.Second,
			},
		},
	}
}

// Run connects to the Mercure hub and processes events until ctx is cancelled.
// It reconnects automatically on connection loss with exponential backoff.
func (s *MercureSubscriber) Run(ctx context.Context) {
	defer func() {
		if r := recover(); r != nil {
			s.logger.Error("mercure subscriber panicked", "panic", r)
		}
	}()

	backoff := time.Second
	maxBackoff := 30 * time.Second
	everConnected := false

	for {
		connected, err := s.connect(ctx)
		if ctx.Err() != nil {
			s.logger.Info("mercure subscriber stopped")
			return
		}

		if connected {
			everConnected = true
		}

		if errors.Is(err, errStreamClosed) {
			s.logger.Info("mercure: stream closed by hub, reconnecting", "backoff", backoff)
		} else if everConnected {
			s.logger.Warn("mercure: connection lost, reconnecting", "error", err, "backoff", backoff)
		} else {
			s.logger.Warn("mercure: initial connection failed, retrying", "error", err, "backoff", backoff)
		}

		select {
		case <-ctx.Done():
			return
		case <-time.After(backoff):
		}

		if errors.Is(err, errStreamClosed) {
			backoff = time.Second // reset on graceful close
		} else {
			backoff = min(backoff*2, maxBackoff)
		}
	}
}

// connect establishes an SSE connection and processes events. Returns whether
// a connection was successfully established (for log messaging) and any error.
func (s *MercureSubscriber) connect(ctx context.Context) (connected bool, err error) {
	q := url.Values{}
	for _, t := range s.topics {
		q.Add("topic", t)
	}
	u := s.hubURL + "?" + q.Encode()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return false, fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Accept", "text/event-stream")

	if s.subscriberKey != "" {
		token, err := signSubscriberJWT(s.subscriberKey, s.topics)
		if err != nil {
			return false, fmt.Errorf("sign subscriber JWT: %w", err)
		}
		req.Header.Set("Authorization", "Bearer "+token)
	}

	resp, err := s.client.Do(req)
	if err != nil {
		return false, fmt.Errorf("connect to hub: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return false, fmt.Errorf("hub returned status %d", resp.StatusCode)
	}

	s.logger.Info("mercure subscriber connected", "topics", s.topics)

	scanner := bufio.NewScanner(resp.Body)
	var event MercureEvent

	for scanner.Scan() {
		line := scanner.Text()

		if line == "" {
			if event.Data != "" {
				s.handler(event)
			}
			event = MercureEvent{}
			continue
		}

		if strings.HasPrefix(line, "data: ") {
			event.Data = strings.TrimPrefix(line, "data: ")
		} else if strings.HasPrefix(line, "event: ") {
			event.Topic = strings.TrimPrefix(line, "event: ")
		}
	}

	if err := scanner.Err(); err != nil {
		return true, fmt.Errorf("read SSE stream: %w", err)
	}

	return true, errStreamClosed
}

// signSubscriberJWT creates an HS256-signed JWT with subscribe permissions for
// the Mercure hub. Long-lived (24h) to avoid reconnect churn on expiry.
func signSubscriberJWT(secret string, topics []string) (string, error) {
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`))

	now := time.Now()
	claims := map[string]any{
		"mercure": map[string]any{
			"subscribe": topics,
		},
		"iat": now.Unix(),
		"exp": now.Add(24 * time.Hour).Unix(),
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
