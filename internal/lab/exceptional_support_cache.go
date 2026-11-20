package lab

import (
	"context"
	"fmt"
	"log/slog"
	"sync"

	"profitofexile/internal/league"
)

// ExceptionalSupportCache retains one league's exceptional-support snapshot
// until the next accepted gem snapshot: the gem-snapshot revision the ranking
// answers already advance on each collector gem event. Overlapping cold polls
// share one read, detached from any one caller; the last waiter leaving
// cancels it, and a cancelled fill is not retained. A fill holds the ranking
// publication barrier across its SQL so it never retains one revision's rows
// under another. Failed reads are not retained.
type ExceptionalSupportCache struct {
	load    func(context.Context, league.Scope) (ExceptionalSupportSnapshot, error)
	answers *RankingAnswers
	scopeID string

	mu         sync.Mutex
	warm       bool
	generation rankingGeneration
	snap       ExceptionalSupportSnapshot
	flight     *exceptionalSupportFlight
}

type exceptionalSupportFlight struct {
	generation rankingGeneration
	done       chan struct{}
	cancel     context.CancelFunc
	waiters    int
	snap       ExceptionalSupportSnapshot
	err        error
}

// NewExceptionalSupportCache serves load's snapshot for cache's league,
// invalidated by cache's gem-snapshot publications. A nil cache serves load
// uncached.
func NewExceptionalSupportCache(load func(context.Context, league.Scope) (ExceptionalSupportSnapshot, error), cache *Cache) *ExceptionalSupportCache {
	c := &ExceptionalSupportCache{load: load}
	if cache != nil {
		c.answers = cache.Rankings()
		c.scopeID = cache.scope.ID()
	}
	return c
}

// ExceptionalSupportPrices returns the league's exceptional-support snapshot.
// The snapshot's Chaos map is shared and must not be modified.
func (c *ExceptionalSupportCache) ExceptionalSupportPrices(ctx context.Context, scope league.Scope) (ExceptionalSupportSnapshot, error) {
	if c.answers == nil || scope.ID() != c.scopeID || ctx.Err() != nil {
		return c.load(ctx, scope)
	}
	c.mu.Lock()
	// Read under mu, so a request holding an older generation cannot displace
	// a newer flight.
	generation := c.answers.gemSnapshotGeneration()
	if c.warm && c.generation == generation {
		snap := c.snap
		c.mu.Unlock()
		return snap, nil
	}
	f := c.flight
	if f == nil || f.generation != generation {
		loadCtx, cancel := context.WithCancel(context.WithoutCancel(ctx))
		f = &exceptionalSupportFlight{generation: generation, done: make(chan struct{}), cancel: cancel}
		c.flight = f
		go c.fill(loadCtx, scope, f)
	}
	f.waiters++
	c.mu.Unlock()

	select {
	case <-f.done:
		return f.snap, f.err
	case <-ctx.Done():
		c.mu.Lock()
		f.waiters--
		// The last waiter gone stops the read and releases the publication
		// barrier; a later poll starts a fresh flight.
		if f.waiters == 0 {
			if c.flight == f {
				c.flight = nil
			}
			f.cancel()
		}
		c.mu.Unlock()
		return ExceptionalSupportSnapshot{}, ctx.Err()
	}
}

func (c *ExceptionalSupportCache) fill(ctx context.Context, scope league.Scope, f *exceptionalSupportFlight) {
	c.answers.barrier.RLock()
	defer c.answers.barrier.RUnlock()
	defer func() {
		if recovered := recover(); recovered != nil {
			slog.Error("lab: exceptional support load panicked", "recover", recovered)
			f.snap, f.err = ExceptionalSupportSnapshot{}, fmt.Errorf("lab: exceptional support load panicked: %v", recovered)
		}
		c.mu.Lock()
		// Retained only when no gem snapshot was accepted since the flight began.
		if f.err == nil && ctx.Err() == nil && c.answers.gemSnapshotGeneration() == f.generation {
			c.warm, c.generation, c.snap = true, f.generation, f.snap
		}
		if c.flight == f {
			c.flight = nil
		}
		c.mu.Unlock()
		f.cancel()
		close(f.done)
	}()
	f.snap, f.err = c.load(ctx, scope)
}

// gemSnapshotGeneration is the current revision of gem_snapshots.
func (rc *RankingAnswers) gemSnapshotGeneration() rankingGeneration {
	rc.mu.Lock()
	defer rc.mu.Unlock()
	return rc.generationLocked(RankingInputGemSnapshots)
}
