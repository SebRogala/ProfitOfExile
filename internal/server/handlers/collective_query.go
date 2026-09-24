package handlers

import (
	"context"
	"log/slog"
	"strings"

	"profitofexile/internal/lab"
	"profitofexile/internal/league"
)

type collectiveQueryInput struct {
	Variant      string
	SparkVariant string
	Budget       float64
	Limit        int
	SearchName   string
	SortBy       lab.SortMode
}

type collectiveCorpus struct {
	transfigure []lab.TransfigureResult
	signals     []lab.GemSignal
	features    []lab.GemFeature
}

type collectiveQueryError struct {
	operation string
	err       error
}

func (e *collectiveQueryError) Error() string { return e.operation + ": " + e.err.Error() }
func (e *collectiveQueryError) Unwrap() error { return e.err }

type collectiveBaseKey struct {
	name    string
	variant string
}

type collectiveQueryResult struct {
	results        []lab.CollectiveResult
	sparklines     map[string][]lab.SparklinePoint
	basePriceIndex map[collectiveBaseKey]float64
	gcpPrice       float64
}

func loadCollectiveCorpus(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, input collectiveQueryInput) (collectiveCorpus, error) {
	// All three corpora are required for ranking. Each has explicit warmth, so
	// a stored empty answer is authoritative and only a cold corpus reaches the
	// repository; variant narrowing and ranking stay after this cache decision.
	if cache != nil {
		transfigure, transfigureWarm := cache.For(scope).Transfigure()
		signals, signalsWarm := cache.For(scope).GemSignals()
		features, featuresWarm := cache.For(scope).GemFeatures()
		if transfigureWarm && signalsWarm && featuresWarm {
			return collectiveCorpus{
				transfigure: filterTransfigure(transfigure, input.Variant, 1000),
				signals:     filterGemSignals(signals, input.Variant, "", 5000),
				features:    features,
			}, nil
		}
	}

	transfigure, err := repo.LatestTransfigureResults(ctx, scope, input.Variant, 1000)
	if err != nil {
		return collectiveCorpus{}, &collectiveQueryError{operation: "transfigure", err: err}
	}
	signals, err := repo.LatestGemSignals(ctx, scope, input.Variant, "", 5000)
	if err != nil {
		return collectiveCorpus{}, &collectiveQueryError{operation: "gem signals", err: err}
	}
	features, err := repo.LatestGemFeatures(ctx, scope, input.Variant, "", 50000)
	if err != nil {
		return collectiveCorpus{}, &collectiveQueryError{operation: "gem features", err: err}
	}
	return collectiveCorpus{transfigure: transfigure, signals: signals, features: features}, nil
}

