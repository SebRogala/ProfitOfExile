package lab

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strings"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/league"
)

// CollectivePage is one page of the Normal collective ranking plus the number
// of rows the same filters match before paging.
type CollectivePage struct {
	Rows  []CollectiveResult
	Total int
}

// collectiveRankedCTE ranks the latest transfigure run, joined to the latest
// signal and feature runs (each at its own MAX(time)), in SQL. It is the port
// of the retired Go ranker:
//   - keep priced NO_BASE rows and ROI > 0 rows; a budget drops NO_BASE and
//     bases above it;
//   - a missing signal row reads as STABLE, and CAUTION is excluded;
//   - weighted = value × sellability/100 × (1 − DUMPING penalty), where the
//     penalty is 0.5 under 15 transfigured listings and 0.15 otherwise.
//
// Sellability falls back to 50 only when the stored signal is empty — a gem with
// no signal row reads as STABLE and so scores 0. That is today's behaviour,
// kept knowingly (ADR030 D5).
//
// Params: $1 league, $2 variant (empty = all), $3 colour (empty = all), $4 budget
// (<= 0 = off), $5 hide low confidence, $6 escaped ILIKE search (empty = none).
const collectiveRankedCTE = `
WITH tr AS (
	SELECT * FROM transfigure_results
	WHERE league = $1 AND time = (SELECT MAX(time) FROM transfigure_results WHERE league = $1)
), s AS (
	SELECT * FROM gem_signals
	WHERE league = $1 AND time = (SELECT MAX(time) FROM gem_signals WHERE league = $1)
), f AS (
	SELECT * FROM gem_features
	WHERE league = $1 AND time = (SELECT MAX(time) FROM gem_features WHERE league = $1)
), joined AS (
	SELECT
		tr.transfigured_name, tr.base_name, tr.variant, tr.gem_color,
		tr.roi::float8 AS roi, tr.roi_pct::float8 AS roi_pct,
		tr.base_price::float8 AS base_price, tr.transfigured_price::float8 AS transfigured_price,
		tr.base_listings, tr.transfigured_listings, tr.confidence,
		s.name IS NOT NULL AS has_signal,
		COALESCE(s.signal, 'STABLE') AS signal,
		COALESCE(s.window_signal, '') AS window_signal,
		COALESCE(s.advanced_signal, '') AS advanced_signal,
		COALESCE(s.tier, '') AS price_tier,
		COALESCE(s.sell_urgency, '') AS sell_urgency,
		COALESCE(s.sell_reason, '') AS sell_reason,
		COALESCE(s.sellability, 0) AS sellability,
		COALESCE(s.sellability_label, '') AS sellability_label,
		COALESCE(s.sell_confidence, '') AS sell_confidence,
		f.name IS NOT NULL AS has_feature,
		COALESCE(f.vel_long_price, 0) AS vel_long_price,
		COALESCE(f.vel_long_listing, 0) AS vel_long_listing,
		COALESCE(f.cv, 0) AS cv,
		COALESCE(f.hist_position, 0) AS hist_position,
		COALESCE(f.low_7d, 0) AS low_7d,
		COALESCE(f.high_7d, 0) AS high_7d,
		COALESCE(f.market_depth, 0) AS market_depth,
		(COALESCE(f.low_confidence, false) OR tr.confidence = 'LOW') AS low_confidence
	FROM tr
	LEFT JOIN s ON s.name = tr.transfigured_name AND s.variant = tr.variant
	LEFT JOIN f ON f.name = tr.transfigured_name AND f.variant = tr.variant
	WHERE ((tr.confidence = 'NO_BASE' AND tr.transfigured_price > 0) OR (tr.confidence <> 'NO_BASE' AND tr.roi > 0))
	  AND COALESCE(s.signal, 'STABLE') <> 'CAUTION'
	  AND ($2::text = '' OR tr.variant = $2::text)
	  AND ($3::text = '' OR tr.gem_color = $3::text)
	  AND ($4::float8 <= 0 OR (tr.confidence <> 'NO_BASE' AND tr.base_price::float8 <= $4::float8))
	  AND (NOT $5::boolean OR NOT (COALESCE(f.low_confidence, false) OR tr.confidence = 'LOW'))
), factored AS (
	SELECT joined.*,
		(CASE WHEN sellability = 0 AND signal = '' THEN 50 ELSE sellability END)::float8 / 100.0::float8 AS liquidity_score,
		CASE WHEN signal <> 'DUMPING' THEN 0.0::float8
		     WHEN transfigured_listings < 15 THEN 0.5::float8
		     ELSE 0.15::float8 END AS saturation_penalty
	FROM joined
), ranked AS (
	SELECT factored.*,
		roi * liquidity_score * (1.0::float8 - saturation_penalty) AS weighted_roi,
		roi_pct * liquidity_score * (1.0::float8 - saturation_penalty) AS weighted_roi_pct
	FROM factored
)`

