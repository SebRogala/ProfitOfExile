package trade

import (
	"context"
	"log/slog"
	"sync/atomic"
	"time"
)

// BackgroundRefresh admits one best-effort background operation, including its
// input reads and completion accounting. The gate remains the only HTTP worker.
type BackgroundRefresh struct {
	gate *Gate
	busy atomic.Bool
}

func NewBackgroundRefresh(gate *Gate) *BackgroundRefresh {
	if gate == nil {
		panic("trade: refresh gate is required")
	}
	return &BackgroundRefresh{gate: gate}
}

// Run sheds overlapping ticks. prepare runs only after idle/rate preflight;
// nil work permits boot reconciliation without claiming a network target.
func (b *BackgroundRefresh) Run(ctx context.Context, prepare func(context.Context) (*GateRequest, error), finish func(*GateRequest, *GateResponse, time.Time)) string {
	if ctx.Err() != nil {
		return "cancelled"
	}
	if !b.busy.CompareAndSwap(false, true) {
		return "busy"
	}
	defer b.busy.Store(false)
	g := b.gate
	g.mu.Lock()
	idle := len(g.inflight) == 0 && len(g.high) == 0 && len(g.low) == 0
	g.mu.Unlock()
	if !idle {
		return "foreground"
	}
	if g.limiter.EstimateWait("search") > 0 || g.limiter.EstimateWait("fetch") > 0 {
		return "limited"
	}
	req, err := prepare(ctx)
	if err != nil {
		slog.Warn("trade refresh: input failed", "error", err)
		return "input-error"
	}
	if req == nil {
		return "idle"
	}
	if ctx.Err() != nil {
		if finish != nil {
			finish(req, &GateResponse{Error: ctx.Err()}, g.now())
		}
		return "cancelled"
	}
	req.Priority = PriorityLow
	req.SubmittedAt = g.now()
	req.Result = make(chan *GateResponse, 1)
	g.Submit(req)
	var resp *GateResponse
	select {
	case resp = <-req.Result:
	case <-ctx.Done():
		resp = &GateResponse{Error: ctx.Err()}
	}
	until := g.now()
	if resp.Error != nil {
		delay := failedLookupRetryDelay
		if isInvalidRequestError(resp.Error) && !isGlobalRestrictionError(resp.Error) {
			delay = invalidLookupRetryDelay
		}
		until = until.Add(delay)
		if paused := g.limiter.restrictionDeadlineAfter(req.SubmittedAt); !paused.IsZero() && paused.Add(failedLookupRetryDelay).After(until) {
			until = paused.Add(failedLookupRetryDelay)
		}
	}
	if finish != nil {
		finish(req, resp, until)
	}
	if resp.Error != nil {
		return "failed"
	}
	return "success"
}

// SubmitRefresh preserves the blocking low-priority helper for explicit callers.
func SubmitRefresh(ctx context.Context, gate *Gate, gem, variant string) {
	submitLowRefresh(ctx, gate, gem, variant, false)
}

// submitLowRefresh is also used by SubmitFixedTargetRefresh.
func submitLowRefresh(ctx context.Context, gate *Gate, gem, variant string, fixed bool) {
	if gate == nil {
		panic("trade: refresh gate is required")
	}
	req := &GateRequest{Gem: gem, Variant: variant, FixedTarget: fixed,
		RequestID: "refresh-" + gem, Priority: PriorityLow, SubmittedAt: gate.now(), Result: make(chan *GateResponse, 1)}
	gate.Submit(req)
	select {
	case <-ctx.Done():
	case resp := <-req.Result:
		if resp.Error != nil {
			slog.Warn("trade refresh failed", "gem", gem, "variant", variant, "error", resp.Error)
		} else {
			slog.Info("trade refresh complete", "gem", gem, "variant", variant, "total", resp.Data.Total, "floor", resp.Data.PriceFloor)
		}
	}
}
