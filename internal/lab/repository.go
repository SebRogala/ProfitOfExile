package lab

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// Repository handles analysis data persistence.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates an analysis repository.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// LatestGemPrices returns all gem prices from the most recent snapshot.
// Returns (nil, zero-time, nil) when no snapshots exist yet.
func (r *Repository) LatestGemPrices(ctx context.Context, scope league.Scope) ([]GemPrice, time.Time, error) {
	if err := scope.Validate(); err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: latest gem prices: %w", err)
	}
	var snapTime *time.Time
	err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots WHERE league = $1", scope.ID()).Scan(&snapTime)
	if err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: latest snapshot time: %w", err)
	}
	if snapTime == nil {
		return nil, time.Time{}, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		       is_transfigured, is_corrupted, COALESCE(gem_color, '')
		FROM gem_snapshots
		WHERE league = $1 AND time = $2`, scope.ID(), *snapTime)
	if err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: query latest gems: %w", err)
	}
	defer rows.Close()

	var gems []GemPrice
	for rows.Next() {
		var g GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings,
			&g.IsTransfigured, &g.IsCorrupted, &g.GemColor); err != nil {
			return nil, time.Time{}, fmt.Errorf("lab repo: scan gem: %w", err)
		}
		gems = append(gems, g)
	}
	if err := rows.Err(); err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: rows iteration: %w", err)
	}

	return gems, *snapTime, nil
}

// SaveTransfigureResults batch-inserts transfigure analysis results.
func (r *Repository) SaveTransfigureResults(ctx context.Context, scope league.Scope, results []TransfigureResult) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save transfigure results: %w", err)
	}
	if len(results) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, r := range results {
		batch.Queue(
			`INSERT INTO transfigure_results
			 (league, time, base_name, transfigured_name, variant, base_price, transfigured_price,
			  roi, roi_pct, base_listings, transfigured_listings, gem_color, confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), r.Time, r.BaseName, r.TransfiguredName, r.Variant,
			r.BasePrice, r.TransfiguredPrice, r.ROI, r.ROIPct,
			r.BaseListings, r.TransfiguredListings, r.GemColor, r.Confidence,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert transfigure result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit transfigure results: %w", err)
	}

	return inserted, nil
}

// LatestGCPPrice returns the most recent GCP (Gemcutter's Prism) price from currency_snapshots.
// Returns an error when no data is available; the caller decides the fallback.
func (r *Repository) LatestGCPPrice(ctx context.Context, scope league.Scope) (float64, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: latest GCP price: %w", err)
	}
	var chaos *float64
	err := r.pool.QueryRow(ctx, `
		SELECT chaos FROM currency_snapshots
		WHERE league = $1 AND currency_id = 'gcp'
		ORDER BY time DESC LIMIT 1`, scope.ID()).Scan(&chaos)
	if err != nil {
		return 0, fmt.Errorf("lab repo: latest GCP price: %w", err)
	}
	if chaos == nil {
		return 0, fmt.Errorf("lab repo: GCP price is NULL in latest snapshot")
	}
	return *chaos, nil
}

// SaveFontResults batch-inserts Font of Divine Skill analysis results.
func (r *Repository) SaveFontResults(ctx context.Context, scope league.Scope, results []FontResult) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save font results: %w", err)
	}
	if len(results) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, r := range results {
		batch.Queue(
			`INSERT INTO font_snapshots
			 (league, time, color, variant, pool, priced_gems, winners, p_win, avg_win, ev, input_cost, profit,
			  mode, thin_pool_gems, liquidity_risk)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), r.Time, r.Color, r.Variant, r.Pool, r.PricedGems, r.Winners,
			r.PWin, r.AvgWin, r.EV, r.InputCost, r.Profit,
			r.Mode, r.ThinPoolGems, r.LiquidityRisk,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert font result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit font results: %w", err)
	}

	return inserted, nil
}

// SaveQualityResults batch-inserts quality-roll analysis results.
func (r *Repository) SaveQualityResults(ctx context.Context, scope league.Scope, results []QualityResult) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save quality results: %w", err)
	}
	if len(results) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, r := range results {
		batch.Queue(
			`INSERT INTO quality_results
			 (league, time, name, level, buy_price, price_q20, roi_4, roi_6, roi_10, roi_15,
			  avg_roi, gcp_price, listings_0, listings_20, gem_color, confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), r.Time, r.Name, r.Level, r.BuyPrice, r.PriceQ20,
			r.ROI4, r.ROI6, r.ROI10, r.ROI15, r.AvgROI,
			r.GCPPrice, r.Listings0, r.Listings20, r.GemColor, r.Confidence,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert quality result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit quality results: %w", err)
	}

	return inserted, nil
}

// LatestFontResults returns the most recent Font analysis results, optionally filtered by variant and/or mode.
func (r *Repository) LatestFontResults(ctx context.Context, scope league.Scope, variant, mode string, limit int) ([]FontResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest font results: %w", err)
	}
	query := `
		SELECT time, color, variant, pool, COALESCE(priced_gems, pool), winners, p_win, avg_win, ev, input_cost, profit,
		       COALESCE(mode, 'safe'), COALESCE(thin_pool_gems, 0), COALESCE(liquidity_risk, 'LOW')
		FROM font_snapshots
		WHERE league = $1 AND time = (SELECT MAX(time) FROM font_snapshots WHERE league = $1)`
	args := []any{scope.ID()}
	argIdx := 2

	if variant != "" {
		query += fmt.Sprintf(` AND variant = $%d`, argIdx)
		args = append(args, variant)
		argIdx++
	}
	if mode != "" {
		query += fmt.Sprintf(` AND mode = $%d`, argIdx)
		args = append(args, mode)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY profit DESC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query font results: %w", err)
	}
	defer rows.Close()

	var results []FontResult
	for rows.Next() {
		var fr FontResult
		if err := rows.Scan(&fr.Time, &fr.Color, &fr.Variant, &fr.Pool, &fr.PricedGems, &fr.Winners,
			&fr.PWin, &fr.AvgWin, &fr.EV, &fr.InputCost, &fr.Profit,
			&fr.Mode, &fr.ThinPoolGems, &fr.LiquidityRisk); err != nil {
			return nil, fmt.Errorf("lab repo: scan font result: %w", err)
		}
		// Derive FontsToHit from PWin (not stored in DB).
		if fr.PWin > 0 {
			fr.FontsToHit = 1.0 / fr.PWin
		}
		results = append(results, fr)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: font rows iteration: %w", err)
	}

	return results, nil
}

