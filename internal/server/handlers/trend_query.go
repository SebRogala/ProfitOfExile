package handlers

import (
	"time"

	"profitofexile/internal/lab"
)

type trendRow struct {
	Time              string  `json:"time"`
	Name              string  `json:"name"`
	Variant           string  `json:"variant"`
	GemColor          string  `json:"gemColor"`
	CurrentPrice      float64 `json:"currentPrice"`
	CurrentListings   int     `json:"currentListings"`
	PriceVelocity     float64 `json:"priceVelocity"`
	ListingVelocity   float64 `json:"listingVelocity"`
	CV                float64 `json:"cv"`
	Signal            string  `json:"signal"`
	HistPosition      float64 `json:"histPosition"`
	PriceHigh7Days    float64 `json:"priceHigh7d"`
	PriceLow7Days     float64 `json:"priceLow7d"`
	BaseListings      int     `json:"baseListings"`
	BaseVelocity      float64 `json:"baseVelocity"`
	RelativeLiquidity float64 `json:"relativeLiquidity"`
	LiquidityTier     string  `json:"liquidityTier"`
	WindowScore       float64 `json:"windowScore"`
	WindowSignal      string  `json:"windowSignal"`
	AdvancedSignal    string  `json:"advancedSignal"`
	PriceTier         string  `json:"priceTier"`
	TierAction        string  `json:"tierAction"`
	SellUrgency       string  `json:"sellUrgency"`
	SellReason        string  `json:"sellReason"`
	Sellability       int     `json:"sellability"`
	SellabilityLabel  string  `json:"sellabilityLabel"`
	PriceTrend        []int   `json:"priceTrend,omitempty"`
	ListingsTrend     []int   `json:"listingsTrend,omitempty"`
	BaseListingsTrend []int   `json:"baseListingsTrend,omitempty"`
}

func assembleTrendRows(query lab.TrendQueryResult) []trendRow {
	rows := make([]trendRow, 0, len(query.Filtered))
	for _, sf := range query.Filtered {
		signal := sf.Signal
		row := trendRow{
			Time:             signal.Time.UTC().Format(time.RFC3339),
			Name:             signal.Name,
			Variant:          signal.Variant,
			Signal:           signal.Signal,
			WindowSignal:     signal.WindowSignal,
			AdvancedSignal:   signal.AdvancedSignal,
			PriceTier:        signal.Tier,
			TierAction:       lab.TierActionFor(signal.Signal, signal.WindowSignal, signal.Tier),
			SellUrgency:      signal.SellUrgency,
			SellReason:       signal.SellReason,
			Sellability:      signal.Sellability,
			SellabilityLabel: signal.SellabilityLabel,
		}

		if feature := sf.Feature; feature != nil {
			row.GemColor = feature.GemColor
			row.CurrentPrice = feature.Chaos
			row.CurrentListings = feature.Listings
			row.PriceVelocity = feature.VelLongPrice
			row.ListingVelocity = feature.VelLongListing
			row.CV = feature.CV
			row.HistPosition = feature.HistPosition
			row.PriceHigh7Days = feature.High7Days
			row.PriceLow7Days = feature.Low7Days
			row.RelativeLiquidity = feature.RelativeListings
			row.LiquidityTier = lab.LiquidityTierFor(feature.MarketDepth)
			// BaseListings and BaseVelocity intentionally stay zero: the v2
			// pipeline does not provide base-gem listings or velocity.
		}

		if data, ok := query.Trends[lab.TrendGemKey{Name: signal.Name, Variant: signal.Variant}]; ok {
			row.PriceTrend = data.Prices
			row.ListingsTrend = data.Listings
			row.BaseListingsTrend = data.BaseListings
		}
		rows = append(rows, row)
	}
	return rows
}