// collectiveOrderBy is the only source of the page's ORDER BY; request input
// selects a key, never text. Every order ends on price, name and variant.
var collectiveOrderBy = map[CollectiveSort]string{
	CollectiveSortPrice:               "transfigured_price DESC, transfigured_name ASC, variant ASC",
	CollectiveSortROI:                 "roi DESC, transfigured_price DESC, transfigured_name ASC, variant ASC",
	CollectiveSortWeightedROI:         "weighted_roi DESC, transfigured_price DESC, transfigured_name ASC, variant ASC",
	CollectiveSortROIPct:              "roi_pct DESC, transfigured_price DESC, transfigured_name ASC, variant ASC",
	CollectiveSortLegacyWeightedChaos: "weighted_roi DESC, transfigured_price DESC, transfigured_name ASC, variant ASC",
	CollectiveSortLegacyWeightedPct:   "weighted_roi_pct DESC, transfigured_price DESC, transfigured_name ASC, variant ASC",
}

const collectivePageColumns = `
	transfigured_name, base_name, variant, gem_color, roi, roi_pct, weighted_roi, weighted_roi_pct,
	base_price, transfigured_price, base_listings, transfigured_listings, confidence,
	has_signal, signal, window_signal, advanced_signal, price_tier, sell_urgency, sell_reason,
	sellability, sellability_label, sell_confidence,
	has_feature, vel_long_price, vel_long_listing, cv, hist_position, low_7d, high_7d, market_depth,
	low_confidence`

// collectiveSearchPredicate matches $6, an escaped ILIKE pattern (empty = none).
const collectiveSearchPredicate = `($6::text = '' OR transfigured_name ILIKE '%' || $6::text || '%')`

// collectiveMatched appends the search stage. A paged search matches every
// ranked row; a legacy search keeps today's operation and matches only within
// the top CollectiveSearchLimit rows of its order.
func collectiveMatched(q CollectiveQuery) string {
	if q.Legacy && q.Search != "" {
		return fmt.Sprintf(`, matched AS (SELECT * FROM (SELECT * FROM ranked ORDER BY %s LIMIT %d) top WHERE %s)`,
			collectiveOrderBy[q.Sort], CollectiveSearchLimit, collectiveSearchPredicate)
	}
	return `, matched AS (SELECT * FROM ranked WHERE ` + collectiveSearchPredicate + `)`
}

