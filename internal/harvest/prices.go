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
	// DivineChaosRate is chaos per divine, or 0 when no divine/chaos row exists.
	DivineChaosRate float64
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
// Each price is the pooled VWAP over every qualifying row of the market: the
// quote-side volume summed over the X-side volume summed. A row qualifies when
// both of its volumes are positive; price_valid is not consulted.
func ComputePrices(rows []exchange.StoredRow, newest time.Time, windowHours int, itemIDs []string) Prices {
	start := newest.Add(-time.Duration(windowHours-1) * time.Hour)
	inWindow := func(h time.Time) bool {
		return !h.Before(start) && !h.After(newest)
	}

	out := Prices{
		Items:     make(map[string]ItemPrice, len(itemIDs)),
		Lifeforce: make(map[string]LifeforcePrice, len(lifeforceSeeds)),
	}
	if rate, ok := vwap(rows, exchange.DivineID, exchange.ChaosID, inWindow); ok {
		out.DivineChaosRate = rate
	}

	for _, id := range itemIDs {
		var ip ItemPrice
		if v, ok := vwap(rows, id, exchange.ChaosID, inWindow); ok {
			ip.Chaos = &v
		} else {
			ip.LastSeenChaos = lastSeenChaos(rows, id, start)
		}
		if v, ok := vwap(rows, id, exchange.DivineID, inWindow); ok {
			ip.Divine = &v
		}
		out.Items[id] = ip
	}

	for _, seed := range lifeforceSeeds {
		chaos, ok := vwap(rows, seed.itemID, exchange.ChaosID, inWindow)
		if !ok {
			last := lastSeenChaos(rows, seed.itemID, start)
			if last == nil {
				continue
			}
			chaos, ok = *last, true
		}
		lfSum, divSum, ok := pooled(rows, seed.itemID, exchange.DivineID, inWindow)
		if !ok {
			continue
		}
		perDivine := math.Round(float64(lfSum) / float64(divSum))
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

// pooled sums the x-side and quote-side volumes over every qualifying row of
// the {x, q} market whose hour is inWindow. Volumes are int64, so the sums are
// exact and independent of row order.
func pooled(rows []exchange.StoredRow, x, q string, inWindow func(time.Time) bool) (xSum, qSum int64, ok bool) {
	for _, r := range rows {
		if !inWindow(r.Hour) {
			continue
		}
		xv, qv, qualifies := orient(r, x, q)
		if !qualifies {
			continue
		}
		xSum += xv
		qSum += qv
		ok = true
	}
	return xSum, qSum, ok
}

// orient returns the x-side and quote-side volumes of a row on the {x, q}
// market. It reports false when the row is another market or when either
// volume is not positive.
func orient(r exchange.StoredRow, x, q string) (xv, qv int64, ok bool) {
	switch {
	case r.ItemA == x && r.ItemB == q:
		xv, qv = r.VolumeA, r.VolumeB
	case r.ItemA == q && r.ItemB == x:
		xv, qv = r.VolumeB, r.VolumeA
	default:
		return 0, 0, false
	}
	if xv <= 0 || qv <= 0 {
		return 0, 0, false
	}
	return xv, qv, true
}

// vwap is the quote-side units per x: the pooled quote volume over the pooled
// x volume.
func vwap(rows []exchange.StoredRow, x, q string, inWindow func(time.Time) bool) (float64, bool) {
	xSum, qSum, ok := pooled(rows, x, q, inWindow)
	if !ok {
		return 0, false
	}
	return float64(qSum) / float64(xSum), true
}

// lastSeenChaos is the chaos VWAP of the newest hour before start that has a
// qualifying x/chaos row, pooling that one hour only. It returns nil when no
// such hour exists.
func lastSeenChaos(rows []exchange.StoredRow, x string, start time.Time) *float64 {
	var newestHour time.Time
	found := false
	for _, r := range rows {
		if !r.Hour.Before(start) {
			continue
		}
		if _, _, ok := orient(r, x, exchange.ChaosID); !ok {
			continue
		}
		if !found || r.Hour.After(newestHour) {
			newestHour = r.Hour
			found = true
		}
	}
	if !found {
		return nil
	}
	v, ok := vwap(rows, x, exchange.ChaosID, func(h time.Time) bool { return h.Equal(newestHour) })
	if !ok {
		return nil
	}
	return &v
}
