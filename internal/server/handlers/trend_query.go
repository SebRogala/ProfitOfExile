package handlers

import (
	"context"
	"log/slog"
	"math"
	"strings"
	"time"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

type trendQueryInput struct {
	Variant  string
	Signal   string
	Window   string
	Advanced string
	Tier     string
	Limit    int
}

type trendCorpus struct {
	signals  []lab.GemSignal
	features []lab.GemFeature
}

type trendQueryError struct {
	operation string
	err       error
}

func (e *trendQueryError) Error() string { return e.operation + ": " + e.err.Error() }
func (e *trendQueryError) Unwrap() error { return e.err }

type trendGemKey struct {
	name    string
	variant string
}

type trendSignalWithFeature struct {
	signal  *lab.GemSignal
	feature *lab.GemFeature
}

type trendData struct {
	prices, listings, baseListings []int
}

type trendQueryResult struct {
	filtered []trendSignalWithFeature
	trends   map[trendGemKey]trendData
}

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

func loadTrendCorpus(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, input trendQueryInput) (trendCorpus, error) {
	// These are separate whole-corpus warmth flags. A warm-and-empty corpus is
	// authoritative, while a cold flag is the only reason to query the DB;
	// variant and tier narrowing happens after the cache read.
	if cache != nil {
		signals, signalsWarm := cache.For(scope).GemSignals()
		features, featuresWarm := cache.For(scope).GemFeatures()
		if signalsWarm && featuresWarm {
			return trendCorpus{signals: signals, features: features}, nil
		}
	}

	signals, err := repo.LatestGemSignals(ctx, scope, input.Variant, input.Tier, 50000)
	if err != nil {
		return trendCorpus{}, &trendQueryError{operation: "gem signals", err: err}
	}
	features, err := repo.LatestGemFeatures(ctx, scope, input.Variant, input.Tier, 50000)
	if err != nil {
		return trendCorpus{}, &trendQueryError{operation: "gem features", err: err}
	}
	return trendCorpus{signals: signals, features: features}, nil
}

func queryTrend(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, input trendQueryInput) (trendQueryResult, error) {
	corpus, err := loadTrendCorpus(ctx, repo, cache, scope, input)
	if err != nil {
		return trendQueryResult{}, err
	}
	filtered := selectTrendSignals(corpus, input)
	trends := loadTrendSparklines(ctx, repo, cache, scope, filtered)
	return trendQueryResult{filtered: filtered, trends: trends}, nil
}

func selectTrendSignals(corpus trendCorpus, input trendQueryInput) []trendSignalWithFeature {
	type featureKey struct {
		name    string
		variant string
	}
	featureIndex := make(map[featureKey]*lab.GemFeature, len(corpus.features))
	for i := range corpus.features {
		feature := &corpus.features[i]
		featureIndex[featureKey{feature.Name, feature.Variant}] = feature
	}

	filtered := make([]trendSignalWithFeature, 0, input.Limit)
	for i := range corpus.signals {
		signal := &corpus.signals[i]
		if input.Variant != "" && signal.Variant != input.Variant {
			continue
		}
		if input.Signal != "" && signal.Signal != input.Signal {
			continue
		}
		if input.Window != "" && signal.WindowSignal != input.Window {
			continue
		}
		if input.Advanced != "" && signal.AdvancedSignal != input.Advanced {
			continue
		}
		if input.Tier != "" && signal.Tier != input.Tier {
			continue
		}
		feature := featureIndex[featureKey{signal.Name, signal.Variant}]
		filtered = append(filtered, trendSignalWithFeature{signal: signal, feature: feature})
		if len(filtered) >= input.Limit {
			break
		}
	}
	return filtered
}

func loadTrendSparklines(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, filtered []trendSignalWithFeature) map[trendGemKey]trendData {
	windowAlerts := map[string]bool{"BREWING": true, "OPENING": true, "OPEN": true, "CLOSING": true}
	seen := make(map[trendGemKey]bool)
	var selected []trendGemKey
	var transNames, baseNames []string
	for _, sf := range filtered {
		key := trendGemKey{sf.signal.Name, sf.signal.Variant}
		if windowAlerts[sf.signal.WindowSignal] && !seen[key] {
			seen[key] = true
			selected = append(selected, key)
			transNames = append(transNames, sf.signal.Name)
			baseName := sf.signal.Name
			if idx := strings.LastIndex(sf.signal.Name, " of "); idx > 0 {
				baseName = sf.signal.Name[:idx]
			}
			baseNames = append(baseNames, baseName)
		}
	}

	trends := make(map[trendGemKey]trendData)
	if len(transNames) == 0 {
		return trends
	}

	type variantGroup struct {
		transNames []string
		baseNames  []string
		gems       []trendGemKey
	}
	groups := make(map[string]*variantGroup)
	for i, key := range selected {
		group := groups[key.variant]
		if group == nil {
			group = &variantGroup{}
			groups[key.variant] = group
		}
		group.transNames = append(group.transNames, key.name)
		group.baseNames = append(group.baseNames, baseNames[i])
		group.gems = append(group.gems, key)
	}

	last4 := func(points []lab.SparklinePoint) []lab.SparklinePoint {
		if len(points) > 4 {
			return points[len(points)-4:]
		}
		return points
	}
	warmSparklines := cache != nil && cache.For(scope).HasSparklines()
	for variant, group := range groups {
		var transSparklines, baseSparklines map[string][]lab.SparklinePoint
		if warmSparklines {
			transSparklines = cachedSparklines(cache, scope, group.transNames, variant, 0)
			baseSparklines = cachedSparklines(cache, scope, group.baseNames, variant, 0)
		} else {
			var err error
			transSparklines, err = repo.SparklineData(ctx, scope, group.transNames, variant, 24*7)
			if err != nil {
				slog.Warn("trend analysis: trans sparkline batch failed", "variant", variant, "error", err)
				transSparklines = make(map[string][]lab.SparklinePoint)
			}
			baseSparklines, err = repo.SparklineData(ctx, scope, group.baseNames, variant, 24*7)
			if err != nil {
				slog.Warn("trend analysis: base sparkline batch failed", "variant", variant, "error", err)
				baseSparklines = make(map[string][]lab.SparklinePoint)
			}
		}

		for idx, key := range group.gems {
			data := trendData{}
			if points := last4(transSparklines[key.name]); len(points) >= 2 {
				for _, point := range points {
					data.prices = append(data.prices, int(math.Round(point.Price)))
					data.listings = append(data.listings, point.Listings)
				}
			}
			if points := last4(baseSparklines[group.baseNames[idx]]); len(points) >= 2 {
				for _, point := range points {
					data.baseListings = append(data.baseListings, point.Listings)
				}
			}
			trends[key] = data
		}
	}
	return trends
}

func assembleTrendRows(query trendQueryResult) []trendRow {
	rows := make([]trendRow, 0, len(query.filtered))
	for _, sf := range query.filtered {
		signal := sf.signal
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

		if feature := sf.feature; feature != nil {
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

		if data, ok := query.trends[trendGemKey{signal.Name, signal.Variant}]; ok {
			row.PriceTrend = data.prices
			row.ListingsTrend = data.listings
			row.BaseListingsTrend = data.baseListings
		}
		rows = append(rows, row)
	}
	return rows
}
