package lab

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
	"strings"

	"profitofexile/internal/league"
)

// CollectiveSparklineSeriesSource is the bulk read the cold sparkline fallback
// prefers: every requested (name, variant) series in one statement.
// *Repository implements it; a CollectiveSource without it is read once per
// variant through SparklineData.
type CollectiveSparklineSeriesSource interface {
	SparklineSeries(ctx context.Context, scope league.Scope, keys []CollectiveSparklineKey, hours int) (map[CollectiveSparklineKey][]SparklinePoint, error)
}

// sparklineFallbackFlights shares a cold page's sparkline read among the
// requests that overlap it with an equal flight key. A read is held only while
// in flight: nothing is stored, so the next cold request reads again and the
// SQL window is evaluated per load. The analyzer alone fills the cache's
// sparkline corpus.
var sparklineFallbackFlights = newFontColdFills()

// sparklineFallbackFlightKey identifies one shareable read: the cache and
// league it serves, the non-corrupted market, the raw SparkVariant, the window
// and the requested series in canonical (sorted, de-duplicated) order.
func sparklineFallbackFlightKey(cache *Cache, scope league.Scope, sparkVariant string, keys []CollectiveSparklineKey, hours int) string {
	canonical := make([]string, 0, len(keys))
	seen := make(map[CollectiveSparklineKey]bool, len(keys))
	for _, k := range keys {
		if !seen[k] {
			seen[k] = true
			canonical = append(canonical, k.Name+"\x1f"+k.Variant)
		}
	}
	sort.Strings(canonical)
	return fmt.Sprintf("%p\x1d%s\x1dcorrupted=false\x1d%q\x1d%d\x1d%s", cache, scope.ID(), sparkVariant, hours, strings.Join(canonical, "\x1e"))
}

// loadCollectiveSparklineFallback decorates a cold page: the series each row
// shows are keyed by the row's own variant but read from SparkVariant when
// one is set. cache may be nil; the read is then unshared.
func loadCollectiveSparklineFallback(ctx context.Context, src CollectiveSource, cache *Cache, scope league.Scope, sparkVariant string, results []CollectiveResult) map[CollectiveSparklineKey][]SparklinePoint {
	queryKey := func(r CollectiveResult) CollectiveSparklineKey {
		if sparkVariant != "" {
			return CollectiveSparklineKey{Name: r.TransfiguredName, Variant: sparkVariant}
		}
		return CollectiveSparklineKey{Name: r.TransfiguredName, Variant: r.Variant}
	}
	keys := make([]CollectiveSparklineKey, 0, len(results))
	for _, r := range results {
		keys = append(keys, queryKey(r))
	}
	read := func(ctx context.Context) (any, error) {
		return readSparklineSeries(ctx, src, scope, keys)
	}

	var v any
	var err error
	if cache == nil {
		v, err = read(ctx)
	} else {
		c := cache.For(scope)
		v, err = sparklineFallbackFlights.load(ctx, sparklineFallbackFlightKey(c, scope, sparkVariant, keys, SparklineWindowHours), read)
	}
	sparklines := make(map[CollectiveSparklineKey][]SparklinePoint)
	if err != nil {
		slog.Error("collective analysis: sparkline query failed", "error", err)
		return sparklines
	}
	series, _ := v.(map[CollectiveSparklineKey][]SparklinePoint)
	for _, r := range results {
		if points, ok := series[queryKey(r)]; ok {
			sparklines[CollectiveSparklineKey{Name: r.TransfiguredName, Variant: r.Variant}] = points
		}
	}
	return sparklines
}

func readSparklineSeries(ctx context.Context, src CollectiveSource, scope league.Scope, keys []CollectiveSparklineKey) (map[CollectiveSparklineKey][]SparklinePoint, error) {
	if bulk, ok := src.(CollectiveSparklineSeriesSource); ok {
		return bulk.SparklineSeries(ctx, scope, keys, SparklineWindowHours)
	}
	byVariant := make(map[string][]string)
	for _, k := range keys {
		byVariant[k.Variant] = append(byVariant[k.Variant], k.Name)
	}
	out := make(map[CollectiveSparklineKey][]SparklinePoint)
	for variant, names := range byVariant {
		points, err := src.SparklineData(ctx, scope, names, variant, SparklineWindowHours)
		if err != nil {
			slog.Error("collective analysis: sparkline query failed", "variant", variant, "error", err)
			continue
		}
		for name, series := range points {
			out[CollectiveSparklineKey{Name: name, Variant: variant}] = series
		}
	}
	return out, nil
}