// LatestQualityResults returns the most recent quality-roll analysis results, optionally filtered by variant.
func (r *Repository) LatestQualityResults(ctx context.Context, scope league.Scope, variant string, limit int) ([]QualityResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest quality results: %w", err)
	}
	query := `
		SELECT time, name, level, buy_price, price_q20, roi_4, roi_6, roi_10, roi_15,
		       avg_roi, gcp_price, listings_0, listings_20, COALESCE(gem_color, ''), confidence
		FROM quality_results
		WHERE league = $1 AND time = (SELECT MAX(time) FROM quality_results WHERE league = $1)`
	args := []any{scope.ID()}

	if variant != "" {
		// variant here maps to level: "1" or "1/20" → level 1, "20" or "20/20" → level 20
		query += ` AND level = $2 ORDER BY avg_roi DESC LIMIT $3`
		level := 20
		if variant == "1" || variant == "1/20" {
			level = 1
		}
		args = append(args, level, limit)
	} else {
		query += ` ORDER BY avg_roi DESC LIMIT $2`
		args = append(args, limit)
	}

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query quality results: %w", err)
	}
	defer rows.Close()

	var results []QualityResult
	for rows.Next() {
		var qr QualityResult
		if err := rows.Scan(&qr.Time, &qr.Name, &qr.Level, &qr.BuyPrice, &qr.PriceQ20,
			&qr.ROI4, &qr.ROI6, &qr.ROI10, &qr.ROI15, &qr.AvgROI,
			&qr.GCPPrice, &qr.Listings0, &qr.Listings20, &qr.GemColor, &qr.Confidence); err != nil {
			return nil, fmt.Errorf("lab repo: scan quality result: %w", err)
		}
		results = append(results, qr)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: quality rows iteration: %w", err)
	}

	return results, nil
}

// BasePriceHistory returns time-series data for base (non-transfigured, non-corrupted) gems
// within the given number of hours. Returns a map of baseName → []PricePoint.
// Only includes analysis variants and excludes Trarthus.
func (r *Repository) BasePriceHistory(ctx context.Context, scope league.Scope, variant string, hours int) (map[string][]PricePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: base price history: %w", err)
	}
	query := `
		SELECT name, time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time > NOW() - make_interval(hours => $2)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = ANY($3)`
	args := []any{scope.ID(), hours, []string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query += ` AND variant = $4`
		args = append(args, variant)
	}

	query += ` ORDER BY name, time ASC LIMIT 500000`

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query base price history: %w", err)
	}
	defer rows.Close()

	result := make(map[string][]PricePoint)
	for rows.Next() {
		var name string
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&name, &t, &chaos, &listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan base price history: %w", err)
		}
		result[name] = append(result[name], PricePoint{Time: t, Chaos: chaos, Listings: listings})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: base price history rows: %w", err)
	}

	return result, nil
}

// MarketAvgBaseListings computes the average listings across all base gems at the latest snapshot.
// This is the denominator for relative liquidity — it naturally adjusts for weekend/weekday,
// league phase, and time of day.
func (r *Repository) MarketAvgBaseListings(ctx context.Context, scope league.Scope, variant string) (float64, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: market avg base listings: %w", err)
	}
	query := `
		SELECT COALESCE(AVG(COALESCE(listings, 0)), 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = ANY($2)`
	args := []any{scope.ID(), []string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query = `
		SELECT COALESCE(AVG(COALESCE(listings, 0)), 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = $2`
		args = []any{scope.ID(), variant}
	}

	var avg float64
	err := r.pool.QueryRow(ctx, query, args...).Scan(&avg)
	if err != nil {
		return 0, fmt.Errorf("lab repo: market avg base listings: %w", err)
	}
	return avg, nil
}

// GemPriceHistoryByVariant returns time-series gem data for transfigured, non-corrupted gems
// within the given number of hours, grouped by (name, variant).
// Only includes variants "1", "1/20", "20", "20/20", chaos > 5, and excludes Trarthus.
func (r *Repository) GemPriceHistoryByVariant(ctx context.Context, scope league.Scope, variant string, hours int) ([]GemPriceHistory, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: gem price history by variant: %w", err)
	}
	query := `
		SELECT name, variant, COALESCE(gem_color, ''), time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time > NOW() - make_interval(hours => $2)
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND COALESCE(chaos, 0) > 5
		  AND variant = ANY($3)`
	args := []any{scope.ID(), hours, []string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query += ` AND variant = $4`
		args = append(args, variant)
	}

	query += ` ORDER BY name, variant, time ASC LIMIT 500000`

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem price history: %w", err)
	}
	defer rows.Close()

	type histKey struct{ name, variant string }
	index := make(map[histKey]*GemPriceHistory)
	var order []histKey

	for rows.Next() {
		var name, v, color string
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&name, &v, &color, &t, &chaos, &listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem price history: %w", err)
		}

		k := histKey{name, v}
		h, exists := index[k]
		if !exists {
			h = &GemPriceHistory{Name: name, Variant: v, GemColor: color}
			index[k] = h
			order = append(order, k)
		}
		h.Points = append(h.Points, PricePoint{Time: t, Chaos: chaos, Listings: listings})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem price history rows: %w", err)
	}

	result := make([]GemPriceHistory, 0, len(order))
	for _, k := range order {
		result = append(result, *index[k])
	}
	return result, nil
}

// LatestTransfigureResults returns the most recent analysis results, optionally filtered by variant.
func (r *Repository) LatestTransfigureResults(ctx context.Context, scope league.Scope, variant string, limit int) ([]TransfigureResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest transfigure results: %w", err)
	}
	query := `
		SELECT time, base_name, transfigured_name, variant, base_price, transfigured_price,
		       roi, roi_pct, base_listings, transfigured_listings, COALESCE(gem_color, ''), confidence
		FROM transfigure_results
		WHERE league = $1 AND time = (SELECT MAX(time) FROM transfigure_results WHERE league = $1)`
	args := []any{scope.ID()}

	if variant != "" {
		query += ` AND variant = $2 ORDER BY roi DESC LIMIT $3`
		args = append(args, variant, limit)
	} else {
		query += ` ORDER BY roi DESC LIMIT $2`
		args = append(args, limit)
	}

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query transfigure results: %w", err)
	}
	defer rows.Close()

	var results []TransfigureResult
	for rows.Next() {
		var r TransfigureResult
		if err := rows.Scan(&r.Time, &r.BaseName, &r.TransfiguredName, &r.Variant,
			&r.BasePrice, &r.TransfiguredPrice, &r.ROI, &r.ROIPct,
			&r.BaseListings, &r.TransfiguredListings, &r.GemColor, &r.Confidence); err != nil {
			return nil, fmt.Errorf("lab repo: scan transfigure result: %w", err)
		}
		results = append(results, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: transfigure rows iteration: %w", err)
	}

	return results, nil
}

