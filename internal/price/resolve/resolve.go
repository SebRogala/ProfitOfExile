// Package resolve turns stored price observations into one observed price per
// key, with its source, age and the reasons a reader should know about. It is a
// stdlib-only leaf: callers own the policy values and the divine rate.
package resolve

import (
	"math"
	"sort"
	"time"
)

// Money is a native listing amount in the currency it was listed in.
type Money struct {
	Amount   float64
	Currency string
}

// Source names where an observation came from.
type Source string

const (
	SourceTrade Source = "trade"
	SourceNinja Source = "ninja"
)

// Observation is one source's evidence for a key at a moment. Prices hold
// native amounts only; never a converted chaos value (L13).
type Observation struct {
	Key         Key
	Source      Source
	At          time.Time
	Prices      []Money
	SampleCount *int
}

// Reason flags a fact about a resolved price.
type Reason string

const (
	ReasonDisagreement  Reason = "disagreement"
	ReasonNeverChecked  Reason = "never-checked"
	ReasonNoDivineRate  Reason = "no-divine-rate"
	ReasonStaleEvidence Reason = "stale-evidence"
	ReasonThinSample    Reason = "thin-sample"
)

// Policy holds caller-owned thresholds. MaxRevisitAge 0 disables the stale flag.
type Policy struct {
	ListingCount     int
	Disagreement     float64
	ThinDisagreement float64
	ThinSample       int
	MaxRevisitAge    time.Duration
}

// Rate is the divine→chaos answer a resolve uses.
type Rate struct {
	Chaos  float64
	Source string
	At     time.Time
}

// Resolved is the one observed price for a key.
type Resolved struct {
	Key        Key
	Source     Source
	Chaos      float64
	ObservedAt time.Time
	Age        time.Duration
	Reasons    []Reason
}

// ChaosOf converts a native amount to chaos. Divine converts at the rate,
// rounded to one decimal like trade.NormalizeToChaos so the shared vectors
// agree; divine without a positive rate and any other currency are not
// convertible.
func ChaosOf(m Money, divineRate float64) (float64, bool) {
	switch {
	case m.Currency == "chaos":
		return m.Amount, true
	case m.Currency == "divine" && divineRate > 0:
		return math.Round(m.Amount*divineRate*10) / 10, true
	default:
		return 0, false
	}
}

// MedianCheapest is the trade price definition (docs/TRADE-LIFECYCLE.md):
// the median of the n cheapest convertible prices, an even count averaging the
// two middles rounded to two decimals. It reports false when none convert.
func MedianCheapest(prices []Money, n int, divineRate float64) (float64, bool) {
	converted := make([]float64, 0, len(prices))
	for _, m := range prices {
		if c, ok := ChaosOf(m, divineRate); ok {
			converted = append(converted, c)
		}
	}
	if n <= 0 || len(converted) == 0 {
		return 0, false
	}
	sort.SliceStable(converted, func(i, j int) bool { return converted[i] < converted[j] })
	cheapest := converted[:min(n, len(converted))]
	k := len(cheapest)
	if k%2 == 0 {
		return math.Round(((cheapest[k/2-1]+cheapest[k/2])/2)*100) / 100, true
	}
	return cheapest[k/2], true
}

// Resolve picks the one observed price for a key as of asOf. A trade price
// with a convertible listing always wins, however old (L10); poe.ninja prices
// only what trade cannot. The price is never scaled by age or confidence (L9).
func Resolve(obs []Observation, asOf time.Time, rate Rate, p Policy) (Resolved, bool) {
	trade, hasTrade := newest(obs, SourceTrade, asOf)
	ninja, hasNinja := newest(obs, SourceNinja, asOf)

	var ninjaChaos float64
	ninjaOK := false
	if hasNinja {
		ninjaChaos, ninjaOK = MedianCheapest(ninja.Prices, p.ListingCount, rate.Chaos)
	}

	if hasTrade {
		if price, ok := MedianCheapest(trade.Prices, p.ListingCount, rate.Chaos); ok {
			var reasons []Reason
			if ninjaOK && disagrees(price, ninjaChaos, thresholdFor(ninja, p)) {
				reasons = append(reasons, ReasonDisagreement)
			}
			if p.MaxRevisitAge > 0 && asOf.Sub(trade.At) > p.MaxRevisitAge {
				reasons = append(reasons, ReasonStaleEvidence)
			}
			return resolved(trade, price, asOf, reasons), true
		}
	}

	if !ninjaOK {
		return Resolved{}, false
	}
	var reasons []Reason
	if hasTrade {
		reasons = append(reasons, ReasonNoDivineRate)
	} else {
		reasons = append(reasons, ReasonNeverChecked)
	}
	if isThin(ninja, p) {
		reasons = append(reasons, ReasonThinSample)
	}
	return resolved(ninja, ninjaChaos, asOf, reasons), true
}

func newest(obs []Observation, source Source, asOf time.Time) (Observation, bool) {
	var best Observation
	found := false
	for _, o := range obs {
		if o.Source != source || o.At.After(asOf) {
			continue
		}
		if !found || o.At.After(best.At) {
			best, found = o, true
		}
	}
	return best, found
}

func isThin(o Observation, p Policy) bool {
	return o.SampleCount != nil && *o.SampleCount <= p.ThinSample
}

func thresholdFor(ninja Observation, p Policy) float64 {
	if isThin(ninja, p) {
		return p.ThinDisagreement
	}
	return p.Disagreement
}

func disagrees(a, b, k float64) bool {
	lo, hi := math.Min(a, b), math.Max(a, b)
	if lo <= 0 {
		return false
	}
	return hi/lo > k
}

func resolved(o Observation, price float64, asOf time.Time, reasons []Reason) Resolved {
	sort.Slice(reasons, func(i, j int) bool { return reasons[i] < reasons[j] })
	return Resolved{
		Key:        o.Key,
		Source:     o.Source,
		Chaos:      price,
		ObservedAt: o.At,
		Age:        asOf.Sub(o.At),
		Reasons:    reasons,
	}
}
