package exchange

import (
	"math"
	"math/big"
)

// executionRun owns run sizing for both storage and display. Sell proceeds
// constrain the lot; the entry is paid once per lot in whole quote units.
func executionRun(play Play) (flips, investment float64) {
	lot, cost := executionLot(play)
	if lot == nil {
		return math.Inf(1), math.Inf(1)
	}
	units, _ := new(big.Float).SetInt(lot).Float64()
	lots := 1.0
	if play.ExpectedRoi > 0 && !math.IsInf(play.ExpectedRoi, 0) {
		lots = math.Max(1, math.Ceil(math.Ceil(ScaleTargetChaos/play.ExpectedRoi)/units))
	}
	return lots * units, lots * cost
}

func executionLot(play Play) (*big.Int, float64) {
	if len(play.Legs) < 2 {
		return nil, math.Inf(1)
	}
	entry := executionPrice(play.Legs[0], true)
	if entry == nil {
		return nil, math.Inf(1)
	}
	lot := big.NewInt(1)
	proceeds := big.NewRat(1, 1)
	for _, leg := range play.Legs[1:] {
		price := executionPrice(leg, false)
		if price == nil {
			return nil, math.Inf(1)
		}
		proceeds.Mul(proceeds, price)
		denominator := proceeds.Denom()
		gcd := new(big.Int).GCD(nil, nil, lot, denominator)
		lot.Mul(new(big.Int).Quo(lot, gcd), denominator)
	}

	// Investment already embeds the scored hour's quote valuation; fixture
	// plays can lack quote IDs, and a later result rate can describe another hour.
	rawEntry := play.Legs[0].Price * (1 + play.Legs[0].Tick)
	rate := play.Investment / rawEntry
	if rawEntry <= 0 || math.IsNaN(rawEntry) || math.IsInf(rawEntry, 0) || rate <= 0 || math.IsNaN(rate) || math.IsInf(rate, 0) {
		return nil, math.Inf(1)
	}
	if lot.Cmp(big.NewInt(1)) == 0 && entry.IsInt() {
		return lot, play.Investment
	}
	quote := new(big.Rat).Mul(entry, new(big.Rat).SetInt(lot))
	whole, remainder := new(big.Int), new(big.Int)
	whole.QuoRem(quote.Num(), quote.Denom(), remainder)
	if remainder.Sign() > 0 {
		whole.Add(whole, big.NewInt(1))
	}
	quoteCost, _ := new(big.Float).SetInt(whole).Float64()
	return lot, quoteCost * rate
}

func executionPrice(leg Leg, buy bool) *big.Rat {
	if leg.PriceItemQty <= 0 || leg.PriceQuoteQty <= 0 || leg.Tick < 0 || math.IsNaN(leg.Tick) || math.IsInf(leg.Tick, 0) {
		return nil
	}
	price := new(big.Rat).SetFrac(big.NewInt(leg.PriceQuoteQty), big.NewInt(leg.PriceItemQty))
	if leg.Tick == 0 {
		return price
	}
	n := new(big.Rat).SetFloat64(math.Round(1 / leg.Tick))
	if n == nil || n.Sign() <= 0 {
		return nil
	}
	factor := new(big.Rat).Set(n)
	if buy {
		factor.Add(factor, big.NewRat(1, 1))
	} else {
		factor.Sub(factor, big.NewRat(1, 1))
	}
	if factor.Sign() <= 0 {
		return nil
	}
	return price.Mul(price, factor.Quo(factor, n))
}
