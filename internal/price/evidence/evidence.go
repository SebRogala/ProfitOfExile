// Package evidence grades how much market evidence stands behind a price,
// reading only the depth of the source that produced it. It imports nothing
// outside the standard library.
package evidence

import "time"

// Evidence is the grade of the evidence behind one price.
type Evidence int

const (
	None Evidence = iota
	Thin
	Solid
)

// Source is the market that produced the price.
type Source int

const (
	SourceNone Source = iota
	SourceTrade
	SourceNinja
)

// Depth is the per-source market depth behind one price.
type Depth struct {
	PricedBy      Source
	TradeListings int
	// TradeSellers is lookup-level `unique_accounts` (interim, Desk poe403-seller-basis-1125).
	TradeSellers  int
	NinjaListings int
	NinjaSample   *int
	// Hold is the key's newest floor-SOLID point inside the hold window, when
	// the caller found one.
	Hold *Hold
}

// Hold is the last floor-SOLID evidence for a key that now fails the floor.
type Hold struct {
	LastSolidPrice float64
	LastSolidAge   time.Duration
}

// Policy is the evidence floor a price must reach to grade Solid.
type Policy struct {
	TradeListings    int
	TradeSellers     int
	NinjaSampleAbove int
	NinjaListings    int
	HoldWindow       time.Duration
	HoldBand         float64
}

// Grade grades the evidence behind a price under the given policy.
func Grade(d Depth, p Policy) Evidence {
	switch d.PricedBy {
	case SourceTrade:
		if d.TradeListings >= p.TradeListings && d.TradeSellers >= p.TradeSellers {
			return Solid
		}
	case SourceNinja:
		if d.NinjaSample != nil && *d.NinjaSample > p.NinjaSampleAbove && d.NinjaListings >= p.NinjaListings {
			return Solid
		}
	default:
		return None
	}
	return Thin
}
