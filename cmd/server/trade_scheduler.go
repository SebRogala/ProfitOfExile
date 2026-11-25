package main

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/price/book"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/trade"
)

type tradeDemandReader interface {
	LatestGemPrices(context.Context, league.Scope) ([]lab.GemPrice, time.Time, error)
}
type tradeCheckReader interface {
	LookupsAsOf(context.Context, league.Scope, time.Time) ([]trade.TradeLookupResult, error)
}

// evidenceHolds attaches sold-through evidence holds to a snapshot's gems.
type evidenceHolds func(context.Context, league.Scope, []lab.GemPrice, time.Time) ([]lab.GemPrice, error)

type tradeScheduler struct {
	scope      league.Scope
	queue      *book.Scheduler
	background *trade.BackgroundRefresh
	fixed      *trade.FixedTargetScheduler
	gems       tradeDemandReader
	checks     tradeCheckReader
	holds      evidenceHolds
	heldStamp  time.Time
	heldGems   []lab.GemPrice
	hasHeld    bool
	rate       func() resolve.Rate
	now        func() time.Time
}

func newTradeScheduler(scope league.Scope, gate *trade.Gate, fixed *trade.FixedTargetScheduler, gems tradeDemandReader, checks tradeCheckReader, holds evidenceHolds, rate func() resolve.Rate) *tradeScheduler {
	queue, err := book.NewScheduler(scope, lab.GemPricePolicy(), 5*time.Minute)
	if err != nil {
		panic(err)
	}
	return &tradeScheduler{scope: scope, queue: queue, background: trade.NewBackgroundRefresh(gate), fixed: fixed, gems: gems, checks: checks, holds: holds, rate: rate, now: time.Now}
}

func (s *tradeScheduler) reconcile(ctx context.Context) error {
	readCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	gems, stamp, err := s.gems.LatestGemPrices(readCtx, s.scope)
	if err != nil {
		return err
	}
	if !s.hasHeld || !s.heldStamp.Equal(stamp) {
		held, err := s.holds(readCtx, s.scope, gems, stamp)
		if err != nil {
			return fmt.Errorf("trade scheduler: evidence holds: %w", err)
		}
		if err := readCtx.Err(); err != nil {
			return fmt.Errorf("trade scheduler: evidence holds: %w", err)
		}
		s.heldGems, s.heldStamp, s.hasHeld = held, stamp, true
	}
	gems = s.heldGems
	rows, err := s.checks.LookupsAsOf(readCtx, s.scope, s.now())
	if err != nil {
		return err
	}
	demand := lab.BuildTradeDemand(s.scope, stamp, gems)
	keys := make(map[resolve.Key]bool, len(demand))
	for _, d := range demand {
		keys[d.Key] = true
	}
	checks := make([]resolve.Observation, 0, len(rows))
	for _, o := range trade.Observations(s.scope.ID(), rows) {
		if keys[o.Key] {
			checks = append(checks, o)
		}
	}
	return s.queue.Reconcile(demand, checks)
}

func (s *tradeScheduler) Seed(ctx context.Context) {
	outcome := s.background.Run(ctx, func(ctx context.Context) (*trade.GateRequest, error) { return nil, s.reconcile(ctx) }, nil)
	s.log(outcome)
}

func (s *tradeScheduler) Tick(ctx context.Context) {
	outcome := s.background.Run(ctx, func(ctx context.Context) (*trade.GateRequest, error) {
		if err := s.reconcile(ctx); err != nil {
			return nil, err
		}
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if s.fixed != nil {
			if target, ok := s.fixed.Claim(s.now()); ok {
				return &trade.GateRequest{Gem: target.Gem, Variant: target.Variant, FixedTarget: true, RequestID: "refresh-" + target.Gem}, nil
			}
		}
		d, ok := s.queue.Take(s.now(), s.rate())
		if !ok {
			return nil, nil
		}
		return &trade.GateRequest{Gem: d.Key.Name, Variant: d.Key.Variant, RequestID: "refresh-" + d.Key.Name}, nil
	}, func(req *trade.GateRequest, resp *trade.GateResponse, until time.Time) {
		if req.FixedTarget {
			return
		}
		var err error
		if resp.Error != nil {
			err = s.queue.Retry(resolve.GemKey(s.scope.ID(), req.Gem, req.Variant), until)
		} else {
			for _, o := range trade.Observations(s.scope.ID(), []trade.TradeLookupResult{*resp.Data}) {
				err = s.queue.Success(o)
			}
		}
		if err != nil {
			slog.Error("trade scheduler: completion failed", "error", err)
		}
	})
	s.log(outcome)
}

func (s *tradeScheduler) log(outcome string) {
	stats := s.queue.Stats(s.now())
	slog.Info("trade scheduler tick", "league", s.scope.ID(), "outcome", outcome, "demand", stats.Demand,
		"never_checked", stats.NeverChecked, "overdue", stats.Overdue, "oldest_age", stats.OldestAge)
}
