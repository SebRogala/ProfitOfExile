package lab

import (
	"context"
	"sync/atomic"
	"testing"
	"time"
)

func TestThrottler_DebouncesMultipleSignals(t *testing.T) {
	var publishCount atomic.Int32

	throttler := NewThrottler("http://mercure/.well-known/mercure", "test-secret", 50*time.Millisecond)
	throttler.publishFn = func(_ context.Context, _, _, topic, _ string) error {
		if topic != "poe/analysis/updated" {
			t.Errorf("unexpected topic: %s", topic)
		}
		publishCount.Add(1)
		return nil
	}

	// Fire 5 rapid signals — only 1 publish should occur.
	for i := 0; i < 5; i++ {
		throttler.Signal()
	}

	// Wait for debounce + margin.
	time.Sleep(150 * time.Millisecond)

	if got := publishCount.Load(); got != 1 {
		t.Errorf("expected 1 publish, got %d", got)
	}
}

func TestThrottler_NilThrottlerSignalIsNoop(t *testing.T) {
	var throttler *Throttler
	// Must not panic.
	throttler.Signal()
}

func TestThrottler_EmptyMercureURLIsNoop(t *testing.T) {
	var publishCount atomic.Int32

	throttler := NewThrottler("", "", 10*time.Millisecond)
	throttler.publishFn = func(_ context.Context, _, _, _, _ string) error {
		publishCount.Add(1)
		return nil
	}

	throttler.Signal()
	time.Sleep(50 * time.Millisecond)

	if got := publishCount.Load(); got != 0 {
		t.Errorf("expected 0 publishes for empty mercureURL, got %d", got)
	}
}

func TestThrottler_SecondBurstAfterDebounce(t *testing.T) {
	var publishCount atomic.Int32

	throttler := NewThrottler("http://mercure/.well-known/mercure", "test-secret", 50*time.Millisecond)
	throttler.publishFn = func(_ context.Context, _, _, _, _ string) error {
		publishCount.Add(1)
		return nil
	}

	// First burst.
	throttler.Signal()
	throttler.Signal()
	time.Sleep(100 * time.Millisecond)

	// Second burst.
	throttler.Signal()
	throttler.Signal()
	time.Sleep(100 * time.Millisecond)

	if got := publishCount.Load(); got != 2 {
		t.Errorf("expected 2 publishes (one per burst), got %d", got)
	}
}