// SparklineData returns raw price points for specific transfigured gems over the given hours.
// Used for sparkline charts in the gem comparator.
func (r *Repository) SparklineData(ctx context.Context, scope league.Scope, names []string, variant string, hours int) (map[string][]SparklinePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline data: %w", err)
	}
	if len(names) == 0 {
		return nil, nil
	}

	query := `
		SELECT name, time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE league = $1 AND name = ANY($2) AND is_corrupted = false
		  AND time > NOW() - make_interval(hours => $3)`
	args := []any{scope.ID(), names, hours}

	if variant != "" {
		query += ` AND variant = $4`
		args = append(args, variant)
	}

	query += ` ORDER BY name, time ASC LIMIT 10000`

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query sparkline data: %w", err)
	}
	defer rows.Close()

	result := make(map[string][]SparklinePoint)
	for rows.Next() {
		var name string
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&name, &t, &chaos, &listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan sparkline point: %w", err)
		}
		result[name] = append(result[name], SparklinePoint{
			Time:     t.UTC().Format(time.RFC3339),
			Price:    chaos,
			Listings: listings,
		})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline rows iteration: %w", err)
	}

	return result, nil
}

// sparklineVariants are the non-corrupted variants sparkline consumers ask for.
// Prices for different variants are different markets, so the variant is part
// of the series identity rather than something merged away.
var sparklineVariants = []string{"1", "1/20", "20", "20/20"}

// sparklineCorruptedVariant is the only corrupted variant sparklines are served
// for — the Dedication (21/23c) pool.
const sparklineCorruptedVariant = "21/23c"

// sparklineRowFilter is the row predicate every sparkline read shares: the
// scoped league, an upper bound on row age, and the served-variant allowlist
// split by corruption. hoursParam names the placeholder carrying the age bound,
// so the same predicate can be reused at a different bound.
//
// Kept in one place because the cold read applies it three times at two
// different bounds. The variant allowlist and corruption split must be identical
// in all three: the tail half and the window half would otherwise cover
// different key spaces, and their union would no longer be one coherent corpus.
func sparklineRowFilter(hoursParam string) string {
	return `league = $1
		  AND time > NOW() - make_interval(hours => ` + hoursParam + `)
		  AND ((is_corrupted = false AND variant = ANY($3))
		    OR (is_corrupted = true AND variant = $4))`
}

// sparklineIncrementalQuery reads only rows strictly newer than the caller's
// high-water mark ($5). On a running server that mark is one tick old, so the
// result is a single snapshot's worth of rows and needs no further bound.
var sparklineIncrementalQuery = `
	SELECT name, variant, time, COALESCE(chaos, 0), COALESCE(listings, 0), is_corrupted
	FROM gem_snapshots
	WHERE ` + sparklineRowFilter("$2") + `
	  AND time > $5
	ORDER BY name, variant, time ASC`

// sparklineColdQuery reads exactly what a cold cache retains and nothing more:
// every row inside the served window ($6 hours), plus the newest $5 rows per
// series within the $2-hour lookback.
//
// That union is a superset of what mergeSparklineSeries keeps. The merge keeps
// every in-window point, and extends backwards only far enough to reach
// SparklineTailPoints — points that are, by construction, among the newest $5
// rows of the series. Reading the flat $2-hour window instead pulls roughly
// fourteen times the rows for the same result, and holds them live for the whole
// merge at the one moment (process start) the analysis pass is also loading its
// own history.
//
// The two halves overlap for any series still trading; UNION collapses the
// duplicates so each timestamp arrives once.
var sparklineColdQuery = `
	WITH keys AS (
		SELECT DISTINCT name, variant, is_corrupted
		FROM gem_snapshots
		WHERE ` + sparklineRowFilter("$2") + `
	),
	tail AS (
		SELECT t.name, t.variant, t.time, t.chaos, t.listings, t.is_corrupted
		FROM keys k
		CROSS JOIN LATERAL (
			SELECT g.name, g.variant, g.time,
			       COALESCE(g.chaos, 0) AS chaos,
			       COALESCE(g.listings, 0) AS listings,
			       g.is_corrupted
			FROM gem_snapshots g
			WHERE g.league = $1
			  AND g.name = k.name
			  AND g.variant = k.variant
			  AND g.is_corrupted = k.is_corrupted
			  AND g.time > NOW() - make_interval(hours => $2)
			ORDER BY g.time DESC
			LIMIT $5
		) t
	),
	win AS (
		SELECT name, variant, time,
		       COALESCE(chaos, 0) AS chaos,
		       COALESCE(listings, 0) AS listings,
		       is_corrupted
		FROM gem_snapshots
		WHERE ` + sparklineRowFilter("$6") + `
	)
	SELECT name, variant, time, chaos, listings, is_corrupted
	FROM (SELECT * FROM win UNION SELECT * FROM tail) u
	ORDER BY name, variant, time ASC`

// SparklineWindow returns raw price points for every gem/variant series the
// sparkline cache serves, in one pass. The first map holds the non-corrupted
// series, the second the corrupted (Dedication) series.
//
// since selects the read shape. A non-zero value takes the incremental path and
// returns only rows newer than the caller's high-water mark. The zero time takes
// the cold path, which is bounded by every field of bounds — see
// sparklineColdQuery for why a flat LookbackHours read is not used there.
//
// The window bound is evaluated against the database clock while the caller
// windows against its own. Skew of a few minutes only shifts which
// about-to-expire points arrive; the per-series tail is unconditional, so every
// series still gets its newest TailPoints rows regardless.
//
// Deliberately unlike GemPriceHistoryByVariant, this applies no chaos floor and
// no Trarthus exclusion — SparklineData has never applied either, and the chaos
// floor is tracked separately as POE-134. It also does not filter
// is_transfigured, because TrendAnalysis charts base gems too.
func (r *Repository) SparklineWindow(ctx context.Context, scope league.Scope, since time.Time, bounds SparklineBounds) (map[sparklineKey][]SparklinePoint, map[sparklineKey][]SparklinePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, nil, fmt.Errorf("lab repo: sparkline window: %w", err)
	}

	query := sparklineIncrementalQuery
	args := []any{scope.ID(), bounds.LookbackHours, sparklineVariants, sparklineCorruptedVariant, since}
	if since.IsZero() {
		query = sparklineColdQuery
		args = []any{
			scope.ID(), bounds.LookbackHours, sparklineVariants, sparklineCorruptedVariant,
			bounds.TailPoints, bounds.WindowHours,
		}
	}

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, nil, fmt.Errorf("lab repo: query sparkline window: %w", err)
	}
	defer rows.Close()

	series := make(map[sparklineKey][]SparklinePoint)
	corrupted := make(map[sparklineKey][]SparklinePoint)

	for rows.Next() {
		var name, variant string
		var t time.Time
		var chaos float64
		var listings int
		var isCorrupted bool
		if err := rows.Scan(&name, &variant, &t, &chaos, &listings, &isCorrupted); err != nil {
			return nil, nil, fmt.Errorf("lab repo: scan sparkline window point: %w", err)
		}

		k := sparklineKey{name: name, variant: variant}
		pt := SparklinePoint{
			Time:     t.UTC().Format(time.RFC3339),
			Price:    chaos,
			Listings: listings,
		}
		if isCorrupted {
			corrupted[k] = append(corrupted[k], pt)
			continue
		}
		series[k] = append(series[k], pt)
	}
	if err := rows.Err(); err != nil {
		return nil, nil, fmt.Errorf("lab repo: sparkline window rows iteration: %w", err)
	}

	return series, corrupted, nil
}

