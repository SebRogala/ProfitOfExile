package lab

import (
	"context"
	"sync"
	"testing"
	"time"

	"profitofexile/internal/league"
)

// snapshotSource serves the snapshot it holds and counts its reads; a non-nil
// gate holds each read until it is closed.
type snapshotSource struct {
	mu      sync.Mutex
	snap    ExceptionalSupportSnapshot
	reads   int
	started chan struct{}
	gate    chan struct{}
}

func (s *snapshotSource) set(snap ExceptionalSupportSnapshot) {
	s.mu.Lock()
	s.snap = snap
	s.mu.Unlock()
}

func (s *snapshotSource) read(context.Context, league.Scope) (ExceptionalSupportSnapshot, error) {
	s.mu.Lock()
	s.reads++
	snap, started, gate := s.snap, s.started, s.gate
	s.mu.Unlock()
	if started != nil {
		started <- struct{}{}
	}
	if gate != nil {
		<-gate
	}
	return snap, nil
}

func (s *snapshotSource) readCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.reads
}

func snapshotAt(hour int, chaos map[string]float64) ExceptionalSupportSnapshot {
	return ExceptionalSupportSnapshot{AsOf: time.Date(2026, 11, 20, hour, 0, 0, 0, time.UTC), Chaos: chaos}
}

func pollSnapshot(t *testing.T, c *ExceptionalSupportCache, scope league.Scope) ExceptionalSupportSnapshot {
	t.Helper()
	snap, err := c.ExceptionalSupportPrices(context.Background(), scope)
	if err != nil {
		t.Fatalf("poll: %v", err)
	}
	return snap
}