var likeEscaper = strings.NewReplacer(`\`, `\\`, `%`, `\%`, `_`, `\_`)

// CollectivePage counts and pages the Normal collective ranking in one
// repeatable-read, read-only transaction, so total and page come from the same
// snapshot. A search ignores Page/PageSize and returns up to
// CollectiveSearchLimit matches.
func (r *Repository) CollectivePage(ctx context.Context, scope league.Scope, q CollectiveQuery) (CollectivePage, error) {
	if err := scope.Validate(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective page: %w", err)
	}
	if err := q.Validate(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective page: %w", err)
	}
	limit, offset := q.PageSize, math.MaxInt64
	// A page whose offset overflows lies past any representable row count.
	if q.Page-1 <= math.MaxInt64/q.PageSize {
		offset = (q.Page - 1) * q.PageSize
	}
	if q.Search != "" {
		limit, offset = CollectiveSearchLimit, 0
	}
	search := ""
	if q.Search != "" {
		search = likeEscaper.Replace(q.Search)
	}
	args := []any{scope.ID(), q.Variant, q.Color, q.Budget, q.HideLowConfidence, search}

	tx, err := r.pool.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective page begin: %w", err)
	}
	defer tx.Rollback(ctx)

	var out CollectivePage
	if err := tx.QueryRow(ctx, collectiveRankedCTE+collectiveMatched(q)+` SELECT COUNT(*) FROM matched`, args...).Scan(&out.Total); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective count: %w", err)
	}
	rows, err := tx.Query(ctx,
		collectiveRankedCTE+collectiveMatched(q)+` SELECT `+collectivePageColumns+` FROM matched ORDER BY `+collectiveOrderBy[q.Sort]+` LIMIT $7 OFFSET $8`,
		append(args, limit, offset)...)
	if err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective page query: %w", err)
	}
	defer rows.Close()
	for rows.Next() {
		var cr CollectiveResult
		var hasSignal, hasFeature bool
		var depth float64
		if err := rows.Scan(
			&cr.TransfiguredName, &cr.BaseName, &cr.Variant, &cr.GemColor, &cr.ROI, &cr.ROIPct, &cr.WeightedROI, &cr.WeightedROIPct,
			&cr.BasePrice, &cr.TransfiguredPrice, &cr.BaseListings, &cr.TransfiguredListings, &cr.Confidence,
			&hasSignal, &cr.Signal, &cr.WindowSignal, &cr.AdvancedSignal, &cr.PriceTier, &cr.SellUrgency, &cr.SellReason,
			&cr.Sellability, &cr.SellabilityLabel, &cr.SellConfidence,
			&hasFeature, &cr.PriceVelocity, &cr.ListingVelocity, &cr.CV, &cr.HistPosition, &cr.Low7Days, &cr.High7Days, &depth,
			&cr.LowConfidence,
		); err != nil {
			return CollectivePage{}, fmt.Errorf("lab repo: scan collective row: %w", err)
		}
		if hasSignal {
			cr.TierAction = tierAction(cr.Signal, cr.WindowSignal, cr.PriceTier)
		}
		if hasFeature {
			cr.LiquidityTier = liquidityTier(depth)
		}
		out.Rows = append(out.Rows, cr)
	}
	if err := rows.Err(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective rows: %w", err)
	}
	if err := tx.Commit(ctx); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: collective page commit: %w", err)
	}
	return out, nil
}

// CollectiveBasePrices returns the 20/0 (stored "20") base price for each
// named base from the latest transfigure run: the GCP recipe's input for a
// 20/20 row.
func (r *Repository) CollectiveBasePrices(ctx context.Context, scope league.Scope, baseNames []string) (map[string]float64, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: collective base prices: %w", err)
	}
	if len(baseNames) == 0 {
		return nil, nil
	}
	rows, err := r.pool.Query(ctx, `
		SELECT DISTINCT ON (base_name) base_name, base_price::float8
		FROM transfigure_results
		WHERE league = $1 AND time = (SELECT MAX(time) FROM transfigure_results WHERE league = $1)
		  AND variant = '20' AND base_name = ANY($2)
		ORDER BY base_name, transfigured_name`, scope.ID(), baseNames)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query collective base prices: %w", err)
	}
	defer rows.Close()
	out := make(map[string]float64, len(baseNames))
	for rows.Next() {
		var name string
		var price float64
		if err := rows.Scan(&name, &price); err != nil {
			return nil, fmt.Errorf("lab repo: scan collective base price: %w", err)
		}
		out[name] = price
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: collective base price rows: %w", err)
	}
	return out, nil
}

// DedicationLegacyMaxLimit caps a legacy Dedication request's limit.
const DedicationLegacyMaxLimit = 500

var dedicationPools = map[string]bool{"skill": true, "transfigured": true}

// DedicationCollectiveQuery is one page request of the Dedication ranking.
// Variant is the stored market ("21/23c"); Pool "" means both pools, Color ""
// every colour, Budget 0 off. A legacy request (no pageSize) ranks by price,
// takes up to DedicationLegacyMaxLimit rows and applies its search before the
// limit; a paged search ignores Page/PageSize.
type DedicationCollectiveQuery struct {
	Variant           string
	Pool              string
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
func (q DedicationCollectiveQuery) Validate() error {
	if !IsDedicationVariant(q.Variant) {
		return errors.New("variant must be one of " + strings.Join(DedicationVariants, ", "))
	}
	if q.Page < 1 {
		return errors.New("page must be a positive integer")
	}
	maxSize := CollectiveMaxPageSize
	if q.Legacy {
		maxSize = DedicationLegacyMaxLimit
	}
	if q.PageSize < 1 || q.PageSize > maxSize {
		return fmt.Errorf("pageSize must be an integer from 1 to %d", maxSize)
	}
	if _, ok := dedicationOrderBy[q.Sort]; !ok {
		return errors.New("sort must be one of price, roi, weightedRoi, roiPct")
	}
	if q.Pool != "" && !dedicationPools[q.Pool] {
		return errors.New("pool must be skill or transfigured")
	}
	if q.Color != "" && !collectiveColors[q.Color] {
		return errors.New("color must be RED, GREEN, BLUE or ALL")
	}
	if q.Budget < 0 || math.IsNaN(q.Budget) || math.IsInf(q.Budget, 0) {
		return errors.New("budget must be a finite non-negative number")
	}
	return nil
}

// dedicationRankedCTE prices the latest snapshot's Dedication outcomes at one
// variant against the input cost of their (colour, pool) from the latest
// Dedication analysis of that variant. The input cost is the same in every mode,
// so any one mode's row serves. No input cost → NO_BASE, low confidence, ROI 0:
// an unknown cost basis is not a free gem. Listings set confidence: ≥ 5 OK,
// ≥ 2 LOW/FAIR, else LOW/RISKY and low confidence.
//
// Params: $1 league, $2 variant, $3 pool (empty = both), $4 colour (empty =
// all), $5 budget (<= 0 = off), $6 hide low confidence, $7 escaped ILIKE search
// (empty = none).
const dedicationRankedCTE = `
WITH g AS (
	SELECT name, COALESCE(chaos, 0)::float8 AS chaos, COALESCE(listings, 0) AS listings, gem_color,
		CASE WHEN is_transfigured THEN 'transfigured' ELSE 'skill' END AS pool
	FROM gem_snapshots
	WHERE league = $1 AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
	  AND variant = $2::text AND ` + sqlIsDedicationOutcome + `
), cost AS (
	SELECT DISTINCT ON (color, gem_type) color, gem_type, COALESCE(input_cost, 0)::float8 AS input_cost
	FROM dedication_snapshots
	WHERE league = $1 AND variant = $2::text
	  AND time = (SELECT MAX(time) FROM dedication_snapshots WHERE league = $1 AND variant = $2::text)
	ORDER BY color, gem_type, mode
), priced AS (
	SELECT g.name, g.chaos, g.listings, g.gem_color, g.pool, COALESCE(cost.input_cost, 0) AS input_cost
	FROM g LEFT JOIN cost ON cost.color = g.gem_color AND cost.gem_type = g.pool
), ranked AS (
	SELECT name, chaos, listings, gem_color, pool,
		input_cost <= 0 AS no_base,
		CASE WHEN input_cost > 0 THEN input_cost ELSE 0 END AS base_price,
		CASE WHEN input_cost > 0 THEN chaos - input_cost ELSE 0 END AS roi,
		CASE WHEN input_cost > 0 THEN (chaos - input_cost) / input_cost * 100 ELSE 0 END AS roi_pct,
		CASE WHEN input_cost <= 0 THEN 'NO_BASE' WHEN listings >= 5 THEN 'OK' ELSE 'LOW' END AS confidence,
		CASE WHEN input_cost <= 0 OR listings >= 5 THEN '' WHEN listings >= 2 THEN 'FAIR' ELSE 'RISKY' END AS sell_confidence,
		(input_cost <= 0 OR listings < 2) AS low_confidence
	FROM priced
), matched AS (
	SELECT * FROM ranked
	WHERE ($3::text = '' OR pool = $3::text)
	  AND ($4::text = '' OR gem_color = $4::text)
	  AND ($5::float8 <= 0 OR (NOT no_base AND base_price <= $5::float8))
	  AND (NOT $6::boolean OR NOT low_confidence)
	  AND ($7::text = '' OR name ILIKE '%' || $7::text || '%')
)`

// dedicationOrderBy is the only source of the Dedication page's ORDER BY.
// weightedRoi is the raw ROI: Dedication rankings carry no risk adjustment.
var dedicationOrderBy = map[CollectiveSort]string{
	CollectiveSortPrice:       "chaos DESC, name ASC",
	CollectiveSortROI:         "roi DESC, chaos DESC, name ASC",
	CollectiveSortWeightedROI: "roi DESC, chaos DESC, name ASC",
	CollectiveSortROIPct:      "roi_pct DESC, chaos DESC, name ASC",
}

// DedicationCollectivePage counts and pages the Dedication ranking of one
// variant in one repeatable-read, read-only transaction.
func (r *Repository) DedicationCollectivePage(ctx context.Context, scope league.Scope, q DedicationCollectiveQuery) (CollectivePage, error) {
	if err := scope.Validate(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective page: %w", err)
	}
	if err := q.Validate(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective page: %w", err)
	}
	limit, offset := q.PageSize, math.MaxInt64
	if q.Page-1 <= math.MaxInt64/q.PageSize {
		offset = (q.Page - 1) * q.PageSize
	}
	if q.Search != "" && !q.Legacy {
		limit, offset = CollectiveSearchLimit, 0
	}
	args := []any{scope.ID(), q.Variant, q.Pool, q.Color, q.Budget, q.HideLowConfidence, likeEscaper.Replace(q.Search)}

	tx, err := r.pool.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective page begin: %w", err)
	}
	defer tx.Rollback(ctx)

	var out CollectivePage
	if err := tx.QueryRow(ctx, dedicationRankedCTE+` SELECT COUNT(*) FROM matched`, args...).Scan(&out.Total); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective count: %w", err)
	}
	rows, err := tx.Query(ctx,
		dedicationRankedCTE+` SELECT name, pool, COALESCE(gem_color, ''), chaos, listings, base_price, roi, roi_pct,
			confidence, sell_confidence, low_confidence
		FROM matched ORDER BY `+dedicationOrderBy[q.Sort]+` LIMIT $8 OFFSET $9`,
		append(args, limit, offset)...)
	if err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective page query: %w", err)
	}
	defer rows.Close()
	variant := strings.TrimSuffix(q.Variant, "c")
	for rows.Next() {
		cr := CollectiveResult{Variant: variant}
		if err := rows.Scan(&cr.TransfiguredName, &cr.BaseName, &cr.GemColor, &cr.TransfiguredPrice, &cr.TransfiguredListings,
			&cr.BasePrice, &cr.ROI, &cr.ROIPct, &cr.Confidence, &cr.SellConfidence, &cr.LowConfidence); err != nil {
			return CollectivePage{}, fmt.Errorf("lab repo: scan dedication collective row: %w", err)
		}
		out.Rows = append(out.Rows, cr)
	}
	if err := rows.Err(); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective rows: %w", err)
	}
	if err := tx.Commit(ctx); err != nil {
		return CollectivePage{}, fmt.Errorf("lab repo: dedication collective page commit: %w", err)
	}
	return out, nil
}