// GemNamesAutocomplete returns distinct transfigured gem names matching all query words (in any order).
func (r *Repository) GemNamesAutocomplete(ctx context.Context, scope league.Scope, query string, limit int) ([]string, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: gem names autocomplete: %w", err)
	}
	if query == "" {
		return nil, nil
	}

	escaper := strings.NewReplacer(`%`, `\%`, `_`, `\_`)
	words := strings.Fields(query)
	// $1 is the league; ILIKE conditions start at $2, LIMIT is the final arg.
	conditions := make([]string, len(words))
	args := make([]any, 0, len(words)+2)
	args = append(args, scope.ID())
	for i, w := range words {
		args = append(args, "%"+escaper.Replace(w)+"%")
		conditions[i] = fmt.Sprintf("name ILIKE $%d", i+2)
	}
	args = append(args, limit)

	// A whitespace-only query has no words — callers use it to mean "every name"
	// (the cache path treats it that way). Emitting an empty condition list here
	// would produce "AND ORDER BY", a syntax error that 500s the endpoint.
	filter := "TRUE"
	if len(conditions) > 0 {
		filter = strings.Join(conditions, " AND ")
	}

	sql := fmt.Sprintf(`
		SELECT DISTINCT name FROM gem_snapshots
		WHERE league = $1 AND is_transfigured = true AND %s
		ORDER BY name LIMIT $%d`,
		filter, len(args))

	rows, err := r.pool.Query(ctx, sql, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem names autocomplete: %w", err)
	}
	defer rows.Close()

	var names []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem name: %w", err)
		}
		names = append(names, name)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem names rows iteration: %w", err)
	}

	return names, nil
}

// SignalChange represents a single signal transition for a gem.
type SignalChange struct {
	Time      time.Time `json:"time"`
	Signal    string    `json:"signal"`
	Window    string    `json:"window"`
	Advanced  string    `json:"advanced"`
	PriceVel  float64   `json:"priceVelocity"`
	ListVel   float64   `json:"listingVelocity"`
	Price     float64   `json:"currentPrice"`
	Listings  int       `json:"currentListings"`
}

