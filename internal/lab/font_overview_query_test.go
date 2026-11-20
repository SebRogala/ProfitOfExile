package lab

import (
	"context"
	"errors"
	"reflect"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

var overviewScope = league.Historical("POE410-Overview")

func dedication(variant, gemType, mode, color string, profit float64) DedicationResult {
	return DedicationResult{Variant: variant, GemType: gemType, Mode: mode, Color: color, Profit: profit}
}

// A cold Dedication pool is the limit most profitable rows of its variant
// across every mode, as one per-pool query with no mode filter answered.
func TestKeepHappyDedicationPools_KeepsTheTopAcrossModesPerPool(t *testing.T) {
	rows := []DedicationResult{
		dedication("21/23c", "skill", "safe", "RED", 10), dedication("21/23c", "skill", "jackpot", "RED", 40),
		dedication("21/23c", "skill", "premium", "BLUE", 30), dedication("21/20c", "skill", "safe", "RED", 99),
		dedication("21/23c", "transfigured", "safe", "GREEN", 5),
	}
	got, err := DedicationPools(context.Background(), &fakeOverview{dedication: rows}, NewCache(overviewScope), overviewScope, "21/23c", 2)
	if err != nil {
		t.Fatalf("DedicationPools: %v", err)
	}
	want := DedicationAnalysis{
		Skills:       []DedicationResult{rows[1], rows[2]},
		Transfigured: []DedicationResult{rows[4]},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("21/23c limit 2 = %+v, want %+v", got, want)
	}
}

// fakeOverview serves fixed grouped rows, counting reads and holding each one
// until release closes (when set).
type fakeOverview struct {
	font       []FontResult
	dedication []DedicationResult
	err        error
	release    chan struct{}
	reads      atomic.Int32
}

func (f *fakeOverview) hold(ctx context.Context) error {
	f.reads.Add(1)
	if f.release != nil {
		select {
		case <-f.release:
		case <-ctx.Done():
			return ctx.Err()
		}
	}
	return f.err
}

func (f *fakeOverview) FontOverviewRows(ctx context.Context, _ league.Scope) ([]FontResult, error) {
	if err := f.hold(ctx); err != nil {
		return nil, err
	}
	return f.font, nil
}

func (f *fakeOverview) DedicationOverviewRows(ctx context.Context, _ league.Scope) ([]DedicationResult, error) {
	if err := f.hold(ctx); err != nil {
		return nil, err
	}
	return f.dedication, nil
}

func waitFor(t *testing.T, cond func() bool) {
	t.Helper()
	deadline := time.Now().Add(5 * time.Second)
	for !cond() {
		if time.Now().After(deadline) {
			t.Fatal("condition not reached")
		}
		time.Sleep(time.Millisecond)
	}
}

// A caller that gives up leaves the shared read to the others; the last one
// leaving cancels it.
func TestKeepHappyFontColdFill_LastWaiterLeavingCancelsTheRead(t *testing.T) {
	src := &fakeOverview{release: make(chan struct{})}
	cache := NewCache(overviewScope)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() {
		_, err := DedicationPools(ctx, src, cache, overviewScope, "21/23c", 50)
		done <- err
	}()
	waitFor(t, func() bool { return cache.FontColdFills().waiting() == 1 })
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("abandoned request err = %v, want context.Canceled", err)
	}
	waitFor(t, func() bool { return cache.FontColdFills().waiting() == 0 })

	src.dedication = []DedicationResult{dedication("21/23c", "skill", "safe", "RED", 1)}
	close(src.release)
	got, err := DedicationPools(context.Background(), src, cache, overviewScope, "21/23c", 50)
	if err != nil || len(got.Skills) != 1 {
		t.Fatalf("request after the abandoned load = %+v, %v; want a fresh read's row", got, err)
	}
}
