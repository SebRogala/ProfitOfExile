package league

import (
	"context"
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

const (
	testEvery        = 50 * time.Millisecond
	testCheckTimeout = 200 * time.Millisecond
	// testSlack absorbs scheduler jitter under -race on top of the bound.
	testSlack = time.Second
)

func waitDone(t *testing.T, ctx context.Context, bound time.Duration) {
	t.Helper()
	select {
	case <-ctx.Done():
	case <-time.After(bound):
		t.Fatalf("context not Done within %v", bound)
	}
}

func TestSuperviseLiveness_FailingCheckCancelsWithErrFenceLost(t *testing.T) {
	sentinel := errors.New("connection reset")
	ctx := superviseLiveness(context.Background(), testEvery, testCheckTimeout,
		func(context.Context) error { return sentinel })

	waitDone(t, ctx, testEvery+testCheckTimeout+testSlack)
	cause := context.Cause(ctx)
	if !errors.Is(cause, ErrFenceLost) {
		t.Fatalf("cause = %v, want ErrFenceLost", cause)
	}
	if !errors.Is(cause, sentinel) {
		t.Fatalf("cause = %v, want it to wrap the check error", cause)
	}
}

func TestSuperviseLiveness_HungCheckCountsAsLossAfterTimeout(t *testing.T) {
	ctx := superviseLiveness(context.Background(), testEvery, testCheckTimeout,
		func(c context.Context) error { <-c.Done(); return c.Err() })

	waitDone(t, ctx, testEvery+testCheckTimeout+testSlack)
	cause := context.Cause(ctx)
	if !errors.Is(cause, ErrFenceLost) || !errors.Is(cause, context.DeadlineExceeded) {
		t.Fatalf("cause = %v, want ErrFenceLost wrapping context.DeadlineExceeded", cause)
	}
}

func TestSuperviseLiveness_ParentCancelIsNotReportedAsLoss(t *testing.T) {
	parent, cancel := context.WithCancel(context.Background())
	ctx := superviseLiveness(parent, testEvery, testCheckTimeout,
		func(context.Context) error { return nil })

	cancel()
	waitDone(t, ctx, testSlack)
	cause := context.Cause(ctx)
	if !errors.Is(cause, context.Canceled) || errors.Is(cause, ErrFenceLost) {
		t.Fatalf("cause = %v, want context.Canceled without ErrFenceLost", cause)
	}
}

func TestSuperviseLiveness_PassingChecksKeepContextLive(t *testing.T) {
	parent, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	var calls atomic.Int32
	ctx := superviseLiveness(parent, testEvery, testCheckTimeout,
		func(context.Context) error { calls.Add(1); return nil })

	deadline := time.After(3*testEvery + testSlack)
	for calls.Load() < 3 {
		select {
		case <-deadline:
			t.Fatalf("only %d checks within bound", calls.Load())
		case <-time.After(testEvery / 5):
		}
	}
	if err := ctx.Err(); err != nil {
		t.Fatalf("ctx.Err() after %d passing checks = %v, want nil (cause %v)", calls.Load(), err, context.Cause(ctx))
	}
}

// busyLock returns a lock whose connection slot is held by a competitor, the
// stand-in for a health CheckHeld parked mid-query on a half-open connection.
// It has no connection: nothing under test may get past the slot wait.
func busyLock() *ProcessLock {
	l := &ProcessLock{key: 1, sem: make(chan struct{}, 1)}
	l.sem <- struct{}{}
	return l
}

func TestSupervise_StalledCompetingCheckSignalsLossWithinCheckTimeout(t *testing.T) {
	l := busyLock()
	ctx := l.Supervise(context.Background(), testEvery, testCheckTimeout)

	waitDone(t, ctx, testEvery+testCheckTimeout+testSlack)
	cause := context.Cause(ctx)
	if !errors.Is(cause, ErrFenceLost) || !errors.Is(cause, context.DeadlineExceeded) {
		t.Fatalf("cause = %v, want ErrFenceLost wrapping context.DeadlineExceeded", cause)
	}
	if len(l.sem) != 1 {
		t.Fatalf("competitor's slot was taken from it: len(sem) = %d, want 1", len(l.sem))
	}
}

func TestCheckHeld_WaitForBusyConnectionHonoursContext(t *testing.T) {
	l := busyLock()
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	errc := make(chan error, 1)
	go func() { errc <- l.CheckHeld(ctx) }()
	select {
	case err := <-errc:
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("CheckHeld = %v, want error wrapping context.DeadlineExceeded", err)
		}
	case <-time.After(testSlack):
		t.Fatal("CheckHeld did not return within 1s while the connection was busy")
	}
}
