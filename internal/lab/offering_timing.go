package lab

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"math"
	"sort"
	"sync"
	"time"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/league"
)

// OfferingTiming holds price timing analysis for a lab offering (Gift, Dedication, etc.).
// It is served by MarketOverview and round-trips through the cache as JSON, so
// its tags and omitempty markers are the wire format.
type OfferingTiming struct {
	Name             string               `json:"name"`
	FragmentID       string               `json:"fragmentId"`
	CurrentPrice     float64              `json:"currentPrice"`
	CheapHours       []OfferingTimingHour `json:"cheapHours,omitempty"`
	ExpensiveHours   []OfferingTimingHour `json:"expensiveHours,omitempty"`
	CheapDays        []OfferingTimingDay  `json:"cheapDays,omitempty"`
	ExpensiveDays    []OfferingTimingDay  `json:"expensiveDays,omitempty"`
	HourlyMedians    []OfferingTimingHour `json:"hourlyMedians,omitempty"`
	TodayHourMedians []OfferingTimingHour `json:"todayHourMedians,omitempty"`
	Sparkline        []OfferingSparkPoint `json:"sparkline,omitempty"`
}

// OfferingSparkPoint is a single sparkline data point for offering price history.
type OfferingSparkPoint struct {
	Time  string  `json:"time"`
	Price float64 `json:"price"`
}

// offeringHourMedian holds hourly median price (internal).
type offeringHourMedian struct {
	Hour   int
	Median float64
}

// OfferingTimingHour is the JSON-serialized hour timing.
type OfferingTimingHour struct {
	Hour   int     `json:"hour"`
	Median float64 `json:"median"`
}

// OfferingTimingDay is the JSON-serialized weekday timing.
type OfferingTimingDay struct {
	Day    string  `json:"day"`
	Median float64 `json:"median"`
}

// offeringFillTimeout bounds one shared offering fill. The fill runs detached
// from the requests that wait on it.
const offeringFillTimeout = 10 * time.Second

// offeringQuerier is the part of the pool the offering fill uses.
type offeringQuerier interface {
	QueryRow(ctx context.Context, sql string, args ...any) pgx.Row
	Query(ctx context.Context, sql string, args ...any) (pgx.Rows, error)
}

type offeringLoader func(context.Context) ([]OfferingTiming, error)

// offeringFill shares one league's offering fill among the market-overview
// requests and the background refresh that overlap it. The fill is held only
// while in flight; its answer lives in the cache's offering slot.
type offeringFill struct {
	mu     sync.Mutex
	flight *offeringFlight
}

type offeringFlight struct {
	done    chan struct{}
	waiters int
	value   []OfferingTiming
	err     error
	// panicked is a panic the fill raised; each waiter re-raises it.
	panicked any
}

// do joins the in-flight fill, starting one with load when none runs. load
// runs once per flight; publish runs with its result before the flight is
// cleared and any waiter wakes. warm, when non-nil, is asked under the same
// lock before a flight starts: a value it returns answers instead, so a caller
// that saw a miss before an earlier flight published loads nothing.
func (f *offeringFill) do(ctx context.Context, load offeringLoader, publish func([]OfferingTiming, error), warm func() ([]OfferingTiming, bool)) ([]OfferingTiming, error) {
	f.mu.Lock()
	fl := f.flight
	if fl == nil && warm != nil {
		if offerings, ok := warm(); ok {
			f.mu.Unlock()
			return offerings, nil
		}
	}
	if fl == nil {
		fl = &offeringFlight{done: make(chan struct{})}
		f.flight = fl
		go f.run(context.WithoutCancel(ctx), fl, load, publish)
	}
	fl.waiters++
	f.mu.Unlock()

	select {
	case <-fl.done:
		if fl.panicked != nil {
			panic(fl.panicked)
		}
		return fl.value, fl.err
	case <-ctx.Done():
		f.mu.Lock()
		fl.waiters--
		f.mu.Unlock()
		return nil, ctx.Err()
	}
}