// SignalHistory returns the last N signal snapshots for a gem, used to show transitions.
// Queries gem_signals joined with gem_features for velocity and price data.
func (r *Repository) SignalHistory(ctx context.Context, scope league.Scope, name, variant string, limit int) ([]SignalChange, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT s.time, s.signal, s.window_signal, COALESCE(s.advanced_signal, ''),
		       COALESCE(f.vel_long_price, 0), COALESCE(f.vel_long_listing, 0),
		       COALESCE(f.chaos, 0), COALESCE(f.listings, 0)
		FROM gem_signals s
		LEFT JOIN gem_features f ON f.league = s.league AND f.time = s.time AND f.name = s.name AND f.variant = s.variant
		WHERE s.league = $1 AND s.name = $2 AND s.variant = $3
		ORDER BY s.time DESC
		LIMIT $4`, scope.ID(), name, variant, limit)
	if err != nil {
		return nil, fmt.Errorf("lab repo: signal history: %w", err)
	}
	defer rows.Close()

	var changes []SignalChange
	for rows.Next() {
		var c SignalChange
		if err := rows.Scan(&c.Time, &c.Signal, &c.Window, &c.Advanced,
			&c.PriceVel, &c.ListVel, &c.Price, &c.Listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan signal history: %w", err)
		}
		changes = append(changes, c)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history iteration: %w", err)
	}

	return changes, nil
}

// ---------------------------------------------------------------------------
// V2 Pre-computed Storage Layer
// ---------------------------------------------------------------------------

// SaveMarketContext persists a single market context snapshot.
// Uses ON CONFLICT DO NOTHING so re-runs for the same time are idempotent.
func (r *Repository) SaveMarketContext(ctx context.Context, scope league.Scope, mc MarketContext) error {
	if err := scope.Validate(); err != nil {
		return fmt.Errorf("lab repo: save market context: %w", err)
	}
	if err := mc.ValidateTemporalSlices(); err != nil {
		return fmt.Errorf("lab repo: save market context: %w", err)
	}

	pricePerc, err := json.Marshal(mc.PricePercentiles)
	if err != nil {
		return fmt.Errorf("lab repo: marshal price percentiles: %w", err)
	}
	listPerc, err := json.Marshal(mc.ListingPercentiles)
	if err != nil {
		return fmt.Errorf("lab repo: marshal listing percentiles: %w", err)
	}
	tierBounds, err := json.Marshal(mc.TierBoundaries)
	if err != nil {
		return fmt.Errorf("lab repo: marshal tier boundaries: %w", err)
	}
	hourly, err := json.Marshal(mc.HourlyBias)
	if err != nil {
		return fmt.Errorf("lab repo: marshal hourly bias: %w", err)
	}
	hourlyVol, err := json.Marshal(mc.HourlyVolatility)
	if err != nil {
		return fmt.Errorf("lab repo: marshal hourly volatility: %w", err)
	}
	hourlyAct, err := json.Marshal(mc.HourlyActivity)
	if err != nil {
		return fmt.Errorf("lab repo: marshal hourly activity: %w", err)
	}
	weekday, err := json.Marshal(mc.WeekdayBias)
	if err != nil {
		return fmt.Errorf("lab repo: marshal weekday bias: %w", err)
	}
	weekdayVol, err := json.Marshal(mc.WeekdayVolatility)
	if err != nil {
		return fmt.Errorf("lab repo: marshal weekday volatility: %w", err)
	}
	weekdayAct, err := json.Marshal(mc.WeekdayActivity)
	if err != nil {
		return fmt.Errorf("lab repo: marshal weekday activity: %w", err)
	}

	variantStats, err := json.Marshal(mc.VariantStats)
	if err != nil {
		return fmt.Errorf("lab repo: marshal variant stats: %w", err)
	}

	// temporal_buckets is JSONB NOT NULL DEFAULT '{}'. A context with no computed
	// buckets (e.g. temporal mode "none") carries a nil slice; bind '{}' so it
	// matches the column default instead of violating the NOT NULL constraint.
	temporalBuckets := mc.TemporalBuckets
	if len(temporalBuckets) == 0 {
		temporalBuckets = []byte("{}")
	}

	_, err = r.pool.Exec(ctx, `
		INSERT INTO market_context
		 (league, time, price_percentiles, listing_percentiles,
		  velocity_mean, velocity_sigma, listing_vel_mean, listing_vel_sigma,
		  total_gems, total_listings, tier_boundaries,
		  hourly_bias, hourly_volatility, hourly_activity,
		  weekday_bias, weekday_volatility, weekday_activity,
		  temporal_coefficient, temporal_mode, temporal_buckets,
		  variant_stats)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
		ON CONFLICT DO NOTHING`,
		scope.ID(), mc.Time, pricePerc, listPerc,
		mc.VelocityMean, mc.VelocitySigma, mc.ListingVelMean, mc.ListingVelSigma,
		mc.TotalGems, mc.TotalListings, tierBounds,
		hourly, hourlyVol, hourlyAct,
		weekday, weekdayVol, weekdayAct,
		mc.TemporalCoefficient, mc.TemporalMode, temporalBuckets,
		variantStats,
	)
	if err != nil {
		return fmt.Errorf("lab repo: insert market context: %w", err)
	}
	return nil
}

// LatestMarketContext returns the most recent market context snapshot.
// Returns (nil, nil) when no rows exist.
func (r *Repository) LatestMarketContext(ctx context.Context, scope league.Scope) (*MarketContext, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest market context: %w", err)
	}
	var mc MarketContext
	var pricePerc, listPerc, tierBounds []byte
	var hourly, hourlyVol, hourlyAct []byte
	var weekday, weekdayVol, weekdayAct []byte
	var variantStatsJSON []byte

	err := r.pool.QueryRow(ctx, `
		SELECT time, price_percentiles, listing_percentiles,
		       velocity_mean, velocity_sigma, listing_vel_mean, listing_vel_sigma,
		       total_gems, total_listings, tier_boundaries,
		       hourly_bias, hourly_volatility, hourly_activity,
		       weekday_bias, weekday_volatility, weekday_activity,
		       temporal_coefficient, temporal_mode, temporal_buckets,
		       variant_stats
		FROM market_context
		WHERE league = $1 AND time = (SELECT MAX(time) FROM market_context WHERE league = $1)`, scope.ID()).
		Scan(&mc.Time, &pricePerc, &listPerc,
			&mc.VelocityMean, &mc.VelocitySigma, &mc.ListingVelMean, &mc.ListingVelSigma,
			&mc.TotalGems, &mc.TotalListings, &tierBounds,
			&hourly, &hourlyVol, &hourlyAct,
			&weekday, &weekdayVol, &weekdayAct,
			&mc.TemporalCoefficient, &mc.TemporalMode, &mc.TemporalBuckets,
			&variantStatsJSON)
	if err != nil {
		if errors.Is(err, pgx.ErrNoRows) {
			return nil, nil
		}
		return nil, fmt.Errorf("lab repo: query latest market context: %w", err)
	}

	if err := json.Unmarshal(pricePerc, &mc.PricePercentiles); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal price percentiles: %w", err)
	}
	if err := json.Unmarshal(listPerc, &mc.ListingPercentiles); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal listing percentiles: %w", err)
	}
	if err := json.Unmarshal(tierBounds, &mc.TierBoundaries); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal tier boundaries: %w", err)
	}
	if err := json.Unmarshal(hourly, &mc.HourlyBias); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal hourly bias: %w", err)
	}
	if err := json.Unmarshal(hourlyVol, &mc.HourlyVolatility); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal hourly volatility: %w", err)
	}
	if err := json.Unmarshal(hourlyAct, &mc.HourlyActivity); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal hourly activity: %w", err)
	}
	if err := json.Unmarshal(weekday, &mc.WeekdayBias); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal weekday bias: %w", err)
	}
	if err := json.Unmarshal(weekdayVol, &mc.WeekdayVolatility); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal weekday volatility: %w", err)
	}
	if err := json.Unmarshal(weekdayAct, &mc.WeekdayActivity); err != nil {
		return nil, fmt.Errorf("lab repo: unmarshal weekday activity: %w", err)
	}
	if len(variantStatsJSON) > 0 {
		if err := json.Unmarshal(variantStatsJSON, &mc.VariantStats); err != nil {
			return nil, fmt.Errorf("lab repo: unmarshal variant stats: %w", err)
		}
	}

	if err := mc.ValidateTemporalSlices(); err != nil {
		return nil, fmt.Errorf("lab repo: latest market context: %w", err)
	}

	return &mc, nil
}

// SaveGemFeatures batch-inserts pre-computed gem feature rows.
func (r *Repository) SaveGemFeatures(ctx context.Context, scope league.Scope, features []GemFeature) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save gem features: %w", err)
	}
	if len(features) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, f := range features {
		batch.Queue(
			`INSERT INTO gem_features
			 (league, time, name, variant, chaos, listings, tier, global_tier,
			  vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
			  vel_long_price, vel_long_listing,
			  cv, hist_position, high_7d, low_7d,
			  flood_count, crash_count, listing_elasticity,
			  relative_price, relative_listings,
			  sell_probability_factor, stability_discount,
			  market_depth, market_regime, low_confidence, gem_color)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
			         $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25,
			         $26, $27, $28, $29)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), f.Time, f.Name, f.Variant, f.Chaos, f.Listings, f.Tier, f.Tier,
			f.VelShortPrice, f.VelShortListing, f.VelMedPrice, f.VelMedListing,
			f.VelLongPrice, f.VelLongListing,
			f.CV, f.HistPosition, f.High7Days, f.Low7Days,
			f.FloodCount, f.CrashCount, f.ListingElasticity,
			f.RelativePrice, f.RelativeListings,
			f.SellProbabilityFactor, f.StabilityDiscount,
			f.MarketDepth, f.MarketRegime, f.LowConfidence, f.GemColor,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range features {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert gem feature: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit gem features: %w", err)
	}

	return inserted, nil
}

