package lab

import (
	"math"
	"strings"
	"time"
)

// FontResult holds the computed EV for a single (color, variant) Font of Divine Skill analysis.
type FontResult struct {
	Time      time.Time
	Color     string  // RED, GREEN, BLUE
	Variant   string  // "1", "1/20", "20", "20/20"
	Pool      int     // total unique transfigured gem names of that color
	Winners   int
	PWin      float64
	AvgWin    float64
	EV        float64
	InputCost float64
	Profit    float64
	Threshold float64
}

// DefaultInputCosts maps gem variant to estimated input cost in chaos.
var DefaultInputCosts = map[string]float64{
	"1":     0.5,
	"1/20":  1.5,
	"20":    2.0,
	"20/20": 3.5,
}

// pWin3Picks computes the probability of getting at least one winner
// when drawing 3 gems without replacement from a pool.
func pWin3Picks(winners, total int) float64 {
	if total < 3 {
		if winners > 0 {
			return 1.0
		}
		return 0.0
	}
	losers := total - winners
	return 1.0 - (float64(losers)/float64(total))*
		(float64(losers-1)/float64(total-1))*
		(float64(losers-2)/float64(total-2))
}

// AnalyzeFont computes Font of Divine Skill EV per (color, variant).
// Pool size = count of distinct transfigured gem NAMES per color (across all variants).
// Winners and EV use variant-specific prices.
func AnalyzeFont(snapTime time.Time, gems []GemPrice) []FontResult {
	// Step 1: Build pool sizes — unique transfigured gem names per color (all variants).
	poolNames := map[string]map[string]struct{}{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	// Also index variant-specific prices for winner calculation.
	type priceEntry struct {
		chaos    float64
		listings int
	}
	// variantGems[color][variant] = []priceEntry
	type colorVariantGems map[string][]priceEntry
	byColor := map[string]colorVariantGems{
		"RED":   {},
		"GREEN": {},
		"BLUE":  {},
	}

	for _, g := range gems {
		if g.IsCorrupted {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}
		if !g.IsTransfigured {
			continue
		}
		color := g.GemColor
		if color != "RED" && color != "GREEN" && color != "BLUE" {
			continue
		}

		poolNames[color][g.Name] = struct{}{}

		if byColor[color][g.Variant] == nil {
			byColor[color][g.Variant] = nil
		}
		byColor[color][g.Variant] = append(byColor[color][g.Variant], priceEntry{
			chaos:    g.Chaos,
			listings: g.Listings,
		})
	}

	variants := []string{"1", "1/20", "20", "20/20"}
	colors := []string{"RED", "GREEN", "BLUE"}
	var results []FontResult

	for _, color := range colors {
		pool := len(poolNames[color])
		if pool == 0 {
			continue
		}

		for _, variant := range variants {
			inputCost := DefaultInputCosts[variant]
			threshold := math.Max(math.Ceil(inputCost*3), 5)

			entries := byColor[color][variant]
			var winnerCount int
			var winnerSum float64
			for _, e := range entries {
				if e.chaos >= threshold && e.listings >= 5 {
					winnerCount++
					winnerSum += e.chaos
				}
			}

			var avgWin float64
			if winnerCount > 0 {
				avgWin = winnerSum / float64(winnerCount)
			}

			pWin := pWin3Picks(winnerCount, pool)
			ev := pWin * avgWin
			profit := ev - inputCost

			results = append(results, FontResult{
				Time:      snapTime,
				Color:     color,
				Variant:   variant,
				Pool:      pool,
				Winners:   winnerCount,
				PWin:      pWin,
				AvgWin:    avgWin,
				EV:        ev,
				InputCost: inputCost,
				Profit:    profit,
				Threshold: threshold,
			})
		}
	}

	return results
}