func (f *offeringFill) run(ctx context.Context, fl *offeringFlight, load offeringLoader, publish func([]OfferingTiming, error)) {
	ctx, cancel := context.WithTimeout(ctx, offeringFillTimeout)
	defer cancel()
	defer func() {
		fl.panicked = recover()
		if fl.panicked != nil {
			slog.Error("lab: offering timing fill panicked", "recover", fl.panicked)
		} else {
			publish(fl.value, fl.err)
		}
		f.mu.Lock()
		f.flight = nil
		f.mu.Unlock()
		close(fl.done)
	}()
	fl.value, fl.err = load(ctx)
}

// waiting reports how many callers wait on the in-flight fill.
func (f *offeringFill) waiting() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.flight == nil {
		return 0
	}
	return f.flight.waiters
}

// publishOfferingTimings stores a complete fill — an empty one included — and
// marks the stored value stale after a failed one, which is never stored.
func publishOfferingTimings(c *Cache) func([]OfferingTiming, error) {
	return func(offerings []OfferingTiming, err error) {
		if err == nil {
			var data []byte
			if data, err = json.Marshal(offerings); err == nil {
				c.SetOfferingTiming(data)
				return
			}
		}
		slog.Warn("lab: offering timing fill incomplete; not cached", "error", err)
		c.MarkOfferingTimingStale()
	}
}

// RefreshOfferingTimings refills the cache's offering timings through the
// shared fill and returns how many offerings the answer holds. A complete
// answer is stored even when empty (an offering with no fragment_snapshots rows
// is dropped, so empty is a real answer — see the cache-state contract at the
// top of internal/lab/cache.go). A failed fill keeps the last complete value,
// marked stale, and returns the error.
//
// This is the tick's entry point (cmd/server/main.go, ninja_fragments event).
func RefreshOfferingTimings(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope) (int, error) {
	c := cache.For(scope)
	offerings, err := c.offeringFill.do(ctx, repo.offeringLoader(scope), publishOfferingTimings(c), nil)
	return len(offerings), err
}

// ServeOfferingTimings answers the market overview's offerings: the stored
// value when one exists (stale when a later fill failed), otherwise the shared
// fill. A failed fill still answers its partial result, uncached and stale. A
// nil repo serves only the stored value.
func ServeOfferingTimings(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope) ([]OfferingTiming, bool) {
	var load offeringLoader
	if repo != nil {
		load = repo.offeringLoader(scope)
	}
	return serveOfferingTimings(ctx, load, cache.For(scope))
}

func serveOfferingTimings(ctx context.Context, load offeringLoader, c *Cache) ([]OfferingTiming, bool) {
	if offerings, stale, ok := storedOfferingTimings(c); ok {
		return offerings, stale
	}
	if load == nil {
		return nil, false
	}
	return fillColdOfferingTimings(ctx, load, c)
}

// fillColdOfferingTimings runs or joins the shared fill after a cold read,
// answering from the cache instead when a fill published since that read.
func fillColdOfferingTimings(ctx context.Context, load offeringLoader, c *Cache) ([]OfferingTiming, bool) {
	stale := false
	warm := func() ([]OfferingTiming, bool) {
		offerings, s, ok := storedOfferingTimings(c)
		stale = s
		return offerings, ok
	}
	offerings, err := c.offeringFill.do(ctx, load, publishOfferingTimings(c), warm)
	return offerings, stale || err != nil
}

// storedOfferingTimings decodes the stored offering slot; ok is false when it
// is COLD or not decodable.
func storedOfferingTimings(c *Cache) ([]OfferingTiming, bool, bool) {
	data, stale := c.OfferingTimingState()
	if data == nil {
		return nil, false, false
	}
	var offerings []OfferingTiming
	if err := json.Unmarshal(data, &offerings); err != nil {
		slog.Warn("lab: cached offering timing is not decodable, refilling", "error", err)
		return nil, false, false
	}
	return offerings, stale, true
}

func (r *Repository) offeringLoader(scope league.Scope) offeringLoader {
	return func(ctx context.Context) ([]OfferingTiming, error) {
		return offeringTimings(ctx, r.pool, scope)
	}
}

