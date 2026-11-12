package lab

import (
	"context"
	"errors"
	"fmt"
	"log/slog"

	"profitofexile/internal/league"
)

// CollectiveSort names the order of a collective ranking page. The first four
// are the client keys (price, roi, riskAdjusted, roiPercent); the legacy two
// are the orders a request without pageSize resolves to.
type CollectiveSort string

const (
	CollectiveSortPrice       CollectiveSort = "price"
	CollectiveSortROI         CollectiveSort = "roi"
	CollectiveSortWeightedROI CollectiveSort = "weightedRoi"
	// CollectiveSortROIPct sorts by the raw roi_pct, not the weighted one.
	CollectiveSortROIPct              CollectiveSort = "roiPct"
	CollectiveSortLegacyWeightedChaos CollectiveSort = "legacyWeightedChaos"
	CollectiveSortLegacyWeightedPct   CollectiveSort = "legacyWeightedPct"
)

// CollectiveMaxPageSize caps a page and the legacy limit alike.
const CollectiveMaxPageSize = 100

// CollectiveSearchLimit bounds a search, which bypasses the page.
const CollectiveSearchLimit = 1000

// collectiveVariants are the Normal markets, in their stored form.
var collectiveVariants = map[string]bool{"1": true, "1/20": true, "20": true, "20/20": true}

var collectiveColors = map[string]bool{"RED": true, "GREEN": true, "BLUE": true}

// CollectiveQuery is one page request of the Normal collective ranking.
// Variant is the stored market ("" = all four); SparkVariant is the raw
// request variant the sparkline read keys on. Color "" means every colour,
// Budget 0 means off.
type CollectiveQuery struct {
	Variant           string
	SparkVariant      string
	Color             string
	Budget            float64
	HideLowConfidence bool
	Search            string
	Sort              CollectiveSort
	Page              int
	PageSize          int
	Legacy            bool
}

// Validate enforces the page bounds and the whitelists.
func (q CollectiveQuery) Validate() error {
	if q.Page < 1 {
		return errors.New("page must be a positive integer")
	}
	if q.PageSize < 1 || q.PageSize > CollectiveMaxPageSize {
		return fmt.Errorf("pageSize must be an integer from 1 to %d", CollectiveMaxPageSize)
	}
	if _, ok := collectiveOrderBy[q.Sort]; !ok {
		return errors.New("sort must be one of price, roi, weightedRoi, roiPct")
	}
	if !q.Legacy && (q.Sort == CollectiveSortLegacyWeightedChaos || q.Sort == CollectiveSortLegacyWeightedPct) {
		return errors.New("sort must be one of price, roi, weightedRoi, roiPct")
	}
	if q.Budget < 0 {
		return errors.New("budget must be a non-negative number")
	}
	if q.Color != "" && !collectiveColors[q.Color] {
		return errors.New("color must be RED, GREEN, BLUE or ALL")
	}
	if q.Variant != "" && !collectiveVariants[q.Variant] {
		return errors.New("variant must be one of 1/0, 1/20, 20/0, 20/20")
	}
	return nil
}

// LegacyCollectiveSort resolves a request without pageSize to today's order:
// sort=pct, or a budget of at most 50, ranks by weighted ROI%; anything else
// by weighted chaos ROI.
func LegacyCollectiveSort(sortParam string, budget float64) CollectiveSort {
	if sortParam == "pct" || (budget > 0 && budget <= 50) {
		return CollectiveSortLegacyWeightedPct
	}
	return CollectiveSortLegacyWeightedChaos
}

// CollectiveSource is what QueryCollective reads: the SQL ranker, the GCP base
// lookup and the sparkline series. *Repository implements it.
type CollectiveSource interface {
	CollectivePage(ctx context.Context, scope league.Scope, q CollectiveQuery) (CollectivePage, error)
	CollectiveBasePrices(ctx context.Context, scope league.Scope, baseNames []string) (map[string]float64, error)
	SparklineData(ctx context.Context, scope league.Scope, names []string, variant string, hours int) (map[string][]SparklinePoint, error)
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
	Results []CollectiveResult
	Total   int
	// BasePriceIndex holds the 20/0 ("20") base price of each 20/20 page row's
	// base, for the GCP recipe.
	BasePriceIndex map[CollectiveBaseKey]float64
	Sparklines     map[CollectiveSparklineKey][]SparklinePoint
	GCPPrice       float64
}

// QueryCollective ranks one page in SQL, then decorates only that page's rows:
// sparklines (cache, or one read per variant), the GCP base lookup (one read)
// and the trade note from the cached signals.
func QueryCollective(ctx context.Context, src CollectiveSource, cache *Cache, scope league.Scope, input CollectiveQuery) (CollectiveQueryResult, error) {
	page, err := src.CollectivePage(ctx, scope, input)
	if err != nil {
		return CollectiveQueryResult{}, &QueryError{Operation: "collective page", Err: err}
	}
	results := page.Rows
	if cache != nil {
		applyCachedTradeNotes(cache.For(scope), results)
	}

	basePriceIndex := make(map[CollectiveBaseKey]float64)
	var bases []string
	seen := make(map[string]bool)
	for _, result := range results {
		if result.Variant == "20/20" && !seen[result.BaseName] {
			seen[result.BaseName] = true
			bases = append(bases, result.BaseName)
		}
	}
	if len(bases) > 0 {
		prices, err := src.CollectiveBasePrices(ctx, scope, bases)
		if err != nil {
			slog.Error("collective analysis: GCP base price query failed", "error", err)
		}
		for name, price := range prices {
			basePriceIndex[CollectiveBaseKey{Name: name, Variant: "20"}] = price
		}
	}

	gcpPrice := 0.0
	if cache != nil {
		gcpPrice = cache.For(scope).GCPPrice()
	}
	if gcpPrice <= 0 {
		gcpPrice = 4.0
		slog.Warn("collective: GCP price not cached, using fallback", "fallback", gcpPrice)
	}

	sparklines := loadCollectiveSparklines(ctx, src, cache, scope, input.SparkVariant, results)
	return CollectiveQueryResult{
		Results:        results,
		Total:          page.Total,
		Sparklines:     sparklines,
		BasePriceIndex: basePriceIndex,
		GCPPrice:       gcpPrice,
	}, nil
}

// applyCachedTradeNotes copies the trade note onto the page rows. gem_signals
// stores no note, so the tick-computed signals in the cache are its only source.
func applyCachedTradeNotes(c *Cache, results []CollectiveResult) {
	signals, warm := c.GemSignals()
	if !warm || len(results) == 0 {
		return
	}
	index := make(map[CollectiveSparklineKey]int, len(results))
	for i, r := range results {
		index[CollectiveSparklineKey{Name: r.TransfiguredName, Variant: r.Variant}] = i
	}
	for _, s := range signals {
		if i, ok := index[CollectiveSparklineKey{Name: s.Name, Variant: s.Variant}]; ok {
			results[i].TradeConfidenceNote = s.TradeConfidenceNote
		}
	}
}

func loadCollectiveSparklines(ctx context.Context, src CollectiveSource, cache *Cache, scope league.Scope, sparkVariant string, results []CollectiveResult) map[CollectiveSparklineKey][]SparklinePoint {
	sparklines := make(map[CollectiveSparklineKey][]SparklinePoint)
	if len(results) == 0 {
		return sparklines
	}
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
		points, err := src.SparklineData(ctx, scope, names, sparkVariant, SparklineWindowHours)
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
		points, err := src.SparklineData(ctx, scope, names, variant, SparklineWindowHours)
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