// LatestGemFeatures returns the most recent gem feature rows, optionally filtered by variant and/or tier.
func (r *Repository) LatestGemFeatures(ctx context.Context, scope league.Scope, variant, tier string, limit int) ([]GemFeature, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest gem features: %w", err)
	}
	query := `
		SELECT time, name, variant, chaos, listings, tier, COALESCE(global_tier, ''),
		       vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
		       vel_long_price, vel_long_listing,
		       cv, hist_position, high_7d, low_7d,
		       flood_count, crash_count, listing_elasticity,
		       relative_price, relative_listings,
		       sell_probability_factor, stability_discount,
		       COALESCE(market_depth, 0), COALESCE(market_regime, 'TEMPORAL'),
		       COALESCE(low_confidence, false), COALESCE(gem_color, '')
		FROM gem_features
		WHERE league = $1 AND time = (SELECT MAX(time) FROM gem_features WHERE league = $1)`
	args := []any{scope.ID()}
	argIdx := 2

	if variant != "" {
		query += fmt.Sprintf(` AND variant = $%d`, argIdx)
		args = append(args, variant)
		argIdx++
	}
	if tier != "" {
		query += fmt.Sprintf(` AND tier = $%d`, argIdx)
		args = append(args, tier)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY chaos DESC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem features: %w", err)
	}
	defer rows.Close()

	var results []GemFeature
	for rows.Next() {
		var f GemFeature
		var ignoredGlobalTier string
		if err := rows.Scan(&f.Time, &f.Name, &f.Variant, &f.Chaos, &f.Listings, &f.Tier, &ignoredGlobalTier,
			&f.VelShortPrice, &f.VelShortListing, &f.VelMedPrice, &f.VelMedListing,
			&f.VelLongPrice, &f.VelLongListing,
			&f.CV, &f.HistPosition, &f.High7Days, &f.Low7Days,
			&f.FloodCount, &f.CrashCount, &f.ListingElasticity,
			&f.RelativePrice, &f.RelativeListings,
			&f.SellProbabilityFactor, &f.StabilityDiscount,
			&f.MarketDepth, &f.MarketRegime, &f.LowConfidence, &f.GemColor); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem feature: %w", err)
		}
		results = append(results, f)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem features rows iteration: %w", err)
	}

	return results, nil
}

// DeleteV2ForSnapshot removes computed v2 data (market_context, gem_features,
// gem_signals, font_snapshots) for a specific snapshot time. Raw collector data
// (gem_snapshots, currency_snapshots, fragment_snapshots) is NOT touched.
// Used on startup to force recomputation with the latest code after a deploy.
func (r *Repository) DeleteV2ForSnapshot(ctx context.Context, scope league.Scope, snapTime time.Time) error {
	if err := scope.Validate(); err != nil {
		return fmt.Errorf("lab repo: delete v2 for snapshot: %w", err)
	}
	// TODO(POE-120): the analyzer writes 7 result tables but only 4 are cleaned
	// on recompute. transfigure_results, quality_results, and dedication_snapshots
	// accumulate stale rows across deploys behind ON CONFLICT DO NOTHING. They are
	// NOT added here because RecomputeLatestV2 re-runs only RunV2; RunTransfigure
	// and RunQuality execute in separate, uncoordinated startup goroutines, so
	// deleting their tables here would race a concurrent write or leave them empty.
	// The real fix is to coordinate the recompute flow — out of scope for POE-120.
	tables := []string{"gem_features", "gem_signals", "market_context", "font_snapshots"}
	for _, table := range tables {
		if _, err := r.pool.Exec(ctx, fmt.Sprintf("DELETE FROM %s WHERE league = $1 AND time = $2", table), scope.ID(), snapTime); err != nil {
			return fmt.Errorf("lab repo: delete %s at %v: %w", table, snapTime, err)
		}
	}
	return nil
}

