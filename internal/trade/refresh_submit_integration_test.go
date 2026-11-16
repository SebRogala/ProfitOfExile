//go:build integration

package trade

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/book"
	"profitofexile/internal/price/resolve"
)

func TestBackgroundRefreshEvidence_SuccessiveRevisits(t *testing.T) {
	backgroundEvidence(t, "positive")
}
func TestBackgroundRefreshEvidence_EmptyRevisit(t *testing.T) {
	backgroundEvidence(t, "empty")
}
func TestBackgroundRefreshEvidence_FailedFetchRevisit(t *testing.T) {
	backgroundEvidence(t, "failure")
}

func backgroundEvidence(t *testing.T, second string) {
	t.Helper()
	pool := tradeIntegrationPool(t)
	id := fmt.Sprintf("refresh-evidence-%d", time.Now().UnixNano())
	registerTradeLeague(t, pool, id)
	otherID := id + "-other"
	registerTradeLeague(t, pool, otherID)
	scope := league.Historical(id)
	ctx := context.Background()
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, "DELETE FROM trade_lookups WHERE league=$1 OR league=$2", id, otherID); err != nil {
			t.Error(err)
		}
	})
	now := time.Now().UTC().Truncate(time.Second)
	indexed := now.Add(-time.Hour)
	var pass atomic.Int32
	pass.Store(1)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		n := pass.Load()
		if r.Method == http.MethodPost {
			ids := []string{"one", "two"}
			total := int(n) * 11
			if n == 2 && second == "empty" {
				ids = []string{}
				total = 0
			}
			json.NewEncoder(w).Encode(tradeSearchResp{ID: "test", Result: ids, Total: total})
			return
		}
		if n == 2 && second == "failure" {
			w.WriteHeader(500)
			return
		}
		currency := "chaos"
		amount := 10.0
		if n == 2 {
			currency = "divine"
			amount = 2
		}
		json.NewEncoder(w).Encode(tradeFetchResp{Result: []tradeFetchEntry{
			{Listing: tradeFetchListing{Indexed: indexed, Account: tradeFetchAccount{Name: "seller-a"}, Price: tradeFetchPrice{Amount: amount, Currency: currency}}, Item: tradeFetchItem{Properties: []tradeFetchProperty{{Name: "Level", Values: [][]interface{}{{"20", 0}}}, {Name: "Quality", Values: [][]interface{}{{"20%", 0}}}}}},
			{Listing: tradeFetchListing{Indexed: indexed.Add(time.Minute), Account: tradeFetchAccount{Name: "seller-b"}, Price: tradeFetchPrice{Amount: 30, Currency: "chaos"}}, Item: tradeFetchItem{Properties: []tradeFetchProperty{{Name: "Level", Values: [][]interface{}{{"20", 0}}}, {Name: "Quality", Values: [][]interface{}{{"20%", 0}}}}}},
		}})
	}))
	defer upstream.Close()
	cfg := TradeConfig{LeagueName: id, CeilingFactor: 1, DefaultSearchRate: 100, DefaultFetchRate: 100, MaxQueueWait: time.Second}
	client := NewClient(cfg)
	client.SetBaseURL(upstream.URL)
	repo := NewRepository(pool)
	g := NewGate(cfg, scope, NewRateLimiter(cfg), client, nil, NewTradeCache(10, scope), func() float64 {
		if pass.Load() == 1 {
			return 200
		}
		return 250
	}, repo)
	// Synchronous execution uses the gate's existing persistence seam so row
	// assertions observe completion without polling async database writes.
	g.persistAsync = func(f func()) { f() }
	clock := new(atomic.Int64)
	clock.Store(now.UnixNano())
	g.now = func() time.Time { return time.Unix(0, clock.Load()).UTC() }
	serviceCtx, _ := runBackground(t, g)
	b := NewBackgroundRefresh(g)
	key := resolve.GemKey(id, "Siege Ballista of Splintering", "20/20")
	q, err := book.NewScheduler(scope, resolve.Policy{MaxRevisitAge: 6 * time.Hour, ListingCount: 3}, 5*time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if err = q.Reconcile([]book.Demand{{Key: key, SnapshotTime: now, Ninja: resolve.Observation{Key: key, Source: resolve.SourceNinja, At: now}}}, nil); err != nil {
		t.Fatal(err)
	}
	for iteration := 1; iteration <= 2; iteration++ {
		if iteration == 2 {
			pass.Store(2)
			clock.Store(now.Add(6 * time.Hour).UnixNano())
		}
		outcome := b.Run(serviceCtx, func(context.Context) (*GateRequest, error) {
			d, ok := q.Take(g.now(), resolve.Rate{Chaos: 250})
			if !ok {
				t.Error("due revisit missing")
				return nil, nil
			}
			if d.Key != key {
				t.Errorf("wrong revisit: %+v", d.Key)
			}
			return &GateRequest{Gem: d.Key.Name, Variant: d.Key.Variant, RequestID: "evidence"}, nil
		}, func(_ *GateRequest, r *GateResponse, until time.Time) {
			if r.Error != nil {
				if err := q.Retry(key, until); err != nil {
					t.Error(err)
				}
				return
			}
			for _, o := range Observations(id, []TradeLookupResult{*r.Data}) {
				if err := q.Success(o); err != nil {
					t.Error(err)
				}
			}
		})
		want := "success"
		if iteration == 2 && second == "failure" {
			want = "failed"
		}
		if outcome != want {
			t.Fatalf("iteration %d outcome=%s want=%s", iteration, outcome, want)
		}
	}
	// Same identity in another league must remain a separate market/history.
	if err := repo.InsertTradeLookup(ctx, league.Historical(otherID), &TradeLookupResult{Gem: key.Name, Variant: key.Variant, FetchedAt: now, Total: 999, Listings: []TradeListingDetail{}}, "desktop-native"); err != nil {
		t.Fatal(err)
	}
	rows, err := pool.Query(ctx, `SELECT time,gem,variant,total_listings,listings,unique_accounts,divine_rate,source,price_floor,price_ceiling,median_top10 FROM trade_lookups WHERE league=$1 AND gem=$2 AND variant=$3 ORDER BY time`, id, key.Name, key.Variant)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	count := 0
	for rows.Next() {
		count++
		var stamp time.Time
		var gem, variant, source string
		var total, sellers int
		var raw []byte
		var rate, floor, ceiling, median float64
		if err := rows.Scan(&stamp, &gem, &variant, &total, &raw, &sellers, &rate, &source, &floor, &ceiling, &median); err != nil {
			t.Fatal(err)
		}
		var listings []TradeListingDetail
		if err := json.Unmarshal(raw, &listings); err != nil {
			t.Fatal(err)
		}
		expectedTime := now
		if count == 2 {
			expectedTime = now.Add(6 * time.Hour)
		}
		if !stamp.Equal(expectedTime) || gem != key.Name || variant != "20/20" || source != "server-gate" {
			t.Fatalf("market/provenance: %s %s %s %s", stamp, gem, variant, source)
		}
		if count == 2 && second == "empty" {
			if len(listings) != 0 || total != 0 || sellers != 0 || floor != 0 || ceiling != 0 || median != 0 || rate != 250 {
				t.Fatalf("empty became price evidence: %s total=%d sellers=%d rate=%g", raw, total, sellers, rate)
			}
			continue
		}
		wantRate := 200.0
		wantAmount := 10.0
		wantCurrency := "chaos"
		wantFloor := 10.0
		wantCeiling := 30.0
		wantMedian := 20.0
		if count == 2 {
			wantRate = 250
			wantAmount = 2
			wantCurrency = "divine"
			wantFloor = 30
			wantCeiling = 500
			wantMedian = 265
		}
		if len(listings) != 2 || total != 11*count || sellers != 2 || rate != wantRate || floor != wantFloor || ceiling != wantCeiling || median != wantMedian {
			t.Fatalf("row %d lost evidence: total=%d sellers=%d rate=%g prices=%g/%g/%g listings=%s", count, total, sellers, rate, floor, ceiling, median, raw)
		}
		l := listings[0]
		// BuildResult sorts by converted price, so find the distinctive native quote.
		for _, candidate := range listings {
			if candidate.Currency == wantCurrency && candidate.Price == wantAmount {
				l = candidate
				break
			}
		}
		if l.Price != wantAmount || l.Currency != wantCurrency || !l.IndexedAt.Equal(indexed) || l.GemLevel != 20 || l.GemQuality != 20 || l.Corrupted {
			t.Fatalf("native evidence lost: %+v", l)
		}
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	wantCount := 2
	if second == "failure" {
		wantCount = 1
	}
	if count != wantCount {
		t.Fatalf("retained %d rows want %d", count, wantCount)
	}
	if second == "failure" {
		if stats := q.Stats(now.Add(6*time.Hour + time.Minute)); stats.Overdue != 1 || stats.NeverChecked != 0 {
			t.Fatalf("failed fetch advanced check: %+v", stats)
		}
	}
}
