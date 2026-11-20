package lab

import (
	"context"
	"reflect"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"profitofexile/internal/league"
)

func TestSparklineFallbackFlightKey_EqualRequestsShareAndDistinctOnesDoNot(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := NewCache(scope).For(scope)
	keys := []CollectiveSparklineKey{{"Spark of Nova", "20/20"}, {"Arc of Surging", "1"}}
	base := sparklineFallbackFlightKey(cache, scope, "", keys, SparklineWindowHours)

	reordered := []CollectiveSparklineKey{{"Arc of Surging", "1"}, {"Spark of Nova", "20/20"}, {"Arc of Surging", "1"}}
	if got := sparklineFallbackFlightKey(cache, scope, "", reordered, SparklineWindowHours); got != base {
		t.Errorf("reordered, duplicated keys give flight %q, want the canonical %q", got, base)
	}
	other := league.Historical("Settlers")
	for name, got := range map[string]string{
		"league":       sparklineFallbackFlightKey(cache, other, "", keys, SparklineWindowHours),
		"cache":        sparklineFallbackFlightKey(NewCache(scope).For(scope), scope, "", keys, SparklineWindowHours),
		"SparkVariant": sparklineFallbackFlightKey(cache, scope, "20/20", keys, SparklineWindowHours),
		"window":       sparklineFallbackFlightKey(cache, scope, "", keys, 24),
		"keys":         sparklineFallbackFlightKey(cache, scope, "", keys[:1], SparklineWindowHours),
		"variant":      sparklineFallbackFlightKey(cache, scope, "", []CollectiveSparklineKey{{"Spark of Nova", "20"}, {"Arc of Surging", "1"}}, SparklineWindowHours),
	} {
		if got == base {
			t.Errorf("a different %s shares the flight %q", name, base)
		}
	}
}

// gatedBulkSource counts bulk reads and holds each until released.
type gatedBulkSource struct {
	fakeCollectiveSource
	reads   atomic.Int32
	release chan struct{}
	series  map[CollectiveSparklineKey][]SparklinePoint
}

func (s *gatedBulkSource) SparklineSeries(_ context.Context, _ league.Scope, _ []CollectiveSparklineKey, _ int) (map[CollectiveSparklineKey][]SparklinePoint, error) {
	s.reads.Add(1)
	<-s.release
	return s.series, nil
}

func TestQueryCollective_OverlappingColdPagesShareOneBulkRead(t *testing.T) {
	scope := league.Historical("Mirage")
	cache := NewCache(scope)
	point := []SparklinePoint{{Time: time.Now().UTC().Format(time.RFC3339), Price: 9, Listings: 4}}
	src := &gatedBulkSource{
		fakeCollectiveSource: fakeCollectiveSource{page: CollectivePage{Rows: []CollectiveResult{
			{TransfiguredName: "Spark of Nova", Variant: "20/20"},
			{TransfiguredName: "Spark of Nova", Variant: "1"},
		}, Total: 2}},
		release: make(chan struct{}),
		series:  map[CollectiveSparklineKey][]SparklinePoint{{"Spark of Nova", "20"}: point},
	}
	q := CollectiveQuery{Page: 1, PageSize: 10, Sort: CollectiveSortPrice, SparkVariant: "20"}
	want := map[CollectiveSparklineKey][]SparklinePoint{{"Spark of Nova", "20/20"}: point, {"Spark of Nova", "1"}: point}

	const n = 50
	var wg sync.WaitGroup
	got := make([]map[CollectiveSparklineKey][]SparklinePoint, n)
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			res, err := QueryCollective(context.Background(), src, cache, scope, q)
			if err != nil {
				t.Errorf("request %d: %v", i, err)
			}
			got[i] = res.Sparklines
		}(i)
	}
	for deadline := time.Now().Add(5 * time.Second); sparklineFallbackFlights.waiting() < n; {
		if time.Now().After(deadline) {
			t.Fatalf("%d of %d requests joined the flight", sparklineFallbackFlights.waiting(), n)
		}
		time.Sleep(time.Millisecond)
	}
	close(src.release)
	wg.Wait()

	if reads := src.reads.Load(); reads != 1 {
		t.Errorf("%d overlapping cold pages ran %d bulk reads, want 1", n, reads)
	}
	for i, g := range got {
		if !reflect.DeepEqual(g, want) {
			t.Fatalf("request %d sparklines = %v, want the SparkVariant series on each row's own key %v", i, g, want)
		}
	}

	src.release = make(chan struct{})
	close(src.release)
	if _, err := QueryCollective(context.Background(), src, cache, scope, q); err != nil {
		t.Fatalf("later request: %v", err)
	}
	if reads := src.reads.Load(); reads != 2 {
		t.Errorf("request after the flight completed ran %d bulk reads in total, want 2 (nothing retained)", reads)
	}
}
