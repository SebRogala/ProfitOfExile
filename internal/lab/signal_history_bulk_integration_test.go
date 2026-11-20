//go:build integration

package lab

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// overlappingHistorySource holds each bulk read until joiners requests wait on
// the bulk-history flight, so a burst overlaps the read whatever its speed.
type overlappingHistorySource struct {
	*Repository
	joiners int
}

func (s overlappingHistorySource) SignalHistoryBulk(ctx context.Context, scope league.Scope, keys []SignalHistoryKey, limit int) (map[SignalHistoryKey][]SignalChange, error) {
	for deadline := time.Now().Add(5 * time.Second); signalHistoryBulkFlights.waiting() < s.joiners && time.Now().Before(deadline); {
		time.Sleep(time.Millisecond)
	}
	return s.Repository.SignalHistoryBulk(ctx, scope, keys, limit)
}

// seedSignalHistory inserts one gem_signals row per age, timed against the
// database clock the history predicate compares with, and a gem_features row
// for each when withFeatures is set. base distinguishes series' values.
func seedSignalHistory(t *testing.T, pool *pgxpool.Pool, scope league.Scope, name, variant string, base float64, withFeatures bool, ages ...time.Duration) {
	t.Helper()
	signals := []string{"STABLE", "RISING", "FALLING", "HERD"}
	for i, age := range ages {
		if _, err := pool.Exec(context.Background(), `
			INSERT INTO gem_signals (league, time, name, variant, signal, sellability, window_signal, advanced_signal)
			VALUES ($1, date_trunc('second', NOW()) - make_interval(secs => $2), $3, $4, $5, 50, $6, NULLIF($7, ''))`,
			scope.ID(), age.Seconds(), name, variant, signals[i%len(signals)], "WINDOW", map[bool]string{true: "ADV"}[i%2 == 0]); err != nil {
			t.Fatalf("seed signal %s %s: %v", name, variant, err)
		}
		if !withFeatures {
			continue
		}
		if _, err := pool.Exec(context.Background(), `
			INSERT INTO gem_features (league, time, name, variant, chaos, listings, vel_long_price, vel_long_listing)
			VALUES ($1, date_trunc('second', NOW()) - make_interval(secs => $2), $3, $4, $5, $6, $7, $8)`,
			scope.ID(), age.Seconds(), name, variant, base+float64(i), 10+i, base/10+float64(i), -float64(i)); err != nil {
			t.Fatalf("seed feature %s %s: %v", name, variant, err)
		}
	}
}

func hourAges(n int) []time.Duration {
	ages := make([]time.Duration, n)
	for i := range ages {
		ages[i] = time.Duration(i+1) * time.Hour
	}
	return ages
}

// signalHistoryBulkFixture seeds twelve keys: deep series past the clamp, a
// short one, one without features, one gem in two variants with distinct
// rows, rows beyond the 14-day window and a requested key with no rows.
func signalHistoryBulkFixture(t *testing.T, pool *pgxpool.Pool, scope league.Scope) []SignalHistoryKey {
	t.Helper()
	const day = 24 * time.Hour
	keys := make([]SignalHistoryKey, 0, 12)
	for i := 0; i < 6; i++ {
		k := SignalHistoryKey{Name: fmt.Sprintf("History Gem %02d", i), Variant: []string{"20/20", "1/20", "20", "1"}[i%4]}
		seedSignalHistory(t, pool, scope, k.Name, k.Variant, float64(100*(i+1)), true, hourAges(25)...)
		keys = append(keys, k)
	}
	seedSignalHistory(t, pool, scope, "History Short", "20/20", 7, true, time.Hour, 2*time.Hour)
	seedSignalHistory(t, pool, scope, "History Bare", "20/20", 0, false, time.Hour, 3*time.Hour, 5*time.Hour)
	seedSignalHistory(t, pool, scope, "History Split", "20/20", 900, true, time.Hour, 4*time.Hour, 6*time.Hour)
	seedSignalHistory(t, pool, scope, "History Split", "1/20", 300, true, 2*time.Hour, 3*time.Hour)
	seedSignalHistory(t, pool, scope, "History Aged", "20/20", 50, true, 2*time.Hour, 15*day, 20*day)
	seedSignalHistory(t, pool, scope, "History Unrequested", "20/20", 1, true, time.Hour, 2*time.Hour)
	keys = append(keys,
		SignalHistoryKey{"History Short", "20/20"},
		SignalHistoryKey{"History Bare", "20/20"},
		SignalHistoryKey{"History Split", "20/20"},
		SignalHistoryKey{"History Split", "1/20"},
		SignalHistoryKey{"History Aged", "20/20"},
		SignalHistoryKey{"History Missing", "20/20"},
	)
	return keys
}