// OfferingTimings computes timing analysis for all lab offerings, keeping
// whatever partial result a failed query leaves.
func (r *Repository) OfferingTimings(ctx context.Context, scope league.Scope) []OfferingTiming {
	offerings, _ := offeringTimings(ctx, r.pool, scope)
	return offerings
}

// offeringTimings computes timing analysis for all lab offerings with the ten
// queries, returning the partial result and the joined error of every failed
// query, scan or iteration.
//
// It never returns nil. An offering with no fragment_snapshots rows is dropped,
// so "no offerings" is a legitimate answer, and callers cache that answer — a
// nil slice marshals to `null`, which is indistinguishable from "never stored"
// on the way back out.
func offeringTimings(ctx context.Context, q offeringQuerier, scope league.Scope) ([]OfferingTiming, error) {
	offerings := []struct {
		name       string
		fragmentID string
	}{
		{"Gift to the Goddess", "offer-gift"},
		{"Dedication to the Goddess", "offer-dedication"},
	}
	dayNames := []string{"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}

	result := make([]OfferingTiming, 0, len(offerings))
	var errs []error
	for _, off := range offerings {
		ot, err := offeringTiming(ctx, q, scope, off.name, off.fragmentID, dayNames)
		if err != nil {
			errs = append(errs, err)
		}
		if ot != nil {
			result = append(result, *ot)
		}
	}
	return result, errors.Join(errs...)
}

// offeringTiming queries fragment_snapshots for a single offering's
// price patterns and returns structured timing data, nil if no data. err joins
// every failure; the timing keeps what the other queries produced.
func offeringTiming(ctx context.Context, q offeringQuerier, scope league.Scope, name, fragmentID string, dayNames []string) (*OfferingTiming, error) {
	ot := OfferingTiming{Name: name, FragmentID: fragmentID}
	var errs []error
	fail := func(msg string, err error) {
		slog.Warn("offering timing: "+msg, "fragment", fragmentID, "error", err)
		errs = append(errs, fmt.Errorf("offering timing %s: %s: %w", fragmentID, msg, err))
	}

	// Current price.
	var currentPrice *float64
	if err := q.QueryRow(ctx, `
		SELECT chaos FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2
		ORDER BY time DESC LIMIT 1`, scope.ID(), fragmentID).Scan(&currentPrice); errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	} else if err != nil {
		fail("current price query failed", err)
		return nil, errors.Join(errs...)
	}
	if currentPrice == nil {
		return nil, nil
	}
	ot.CurrentPrice = math.Round(*currentPrice)

	// Hourly medians (14-day window).
	hourRows, err := q.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 3
		ORDER BY median`, scope.ID(), fragmentID)
	if err != nil {
		fail("hourly query failed", err)
	} else {
		var hours []offeringHourMedian
		for hourRows.Next() {
			var hm offeringHourMedian
			if err := hourRows.Scan(&hm.Hour, &hm.Median); err != nil {
				fail("hourly scan failed", err)
				continue
			}
			hours = append(hours, hm)
		}
		hourRows.Close()
		if err := hourRows.Err(); err != nil {
			fail("hourly iteration error", err)
		}
		// Store all hourly medians for prediction chart line.
		for _, h := range hours {
			ot.HourlyMedians = append(ot.HourlyMedians, OfferingTimingHour{Hour: h.Hour, Median: math.Round(h.Median)})
		}
		sort.Slice(ot.HourlyMedians, func(i, j int) bool { return ot.HourlyMedians[i].Hour < ot.HourlyMedians[j].Hour })

		if len(hours) >= 4 {
			for _, h := range hours[:3] {
				ot.CheapHours = append(ot.CheapHours, OfferingTimingHour{Hour: h.Hour, Median: math.Round(h.Median)})
			}
			for i := len(hours) - 1; i >= len(hours)-3 && i >= 0; i-- {
				ot.ExpensiveHours = append(ot.ExpensiveHours, OfferingTimingHour{Hour: hours[i].Hour, Median: math.Round(hours[i].Median)})
			}
			// Sort by hour, not by price.
			sort.Slice(ot.CheapHours, func(i, j int) bool { return ot.CheapHours[i].Hour < ot.CheapHours[j].Hour })
			sort.Slice(ot.ExpensiveHours, func(i, j int) bool { return ot.ExpensiveHours[i].Hour < ot.ExpensiveHours[j].Hour })
		}
	}

	// Today's weekday-specific hourly medians (for prediction accuracy).
	todayDOW := int(time.Now().UTC().Weekday())
	todayRows, err := q.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		  AND EXTRACT(DOW FROM time)::int = $3
		GROUP BY 1 HAVING COUNT(*) >= 1
		ORDER BY h`, scope.ID(), fragmentID, todayDOW)
	if err != nil {
		fail("today hourly query failed", err)
	} else {
		for todayRows.Next() {
			var hm offeringHourMedian
			if err := todayRows.Scan(&hm.Hour, &hm.Median); err != nil {
				fail("today hourly scan failed", err)
				continue
			}
			ot.TodayHourMedians = append(ot.TodayHourMedians, OfferingTimingHour{Hour: hm.Hour, Median: math.Round(hm.Median)})
		}
		todayRows.Close()
		if err := todayRows.Err(); err != nil {
			fail("today hourly iteration error", err)
		}
	}

	// Weekday medians (14-day window).
	type dayMedian struct {
		Day    int
		Median float64
	}
	dayRows, err := q.Query(ctx, `
		SELECT EXTRACT(DOW FROM time)::int AS d,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 5
		ORDER BY median`, scope.ID(), fragmentID)
	if err != nil {
		fail("weekday query failed", err)
	} else {
		var days []dayMedian
		for dayRows.Next() {
			var dm dayMedian
			if err := dayRows.Scan(&dm.Day, &dm.Median); err != nil {
				fail("weekday scan failed", err)
				continue
			}
			days = append(days, dm)
		}
		dayRows.Close()
		if err := dayRows.Err(); err != nil {
			fail("weekday iteration error", err)
		}
		if len(days) >= 4 {
			type dayWithIdx struct {
				idx   int
				entry OfferingTimingDay
			}
			var cheapD, expD []dayWithIdx
			for i := 0; i < 2 && i < len(days); i++ {
				cheapD = append(cheapD, dayWithIdx{days[i].Day, OfferingTimingDay{Day: dayNames[days[i].Day], Median: math.Round(days[i].Median)}})
			}
			for i := len(days) - 1; i >= len(days)-2 && i >= 0; i-- {
				expD = append(expD, dayWithIdx{days[i].Day, OfferingTimingDay{Day: dayNames[days[i].Day], Median: math.Round(days[i].Median)}})
			}
			// Sort by weekday order (Sun=0 .. Sat=6).
			sort.Slice(cheapD, func(i, j int) bool { return cheapD[i].idx < cheapD[j].idx })
			sort.Slice(expD, func(i, j int) bool { return expD[i].idx < expD[j].idx })
			for _, d := range cheapD {
				ot.CheapDays = append(ot.CheapDays, d.entry)
			}
			for _, d := range expD {
				ot.ExpensiveDays = append(ot.ExpensiveDays, d.entry)
			}
		}
	}

	// Sparkline: 3-day price history, 1 point per hour.
	sparkRows, err := q.Query(ctx, `
		SELECT time_bucket('1 hour', time) AS bucket, AVG(chaos) AS avg_price
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '3 days'
		GROUP BY 1 ORDER BY 1`, scope.ID(), fragmentID)
	if err != nil {
		fail("sparkline query failed", err)
	} else {
		for sparkRows.Next() {
			var t time.Time
			var p float64
			if err := sparkRows.Scan(&t, &p); err != nil {
				fail("sparkline scan failed", err)
				continue
			}
			ot.Sparkline = append(ot.Sparkline, OfferingSparkPoint{
				Time:  t.UTC().Format(time.RFC3339),
				Price: math.Round(p),
			})
		}
		sparkRows.Close()
		if err := sparkRows.Err(); err != nil {
			fail("sparkline iteration error", err)
		}
	}

	return &ot, errors.Join(errs...)
}