// SaveGemSignals batch-inserts pre-computed gem signal rows.
func (r *Repository) SaveGemSignals(ctx context.Context, scope league.Scope, signals []GemSignal) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save gem signals: %w", err)
	}
	if len(signals) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, s := range signals {
		batch.Queue(
			`INSERT INTO gem_signals
			 (league, time, name, variant, signal, confidence,
			  sell_urgency, sell_reason, sellability, sellability_label,
			  window_signal, advanced_signal, phase_modifier,
			  recommendation, tier,
			  risk_adjusted_value, quick_sell_price, sell_confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15,
			         $16, $17, $18)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), s.Time, s.Name, s.Variant, s.Signal, s.Confidence,
			s.SellUrgency, s.SellReason, s.Sellability, s.SellabilityLabel,
			s.WindowSignal, s.AdvancedSignal, s.PhaseModifier,
			s.Recommendation, s.Tier,
			s.RiskAdjustedValue, s.QuickSellPrice, s.SellConfidence,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range signals {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert gem signal: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit gem signals: %w", err)
	}

	return inserted, nil
}

// LatestGemSignals returns the most recent gem signal rows, optionally filtered by variant and/or tier.
func (r *Repository) LatestGemSignals(ctx context.Context, scope league.Scope, variant, tier string, limit int) ([]GemSignal, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest gem signals: %w", err)
	}
	query := `
		SELECT time, name, variant, signal, confidence,
		       sell_urgency, sell_reason, sellability, sellability_label,
		       window_signal, advanced_signal, phase_modifier,
		       recommendation, tier,
		       risk_adjusted_value, quick_sell_price, sell_confidence
		FROM gem_signals
		WHERE league = $1 AND time = (SELECT MAX(time) FROM gem_signals WHERE league = $1)`
	args := []any{scope.ID()}
	argIdx := 2

	if variant != "" {
		query += fmt.Sprintf(` AND variant = $%d`, argIdx)
		args = append(args, variant)
		argIdx++
	}
	if tier != "" {
		query += fmt.Sprintf(` AND tier = $%d`, argIdx)
		args = append(args, tier)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY confidence DESC, sellability DESC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem signals: %w", err)
	}
	defer rows.Close()

	var results []GemSignal
	for rows.Next() {
		var s GemSignal
		if err := rows.Scan(&s.Time, &s.Name, &s.Variant, &s.Signal, &s.Confidence,
			&s.SellUrgency, &s.SellReason, &s.Sellability, &s.SellabilityLabel,
			&s.WindowSignal, &s.AdvancedSignal, &s.PhaseModifier,
			&s.Recommendation, &s.Tier,
			&s.RiskAdjustedValue, &s.QuickSellPrice, &s.SellConfidence); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem signal: %w", err)
		}
		results = append(results, s)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem signals rows iteration: %w", err)
	}

	return results, nil
}

// AllGemFeaturesInRange returns all gem feature rows within the given time range (hours ago to now).
// No variant/tier filters, no limit. Ordered by time, name, variant.
func (r *Repository) AllGemFeaturesInRange(ctx context.Context, scope league.Scope, hours int) ([]GemFeature, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: all gem features in range: %w", err)
	}
	query := `
		SELECT time, name, variant, chaos, listings, tier, COALESCE(global_tier, ''),
		       vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
		       vel_long_price, vel_long_listing,
		       cv, hist_position, high_7d, low_7d,
		       flood_count, crash_count, listing_elasticity,
		       relative_price, relative_listings,
		       sell_probability_factor, stability_discount,
		       COALESCE(market_depth, 0), COALESCE(market_regime, 'TEMPORAL'),
		       COALESCE(low_confidence, false), COALESCE(gem_color, '')
		FROM gem_features
		WHERE league = $1
		  AND time > NOW() - make_interval(hours => $2)
		ORDER BY time, name, variant`

	rows, err := r.pool.Query(ctx, query, scope.ID(), hours)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query all gem features in range: %w", err)
	}
	defer rows.Close()

	var results []GemFeature
	for rows.Next() {
		var f GemFeature
		var ignoredGlobalTier string
		if err := rows.Scan(&f.Time, &f.Name, &f.Variant, &f.Chaos, &f.Listings, &f.Tier, &ignoredGlobalTier,
			&f.VelShortPrice, &f.VelShortListing, &f.VelMedPrice, &f.VelMedListing,
			&f.VelLongPrice, &f.VelLongListing,
			&f.CV, &f.HistPosition, &f.High7Days, &f.Low7Days,
			&f.FloodCount, &f.CrashCount, &f.ListingElasticity,
			&f.RelativePrice, &f.RelativeListings,
			&f.SellProbabilityFactor, &f.StabilityDiscount,
			&f.MarketDepth, &f.MarketRegime, &f.LowConfidence, &f.GemColor); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem feature in range: %w", err)
		}
		results = append(results, f)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem features in range rows iteration: %w", err)
	}

	return results, nil
}

// SnapshotPricesInRange returns lightweight price observations for transfigured,
// non-corrupted gems with chaos > 5, within the given time range (hours ago to now).
func (r *Repository) SnapshotPricesInRange(ctx context.Context, scope league.Scope, hours int) ([]SnapshotPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: snapshot prices in range: %w", err)
	}
	query := `
		SELECT time, name, variant, COALESCE(chaos, 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time > NOW() - make_interval(hours => $2)
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND chaos > 5
		ORDER BY time, name, variant`

	rows, err := r.pool.Query(ctx, query, scope.ID(), hours)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query snapshot prices in range: %w", err)
	}
	defer rows.Close()

	var results []SnapshotPrice
	for rows.Next() {
		var p SnapshotPrice
		if err := rows.Scan(&p.Time, &p.Name, &p.Variant, &p.Chaos); err != nil {
			return nil, fmt.Errorf("lab repo: scan snapshot price: %w", err)
		}
		results = append(results, p)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: snapshot prices rows iteration: %w", err)
	}

	return results, nil
}

// ---------------------------------------------------------------------------
// Dedication Lab Analysis
// ---------------------------------------------------------------------------

// SaveDedicationResults batch-inserts Dedication lab analysis results.
func (r *Repository) SaveDedicationResults(ctx context.Context, scope league.Scope, results []DedicationResult) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save dedication results: %w", err)
	}
	if len(results) == 0 {
		return 0, nil
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("lab repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	batch := &pgx.Batch{}
	for _, dr := range results {
		poolBreakdownJSON, err := json.Marshal(dr.PoolBreakdown)
		if err != nil {
			slog.Warn("lab repo: marshal pool breakdown failed, using empty array", "error", err)
			poolBreakdownJSON = []byte("[]")
		}
		lowConfJSON, err := json.Marshal(dr.LowConfidenceGems)
		if err != nil {
			slog.Warn("lab repo: marshal low confidence gems failed, using empty array", "error", err)
			lowConfJSON = []byte("[]")
		}

		batch.Queue(
			`INSERT INTO dedication_snapshots
			 (league, time, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
			  input_cost, profit, fonts_to_hit, mode, thin_pool_gems, liquidity_risk,
			  pool_breakdown, low_confidence_gems)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), dr.Time, dr.Color, dr.GemType, dr.Pool, dr.Winners,
			dr.PWin, dr.AvgWinRaw, dr.EVRaw,
			dr.InputCost, dr.Profit, dr.FontsToHit,
			dr.Mode, dr.ThinPoolGems, dr.LiquidityRisk,
			poolBreakdownJSON, lowConfJSON,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert dedication result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit dedication results: %w", err)
	}

	return inserted, nil
}

// LatestDedicationResults returns the most recent Dedication analysis results,
// optionally filtered by gemType and/or mode.
func (r *Repository) LatestDedicationResults(ctx context.Context, scope league.Scope, gemType, mode string, limit int) ([]DedicationResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest dedication results: %w", err)
	}
	query := `
		SELECT time, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
		       input_cost, profit, fonts_to_hit,
		       COALESCE(mode, 'safe'), COALESCE(thin_pool_gems, 0), COALESCE(liquidity_risk, 'LOW'),
		       COALESCE(pool_breakdown, '[]'::jsonb), COALESCE(low_confidence_gems, '[]'::jsonb)
		FROM dedication_snapshots
		WHERE league = $1 AND time = (SELECT MAX(time) FROM dedication_snapshots WHERE league = $1)`
	args := []any{scope.ID()}
	argIdx := 2

	if gemType != "" {
		query += fmt.Sprintf(` AND gem_type = $%d`, argIdx)
		args = append(args, gemType)
		argIdx++
	}
	if mode != "" {
		query += fmt.Sprintf(` AND mode = $%d`, argIdx)
		args = append(args, mode)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY profit DESC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query dedication results: %w", err)
	}
	defer rows.Close()

	var results []DedicationResult
	for rows.Next() {
		var dr DedicationResult
		var poolBreakdownJSON, lowConfJSON []byte
		if err := rows.Scan(&dr.Time, &dr.Color, &dr.GemType, &dr.Pool, &dr.Winners,
			&dr.PWin, &dr.AvgWinRaw, &dr.EVRaw,
			&dr.InputCost, &dr.Profit, &dr.FontsToHit,
			&dr.Mode, &dr.ThinPoolGems, &dr.LiquidityRisk,
			&poolBreakdownJSON, &lowConfJSON); err != nil {
			return nil, fmt.Errorf("lab repo: scan dedication result: %w", err)
		}
		if len(poolBreakdownJSON) > 0 {
			if err := json.Unmarshal(poolBreakdownJSON, &dr.PoolBreakdown); err != nil {
				return nil, fmt.Errorf("lab repo: unmarshal pool_breakdown: %w", err)
			}
		}
		if len(lowConfJSON) > 0 {
			if err := json.Unmarshal(lowConfJSON, &dr.LowConfidenceGems); err != nil {
				return nil, fmt.Errorf("lab repo: unmarshal low_confidence_gems: %w", err)
			}
		}
		results = append(results, dr)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: dedication rows iteration: %w", err)
	}

	return results, nil
}

