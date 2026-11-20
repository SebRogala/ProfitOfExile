package lab

import (
	"context"

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

// signalHistoryBulkFlights shares a cold bulk-history read among overlapping
// requests with an equal flight key.
var signalHistoryBulkFlights = newFontColdFills()

// LoadSignalHistoryBulk answers a bulk-history request.
func LoadSignalHistoryBulk(ctx context.Context, src SignalHistoryBulkSource, cache *Cache, scope league.Scope, keys []SignalHistoryKey, limit int) (map[SignalHistoryKey][]SignalChange, error) {
	return src.SignalHistoryBulk(ctx, scope, keys, limit)
}
