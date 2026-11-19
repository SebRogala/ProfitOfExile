package lab

import (
	"context"
	"errors"
	"reflect"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

var overviewScope = league.Historical("POE410-Overview")

func font(variant, mode, color string, profit float64) FontResult {
	return FontResult{Variant: variant, Mode: mode, Color: color, Profit: profit}
}

func dedication(variant, gemType, mode, color string, profit float64) DedicationResult {
	return DedicationResult{Variant: variant, GemType: gemType, Mode: mode, Color: color, Profit: profit}
}

// The grouped rows of every variant answer an unfiltered request by merging
// them in profit order before the global limit, and a filtered one from its
// own variant's partition only.
func TestFontModes_SlicesThePartitionsAsThePerModeQueriesDid(t *testing.T) {
	rows := []FontResult{
		font("20/20", "safe", "RED", 90), font("1", "safe", "BLUE", 80), font("20/20", "safe", "GREEN", 70),
		font("1", "safe", "RED", 95), font("20", "premium", "RED", 60), font("20/20", "jackpot", "BLUE", 50),
		font("20/20", "weird", "BLUE", 999),
	}
	src := &fakeOverview{font: rows}

	got, err := FontModes(context.Background(), src, NewCache(overviewScope), overviewScope, "", 3)
	if err != nil {
		t.Fatalf("FontModes: %v", err)
	}
	want := FontAnalysis{
		Safe:    []FontResult{rows[3], rows[0], rows[1]},
		Premium: []FontResult{rows[4]},
		Jackpot: []FontResult{rows[5]},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("unfiltered limit 3 = %+v, want %+v", got, want)
	}

	got, err = FontModes(context.Background(), src, NewCache(overviewScope), overviewScope, "20/20", 1)
	if err != nil {
		t.Fatalf("FontModes: %v", err)
	}
	want = FontAnalysis{Safe: []FontResult{rows[0]}, Jackpot: []FontResult{rows[5]}}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("20/20 limit 1 = %+v, want %+v", got, want)
	}
}

// A cold Dedication pool is the limit most profitable rows of its variant
// across every mode, as one per-pool query with no mode filter answered.
func TestDedicationPools_KeepsTheTopAcrossModesPerPool(t *testing.T) {
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

// Overlapping cold requests of any variant and limit share one read; a
// request after it completed reads again.
func TestFontModes_OverlappingColdRequestsShareOneRead(t *testing.T) {
	src := &fakeOverview{font: []FontResult{font("20/20", "safe", "RED", 9), font("1", "safe", "RED", 8)}, release: make(chan struct{})}
	cache := NewCache(overviewScope)
	var wg sync.WaitGroup
	results := make([]FontAnalysis, 8)
	for i := range results {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			variant := []string{"", "20/20", "1", "20"}[i%4]
			results[i], _ = FontModes(context.Background(), src, cache, overviewScope, variant, 1+i%2)
		}(i)
	}
	waitFor(t, func() bool { return cache.FontColdFills().waiting() == len(results) })
	close(src.release)
	wg.Wait()

	if n := src.reads.Load(); n != 1 {
		t.Errorf("8 overlapping cold requests made %d reads, want 1", n)
	}
	if got := results[1].Safe; len(got) != 1 || got[0].Variant != "20/20" {
		t.Errorf("20/20 request served %+v, want its own variant's row", got)
	}
	if _, err := FontModes(context.Background(), src, cache, overviewScope, "", 1); err != nil || src.reads.Load() != 2 {
		t.Errorf("a request after the load completed made %d reads in total (err %v), want a second read", src.reads.Load(), err)
	}
}

// A caller that gives up leaves the shared read to the others; the last one
// leaving cancels it.
func TestFontModes_LastWaiterLeavingCancelsTheRead(t *testing.T) {
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