// CorruptedGemNamesAutocomplete returns distinct corrupted gem names for autocomplete.
// When isTransfigured is true, returns only transfigured corrupted gems; when false,
// returns only non-transfigured corrupted gems. Excludes support gems.
// Restricts to the last 2 hours to avoid full hypertable scans.
func (r *Repository) CorruptedGemNamesAutocomplete(ctx context.Context, scope league.Scope, isTransfigured bool, limit int) ([]string, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem names autocomplete: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT DISTINCT name FROM gem_snapshots
		WHERE league = $1
		  AND is_corrupted = true
		  AND is_transfigured = $2
		  AND name NOT LIKE '%Support%'
		  AND name NOT LIKE '%Trarthus%'
		  AND time > NOW() - INTERVAL '2 hours'
		ORDER BY name
		LIMIT $3`, scope.ID(), isTransfigured, limit)
	if err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem names autocomplete: %w", err)
	}
	defer rows.Close()

	var names []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return nil, fmt.Errorf("lab repo: scan corrupted gem name: %w", err)
		}
		names = append(names, name)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem names rows iteration: %w", err)
	}

	return names, nil
}

// CorruptedGemPricesByNames returns the latest snapshot prices for specific corrupted gems.
// Only returns gems matching variant "21/23c". Used by the Dedication compare endpoint.
func (r *Repository) CorruptedGemPricesByNames(ctx context.Context, scope league.Scope, names []string) ([]GemPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem prices by names: %w", err)
	}
	if len(names) == 0 {
		return nil, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		       is_transfigured, is_corrupted, COALESCE(gem_color, '')
		FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
		  AND is_corrupted = true
		  AND variant = '21/23c'
		  AND name = ANY($2)`, scope.ID(), names)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query corrupted gem prices by names: %w", err)
	}
	defer rows.Close()

	var gems []GemPrice
	for rows.Next() {
		var g GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings,
			&g.IsTransfigured, &g.IsCorrupted, &g.GemColor); err != nil {
			return nil, fmt.Errorf("lab repo: scan corrupted gem price: %w", err)
		}
		gems = append(gems, g)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem prices rows iteration: %w", err)
	}

	return gems, nil
}

// SparklineDataCorrupted returns sparkline data for corrupted gems (is_corrupted = true).
// Same shape as SparklineData but filters for corrupted gems instead.
func (r *Repository) SparklineDataCorrupted(ctx context.Context, scope league.Scope, names []string, variant string, hours int) (map[string][]SparklinePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline data corrupted: %w", err)
	}
	if len(names) == 0 {
		return nil, nil
	}

	query := `
		SELECT name, time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE league = $1 AND name = ANY($2) AND is_corrupted = true
		  AND time > NOW() - make_interval(hours => $3)`
	args := []any{scope.ID(), names, hours}

	if variant != "" {
		query += ` AND variant = $4`
		args = append(args, variant)
	}

	query += ` ORDER BY name, time ASC LIMIT 10000`

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query corrupted sparkline data: %w", err)
	}
	defer rows.Close()

	result := make(map[string][]SparklinePoint)
	for rows.Next() {
		var name string
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&name, &t, &chaos, &listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan corrupted sparkline point: %w", err)
		}
		result[name] = append(result[name], SparklinePoint{
			Time:     t.UTC().Format(time.RFC3339),
			Price:    chaos,
			Listings: listings,
		})
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted sparkline rows iteration: %w", err)
	}

	return result, nil
}

// GemNameDictionary returns known gem names for OCR matching: every name in
// gem_colors, plus every transfigured name the scoped league has ever had a
// snapshot for.
//
// It exists because recognising the gem under the cursor must not depend on
// whether the market prices it. The previous source (transfigure results) needed
// BOTH the gem and its base priced, collapsing to a handful of names at league
// start — silently blinding the desktop matcher exactly when players run the
// most labs.
//
// gem_colors carries no league and no prices, but it is not pure game data: the
// collector's gemcolor.Resolver upserts names into it as it sees them (ADR-006),
// so a brand-new transfigured gem lands there only once it has been priced at
// least once. Unioning the league's snapshot names keeps this a strict superset
// of the market-scoped endpoint it replaced, so the swap can never lose a name
// the old path would have returned.
//
// transfigured selects between the two halves:
//   - true  — names whose base gem is itself a known gem ("Rain of Arrows of
//     Saturation" has base "Rain of Arrows"). This is what distinguishes a
//     transfigured gem from a base gem that merely contains " of "
//     ("Herald of Ash" has no base gem "Herald"). Names flagged transfigured in
//     the league's snapshots are trusted directly — the market already told us.
//   - false — everything else, minus support gems, which never appear in the
//     Font or Dedication pools.
func (r *Repository) GemNameDictionary(ctx context.Context, scope league.Scope, transfigured bool) ([]string, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: gem name dictionary: %w", err)
	}

	rows, err := r.pool.Query(ctx, "SELECT name FROM gem_colors ORDER BY name")
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem name dictionary: %w", err)
	}
	defer rows.Close()

	var all []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem dictionary name: %w", err)
		}
		all = append(all, name)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem dictionary rows iteration: %w", err)
	}
	rows.Close()

	// Names the market has shown this league, with is_transfigured straight from
	// the source rather than inferred from the base-name rule.
	seen, err := r.pool.Query(ctx,
		`SELECT DISTINCT name FROM gem_snapshots WHERE league = $1 AND is_transfigured = $2`,
		scope.ID(), transfigured)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem dictionary snapshots: %w", err)
	}
	defer seen.Close()

	var fromSnapshots []string
	for seen.Next() {
		var name string
		if err := seen.Scan(&name); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem dictionary snapshot name: %w", err)
		}
		fromSnapshots = append(fromSnapshots, name)
	}
	if err := seen.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem dictionary snapshot rows iteration: %w", err)
	}

	return MergeGemDictionary(FilterGemDictionary(all, transfigured), fromSnapshots), nil
}

// MergeGemDictionary unions two name lists into one sorted, de-duplicated list.
func MergeGemDictionary(a, b []string) []string {
	seen := make(map[string]struct{}, len(a)+len(b))
	merged := make([]string, 0, len(a)+len(b))
	for _, list := range [][]string{a, b} {
		for _, n := range list {
			if _, dup := seen[n]; dup {
				continue
			}
			seen[n] = struct{}{}
			merged = append(merged, n)
		}
	}
	sort.Strings(merged)
	return merged
}

// FilterGemDictionary splits a full gem-name list into transfigured and
// non-transfigured halves. Separated from the query so the rule is testable
// without a database.
func FilterGemDictionary(all []string, transfigured bool) []string {
	known := make(map[string]struct{}, len(all))
	for _, n := range all {
		known[n] = struct{}{}
	}

	names := make([]string, 0, len(all))
	for _, n := range all {
		base := extractBaseName(n)
		_, isTransfigured := known[base]
		isTransfigured = isTransfigured && base != n
		if isTransfigured != transfigured {
			continue
		}
		if !transfigured && strings.HasSuffix(n, " Support") {
			continue
		}
		names = append(names, n)
	}
	return names
}
