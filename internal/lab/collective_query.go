package lab

import (
	"context"
	"log/slog"
	"strings"

	"profitofexile/internal/league"
)

type CollectiveQuery struct {
	Variant      string
	SparkVariant string
	Budget       float64
	Limit        int
	SearchName   string
	SortBy       SortMode
}

type collectiveCorpus struct {
	transfigure []TransfigureResult
	signals     []GemSignal
	features    []GemFeature
}

type CollectiveBaseKey struct {
	Name    string
	Variant string
}

type CollectiveSparklineKey struct {
	Name    string
	Variant string
}

type CollectiveQueryResult struct {
	Results        []CollectiveResult
	Sparklines     map[CollectiveSparklineKey][]SparklinePoint
	BasePriceIndex map[CollectiveBaseKey]float64
	GCPPrice       float64
}

func loadCollectiveCorpus(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, input CollectiveQuery) (collectiveCorpus, error) {
	// All three corpora are required for ranking. Each has explicit warmth, so
	// a stored empty answer is authoritative and only a cold corpus reaches the
	// repository; variant narrowing and ranking stay after this cache decision.
	if cache != nil {
		transfigure, transfigureWarm := cache.For(scope).Transfigure()
		signals, signalsWarm := cache.For(scope).GemSignals()
		features, featuresWarm := cache.For(scope).GemFeatures()
		if transfigureWarm && signalsWarm && featuresWarm {
			return collectiveCorpus{
				transfigure: FilterTransfigure(transfigure, input.Variant, 1000),
				signals:     FilterGemSignals(signals, input.Variant, "", 5000),
				features:    features,
			}, nil
		}
	}

	transfigure, err := repo.LatestTransfigureResults(ctx, scope, input.Variant, 1000)
	if err != nil {
		return collectiveCorpus{}, &QueryError{Operation: "transfigure", Err: err}
	}
	signals, err := repo.LatestGemSignals(ctx, scope, input.Variant, "", 5000)
	if err != nil {
		return collectiveCorpus{}, &QueryError{Operation: "gem signals", Err: err}
	}
	features, err := repo.LatestGemFeatures(ctx, scope, input.Variant, "", 50000)
	if err != nil {
		return collectiveCorpus{}, &QueryError{Operation: "gem features", Err: err}
	}
	return collectiveCorpus{transfigure: transfigure, signals: signals, features: features}, nil
}

func QueryCollective(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, input CollectiveQuery) (CollectiveQueryResult, error) {
	corpus, err := loadCollectiveCorpus(ctx, repo, cache, scope, input)
	if err != nil {
		return CollectiveQueryResult{}, err
	}

	effectiveLimit := input.Limit
	if input.SearchName != "" {
		effectiveLimit = 1000
	}
	results := RankCollective(corpus.transfigure, corpus.signals, corpus.features, input.Budget, effectiveLimit, input.SortBy)
	if input.SearchName != "" {
		query := strings.ToLower(input.SearchName)
		matched := make([]CollectiveResult, 0, len(results))
		for _, result := range results {
			if strings.Contains(strings.ToLower(result.TransfiguredName), query) {
				matched = append(matched, result)
			}
		}
		results = matched
	}

	basePriceIndex := make(map[CollectiveBaseKey]float64, len(corpus.transfigure))
	for _, result := range corpus.transfigure {
		basePriceIndex[CollectiveBaseKey{Name: result.BaseName, Variant: result.Variant}] = result.BasePrice
	}

	gcpPrice := 0.0
	if cache != nil {
		gcpPrice = cache.For(scope).GCPPrice()
	}
	if gcpPrice <= 0 {
		gcpPrice = 4.0
		slog.Warn("collective: GCP price not cached, using fallback", "fallback", gcpPrice)
	}

	sparklines := loadCollectiveSparklines(ctx, repo, cache, scope, input.SparkVariant, results)
	return CollectiveQueryResult{
		Results:        results,
		Sparklines:     sparklines,
		BasePriceIndex: basePriceIndex,
		GCPPrice:       gcpPrice,
	}, nil
}

func loadCollectiveSparklines(ctx context.Context, repo *Repository, cache *Cache, scope league.Scope, sparkVariant string, results []CollectiveResult) map[CollectiveSparklineKey][]SparklinePoint {
	sparklines := make(map[CollectiveSparklineKey][]SparklinePoint)
	if cache != nil && cache.For(scope).HasSparklines() {
		c := cache.For(scope)
		for _, result := range results {
			variant := sparkVariant
			if variant == "" {
				variant = result.Variant
			}
			// Raw prices — normalization creates edge artifacts.
			if points := TrimSparkline(c.Sparklines(result.TransfiguredName, variant), SparklineWindowHours); len(points) > 0 {
				sparklines[CollectiveSparklineKey{Name: result.TransfiguredName, Variant: result.Variant}] = points
			}
		}
		return sparklines
	}

	if sparkVariant != "" {
		names := make([]string, 0, len(results))
		for _, result := range results {
			names = append(names, result.TransfiguredName)
		}
		points, err := repo.SparklineData(ctx, scope, names, sparkVariant, SparklineWindowHours)
		if err != nil {
			slog.Error("collective analysis: sparkline query failed", "error", err)
		} else {
			for _, result := range results {
				if series, ok := points[result.TransfiguredName]; ok {
					sparklines[CollectiveSparklineKey{Name: result.TransfiguredName, Variant: result.Variant}] = series
				}
			}
		}
		return sparklines
	}

	byVariant := make(map[string][]string)
	for _, result := range results {
		byVariant[result.Variant] = append(byVariant[result.Variant], result.TransfiguredName)
	}
	for variant, names := range byVariant {
		points, err := repo.SparklineData(ctx, scope, names, variant, SparklineWindowHours)
		if err != nil {
			slog.Error("collective analysis: sparkline query failed", "variant", variant, "error", err)
			continue
		}
		for name, series := range points {
			sparklines[CollectiveSparklineKey{Name: name, Variant: variant}] = series
		}
	}
	return sparklines
}
