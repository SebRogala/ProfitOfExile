//go:build integration

package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"sort"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

func offeringIntegrationPool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(pool.Close)

	var exists bool
	if err := pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'fragment_snapshots')").
		Scan(&exists); err != nil {
		t.Fatalf("check fragment_snapshots table: %v", err)
	}
	if !exists {
		t.Skip("fragment_snapshots table not found, skipping (migrations not applied)")
	}
	return pool
}

// The production sequence behind POE-152's dormant half, measured against a real
// database: a league with no fragment_snapshots rows makes ComputeOfferingTimings
// return nothing, and that nothing must be stored so the next request is served
// from the cache instead of re-running the ten queries that produced it.
//
// The acquire count is the observable. It is the direct form of the acceptance
// criterion — "served from cache, zero database work" — rather than a proxy for
// it, and it fails if the empty answer is either not stored or not trusted.
func TestMarketOverview_EmptyOfferingTimingIsStoredAndThenServedWithoutTheDatabase(t *testing.T) {
	pool := offeringIntegrationPool(t)
	// A league that owns no fragment_snapshots rows: nothing is inserted for it,
	// so every offering's current-price query returns NULL.
	scope := league.Historical("POE152-NoFragments")
	cache := lab.NewCache(scope)
	handler := MarketOverview(cache, lab.NewRepository(pool), scope)

	serve := func() {
		t.Helper()
		w := httptest.NewRecorder()
		handler.ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
		if w.Code != http.StatusOK {
			t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
		}
	}

	beforeFirst := pool.Stat().AcquireCount()
	serve()
	if pool.Stat().AcquireCount() == beforeFirst {
		t.Fatal("first request did not query the database; the test is not exercising the compute path")
	}

	stored := cache.For(scope).OfferingTiming()
	if stored == nil {
		t.Fatal("cache holds no offering timing after a request that computed an empty one; every later request will recompute it")
	}
	if string(stored) != "[]" {
		t.Errorf("cached offering timing = %s, want []", stored)
	}

	beforeSecond := pool.Stat().AcquireCount()
	serve()
	if got := pool.Stat().AcquireCount(); got != beforeSecond {
		t.Errorf("second request acquired %d connection(s), want 0 — the empty answer was recomputed", got-beforeSecond)
	}
}

