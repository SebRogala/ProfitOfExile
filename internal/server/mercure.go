package server

import (
	"bufio"
	"context"
	"fmt"
	"log/slog"
	"net"
	"net/http"
	"net/url"
	"strings"
	"time"
)

// MercureEvent represents a parsed SSE event from the Mercure hub.
type MercureEvent struct {
	Topic string
	Data  string
}

// MercureSubscriber connects to a Mercure hub and dispatches events to a handler.
type MercureSubscriber struct {
	hubURL  string
	topics  []string
	handler func(MercureEvent)
	logger  *slog.Logger
	client  *http.Client
}

// NewMercureSubscriber creates a subscriber that listens to the given topics.
func NewMercureSubscriber(hubURL string, topics []string, handler func(MercureEvent)) *MercureSubscriber {
	return &MercureSubscriber{
		hubURL:  hubURL,
		topics:  topics,
		handler: handler,
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
	backoff := time.Second
	maxBackoff := 30 * time.Second

	for {
		err := s.connect(ctx)
		if ctx.Err() != nil {
			s.logger.Info("mercure subscriber stopped")
			return
		}

		s.logger.Warn("mercure connection lost, reconnecting", "error", err, "backoff", backoff)

		select {
		case <-ctx.Done():
			return
		case <-time.After(backoff):
		}

		backoff = min(backoff*2, maxBackoff)
	}
}

func (s *MercureSubscriber) connect(ctx context.Context) error {
	q := url.Values{}
	for _, t := range s.topics {
		q.Add("topic", t)
	}
	u := s.hubURL + "?" + q.Encode()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return fmt.Errorf("create request: %w", err)
	}
	req.Header.Set("Accept", "text/event-stream")

	resp, err := s.client.Do(req)
	if err != nil {
		return fmt.Errorf("connect to hub: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("hub returned status %d", resp.StatusCode)
	}

	s.logger.Info("mercure subscriber connected", "topics", s.topics)

	scanner := bufio.NewScanner(resp.Body)
	var event MercureEvent

	for scanner.Scan() {
		line := scanner.Text()

		if line == "" {
			// Empty line = end of event
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
		return fmt.Errorf("read SSE stream: %w", err)
	}

	return fmt.Errorf("stream ended")
}