// referenceSignalHistory is the single route's read the bulk read replaces:
// one SignalHistory query per key, keys without rows absent.
func referenceSignalHistory(t *testing.T, repo *Repository, scope league.Scope, keys []SignalHistoryKey, limit int) map[SignalHistoryKey][]SignalChange {
	t.Helper()
	out := make(map[SignalHistoryKey][]SignalChange)
	for _, k := range keys {
		changes, err := repo.SignalHistory(context.Background(), scope, k.Name, k.Variant, limit)
		if err != nil {
			t.Fatalf("reference SignalHistory %v: %v", k, err)
		}
		if len(changes) > 0 {
			out[k] = changes
		}
	}
	return out
}

// TestKeepHappySignalHistoryBulk: a cold bulk-history request is one bounded
// statement for all its keys, 300 overlapping equal requests share it, a warm
// analyzer cache runs none, and every key's history equals the single route.
func TestKeepHappySignalHistoryBulk(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-HistoryBulk")
	keys := signalHistoryBulkFixture(t, pool, scope)
	plain := NewRepository(pool)
	counted, counter := countingRepository(t, pool)

	for _, limit := range []int{1, 4, 20} {
		want := referenceSignalHistory(t, plain, scope, keys, limit)
		counter.reset()
		got, err := LoadSignalHistoryBulk(context.Background(), counted, NewCache(scope), scope, keys, limit)
		statements := counter.reset()
		if err != nil {
			t.Fatalf("limit %d: LoadSignalHistoryBulk: %v", limit, err)
		}
		t.Run(fmt.Sprintf("limit %d runs one statement", limit), func(t *testing.T) {
			if statements != 1 {
				t.Errorf("cold 12-key request ran %d statements, want 1", statements)
			}
		})
		t.Run(fmt.Sprintf("limit %d equals the single route per key", limit), func(t *testing.T) {
			if !reflect.DeepEqual(got, want) {
				t.Errorf("bulk history = %v, want the single-route reads %v", got, want)
			}
		})
	}

	want := referenceSignalHistory(t, plain, scope, keys, 4)
	t.Run("fixture spans clamp, window and variants", func(t *testing.T) {
		if n := len(referenceSignalHistory(t, plain, scope, keys[:1], 20)[keys[0]]); n != 20 {
			t.Errorf("deep series serves %d rows at limit 20, want 20", n)
		}
		if n := len(want[SignalHistoryKey{"History Aged", "20/20"}]); n != 1 {
			t.Errorf("aged series serves %d rows, want only its in-window row", n)
		}
		if _, ok := want[SignalHistoryKey{"History Missing", "20/20"}]; ok {
			t.Error("a key without rows has history")
		}
	})
	t.Run("no cross-variant leakage", func(t *testing.T) {
		got, err := LoadSignalHistoryBulk(context.Background(), plain, NewCache(scope), scope, keys, 4)
		if err != nil {
			t.Fatalf("LoadSignalHistoryBulk: %v", err)
		}
		a, b := got[SignalHistoryKey{"History Split", "20/20"}], got[SignalHistoryKey{"History Split", "1/20"}]
		if len(a) != 3 || len(b) != 2 {
			t.Fatalf("split series = %d + %d rows, want 3 + 2", len(a), len(b))
		}
		for _, c := range b {
			if c.Price < 300 || c.Price >= 900 {
				t.Errorf("1/20 series carries price %v from another variant", c.Price)
			}
		}
	})

	cold := NewCache(scope)
	counter.reset()
	outcomes := burst(300, func() error {
		got, err := LoadSignalHistoryBulk(context.Background(), overlappingHistorySource{counted, 300}, cold, scope, keys, 4)
		if err == nil && !reflect.DeepEqual(got, want) {
			return fmt.Errorf("history = %v, want the single-route reads", got)
		}
		return err
	})
	statements := counter.reset()
	t.Run("overlapping burst shares one statement", func(t *testing.T) {
		if statements != 1 {
			t.Errorf("300 overlapping cold requests ran %d statements, want 1", statements)
		}
	})
	t.Run("overlapping burst bodies equal the single route", func(t *testing.T) {
		for i, err := range outcomes {
			if err != nil {
				t.Fatalf("burst request %d: %v", i, err)
			}
		}
	})
	t.Run("bulk read leaves the corpus cold", func(t *testing.T) {
		if cold.For(scope).HasSignalHistory() {
			t.Error("a bulk read marked the signal-history corpus warm")
		}
	})

	t.Run("warm cache runs no statement and keeps ring depth", func(t *testing.T) {
		ring := make([]SignalChange, 6)
		for i := range ring {
			ring[i] = SignalChange{Time: time.Unix(int64(1000-i), 0).UTC(), Signal: "RISING", Price: float64(i)}
		}
		warm := NewCache(scope)
		warm.For(scope).SetSignalHistoryByName(map[string]map[string][]SignalChange{"History Gem 00": {"20/20": ring}})
		counter.reset()
		got, err := LoadSignalHistoryBulk(context.Background(), counted, warm, scope, keys, 4)
		if err != nil {
			t.Fatalf("LoadSignalHistoryBulk: %v", err)
		}
		if n := counter.reset(); n != 0 {
			t.Errorf("warm request ran %d statements, want 0", n)
		}
		wantWarm := map[SignalHistoryKey][]SignalChange{{"History Gem 00", "20/20"}: ring[:4]}
		if !reflect.DeepEqual(got, wantWarm) {
			t.Errorf("warm history = %v, want the ring truncated to the limit %v", got, wantWarm)
		}
	})
}