// registerOfferingLeague inserts a leagues row for a test-owned league (the
// fragment_snapshots league FK needs it) and deletes the league's snapshot rows
// and the league itself when the test ends.
func registerOfferingLeague(t *testing.T, pool *pgxpool.Pool, prefix string) league.Scope {
	t.Helper()
	id := fmt.Sprintf("%s-%d", prefix, time.Now().UnixNano())
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	t.Cleanup(func() {
		ctx := context.Background()
		if _, err := pool.Exec(ctx, `DELETE FROM fragment_snapshots WHERE league = $1`, id); err != nil {
			t.Logf("cleanup warning: delete fragment_snapshots for %q: %v", id, err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id = $1`, id); err != nil {
			t.Logf("cleanup warning: delete league %q: %v", id, err)
		}
	})
	return league.Historical(id)
}

func seedFragment(t *testing.T, pool *pgxpool.Pool, scope league.Scope, fragmentID string, at time.Time, chaos float64) {
	t.Helper()
	if _, err := pool.Exec(context.Background(),
		`INSERT INTO fragment_snapshots (league, time, fragment_id, chaos) VALUES ($1, $2, $3, $4)`,
		scope.ID(), at, fragmentID, chaos); err != nil {
		t.Fatalf("seed fragment %s at %s: %v", fragmentID, at, err)
	}
}

// utcMidnight is the start of today in UTC; fixture timestamps are offsets from
// it so their hour and weekday are known without asking the database.
func utcMidnight() time.Time { return time.Now().UTC().Truncate(24 * time.Hour) }

type offeringHour struct {
	Hour   int     `json:"hour"`
	Median float64 `json:"median"`
}

type offeringDay struct {
	Day    string  `json:"day"`
	Median float64 `json:"median"`
}

type offeringJSON struct {
	Name             string         `json:"name"`
	FragmentID       string         `json:"fragmentId"`
	CurrentPrice     float64        `json:"currentPrice"`
	CheapHours       []offeringHour `json:"cheapHours"`
	ExpensiveHours   []offeringHour `json:"expensiveHours"`
	CheapDays        []offeringDay  `json:"cheapDays"`
	ExpensiveDays    []offeringDay  `json:"expensiveDays"`
	HourlyMedians    []offeringHour `json:"hourlyMedians"`
	TodayHourMedians []offeringHour `json:"todayHourMedians"`
	Sparkline        []struct {
		Time  string  `json:"time"`
		Price float64 `json:"price"`
	} `json:"sparkline"`
}

// serveOfferings runs MarketOverview with the given cache and returns each
// offering both decoded and as raw keys, so a test can tell an omitted key from
// an empty one.
func serveOfferings(t *testing.T, pool *pgxpool.Pool, cache *lab.Cache, scope league.Scope) ([]offeringJSON, []map[string]json.RawMessage) {
	t.Helper()
	w := httptest.NewRecorder()
	MarketOverview(cache, lab.NewRepository(pool), scope).ServeHTTP(w, httptest.NewRequest(http.MethodGet, "/api/analysis/market-overview", nil))
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d; body: %s", w.Code, http.StatusOK, w.Body.String())
	}
	var typed struct {
		Offerings []offeringJSON `json:"offerings"`
	}
	var raw struct {
		Offerings []map[string]json.RawMessage `json:"offerings"`
	}
	body := w.Body.Bytes()
	if err := json.Unmarshal(body, &typed); err != nil {
		t.Fatalf("decode response: %v; body: %s", err, body)
	}
	if err := json.Unmarshal(body, &raw); err != nil {
		t.Fatalf("decode raw response: %v; body: %s", err, body)
	}
	return typed.Offerings, raw.Offerings
}

func onlyGift(t *testing.T, offerings []offeringJSON) offeringJSON {
	t.Helper()
	if len(offerings) != 1 || offerings[0].FragmentID != "offer-gift" {
		t.Fatalf("offerings = %+v, want exactly the offer-gift offering", offerings)
	}
	return offerings[0]
}

func TestMarketOverview_CurrentPriceIsTheNewestSnapshotRounded(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-CurrentPrice")
	now := time.Now().UTC()
	seedFragment(t, pool, scope, "offer-gift", now.Add(-5*time.Hour), 10)
	seedFragment(t, pool, scope, "offer-gift", now.Add(-1*time.Hour), 41.6)

	offerings, _ := serveOfferings(t, pool, lab.NewCache(scope), scope)

	if got := onlyGift(t, offerings).CurrentPrice; got != 42 {
		t.Errorf("currentPrice = %v, want 42 (newest snapshot 41.6 rounded)", got)
	}
}

func TestMarketOverview_CheapAndExpensiveHoursAreTheThreeExtremesSortedByHour(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-Hours")
	// hour → price; three rows each (days 4..6 back), so the median is the price.
	// Median order (2,1,4,5,3) differs from hour order within both extremes.
	prices := map[int]float64{1: 20.4, 2: 10.4, 3: 50.4, 4: 30.4, 5: 40.4}
	for h, p := range prices {
		for d := 4; d <= 6; d++ {
			seedFragment(t, pool, scope, "offer-gift", utcMidnight().AddDate(0, 0, -d).Add(time.Duration(h)*time.Hour), p)
		}
	}

	offerings, _ := serveOfferings(t, pool, lab.NewCache(scope), scope)
	gift := onlyGift(t, offerings)

	wantCheap := []offeringHour{{1, 20}, {2, 10}, {4, 30}}
	wantExpensive := []offeringHour{{3, 50}, {4, 30}, {5, 40}}
	wantAll := []offeringHour{{1, 20}, {2, 10}, {3, 50}, {4, 30}, {5, 40}}
	if !reflect.DeepEqual(gift.CheapHours, wantCheap) {
		t.Errorf("cheapHours = %+v, want %+v", gift.CheapHours, wantCheap)
	}
	if !reflect.DeepEqual(gift.ExpensiveHours, wantExpensive) {
		t.Errorf("expensiveHours = %+v, want %+v", gift.ExpensiveHours, wantExpensive)
	}
	if !reflect.DeepEqual(gift.HourlyMedians, wantAll) {
		t.Errorf("hourlyMedians = %+v, want %+v", gift.HourlyMedians, wantAll)
	}
}

func TestMarketOverview_FewerThanFourQualifyingHoursOmitsCheapAndExpensiveHours(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-ThreeHours")
	for h, p := range map[int]float64{1: 10, 2: 20, 3: 30} {
		for d := 4; d <= 6; d++ {
			seedFragment(t, pool, scope, "offer-gift", utcMidnight().AddDate(0, 0, -d).Add(time.Duration(h)*time.Hour), p)
		}
	}

	offerings, raw := serveOfferings(t, pool, lab.NewCache(scope), scope)
	gift := onlyGift(t, offerings)

	if len(gift.HourlyMedians) != 3 {
		t.Fatalf("hourlyMedians = %+v, want 3 qualifying hours; the fixture is not at the boundary", gift.HourlyMedians)
	}
	for _, key := range []string{"cheapHours", "expensiveHours"} {
		if v, ok := raw[0][key]; ok {
			t.Errorf("offering has %q = %s with only 3 qualifying hours, want the key omitted", key, v)
		}
	}
}

func TestMarketOverview_CheapAndExpensiveDaysAreTheTwoExtremesInWeekdayOrder(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-Days")
	// days back → price; five rows each, so the median is the price.
	prices := map[int]float64{2: 40.4, 3: 10.4, 4: 30.4, 5: 20.4}
	for d, p := range prices {
		for h := 0; h < 5; h++ {
			seedFragment(t, pool, scope, "offer-gift", utcMidnight().AddDate(0, 0, -d).Add(time.Duration(h)*time.Hour), p)
		}
	}
	day := func(back int, median float64) offeringDay {
		return offeringDay{utcMidnight().AddDate(0, 0, -back).Weekday().String()[:3], median}
	}
	inWeekdayOrder := func(back ...int) []int {
		sort.Slice(back, func(i, j int) bool {
			return utcMidnight().AddDate(0, 0, -back[i]).Weekday() < utcMidnight().AddDate(0, 0, -back[j]).Weekday()
		})
		return back
	}
	var wantCheap, wantExpensive []offeringDay
	for _, b := range inWeekdayOrder(3, 5) {
		wantCheap = append(wantCheap, day(b, map[int]float64{3: 10, 5: 20}[b]))
	}
	for _, b := range inWeekdayOrder(2, 4) {
		wantExpensive = append(wantExpensive, day(b, map[int]float64{2: 40, 4: 30}[b]))
	}

	offerings, _ := serveOfferings(t, pool, lab.NewCache(scope), scope)
	gift := onlyGift(t, offerings)

	if !reflect.DeepEqual(gift.CheapDays, wantCheap) {
		t.Errorf("cheapDays = %+v, want %+v", gift.CheapDays, wantCheap)
	}
	if !reflect.DeepEqual(gift.ExpensiveDays, wantExpensive) {
		t.Errorf("expensiveDays = %+v, want %+v", gift.ExpensiveDays, wantExpensive)
	}
}

func TestMarketOverview_SparklineHasOneRoundedPointPerHourBucket(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-Sparkline")
	hour := time.Now().UTC().Truncate(time.Hour)
	b1, b2, b3 := hour.Add(-50*time.Hour), hour.Add(-26*time.Hour), hour.Add(-2*time.Hour)
	seedFragment(t, pool, scope, "offer-gift", hour.Add(-5*24*time.Hour), 99) // outside the 3-day window
	seedFragment(t, pool, scope, "offer-gift", b1.Add(5*time.Minute), 12.4)
	seedFragment(t, pool, scope, "offer-gift", b2.Add(5*time.Minute), 10)
	seedFragment(t, pool, scope, "offer-gift", b2.Add(20*time.Minute), 11) // bucket average 10.5
	seedFragment(t, pool, scope, "offer-gift", b3.Add(5*time.Minute), 20.6)

	offerings, _ := serveOfferings(t, pool, lab.NewCache(scope), scope)
	gift := onlyGift(t, offerings)

	type point struct {
		Time  string
		Price float64
	}
	want := []point{{b1.Format(time.RFC3339), 12}, {b2.Format(time.RFC3339), 11}, {b3.Format(time.RFC3339), 21}}
	var got []point
	for _, p := range gift.Sparkline {
		got = append(got, point{p.Time, p.Price})
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("sparkline = %+v, want %+v", got, want)
	}
}

func TestMarketOverview_OfferingWithoutSnapshotsIsDropped(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope := registerOfferingLeague(t, pool, "POE344-GiftOnly")
	seedFragment(t, pool, scope, "offer-gift", time.Now().UTC().Add(-time.Hour), 30)

	offerings, _ := serveOfferings(t, pool, lab.NewCache(scope), scope)

	if len(offerings) != 1 || offerings[0].FragmentID != "offer-gift" {
		t.Errorf("offerings = %+v, want only offer-gift (offer-dedication has no rows)", offerings)
	}
}

// serveWithStableWeekday seeds a fixture aligned to today's UTC weekday (passed
// as midnight seven days back) and serves it with a cold cache. The handler reads
// the weekday from the clock, so a UTC midnight crossing during the request would
// misalign the fixture; it then rebuilds the fixture once in a new league.
func serveWithStableWeekday(t *testing.T, pool *pgxpool.Pool, prefix string, seed func(scope league.Scope, sameWeekday time.Time)) (league.Scope, *lab.Cache, []offeringJSON, []map[string]json.RawMessage) {
	t.Helper()
	for attempt := 0; attempt < 2; attempt++ {
		dow := time.Now().UTC().Weekday()
		scope := registerOfferingLeague(t, pool, prefix)
		seed(scope, utcMidnight().AddDate(0, 0, -7))
		cache := lab.NewCache(scope)
		offerings, raw := serveOfferings(t, pool, cache, scope)
		if time.Now().UTC().Weekday() == dow {
			return scope, cache, offerings, raw
		}
	}
	t.Fatal("UTC weekday changed during both attempts")
	return league.Scope{}, nil, nil, nil
}

func TestMarketOverview_TodayHourMediansAreTodaysWeekdayHoursRounded(t *testing.T) {
	pool := offeringIntegrationPool(t)
	scope, cache, offerings, raw := serveWithStableWeekday(t, pool, "POE344-Today", func(scope league.Scope, same time.Time) {
		seedFragment(t, pool, scope, "offer-gift", same.Add(3*time.Hour), 15.4) // single row: COUNT(*) >= 1
		seedFragment(t, pool, scope, "offer-gift", same.Add(7*time.Hour), 20)
		seedFragment(t, pool, scope, "offer-gift", same.Add(7*time.Hour+10*time.Minute), 21) // median 20.5
		seedFragment(t, pool, scope, "offer-gift", same.Add(11*time.Hour), 30.6)
		seedFragment(t, pool, scope, "offer-gift", same.AddDate(0, 0, -1).Add(5*time.Hour), 99) // decoy weekday
	})
	want := []offeringHour{{3, 15}, {7, 21}, {11, 31}}

	gift := onlyGift(t, offerings)
	if _, ok := raw[0]["todayHourMedians"]; !ok {
		t.Fatal("offering has no todayHourMedians key")
	}
	if !reflect.DeepEqual(gift.TodayHourMedians, want) {
		t.Errorf("todayHourMedians = %+v, want %+v", gift.TodayHourMedians, want)
	}

	warm, _ := serveOfferings(t, pool, cache, scope)
	if got := onlyGift(t, warm).TodayHourMedians; !reflect.DeepEqual(got, want) {
		t.Errorf("warm-cache todayHourMedians = %+v, want %+v", got, want)
	}
}

func TestMarketOverview_NoRowsOnTodaysWeekdayOmitsTodayHourMedians(t *testing.T) {
	pool := offeringIntegrationPool(t)
	_, _, offerings, raw := serveWithStableWeekday(t, pool, "POE344-NotToday", func(scope league.Scope, same time.Time) {
		seedFragment(t, pool, scope, "offer-gift", same.AddDate(0, 0, -1).Add(5*time.Hour), 10)
		seedFragment(t, pool, scope, "offer-gift", same.AddDate(0, 0, -2).Add(6*time.Hour), 20)
	})

	onlyGift(t, offerings)
	if v, ok := raw[0]["todayHourMedians"]; ok {
		t.Errorf("offering has todayHourMedians = %s with no rows on today's weekday, want the key omitted", v)
	}
}