func TestKeepCriticalExceptionalSupportsGeneration(t *testing.T) {
	scope := league.Historical("Mirage")
	publishGemSnapshot := func(c *Cache) { c.Rankings().Publish(RankingInputGemSnapshots) }

	// invalidated warms a cache at an hour-1 snapshot holding Enhance and
	// Enlighten, swaps the source to an hour-2 snapshot without Enlighten and
	// accepts a gem snapshot.
	invalidated := func(t *testing.T) (*ExceptionalSupportCache, *snapshotSource) {
		src := &snapshotSource{snap: snapshotAt(1, map[string]float64{"Enhance Support": 9, "Enlighten Support": 185})}
		labCache := NewCache(scope)
		c := NewExceptionalSupportCache(src.read, labCache)
		pollSnapshot(t, c, scope)
		src.set(snapshotAt(2, map[string]float64{"Enhance Support": 11}))
		publishGemSnapshot(labCache)
		return c, src
	}

	t.Run("an accepted gem snapshot serves the new snapshot", func(t *testing.T) {
		c, src := invalidated(t)

		got := pollSnapshot(t, c, scope)

		if !got.AsOf.Equal(snapshotAt(2, nil).AsOf) || got.Chaos["Enhance Support"] != 11 || src.readCount() != 2 {
			t.Errorf("after the event = %+v after %d reads, want the hour-2 snapshot after 2", got, src.readCount())
		}
	})

	t.Run("a support missing from the new snapshot never carries forward", func(t *testing.T) {
		c, _ := invalidated(t)

		if chaos, ok := pollSnapshot(t, c, scope).Chaos["Enlighten Support"]; ok {
			t.Errorf("Enlighten = %v carried forward, want no price", chaos)
		}
	})

	t.Run("a fill begun before an event cannot serve the new generation", func(t *testing.T) {
		src := &snapshotSource{snap: snapshotAt(1, map[string]float64{"Enhance Support": 9}), started: make(chan struct{}, 1), gate: make(chan struct{})}
		labCache := NewCache(scope)
		c := NewExceptionalSupportCache(src.read, labCache)
		oldPoll := make(chan ExceptionalSupportSnapshot)
		go func() {
			snap, _ := c.ExceptionalSupportPrices(context.Background(), scope)
			oldPoll <- snap
		}()
		<-src.started
		published := make(chan struct{})
		go func() {
			publishGemSnapshot(labCache)
			close(published)
		}()
		src.set(snapshotAt(2, map[string]float64{"Enhance Support": 11}))
		close(src.gate)
		<-oldPoll
		<-published
		src.mu.Lock()
		src.started, src.gate = nil, nil
		src.mu.Unlock()

		if got := pollSnapshot(t, c, scope); !got.AsOf.Equal(snapshotAt(2, nil).AsOf) {
			t.Errorf("after the event asOf = %v, want the hour-2 snapshot", got.AsOf)
		}
	})

	t.Run("an empty league stays warm", func(t *testing.T) {
		src := &snapshotSource{}
		c := NewExceptionalSupportCache(src.read, NewCache(scope))
		pollSnapshot(t, c, scope)
		pollSnapshot(t, c, scope)

		if n := src.readCount(); n != 1 {
			t.Errorf("reads = %d, want 1", n)
		}
	})

	t.Run("an empty league refills on invalidation", func(t *testing.T) {
		src := &snapshotSource{}
		labCache := NewCache(scope)
		c := NewExceptionalSupportCache(src.read, labCache)
		pollSnapshot(t, c, scope)
		src.set(snapshotAt(3, map[string]float64{"Empower Support": 43.2}))
		publishGemSnapshot(labCache)

		if got := pollSnapshot(t, c, scope); got.AsOf.IsZero() {
			t.Errorf("after the event asOf is zero, want the hour-3 snapshot")
		}
	})

	t.Run("without an accepted event the prior snapshot stays", func(t *testing.T) {
		src := &snapshotSource{snap: snapshotAt(1, map[string]float64{"Enhance Support": 9})}
		c := NewExceptionalSupportCache(src.read, NewCache(scope))
		pollSnapshot(t, c, scope)
		src.set(snapshotAt(2, map[string]float64{"Enhance Support": 11}))

		if got := pollSnapshot(t, c, scope); !got.AsOf.Equal(snapshotAt(1, nil).AsOf) {
			t.Errorf("without an event asOf = %v, want the prior hour-1 snapshot", got.AsOf)
		}
	})

	t.Run("the next accepted event after a suppressed one heals the snapshot", func(t *testing.T) {
		src := &snapshotSource{snap: snapshotAt(1, map[string]float64{"Enhance Support": 9})}
		labCache := NewCache(scope)
		c := NewExceptionalSupportCache(src.read, labCache)
		pollSnapshot(t, c, scope)
		src.set(snapshotAt(2, map[string]float64{"Enhance Support": 11}))
		pollSnapshot(t, c, scope)
		publishGemSnapshot(labCache)

		if got := pollSnapshot(t, c, scope); !got.AsOf.Equal(snapshotAt(2, nil).AsOf) {
			t.Errorf("after the next event asOf = %v, want the hour-2 snapshot", got.AsOf)
		}
	})

	t.Run("the last waiter leaving cancels the fill and releases the publication barrier", func(t *testing.T) {
		src := &snapshotSource{started: make(chan struct{}, 1)}
		labCache := NewCache(scope)
		cancelled := make(chan struct{})
		c := NewExceptionalSupportCache(func(ctx context.Context, s league.Scope) (ExceptionalSupportSnapshot, error) {
			src.read(ctx, s)
			<-ctx.Done()
			close(cancelled)
			return ExceptionalSupportSnapshot{}, ctx.Err()
		}, labCache)
		ctx, cancel := context.WithCancel(context.Background())
		polled := make(chan error)
		go func() {
			_, err := c.ExceptionalSupportPrices(ctx, scope)
			polled <- err
		}()
		<-src.started
		cancel()
		<-polled

		published := make(chan struct{})
		go func() {
			publishGemSnapshot(labCache)
			close(published)
		}()
		select {
		case <-cancelled:
		case <-time.After(5 * time.Second):
			t.Fatal("the abandoned fill was not cancelled")
		}
		select {
		case <-published:
		case <-time.After(5 * time.Second):
			t.Fatal("Publish stayed blocked behind the abandoned fill")
		}
	})
}