// TestKeepCriticalSignalHistoryBulkWindow: nothing outlives a flight, so a
// later cold request reads again and drops a row that has aged past the
// 14-day window meanwhile.
func TestKeepCriticalSignalHistoryBulkWindow(t *testing.T) {
	pool := labIntegrationPool(t)
	scope := collectiveLeague(t, pool, "POE410-HistoryWindow")
	const edge = 3 * time.Second
	seedSignalHistory(t, pool, scope, "Window History", "20/20", 10, true, time.Duration(signalHistorySeedMaxDays)*24*time.Hour-edge, time.Hour)
	seeded := time.Now()
	key := SignalHistoryKey{"Window History", "20/20"}
	counted, counter := countingRepository(t, pool)
	cache := NewCache(scope)

	first, err := LoadSignalHistoryBulk(context.Background(), counted, cache, scope, []SignalHistoryKey{key}, 4)
	if err != nil || len(first[key]) != 2 {
		t.Fatalf("first request = %d rows (err %v), want 2 (edge row still inside the window)", len(first[key]), err)
	}
	time.Sleep(time.Until(seeded.Add(edge + 1500*time.Millisecond)))
	counter.reset()
	got, err := LoadSignalHistoryBulk(context.Background(), counted, cache, scope, []SignalHistoryKey{key}, 4)
	statements := counter.reset()
	if err != nil {
		t.Fatalf("later request: %v", err)
	}
	t.Run("later request reads again", func(t *testing.T) {
		if statements != 1 {
			t.Errorf("later cold request ran %d statements, want 1", statements)
		}
	})
	t.Run("aged row is dropped", func(t *testing.T) {
		if len(got[key]) != 1 {
			t.Errorf("later request = %+v, want only the 1h row", got[key])
		}
	})
}
