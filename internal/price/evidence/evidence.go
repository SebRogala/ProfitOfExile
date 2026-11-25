// Package evidence grades how much market evidence stands behind a price,
// reading only the depth of the source that produced it. It imports nothing
// outside the standard library.
package evidence

// Evidence is the grade of the evidence behind one price.
type Evidence int

const (
	None Evidence = iota
	Thin
	Solid
)

// unknown is a red-phase placeholder that matches no grade.
const unknown Evidence = -1

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
}

// Policy is the evidence floor a price must reach to grade Solid.
type Policy struct {
	TradeListings    int
	TradeSellers     int
	NinjaSampleAbove int
	NinjaListings    int
}

// Grade grades the evidence behind a price under the given policy.
func Grade(d Depth, p Policy) Evidence {
	return unknown
}