func queryCollective(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, input collectiveQueryInput) (collectiveQueryResult, error) {
	corpus, err := loadCollectiveCorpus(ctx, repo, cache, scope, input)
	if err != nil {
		return collectiveQueryResult{}, err
	}

	effectiveLimit := input.Limit
	if input.SearchName != "" {
		effectiveLimit = 1000
	}
	results := lab.RankCollective(corpus.transfigure, corpus.signals, corpus.features, input.Budget, effectiveLimit, input.SortBy)
	if input.SearchName != "" {
		query := strings.ToLower(input.SearchName)
		matched := make([]lab.CollectiveResult, 0, len(results))
		for _, result := range results {
			if strings.Contains(strings.ToLower(result.TransfiguredName), query) {
				matched = append(matched, result)
			}
		}
		results = matched
	}

	basePriceIndex := make(map[collectiveBaseKey]float64, len(corpus.transfigure))
	for _, result := range corpus.transfigure {
		basePriceIndex[collectiveBaseKey{result.BaseName, result.Variant}] = result.BasePrice
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
	return collectiveQueryResult{
		results:        results,
		sparklines:     sparklines,
		basePriceIndex: basePriceIndex,
		gcpPrice:       gcpPrice,
	}, nil
}

func loadCollectiveSparklines(ctx context.Context, repo *lab.Repository, cache *lab.Cache, scope league.Scope, sparkVariant string, results []lab.CollectiveResult) map[string][]lab.SparklinePoint {
	sparklines := make(map[string][]lab.SparklinePoint)
	if cache != nil && cache.For(scope).HasSparklines() {
		c := cache.For(scope)
		for _, result := range results {
			variant := sparkVariant
			if variant == "" {
				variant = result.Variant
			}
			// Raw prices — normalization creates edge artifacts.
			if points := trimSparkline(c.Sparklines(result.TransfiguredName, variant), sparklineWindowHours); len(points) > 0 {
				sparklines[result.TransfiguredName] = points
			}
		}
		return sparklines
	}

	if sparkVariant != "" {
		names := make([]string, 0, len(results))
		for _, result := range results {
			names = append(names, result.TransfiguredName)
		}
		points, err := repo.SparklineData(ctx, scope, names, sparkVariant, sparklineWindowHours)
		if err != nil {
			slog.Error("collective analysis: sparkline query failed", "error", err)
		} else {
			sparklines = points
		}
		return sparklines
	}

	byVariant := make(map[string][]string)
	for _, result := range results {
		byVariant[result.Variant] = append(byVariant[result.Variant], result.TransfiguredName)
	}
	for variant, names := range byVariant {
		points, err := repo.SparklineData(ctx, scope, names, variant, sparklineWindowHours)
		if err != nil {
			slog.Error("collective analysis: sparkline query failed", "variant", variant, "error", err)
			continue
		}
		for name, series := range points {
			sparklines[name] = series
		}
	}
	return sparklines
}

func assembleCollectiveRows(query collectiveQueryResult) []collectiveRow {
	rows := make([]collectiveRow, 0, len(query.results))
	for _, result := range query.results {
		row := collectiveRow{
			TransfiguredName:     result.TransfiguredName,
			BaseName:             result.BaseName,
			Variant:              result.Variant,
			GemColor:             result.GemColor,
			ROI:                  result.ROI,
			ROIPct:               result.ROIPct,
			WeightedROI:          result.WeightedROI,
			WeightedROIPct:       result.WeightedROIPct,
			BasePrice:            result.BasePrice,
			TransfiguredPrice:    result.TransfiguredPrice,
			BaseListings:         result.BaseListings,
			TransfiguredListings: result.TransfiguredListings,
			Confidence:           result.Confidence,
			Signal:               result.Signal,
			PriceVelocity:        result.PriceVelocity,
			ListingVelocity:      result.ListingVelocity,
			CV:                   result.CV,
			HistPosition:         result.HistPosition,
			WindowSignal:         result.WindowSignal,
			AdvancedSignal:       result.AdvancedSignal,
			LiquidityTier:        result.LiquidityTier,
			PriceTier:            result.PriceTier,
			TierAction:           result.TierAction,
			SellUrgency:          result.SellUrgency,
			SellReason:           result.SellReason,
			Sellability:          result.Sellability,
			SellabilityLabel:     result.SellabilityLabel,
			Sparkline:            nonNilSparkline(query.sparklines[result.TransfiguredName]),
			Low7Days:             result.Low7Days,
			High7Days:            result.High7Days,
			SellConfidence:       result.SellConfidence,
			TradeConfidenceNote:  result.TradeConfidenceNote,
			LowConfidence:        result.LowConfidence,
		}

		// GCP recipe for 20/20 variants: buy 20/0 base + 20×GCP.
		if result.Variant == "20/20" {
			if base20, ok := query.basePriceIndex[collectiveBaseKey{result.BaseName, "20"}]; ok && base20 > 0 {
				recipeCost := base20 + 20*query.gcpPrice
				row.GCPRecipeCost = recipeCost
				row.GCPRecipeBase = base20
				row.GCPRecipeSaves = result.BasePrice - recipeCost
			}
		}
		rows = append(rows, row)
	}
	return rows
}
