package harvest

import (
	"math"
	"time"

	"profitofexile/internal/exchange"
)

// Lifeforce item ids and their Harvest colours.
const (
	LifeforcePrimal = "Metadata/Items/Currency/HarvestSeedBlue"
	LifeforceVivid  = "Metadata/Items/Currency/HarvestSeedGreen"
	LifeforceWild   = "Metadata/Items/Currency/HarvestSeedRed"
)

// ItemPrice is one item's prices over a window. Each field is nil when its
// market has no qualifying row; a missing price is never written as zero.
type ItemPrice struct {
	Chaos         *float64 `json:"chaos"`
	Divine        *float64 `json:"divine"`
	LastSeenChaos *float64 `json:"lastSeenChaos"`
}

// LifeforcePrice is one lifeforce colour's price. A colour is only present
// when both Chaos and PerDivine are known and PerDivine rounds above zero.
type LifeforcePrice struct {
	ItemID    string  `json:"itemId"`
	Colour    string  `json:"colour"`
	Chaos     float64 `json:"chaos"`
	PerDivine float64 `json:"perDivine"`
}

// Prices is the pooled-VWAP read over one window.
type Prices struct {
	// Items holds every requested id, even one with no rows at all.
	Items map[string]ItemPrice
	// Lifeforce is keyed "Primal", "Vivid" or "Wild".
	Lifeforce map[string]LifeforcePrice
}

type lifeforceSeed struct {
	key    string
	itemID string
	colour string
}

var lifeforceSeeds = []lifeforceSeed{
	{key: "Primal", itemID: LifeforcePrimal, colour: "blue"},
	{key: "Vivid", itemID: LifeforceVivid, colour: "yellow"},
	{key: "Wild", itemID: LifeforceWild, colour: "purple"},
}

// ComputePrices prices itemIDs and the three lifeforce colours over the
// windowHours clock hours ending at newest (inclusive), from rows in any order.
//
// Each price is exchange.PooledVWAP over the window; a row qualifies when both
// of its volumes are positive and price_valid is not consulted. lastSeenChaos
// is exchange.NewestHourVWAP before the window's first hour.
func ComputePrices(rows []exchange.StoredRow, newest time.Time, windowHours int, itemIDs []string) Prices {
	start := newest.Add(-time.Duration(windowHours-1) * time.Hour)
	inWindow := func(h time.Time) bool {
		return !h.Before(start) && !h.After(newest)
	}

	out := Prices{
		Items:     make(map[string]ItemPrice, len(itemIDs)),
		Lifeforce: make(map[string]LifeforcePrice, len(lifeforceSeeds)),
	}

	for _, id := range itemIDs {
		var ip ItemPrice
		if v, ok := exchange.PooledVWAP(rows, id, exchange.ChaosID, inWindow); ok {
			ip.Chaos = &v
		} else if v, ok := exchange.NewestHourVWAP(rows, id, exchange.ChaosID, start); ok {
			ip.LastSeenChaos = &v
		}
		if v, ok := exchange.PooledVWAP(rows, id, exchange.DivineID, inWindow); ok {
			ip.Divine = &v
		}
		out.Items[id] = ip
	}

	for _, seed := range lifeforceSeeds {
		chaos, ok := exchange.PooledVWAP(rows, seed.itemID, exchange.ChaosID, inWindow)
		if !ok {
			chaos, ok = exchange.NewestHourVWAP(rows, seed.itemID, exchange.ChaosID, start)
			if !ok {
				continue
			}
		}
		lfPerDivine, ok := exchange.PooledVWAP(rows, exchange.DivineID, seed.itemID, inWindow)
		if !ok {
			continue
		}
		perDivine := math.Round(lfPerDivine)
		if perDivine <= 0 {
			continue
		}
		out.Lifeforce[seed.key] = LifeforcePrice{
			ItemID:    seed.itemID,
			Colour:    seed.colour,
			Chaos:     chaos,
			PerDivine: perDivine,
		}
	}
	return out
}
