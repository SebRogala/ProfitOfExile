package book

import (
	"container/heap"
	"fmt"
	"math"
	"strconv"
	"strings"
	"sync"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

// Demand describes a current snapshot market and its potential Font exposure.
type Demand struct {
	Key                   resolve.Key
	SnapshotTime          time.Time
	Ninja                 resolve.Observation
	PotentialContribution float64
	PickProbability       float64
}

// Scheduler owns demand, successful checks and deferred attempts for one league.
// Callers admit network work before Take and finish it with Success or Retry.
type Scheduler struct {
	mu      sync.Mutex
	scope   league.Scope
	policy  resolve.Policy
	minAge  time.Duration
	demand  map[resolve.Key]Demand
	checks  map[resolve.Key]resolve.Observation
	retry   map[resolve.Key]time.Time
	pending map[resolve.Key]bool
	queue   demandHeap
	force   []resolve.Key
}

// NewScheduler receives the caller's shared revisit policy and opportunistic age.
func NewScheduler(scope league.Scope, policy resolve.Policy, minAge time.Duration) (*Scheduler, error) {
	if err := scope.Validate(); err != nil {
		return nil, err
	}
	if policy.MaxRevisitAge <= 0 || policy.ListingCount <= 0 || minAge < 0 {
		return nil, fmt.Errorf("scheduler: invalid revisit policy")
	}
	return &Scheduler{scope: scope, policy: policy, minAge: minAge,
		demand: make(map[resolve.Key]Demand), checks: make(map[resolve.Key]resolve.Observation),
		retry: make(map[resolve.Key]time.Time), pending: make(map[resolve.Key]bool)}, nil
}

func (s *Scheduler) validateKey(k resolve.Key) error {
	if err := s.scope.Admit(k.League); err != nil {
		return err
	}
	level, quality, hasQuality := strings.Cut(k.Variant, "/")
	l, le := strconv.Atoi(level)
	q, qe := strconv.Atoi(quality)
	if k.Kind != resolve.KindGem || strings.TrimSpace(k.Name) == "" || le != nil || l <= 0 ||
		(hasQuality && (qe != nil || q <= 0)) || resolve.GemKey(k.League, k.Name, k.Variant) != (resolve.Key{
		League: k.League, Kind: k.Kind, Name: k.Name, Variant: k.Variant}) {
		return fmt.Errorf("scheduler: invalid canonical gem key %q", k.String())
	}
	return nil
}

func finiteNonnegative(v float64) bool { return v >= 0 && !math.IsInf(v, 0) && !math.IsNaN(v) }

func (s *Scheduler) validateCheck(o resolve.Observation) error {
	if err := s.validateKey(o.Key); err != nil {
		return err
	}
	if o.Source != resolve.SourceTrade || o.At.IsZero() {
		return fmt.Errorf("scheduler: invalid successful check")
	}
	return nil
}

// Reconcile replaces the complete demand set atomically and merges canonical
// successful observations. Older durable rows cannot undo an immediate success.
func (s *Scheduler) Reconcile(demand []Demand, checks []resolve.Observation) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	next := make(map[resolve.Key]Demand, len(demand))
	for _, d := range demand {
		if err := s.validateKey(d.Key); err != nil {
			return err
		}
		if d.SnapshotTime.IsZero() || d.Ninja.Key != d.Key || d.Ninja.Source != resolve.SourceNinja ||
			!d.Ninja.At.Equal(d.SnapshotTime) || !finiteNonnegative(d.PotentialContribution) ||
			!finiteNonnegative(d.PickProbability) || d.PickProbability > 1 {
			return fmt.Errorf("scheduler: invalid demand for %q", d.Key.String())
		}
		if _, exists := next[d.Key]; exists {
			return fmt.Errorf("scheduler: duplicate demand key %q", d.Key.String())
		}
		d.Ninja = owned(d.Ninja)
		next[d.Key] = d
	}
	for _, o := range checks {
		if err := s.validateCheck(o); err != nil {
			return err
		}
	}
	s.demand = next
	for _, o := range checks {
		s.mergeCheck(o)
	}
	for k := range s.retry {
		if _, ok := next[k]; !ok {
			delete(s.retry, k)
		}
	}
	for k := range s.pending {
		if _, ok := next[k]; !ok {
			delete(s.pending, k)
		}
	}
	force := s.force[:0]
	for _, k := range s.force {
		if _, ok := next[k]; ok {
			force = append(force, k)
		}
	}
	s.force = force
	s.queue = nil
	return nil
}

func (s *Scheduler) mergeCheck(o resolve.Observation) {
	if held, ok := s.checks[o.Key]; !ok || o.At.After(held.At) {
		s.checks[o.Key] = owned(o)
	}
}

// Success records even an empty successful lookup and releases its in-flight key.
func (s *Scheduler) Success(o resolve.Observation) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.validateCheck(o); err != nil {
		return err
	}
	s.mergeCheck(o)
	delete(s.pending, o.Key)
	delete(s.retry, o.Key)
	return nil
}

// Retry releases a failed attempt without advancing its last successful check.
// The execution adapter owns the delay and error classification.
func (s *Scheduler) Retry(k resolve.Key, until time.Time) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.validateKey(k); err != nil {
		return err
	}
	if _, ok := s.demand[k]; !ok || until.IsZero() {
		return fmt.Errorf("scheduler: invalid retry")
	}
	if until.After(s.retry[k]) {
		s.retry[k] = until
	}
	delete(s.pending, k)
	return nil
}

