package harvest

import (
	"math"

	"profitofexile/internal/price/tier"
)

// FamilyTiers is one family's tiers in the desktop HarvestTiers shape
// (desktop/src/lib/harvest/seam.ts), less priceHour.
type FamilyTiers struct {
	BoundariesChaos  []float64         `json:"boundariesChaos"`
	ByItem           map[string]string `json:"byItem"`
	Names            []string          `json:"names"`
	TopBoundaryChaos *float64          `json:"topBoundaryChaos"`
}

// tierPolicy has no cutoff: sub-5c oils and catalysts are tiered so they can be
// fed (P49), and depth is unknown so the tiers are price-only (C3).
var tierPolicy = tier.Policy{DepthUnknown: true}

// ClassifyFamilies classifies each family of prices ({family: {itemId: chaos}})
// as its own group and returns the tiers keyed by family. A family with no
// items is absent from the result.
func ClassifyFamilies(prices map[string]map[string]float64) map[string]FamilyTiers {
	out := make(map[string]FamilyTiers, len(prices))
	for fam, items := range prices {
		if len(items) == 0 {
			continue
		}
		in := make([]tier.Item, 0, len(items))
		for id, chaos := range items {
			in = append(in, tier.Item{Group: fam, Key: id, Price: chaos})
		}
		res := tier.Classify(in, tierPolicy)

		byItem := make(map[string]string, len(res.Items))
		for k, c := range res.Items {
			byItem[k.Key] = c.Tier
		}
		b := res.Boundaries[fam]
		rounded := make([]float64, len(b.Boundaries))
		for i, x := range b.Boundaries {
			rounded[i] = math.Round(x*100) / 100
		}
		var top *float64
		if v, ok := res.TopBoundary[fam]; ok {
			top = &v
		}
		out[fam] = FamilyTiers{
			BoundariesChaos:  rounded,
			ByItem:           byItem,
			Names:            b.Names,
			TopBoundaryChaos: top,
		}
	}
	return out
}
