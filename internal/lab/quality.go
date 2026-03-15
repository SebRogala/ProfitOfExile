package lab

import (
	"strings"
	"time"
)

// QualityResult holds the computed ROI for a quality-roll gem analysis.
type QualityResult struct {
	Time       time.Time
	Name       string
	Level      int     // 1 or 20
	BuyPrice   float64 // 0% quality price
	PriceQ20   float64 // 20% quality price
	ROI4       float64
	ROI6       float64
	ROI10      float64
	ROI15      float64
	ROI20      float64 // full quality — no GCP needed (Uber/Gift labs)
	AvgROI     float64
	GCPPrice   float64
	Listings0  int
	Listings20 int
	GemColor   string
	Confidence string // "OK" or "LOW"
}

// qualityTiers are the Merciless Lab quality roll amounts.
var qualityTiers = []int{4, 6, 10, 15}

// AnalyzeQuality computes quality-roll ROI for all gems that have both
// 0% quality (variant "1" or "20") and 20% quality (variant "1/20" or "20/20") prices.
func AnalyzeQuality(snapTime time.Time, gems []GemPrice, gcpPrice float64) []QualityResult {
	type priceEntry struct {
		chaos    float64
		listings int
		color    string
	}

	// Index: name → variant → priceEntry (transfigured gems included)
	index := make(map[string]map[string]priceEntry)

	for _, g := range gems {
		if g.IsCorrupted {
			continue
		}
		if strings.Contains(g.Name, "Trarthus") {
			continue
		}

		if index[g.Name] == nil {
			index[g.Name] = make(map[string]priceEntry)
		}
		index[g.Name][g.Variant] = priceEntry{
			chaos:    g.Chaos,
			listings: g.Listings,
			color:    g.GemColor,
		}
	}

	// Variant pairs: 0% quality → 20% quality
	pairs := []struct {
		zeroVar string
		qualVar string
		level   int
	}{
		{"1", "1/20", 1},
		{"20", "20/20", 20},
	}

	var results []QualityResult

	for name, variants := range index {
		for _, pair := range pairs {
			zeroQ, hasZero := variants[pair.zeroVar]
			fullQ, hasFull := variants[pair.qualVar]
			if !hasZero || !hasFull {
				continue
			}

			// Compute ROI for each quality tier.
			// Sell price at N% quality = price_at_20pct - (20-N) * gcpPrice
			// ROI = sell_price - buy_price (buy at 0% quality)
			rois := make([]float64, len(qualityTiers))
			for i, tier := range qualityTiers {
				sellPrice := fullQ.chaos - float64(20-tier)*gcpPrice
				rois[i] = sellPrice - zeroQ.chaos
			}

			// ROI at full quality (20%): no GCP cost, just price difference.
			roi20 := fullQ.chaos - zeroQ.chaos

			// Filter: skip if roi15 <= 0
			if rois[3] <= 0 {
				continue
			}

			var roiSum float64
		for _, r := range rois {
			roiSum += r
		}
		avgROI := roiSum / float64(len(qualityTiers))

			confidence := "LOW"
			if zeroQ.listings >= 5 && fullQ.listings >= 5 {
				confidence = "OK"
			}

			color := zeroQ.color
			if fullQ.color != "" {
				color = fullQ.color
			}

			results = append(results, QualityResult{
				Time:       snapTime,
				Name:       name,
				Level:      pair.level,
				BuyPrice:   zeroQ.chaos,
				PriceQ20:   fullQ.chaos,
				ROI4:       rois[0],
				ROI6:       rois[1],
				ROI10:      rois[2],
				ROI15:      rois[3],
				ROI20:      roi20,
				AvgROI:     avgROI,
				GCPPrice:   gcpPrice,
				Listings0:  zeroQ.listings,
				Listings20: fullQ.listings,
				GemColor:   color,
				Confidence: confidence,
			})
		}
	}

	return results
}
