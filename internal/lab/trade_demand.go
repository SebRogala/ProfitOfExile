package lab

import (
	"math"
	"slices"
	"sort"
	"time"

	"profitofexile/internal/league"
	"profitofexile/internal/price/book"
	"profitofexile/internal/price/resolve"
	"profitofexile/internal/price/tier"
)

// BuildTradeDemand applies the demand-eligibility rule in docs/TRADE-LIFECYCLE.md.
func BuildTradeDemand(scope league.Scope, snapshotTime time.Time, gems []GemPrice) []book.Demand {
	variants := FontTradeVariants()
	rows := make(map[resolve.Key]GemPrice)
	pools := make(map[string]map[string]struct{})
	for _, g := range gems {
		key := resolve.GemKey(scope.ID(), g.Name, g.Variant)
		if !isFontOutcome(g) || key.Corrupted || !slices.Contains(variants, key.Variant) {
			continue
		}
		rows[key] = g
		if hasAttributeColor(g) {
			if pools[g.GemColor] == nil {
				pools[g.GemColor] = make(map[string]struct{})
			}
			pools[g.GemColor][g.Name] = struct{}{}
		}
	}

	probabilities := make(map[resolve.Key]float64)
	for color, names := range pools {
		for _, variant := range variants {
			keys := make([]resolve.Key, 0, len(names))
			prices := make(map[resolve.Key]float64, len(names))
			for name := range names {
				key := resolve.GemKey(scope.ID(), name, variant)
				keys = append(keys, key)
				if g, ok := rows[key]; ok && g.GemColor == color {
					if g.Chaos > 0 && !math.IsNaN(g.Chaos) && !math.IsInf(g.Chaos, 0) {
						prices[key] = g.Chaos
					}
				}
			}
			sort.Slice(keys, func(i, j int) bool {
				if prices[keys[i]] != prices[keys[j]] {
					return prices[keys[i]] > prices[keys[j]]
				}
				return keys[i].String() < keys[j].String()
			})
			for rank, key := range keys {
				if g, ok := rows[key]; ok && g.GemColor == color {
					probabilities[key] = fontPickProbability(rank, len(keys))
				}
			}
		}
	}

	// Scheduling exclusions must not shrink the Font's actual draw pool.
	classificationGems := slices.Clone(gems)
	for i := range classificationGems {
		price := classificationGems[i].Chaos
		if price < 0 || math.IsNaN(price) || math.IsInf(price, 0) {
			// Invalid source amounts cannot participate in tier boundary arithmetic.
			classificationGems[i].Chaos = 0
		}
	}
	classification := ComputeGemClassification(classificationGems)
	demand := make([]book.Demand, 0, len(rows))
	for key, g := range rows {
		classified := classification.Gems[GemClassificationKey{Name: g.Name, Variant: g.Variant}]
		if classified.Tier == tier.Floor && !classified.LowConfidence {
			continue
		}
		probability := probabilities[key]
		contribution := 0.0
		if g.Chaos > 0 && !math.IsNaN(g.Chaos) && !math.IsInf(g.Chaos, 0) {
			contribution = g.Chaos * probability
		}
		demand = append(demand, book.Demand{
			Key: key, SnapshotTime: snapshotTime,
			Ninja: resolve.Observation{
				Key: key, Source: resolve.SourceNinja, At: snapshotTime,
				Prices: []resolve.Money{{Amount: g.Chaos, Currency: "chaos"}}, SampleCount: g.SampleCount,
			},
			PotentialContribution: contribution, PickProbability: probability,
		})
	}
	sort.Slice(demand, func(i, j int) bool { return demand[i].Key.String() < demand[j].Key.String() })
	return demand
}
