// Package book holds one league's resolved prices in memory. Reads are a
// pointer load and a map read; only Seed reads the stores.
package book

import (
	"context"
	"fmt"
	"sync"
	"sync/atomic"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

// Snapshot is one published, immutable view of the book.
type Snapshot struct {
	obs      map[resolve.Key][]resolve.Observation
	resolved map[resolve.Key]resolve.Resolved
	rate     resolve.Rate
	at       time.Time
}

// Get returns the resolved price of key in this snapshot.
func (s *Snapshot) Get(key resolve.Key) (resolve.Resolved, bool) {
	if s == nil {
		return resolve.Resolved{}, false
	}
	r, ok := s.resolved[key]
	return detached(r, ok)
}

// Book is bound to one league scope and one policy.
type Book struct {
	scope  league.Scope
	policy resolve.Policy
	trade  TradeObservations
	ninja  NinjaGemObservations
	rate   DivineRate

	writeMu sync.Mutex
	current atomic.Pointer[Snapshot]
}

// New builds an empty book; Seed fills it.
func New(scope league.Scope, policy resolve.Policy, trade TradeObservations, ninja NinjaGemObservations, rate DivineRate) *Book {
	return &Book{scope: scope, policy: policy, trade: trade, ninja: ninja, rate: rate}
}

// Seed reads both stores once as of now and publishes every key resolved at
// the current divine rate. On error the previous snapshot stays.
func (b *Book) Seed(ctx context.Context, now time.Time) error {
	tradeObs, err := b.trade.ObservationsAsOf(ctx, b.scope, now)
	if err != nil {
		return fmt.Errorf("price book: trade observations: %w", err)
	}
	ninjaObs, err := b.ninja.GemObservationsAsOf(ctx, b.scope, now)
	if err != nil {
		return fmt.Errorf("price book: ninja observations: %w", err)
	}
	obs := make(map[resolve.Key][]resolve.Observation)
	for _, set := range [][]resolve.Observation{tradeObs, ninjaObs} {
		for _, o := range set {
			if o.Key.League != b.scope.ID() {
				continue
			}
			obs[o.Key] = append(obs[o.Key], owned(o))
		}
	}
	rate := b.rate()

	b.writeMu.Lock()
	defer b.writeMu.Unlock()
	b.current.Store(b.build(obs, rate, now))
	return nil
}

// Snapshot returns the currently published snapshot (nil before any publish).
func (b *Book) Snapshot() *Snapshot { return b.current.Load() }

// Get reads the published snapshot; it never calls a port.
func (b *Book) Get(key resolve.Key) (resolve.Resolved, bool) {
	return b.current.Load().Get(key)
}

// Observe adds one observation and re-resolves that key only, publishing a new
// snapshot. An observation of another league is refused.
func (b *Book) Observe(o resolve.Observation) error {
	if o.Key.League != b.scope.ID() {
		return fmt.Errorf("price book: observation for league %q refused by the %q book", o.Key.League, b.scope.ID())
	}
	b.writeMu.Lock()
	defer b.writeMu.Unlock()

	prev := b.current.Load()
	next := &Snapshot{
		obs:      make(map[resolve.Key][]resolve.Observation),
		resolved: make(map[resolve.Key]resolve.Resolved),
	}
	if prev != nil {
		for k, v := range prev.obs {
			next.obs[k] = v
		}
		for k, v := range prev.resolved {
			next.resolved[k] = v
		}
		next.rate, next.at = prev.rate, prev.at
	}
	if o.At.After(next.at) {
		next.at = o.At
	}
	held := next.obs[o.Key]
	keyObs := make([]resolve.Observation, len(held), len(held)+1)
	copy(keyObs, held)
	next.obs[o.Key] = append(keyObs, owned(o))
	if r, ok := resolve.Resolve(next.obs[o.Key], next.at, next.rate, b.policy); ok {
		next.resolved[o.Key] = r
	} else {
		delete(next.resolved, o.Key)
	}
	b.current.Store(next)
	return nil
}

// SetRate re-resolves every key from the held observations at rate and
// publishes one snapshot carrying it. No port is called.
func (b *Book) SetRate(rate resolve.Rate) {
	b.writeMu.Lock()
	defer b.writeMu.Unlock()
	prev := b.current.Load()
	if prev == nil {
		b.current.Store(b.build(nil, rate, time.Time{}))
		return
	}
	b.current.Store(b.build(prev.obs, rate, prev.at))
}

func (b *Book) build(obs map[resolve.Key][]resolve.Observation, rate resolve.Rate, at time.Time) *Snapshot {
	s := &Snapshot{
		obs:      make(map[resolve.Key][]resolve.Observation, len(obs)),
		resolved: make(map[resolve.Key]resolve.Resolved, len(obs)),
		rate:     rate,
		at:       at,
	}
	for k, v := range obs {
		s.obs[k] = v
		if r, ok := resolve.Resolve(v, at, rate, b.policy); ok {
			s.resolved[k] = r
		}
	}
	if rate.Chaos > 0 {
		divine := resolve.CurrencyKey(b.scope.ID(), "divine")
		s.resolved[divine] = resolve.Resolved{Key: divine, Source: resolve.Source(rate.Source), Chaos: rate.Chaos, ObservedAt: rate.At}
	}
	return s
}

// owned copies the observation's slices and pointers so the book never shares
// memory with a port or caller.
func owned(o resolve.Observation) resolve.Observation {
	o.Prices = append([]resolve.Money(nil), o.Prices...)
	if o.SampleCount != nil {
		n := *o.SampleCount
		o.SampleCount = &n
	}
	return o
}

// detached returns r with its own Reasons backing array.
func detached(r resolve.Resolved, ok bool) (resolve.Resolved, bool) {
	r.Reasons = append([]resolve.Reason(nil), r.Reasons...)
	return r, ok
}
