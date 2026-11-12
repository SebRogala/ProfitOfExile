package lab

import (
	"context"
	"encoding/json"
	"log/slog"
	"math"
	"sort"
	"time"

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

// RefreshOfferingTimings recomputes lab offering timings and stores the result
// in cache, returning how many offerings the answer holds.
//
// It stores the answer even when it is empty. An offering with no
// fragment_snapshots rows is dropped, so an empty result is the tick's real
// answer, not a failure — withholding it leaves the cache COLD, and then no
// amount of reader discipline helps: MarketOverview re-runs all ten queries on
// every request until rows happen to exist. See the cache-state contract at the
// top of internal/lab/cache.go.
//
// This is the tick's entry point (cmd/server/main.go, ninja_fragments event).
// MarketOverview's own cold path computes inline because it needs the values it
// serves, not just the stored bytes.
func RefreshOfferingTimings(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope) (int, error) {
	offerings := repo.OfferingTimings(ctx, scope)
	data, err := json.Marshal(offerings)
	if err != nil {
		return 0, err
	}
	cache.For(scope).SetOfferingTiming(data)
	return len(offerings), nil
}

// OfferingTimings computes timing analysis for all lab offerings.
//
// It never returns nil. An offering with no fragment_snapshots rows is dropped,
// so "no offerings" is a legitimate answer, and callers cache that answer — a
// nil slice marshals to `null`, which is indistinguishable from "never stored"
// on the way back out. Returning an empty slice lets every caller store the
// result as it stands.
func (r *Repository) OfferingTimings(ctx context.Context, scope league.Scope) []OfferingTiming {
	offerings := []struct {
		name       string
		fragmentID string
	}{
		{"Gift to the Goddess", "offer-gift"},
		{"Dedication to the Goddess", "offer-dedication"},
	}
	dayNames := []string{"Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"}

	result := make([]OfferingTiming, 0, len(offerings))
	for _, off := range offerings {
		ot := r.offeringTiming(ctx, scope, off.name, off.fragmentID, dayNames)
		if ot != nil {
			result = append(result, *ot)
		}
	}
	return result
}

// offeringTiming queries fragment_snapshots for a single offering's
// price patterns and returns structured timing data. Returns nil if no data.
func (r *Repository) offeringTiming(ctx context.Context, scope league.Scope, name, fragmentID string, dayNames []string) *OfferingTiming {
	ot := OfferingTiming{Name: name, FragmentID: fragmentID}

	// Current price.
	var currentPrice *float64
	if err := r.pool.QueryRow(ctx, `
		SELECT chaos FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2
		ORDER BY time DESC LIMIT 1`, scope.ID(), fragmentID).Scan(&currentPrice); err != nil {
		slog.Warn("offering timing: current price query failed", "fragment", fragmentID, "error", err)
		return nil
	}
	if currentPrice == nil {
		return nil
	}
	ot.CurrentPrice = math.Round(*currentPrice)

	// Hourly medians (14-day window).
	hourRows, err := r.pool.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 3
		ORDER BY median`, scope.ID(), fragmentID)
	if err != nil {
		slog.Warn("offering timing: hourly query failed", "fragment", fragmentID, "error", err)
	} else {
		var hours []offeringHourMedian
		for hourRows.Next() {
			var hm offeringHourMedian
			if err := hourRows.Scan(&hm.Hour, &hm.Median); err != nil {
				slog.Warn("offering timing: hourly scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			hours = append(hours, hm)
		}
		hourRows.Close()
		if err := hourRows.Err(); err != nil {
			slog.Warn("offering timing: hourly iteration error", "fragment", fragmentID, "error", err)
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
	todayRows, err := r.pool.Query(ctx, `
		SELECT EXTRACT(HOUR FROM time)::int AS h,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		  AND EXTRACT(DOW FROM time)::int = $3
		GROUP BY 1 HAVING COUNT(*) >= 1
		ORDER BY h`, scope.ID(), fragmentID, todayDOW)
	if err != nil {
		slog.Warn("offering timing: today hourly query failed", "fragment", fragmentID, "error", err)
	} else {
		for todayRows.Next() {
			var hm offeringHourMedian
			if err := todayRows.Scan(&hm.Hour, &hm.Median); err != nil {
				slog.Warn("offering timing: today hourly scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			ot.TodayHourMedians = append(ot.TodayHourMedians, OfferingTimingHour{Hour: hm.Hour, Median: math.Round(hm.Median)})
		}
		todayRows.Close()
		if err := todayRows.Err(); err != nil {
			slog.Warn("offering timing: today hourly iteration error", "fragment", fragmentID, "error", err)
		}
	}

	// Weekday medians (14-day window).
	type dayMedian struct {
		Day    int
		Median float64
	}
	dayRows, err := r.pool.Query(ctx, `
		SELECT EXTRACT(DOW FROM time)::int AS d,
		       PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY chaos) AS median
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '14 days'
		GROUP BY 1 HAVING COUNT(*) >= 5
		ORDER BY median`, scope.ID(), fragmentID)
	if err != nil {
		slog.Warn("offering timing: weekday query failed", "fragment", fragmentID, "error", err)
	} else {
		var days []dayMedian
		for dayRows.Next() {
			var dm dayMedian
			if err := dayRows.Scan(&dm.Day, &dm.Median); err != nil {
				slog.Warn("offering timing: weekday scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			days = append(days, dm)
		}
		dayRows.Close()
		if err := dayRows.Err(); err != nil {
			slog.Warn("offering timing: weekday iteration error", "fragment", fragmentID, "error", err)
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
	sparkRows, err := r.pool.Query(ctx, `
		SELECT time_bucket('1 hour', time) AS bucket, AVG(chaos) AS avg_price
		FROM fragment_snapshots
		WHERE league = $1 AND fragment_id = $2 AND time > NOW() - INTERVAL '3 days'
		GROUP BY 1 ORDER BY 1`, scope.ID(), fragmentID)
	if err != nil {
		slog.Warn("offering timing: sparkline query failed", "fragment", fragmentID, "error", err)
	} else {
		for sparkRows.Next() {
			var t time.Time
			var p float64
			if err := sparkRows.Scan(&t, &p); err != nil {
				slog.Warn("offering timing: sparkline scan failed", "fragment", fragmentID, "error", err)
				continue
			}
			ot.Sparkline = append(ot.Sparkline, OfferingSparkPoint{
				Time:  t.UTC().Format(time.RFC3339),
				Price: math.Round(p),
			})
		}
		sparkRows.Close()
		if err := sparkRows.Err(); err != nil {
			slog.Warn("offering timing: sparkline iteration error", "fragment", fragmentID, "error", err)
		}
	}

	return &ot
}
