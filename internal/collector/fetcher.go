// Package collector implements the price collection pipeline: fetch from external
// sources, store snapshots in TimescaleDB, and publish update events.
package collector

import (
	"time"
)

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
	IsCorrupted    bool
	GemColor       string // RED, GREEN, BLUE, WHITE, or "" if unresolved
}

// CurrencySnapshot represents a single currency price observation matching the
// currency_snapshots hypertable columns. The currency_snapshots table also has a
// volume column; this struct omits it because poe.ninja does not provide volume
// data.
type CurrencySnapshot struct {
	Time            time.Time
	CurrencyID      string
	Chaos           float64
	SparklineChange float64
}

// FragmentSnapshot represents a single fragment price observation matching the
// fragment_snapshots hypertable columns. Same exchange endpoint structure as
// currency but stored separately for cleaner analysis queries.
type FragmentSnapshot struct {
	Time            time.Time
	FragmentID      string
	Chaos           float64
	SparklineChange float64
}

// ItemSnapshot represents a single price observation from one of poe.ninja's
// item-overview categories (IncursionTemple, Vial and the five unique slots),
// matching the item_snapshots hypertable columns. Category holds poe.ninja's
// own type name verbatim, and NinjaID its numeric line id — the identity the
// table is keyed on, because a category's lines are not unique by name, variant
// and links (see the migration comment). The Time field is populated when
// reading from the database; on insert the repository uses a separate snapTime
// parameter so every row in a batch shares one timestamp.
type ItemSnapshot struct {
	Time            time.Time
	Category        string
	NinjaID         int64
	DetailsID       string
	Name            string
	Variant         string // "" when the line carries no variant
	Links           int    // 0 when the line carries no link count
	Chaos           float64
	Divine          float64
	Exalted         float64
	Listings        int // poe.ninja's listingCount: total listings
	SampleCount     int // poe.ninja's count: listings sampled for the price
	StackSize       int // 0 on categories that do not stack (only Vial carries it)
	Icon            string
	ItemClass       int
	ItemType        string // "" when the line carries no item type
	BaseType        string
	LevelRequired   int // 0 when the line carries no level requirement
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
