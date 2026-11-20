package lab

import (
	"context"
	"fmt"
	"sort"
	"strings"

	"profitofexile/internal/league"
)

// SignalHistoryKey addresses one gem series of /api/analysis/history.
type SignalHistoryKey struct {
	Name    string
	Variant string
}

// SignalHistoryBulkSource reads several series' signal history at once, each
// with the single-series SignalHistory predicate, ordering and limit.
// *Repository implements it.
type SignalHistoryBulkSource interface {
	SignalHistoryBulk(ctx context.Context, scope league.Scope, keys []SignalHistoryKey, limit int) (map[SignalHistoryKey][]SignalChange, error)
}

// signalHistoryBulkFlights shares a cold bulk-history read among the requests
// that overlap it with an equal flight key. A read is held only while in
// flight: nothing is stored, so the next cold request reads again and the
// 14-day window is evaluated per load. The analyzer alone fills the rings.
var signalHistoryBulkFlights = newFontColdFills()

// signalHistoryBulkFlightKey identifies one shareable read: the cache and
// league it serves, the limit and the keys in canonical (sorted,
// de-duplicated) order. Each name and variant is length-prefixed, so names
// holding separator bytes cannot make two different key sets collide.
func signalHistoryBulkFlightKey(cache *Cache, scope league.Scope, keys []SignalHistoryKey, limit int) string {
	canonical := make([]string, 0, len(keys))
	seen := make(map[SignalHistoryKey]bool, len(keys))
	for _, k := range keys {
		if !seen[k] {
			seen[k] = true
			canonical = append(canonical, fmt.Sprintf("%d:%s%d:%s", len(k.Name), k.Name, len(k.Variant), k.Variant))
		}
	}
	sort.Strings(canonical)
	return fmt.Sprintf("%p\x1d%s\x1d%d\x1d%s", cache, scope.ID(), limit, strings.Join(canonical, ""))
}

// LoadSignalHistoryBulk answers a bulk-history request: from the analyzer's
// rings when the corpus is warm (truncated to limit, as the single route
// does), otherwise from one bulk read shared with every overlapping equal
// request. Keys without history are absent. cache may be nil.
func LoadSignalHistoryBulk(ctx context.Context, src SignalHistoryBulkSource, cache *Cache, scope league.Scope, keys []SignalHistoryKey, limit int) (map[SignalHistoryKey][]SignalChange, error) {
	var c *Cache
	if cache != nil {
		c = cache.For(scope)
	}
	if c != nil && c.HasSignalHistory() {
		out := make(map[SignalHistoryKey][]SignalChange, len(keys))
		for _, k := range keys {
			changes := c.SignalHistory(k.Name, k.Variant)
			if len(changes) > limit {
				changes = changes[:limit]
			}
			if len(changes) > 0 {
				out[k] = changes
			}
		}
		return out, nil
	}
	v, err := signalHistoryBulkFlights.load(ctx, signalHistoryBulkFlightKey(c, scope, keys, limit), func(ctx context.Context) (any, error) {
		return src.SignalHistoryBulk(ctx, scope, keys, limit)
	})
	if err != nil {
		return nil, err
	}
	history, _ := v.(map[SignalHistoryKey][]SignalChange)
	return history, nil
}
