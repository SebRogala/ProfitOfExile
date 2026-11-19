package lab

import (
	"context"
	"sort"

	"profitofexile/internal/league"
)

// FontOverviewMaxLimit is the largest limit a Font or Dedication overview
// request may ask for.
const FontOverviewMaxLimit = 500

// FontOverviewSource reads the latest Font and Dedication snapshots grouped:
// each one statement covering every variant, mode and pool.
type FontOverviewSource interface {
	FontOverviewRows(ctx context.Context, scope league.Scope) ([]FontResult, error)
	DedicationOverviewRows(ctx context.Context, scope league.Scope) ([]DedicationResult, error)
}

// FontColdFills holds the in-flight cold Font and Dedication loads of one league.
type FontColdFills struct{}

func (f *FontColdFills) waiting() int { return 0 }

// FontModes answers one Font overview request.
func FontModes(ctx context.Context, src FontOverviewSource, cache *Cache, scope league.Scope, variant string, limit int) (FontAnalysis, error) {
	if cache != nil {
		if analysis, warm := cache.For(scope).Font(); warm {
			return sliceFontModes(analysis, variant, limit), nil
		}
	}
	rows, err := src.FontOverviewRows(ctx, scope)
	if err != nil {
		return FontAnalysis{}, err
	}
	return sliceFontModes(groupFontModes(rows), variant, limit), nil
}

// DedicationPools answers one Dedication overview request.
func DedicationPools(ctx context.Context, src FontOverviewSource, cache *Cache, scope league.Scope, variant string, limit int) (DedicationAnalysis, error) {
	if cache != nil {
		if ded, warm := cache.For(scope).Dedication(); warm {
			return DedicationAnalysis{
				Skills:       FilterDedicationVariant(ded.Skills, variant),
				Transfigured: FilterDedicationVariant(ded.Transfigured, variant),
			}, nil
		}
	}
	rows, err := src.DedicationOverviewRows(ctx, scope)
	if err != nil {
		return DedicationAnalysis{}, err
	}
	return sliceDedicationPools(groupDedicationPools(rows), variant, limit), nil
}

func sliceFontModes(all FontAnalysis, variant string, limit int) FontAnalysis {
	return FontAnalysis{
		Safe:    FilterFont(all.Safe, variant, limit),
		Premium: FilterFont(all.Premium, variant, limit),
		Jackpot: FilterFont(all.Jackpot, variant, limit),
	}
}

// groupFontModes splits grouped rows into their modes.
func groupFontModes(rows []FontResult) FontAnalysis {
	var out FontAnalysis
	for _, r := range rows {
		switch r.Mode {
		case "safe":
			out.Safe = append(out.Safe, r)
		case "premium":
			out.Premium = append(out.Premium, r)
		case "jackpot":
			out.Jackpot = append(out.Jackpot, r)
		}
	}
	return out
}

// groupDedicationPools splits grouped rows into their pools.
func groupDedicationPools(rows []DedicationResult) DedicationAnalysis {
	var out DedicationAnalysis
	for _, r := range rows {
		switch r.GemType {
		case "skill":
			out.Skills = append(out.Skills, r)
		case "transfigured":
			out.Transfigured = append(out.Transfigured, r)
		}
	}
	return out
}

// sliceDedicationPools keeps, per pool, the limit most profitable rows of
// variant across every mode, as one cold per-pool query did.
func sliceDedicationPools(all DedicationAnalysis, variant string, limit int) DedicationAnalysis {
	return DedicationAnalysis{
		Skills:       TopDedication(all.Skills, variant, limit),
		Transfigured: TopDedication(all.Transfigured, variant, limit),
	}
}

// FilterFont narrows Font results to variant ("" keeps every variant), orders
// them by profit descending and keeps the first limit.
func FilterFont(all []FontResult, variant string, limit int) []FontResult {
	var filtered []FontResult
	for _, r := range all {
		if variant != "" && r.Variant != variant {
			continue
		}
		filtered = append(filtered, r)
	}
	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Profit > filtered[j].Profit
	})
	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

// TopDedication narrows Dedication results to variant and keeps the limit most
// profitable across every mode.
func TopDedication(all []DedicationResult, variant string, limit int) []DedicationResult {
	filtered := FilterDedicationVariant(all, variant)
	sort.Slice(filtered, func(i, j int) bool {
		return filtered[i].Profit > filtered[j].Profit
	})
	if len(filtered) > limit {
		filtered = filtered[:limit]
	}
	return filtered
}

func newFontColdFills() *FontColdFills { return &FontColdFills{} }
