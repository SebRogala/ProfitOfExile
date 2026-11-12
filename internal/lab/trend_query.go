package lab

import (
	"context"
	"log/slog"
	"math"
	"strings"

	"profitofexile/internal/league"
)

type TrendQuery struct {
	Variant  string
	Signal   string
	Window   string
	Advanced string
	Tier     string
	Limit    int
}

type trendCorpus struct {
	signals  []GemSignal
	features []GemFeature
}

type QueryError struct {
	Operation string
	Err       error
}

func (e *QueryError) Error() string { return e.Operation + ": " + e.Err.Error() }
func (e *QueryError) Unwrap() error { return e.Err }

type TrendGemKey struct {
	Name    string
	Variant string
}

type TrendSignalWithFeature struct {
	Signal  *GemSignal
	Feature *GemFeature
}

type TrendData struct {
	Prices, Listings, BaseListings []int
}

type TrendQueryResult struct {
	Filtered []TrendSignalWithFeature
	Trends   map[TrendGemKey]TrendData
}

func loadTrendCorpus(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, input TrendQuery) (trendCorpus, error) {
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
		return trendCorpus{}, &QueryError{Operation: "gem signals", Err: err}
	}
	features, err := repo.LatestGemFeatures(ctx, scope, input.Variant, input.Tier, 50000)
	if err != nil {
		return trendCorpus{}, &QueryError{Operation: "gem features", Err: err}
	}
	return trendCorpus{signals: signals, features: features}, nil
}

func QueryTrend(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, input TrendQuery) (TrendQueryResult, error) {
	corpus, err := loadTrendCorpus(ctx, repo, cache, scope, input)
	if err != nil {
		return TrendQueryResult{}, err
	}
	filtered := selectTrendSignals(corpus, input)
	trends := loadTrendSparklines(ctx, repo, cache, scope, filtered)
	return TrendQueryResult{Filtered: filtered, Trends: trends}, nil
}

func selectTrendSignals(corpus trendCorpus, input TrendQuery) []TrendSignalWithFeature {
	type featureKey struct {
		name    string
		variant string
	}
	featureIndex := make(map[featureKey]*GemFeature, len(corpus.features))
	for i := range corpus.features {
		feature := &corpus.features[i]
		featureIndex[featureKey{feature.Name, feature.Variant}] = feature
	}

	filtered := make([]TrendSignalWithFeature, 0, input.Limit)
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
		filtered = append(filtered, TrendSignalWithFeature{Signal: signal, Feature: feature})
		if len(filtered) >= input.Limit {
			break
		}
	}
	return filtered
}

func loadTrendSparklines(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, filtered []TrendSignalWithFeature) map[TrendGemKey]TrendData {
	windowAlerts := map[string]bool{"BREWING": true, "OPENING": true, "OPEN": true, "CLOSING": true}
	seen := make(map[TrendGemKey]bool)
	var selected []TrendGemKey
	var transNames, baseNames []string
	for _, sf := range filtered {
		key := TrendGemKey{sf.Signal.Name, sf.Signal.Variant}
		if windowAlerts[sf.Signal.WindowSignal] && !seen[key] {
			seen[key] = true
			selected = append(selected, key)
			transNames = append(transNames, sf.Signal.Name)
			baseName := sf.Signal.Name
			if idx := strings.LastIndex(sf.Signal.Name, " of "); idx > 0 {
				baseName = sf.Signal.Name[:idx]
			}
			baseNames = append(baseNames, baseName)
		}
	}

	trends := make(map[TrendGemKey]TrendData)
	if len(transNames) == 0 {
		return trends
	}

	type variantGroup struct {
		transNames []string
		baseNames  []string
		gems       []TrendGemKey
	}
	groups := make(map[string]*variantGroup)
	for i, key := range selected {
		group := groups[key.Variant]
		if group == nil {
			group = &variantGroup{}
			groups[key.Variant] = group
		}
		group.transNames = append(group.transNames, key.Name)
		group.baseNames = append(group.baseNames, baseNames[i])
		group.gems = append(group.gems, key)
	}

	last4 := func(points []SparklinePoint) []SparklinePoint {
		if len(points) > 4 {
			return points[len(points)-4:]
		}
		return points
	}
	warmSparklines := cache != nil && cache.For(scope).HasSparklines()
	for variant, group := range groups {
		var transSparklines, baseSparklines map[string][]SparklinePoint
		if warmSparklines {
			transSparklines = CachedSparklines(cache, scope, group.transNames, variant, 0)
			baseSparklines = CachedSparklines(cache, scope, group.baseNames, variant, 0)
		} else {
			var err error
			transSparklines, err = repo.SparklineData(ctx, scope, group.transNames, variant, 24*7)
			if err != nil {
				slog.Warn("trend analysis: trans sparkline batch failed", "variant", variant, "error", err)
				transSparklines = make(map[string][]SparklinePoint)
			}
			baseSparklines, err = repo.SparklineData(ctx, scope, group.baseNames, variant, 24*7)
			if err != nil {
				slog.Warn("trend analysis: base sparkline batch failed", "variant", variant, "error", err)
				baseSparklines = make(map[string][]SparklinePoint)
			}
		}

		for idx, key := range group.gems {
			data := TrendData{}
			if points := last4(transSparklines[key.Name]); len(points) >= 2 {
				for _, point := range points {
					data.Prices = append(data.Prices, int(math.Round(point.Price)))
					data.Listings = append(data.Listings, point.Listings)
				}
			}
			if points := last4(baseSparklines[group.baseNames[idx]]); len(points) >= 2 {
				for _, point := range points {
					data.BaseListings = append(data.BaseListings, point.Listings)
				}
			}
			trends[key] = data
		}
	}
	return trends
}