// Force queues a demanded key ahead of ordinary work. It has no production
// caller and does not perform admission or bypass retry eligibility.
func (s *Scheduler) Force(k resolve.Key) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.validateKey(k); err != nil {
		return err
	}
	if _, ok := s.demand[k]; !ok {
		return fmt.Errorf("scheduler: force key is not demanded")
	}
	for _, held := range s.force {
		if held == k {
			return nil
		}
	}
	s.force = append(s.force, k)
	return nil
}

// Take re-evaluates every current candidate at now and the current divine rate.
// A taken key stays in flight until Success or Retry; demand itself is retained.
func (s *Scheduler) Take(now time.Time, rate resolve.Rate) (Demand, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for i, k := range s.force {
		if s.pending[k] || now.Before(s.retry[k]) {
			continue
		}
		s.force = append(s.force[:i], s.force[i+1:]...)
		s.pending[k] = true
		d := s.demand[k]
		d.Ninja = owned(d.Ninja)
		return d, true
	}
	s.queue = s.queue[:0]
	for k, d := range s.demand {
		if s.pending[k] || now.Before(s.retry[k]) {
			continue
		}
		o, checked := s.checks[k]
		age := max(time.Duration(0), now.Sub(o.At))
		item := demandPriority{key: k, exposure: d.PotentialContribution, checkedAt: o.At}
		switch {
		case !checked:
			item.class = 0
		case !now.Before(o.At.Add(s.policy.MaxRevisitAge)):
			item.class = 1
			item.deadline = o.At.Add(s.policy.MaxRevisitAge)
		default:
			if age < s.minAge {
				continue
			}
			item.class = 2
			n, nok := s.sourcePrice(d.Ninja, rate)
			t, tok := s.sourcePrice(o, rate)
			v, disagreement := max(n, t), 0.0
			if nok && tok {
				disagreement = math.Abs(n - t)
			}
			exposure := v + disagreement
			if math.IsInf(exposure, 0) {
				exposure = math.MaxFloat64
			}
			item.risk = exposure * d.PickProbability * (1 + float64(age)/float64(s.policy.MaxRevisitAge))
			if math.IsInf(item.risk, 0) {
				item.risk = math.MaxFloat64
			}
		}
		s.queue = append(s.queue, item)
	}
	heap.Init(&s.queue)
	if len(s.queue) == 0 {
		return Demand{}, false
	}
	k := heap.Pop(&s.queue).(demandPriority).key
	s.pending[k] = true
	d := s.demand[k]
	d.Ninja = owned(d.Ninja)
	return d, true
}

func (s *Scheduler) sourcePrice(o resolve.Observation, rate resolve.Rate) (float64, bool) {
	prices := make([]resolve.Money, 0, len(o.Prices))
	for _, p := range o.Prices {
		if finiteNonnegative(p.Amount) && p.Amount > 0 {
			prices = append(prices, p)
		}
	}
	divine := rate.Chaos
	if !finiteNonnegative(divine) {
		divine = 0
	}
	p, ok := resolve.MedianCheapest(prices, s.policy.ListingCount, divine)
	if !ok || !finiteNonnegative(p) {
		return 0, false
	}
	return p, true
}

// SchedulerStats describes current demand, including deferred/in-flight work.
type SchedulerStats struct {
	Demand       int
	NeverChecked int
	Overdue      int
	OldestAge    time.Duration
}

func (s *Scheduler) Stats(now time.Time) SchedulerStats {
	s.mu.Lock()
	defer s.mu.Unlock()
	stats := SchedulerStats{Demand: len(s.demand)}
	for k := range s.demand {
		o, ok := s.checks[k]
		if !ok {
			stats.NeverChecked++
			continue
		}
		age := max(time.Duration(0), now.Sub(o.At))
		stats.OldestAge = max(stats.OldestAge, age)
		if age > s.policy.MaxRevisitAge {
			stats.Overdue++
		}
	}
	return stats
}

type demandPriority struct {
	key                 resolve.Key
	class               int
	exposure, risk      float64
	checkedAt, deadline time.Time
}
type demandHeap []demandPriority

func (h demandHeap) Len() int { return len(h) }
func (h demandHeap) Less(i, j int) bool {
	a, b := h[i], h[j]
	if a.class != b.class {
		return a.class < b.class
	}
	switch a.class {
	case 0:
		if a.exposure != b.exposure {
			return a.exposure > b.exposure
		}
	case 1:
		if !a.deadline.Equal(b.deadline) {
			return a.deadline.Before(b.deadline)
		}
		if a.exposure != b.exposure {
			return a.exposure > b.exposure
		}
	case 2:
		if a.risk != b.risk {
			return a.risk > b.risk
		}
		if !a.checkedAt.Equal(b.checkedAt) {
			return a.checkedAt.Before(b.checkedAt)
		}
	}
	return a.key.String() < b.key.String()
}
func (h demandHeap) Swap(i, j int) { h[i], h[j] = h[j], h[i] }
func (h *demandHeap) Push(v any)   { *h = append(*h, v.(demandPriority)) }
func (h *demandHeap) Pop() any     { old := *h; v := old[len(old)-1]; *h = old[:len(old)-1]; return v }
