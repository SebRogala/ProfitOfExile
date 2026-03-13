// Package collector implements the price collection pipeline: fetch from external
// sources, store snapshots in TimescaleDB, and publish update events.
package collector

import (
	"context"
	"time"
)

// Fetcher abstracts an external price data source (poe.ninja, TFT, etc.).
// Each implementation handles its own API format and returns normalised domain types.
type Fetcher interface {
	FetchGems(ctx context.Context, league string) ([]GemSnapshot, error)
	FetchCurrency(ctx context.Context, league string) ([]CurrencySnapshot, error)
}

// GemSnapshot represents a single gem price observation matching the gem_snapshots
// hypertable columns. The Time field is populated when reading from the database;
// on insert, the repository uses a separate snapTime parameter to ensure all rows
// in a batch share the same timestamp (Time is ignored on writes).
type GemSnapshot struct {
	Time           time.Time
	Name           string
	Variant        string
	Chaos          float64
	Listings       int
	IsTransfigured bool
	GemColor       string // RED, GREEN, BLUE, WHITE, or "" if unresolved
}

// CurrencySnapshot represents a single currency price observation matching the
// currency_snapshots hypertable columns. The currency_snapshots table also has a
// volume column; this struct omits it because poe.ninja does not provide volume
// data. Future fetchers (e.g. TFT) may need to extend the struct if volume
// becomes available.
type CurrencySnapshot struct {
	Time            time.Time
	CurrencyID      string
	Chaos           float64
	SparklineChange float64
}

// SnapshotSummary provides a quick overview of the latest stored snapshot,
// used by debug endpoints.
type SnapshotSummary struct {
	LastGemTime      time.Time
	GemCount         int
	LastCurrencyTime time.Time
	CurrencyCount    int
}
