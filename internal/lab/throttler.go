package lab

import (
	"context"
	"encoding/json"
	"log/slog"
	"sync"
	"time"

	"profitofexile/internal/collector"
)

// Throttler debounces analysis-complete signals and publishes a single Mercure
// event after a quiet period. This prevents flooding the frontend when multiple
// analyzers finish at slightly different times.
type Throttler struct {
	mercureURL    string
	mercureSecret string
	debounce      time.Duration
	mu            sync.Mutex
	timer         *time.Timer
	logger        *slog.Logger

	// publishFn is the function used to publish Mercure events. Defaults to
	// collector.PublishMercureEvent but can be overridden in tests.
	publishFn func(ctx context.Context, mercureURL, secret, topic, payload string) error
}

// NewThrottler creates a throttler that publishes to the Mercure hub after the
// debounce period elapses with no new signals. If mercureURL or mercureSecret
// is empty, Signal() becomes a no-op.
func NewThrottler(mercureURL, mercureSecret string, debounce time.Duration) *Throttler {
	return &Throttler{
		mercureURL:    mercureURL,
		mercureSecret: mercureSecret,
		debounce:      debounce,
		logger:        slog.Default(),
		publishFn:     collector.PublishMercureEvent,
	}
}

// Signal is called by analyzers when they complete. It resets the debounce
// timer. After the debounce period with no new signals, it publishes a single
// Mercure event on topic "poe/analysis/updated".
func (t *Throttler) Signal() {
	if t == nil {
		return
	}
	if t.mercureURL == "" || t.mercureSecret == "" {
		return
	}

	t.mu.Lock()
	defer t.mu.Unlock()

	if t.timer != nil {
		t.timer.Stop()
	}

	t.timer = time.AfterFunc(t.debounce, t.publish)
}

// publish sends the analysis-updated event to the Mercure hub.
func (t *Throttler) publish() {
	payload, err := json.Marshal(map[string]string{
		"timestamp": time.Now().UTC().Format(time.RFC3339),
		"type":      "analysis-batch",
	})
	if err != nil {
		t.logger.Error("throttler: failed to marshal payload", "error", err)
		return
	}

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := t.publishFn(ctx, t.mercureURL, t.mercureSecret, "poe/analysis/updated", string(payload)); err != nil {
		t.logger.Error("throttler: failed to publish analysis event", "error", err)
		return
	}

	t.logger.Info("throttler: published analysis-updated event")
}
