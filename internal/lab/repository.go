package lab

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
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
func (r *Repository) LatestGemPrices(ctx context.Context) ([]GemPrice, time.Time, error) {
	var snapTime *time.Time
	err := r.pool.QueryRow(ctx, "SELECT MAX(time) FROM gem_snapshots").Scan(&snapTime)
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
		WHERE time = $1`, *snapTime)
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
func (r *Repository) SaveTransfigureResults(ctx context.Context, results []TransfigureResult) (int, error) {
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
			 (time, base_name, transfigured_name, variant, base_price, transfigured_price,
			  roi, roi_pct, base_listings, transfigured_listings, gem_color, confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.BaseName, r.TransfiguredName, r.Variant,
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
func (r *Repository) LatestGCPPrice(ctx context.Context) (float64, error) {
	var chaos *float64
	err := r.pool.QueryRow(ctx, `
		SELECT chaos FROM currency_snapshots
		WHERE currency_id = 'gemcutters-prism'
		ORDER BY time DESC LIMIT 1`).Scan(&chaos)
	if err != nil {
		return 0, fmt.Errorf("lab repo: latest GCP price: %w", err)
	}
	if chaos == nil {
		return 0, fmt.Errorf("lab repo: GCP price is NULL in latest snapshot")
	}
	return *chaos, nil
}

// SaveFontResults batch-inserts Font of Divine Skill analysis results.
func (r *Repository) SaveFontResults(ctx context.Context, results []FontResult) (int, error) {
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
			 (time, color, variant, pool, winners, p_win, avg_win, ev, input_cost, profit,
			  mode, thin_pool_gems, liquidity_risk)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.Color, r.Variant, r.Pool, r.Winners,
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
func (r *Repository) SaveQualityResults(ctx context.Context, results []QualityResult) (int, error) {
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
			 (time, name, level, buy_price, price_q20, roi_4, roi_6, roi_10, roi_15,
			  avg_roi, gcp_price, listings_0, listings_20, gem_color, confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.Name, r.Level, r.BuyPrice, r.PriceQ20,
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
func (r *Repository) LatestFontResults(ctx context.Context, variant, mode string, limit int) ([]FontResult, error) {
	query := `
		SELECT time, color, variant, pool, winners, p_win, avg_win, ev, input_cost, profit,
		       COALESCE(mode, 'safe'), COALESCE(thin_pool_gems, 0), COALESCE(liquidity_risk, 'LOW')
		FROM font_snapshots
		WHERE time = (SELECT MAX(time) FROM font_snapshots)`
	args := []any{}
	argIdx := 1

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
		if err := rows.Scan(&fr.Time, &fr.Color, &fr.Variant, &fr.Pool, &fr.Winners,
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
func (r *Repository) LatestQualityResults(ctx context.Context, variant string, limit int) ([]QualityResult, error) {
	query := `
		SELECT time, name, level, buy_price, price_q20, roi_4, roi_6, roi_10, roi_15,
		       avg_roi, gcp_price, listings_0, listings_20, COALESCE(gem_color, ''), confidence
		FROM quality_results
		WHERE time = (SELECT MAX(time) FROM quality_results)`
	args := []any{}

	if variant != "" {
		// variant here maps to level: "1" or "1/20" → level 1, "20" or "20/20" → level 20
		query += ` AND level = $1 ORDER BY avg_roi DESC LIMIT $2`
		level := 20
		if variant == "1" || variant == "1/20" {
			level = 1
		}
		args = append(args, level, limit)
	} else {
		query += ` ORDER BY avg_roi DESC LIMIT $1`
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
func (r *Repository) BasePriceHistory(ctx context.Context, variant string, hours int) (map[string][]PricePoint, error) {
	query := `
		SELECT name, time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE time > NOW() - make_interval(hours => $1)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = ANY($2)`
	args := []any{hours, []string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query += ` AND variant = $3`
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
func (r *Repository) MarketAvgBaseListings(ctx context.Context, variant string) (float64, error) {
	query := `
		SELECT COALESCE(AVG(COALESCE(listings, 0)), 0)
		FROM gem_snapshots
		WHERE time = (SELECT MAX(time) FROM gem_snapshots)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = ANY($1)`
	args := []any{[]string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query = `
		SELECT COALESCE(AVG(COALESCE(listings, 0)), 0)
		FROM gem_snapshots
		WHERE time = (SELECT MAX(time) FROM gem_snapshots)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND variant = $1`
		args = []any{variant}
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
func (r *Repository) GemPriceHistoryByVariant(ctx context.Context, variant string, hours int) ([]GemPriceHistory, error) {
	query := `
		SELECT name, variant, COALESCE(gem_color, ''), time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE time > NOW() - make_interval(hours => $1)
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND name NOT LIKE '%Trarthus%'
		  AND COALESCE(chaos, 0) > 5
		  AND variant = ANY($2)`
	args := []any{hours, []string{"1", "1/20", "20", "20/20"}}

	if variant != "" {
		query += ` AND variant = $3`
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

// SaveTrendResults batch-inserts trend analysis results.
func (r *Repository) SaveTrendResults(ctx context.Context, results []TrendResult) (int, error) {
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
			`INSERT INTO trend_results
			 (time, name, variant, gem_color, current_price, current_listings,
			  price_velocity, listing_velocity, cv, signal, hist_position,
			  price_high_7d, price_low_7d,
			  base_listings, base_velocity, relative_liquidity, liquidity_tier,
			  window_score, window_signal, advanced_signal, price_tier, tier_action,
			  sell_urgency, sell_reason, sellability, sellability_label)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
			         $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.Name, r.Variant, r.GemColor, r.CurrentPrice, r.CurrentListings,
			r.PriceVelocity, r.ListingVelocity, r.CV, r.Signal, r.HistPosition,
			r.PriceHigh7d, r.PriceLow7d,
			r.BaseListings, r.BaseVelocity, r.RelativeLiquidity, r.LiquidityTier,
			r.WindowScore, r.WindowSignal, r.AdvancedSignal, r.PriceTier, r.TierAction,
			r.SellUrgency, r.SellReason, r.Sellability, r.SellabilityLabel,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert trend result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit trend results: %w", err)
	}

	return inserted, nil
}

// LatestTrendResults returns the most recent trend results, optionally filtered by variant, signal, window, advanced, and/or tier.
func (r *Repository) LatestTrendResults(ctx context.Context, variant, signal, window, advanced, tier string, limit int) ([]TrendResult, error) {
	query := `
		SELECT time, name, variant, gem_color, current_price, current_listings,
		       price_velocity, listing_velocity, cv, signal, hist_position,
		       price_high_7d, price_low_7d,
		       base_listings, base_velocity, relative_liquidity, liquidity_tier,
		       window_score, window_signal, COALESCE(advanced_signal, ''),
		       COALESCE(price_tier, 'LOW'), COALESCE(tier_action, ''),
		       COALESCE(sell_urgency, ''), COALESCE(sell_reason, ''),
		       COALESCE(sellability, 50), COALESCE(sellability_label, 'MODERATE')
		FROM trend_results
		WHERE time = (SELECT MAX(time) FROM trend_results)`
	args := []any{}
	argIdx := 1

	if variant != "" {
		query += fmt.Sprintf(` AND variant = $%d`, argIdx)
		args = append(args, variant)
		argIdx++
	}
	if signal != "" {
		query += fmt.Sprintf(` AND signal = $%d`, argIdx)
		args = append(args, signal)
		argIdx++
	}
	if window != "" {
		query += fmt.Sprintf(` AND window_signal = $%d`, argIdx)
		args = append(args, window)
		argIdx++
	}
	if advanced != "" {
		query += fmt.Sprintf(` AND advanced_signal = $%d`, argIdx)
		args = append(args, advanced)
		argIdx++
	}
	if tier != "" {
		query += fmt.Sprintf(` AND price_tier = $%d`, argIdx)
		args = append(args, tier)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY cv DESC, current_price DESC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query trend results: %w", err)
	}
	defer rows.Close()

	var results []TrendResult
	for rows.Next() {
		var tr TrendResult
		if err := rows.Scan(&tr.Time, &tr.Name, &tr.Variant, &tr.GemColor,
			&tr.CurrentPrice, &tr.CurrentListings,
			&tr.PriceVelocity, &tr.ListingVelocity, &tr.CV, &tr.Signal, &tr.HistPosition,
			&tr.PriceHigh7d, &tr.PriceLow7d,
			&tr.BaseListings, &tr.BaseVelocity, &tr.RelativeLiquidity, &tr.LiquidityTier,
			&tr.WindowScore, &tr.WindowSignal, &tr.AdvancedSignal,
			&tr.PriceTier, &tr.TierAction,
			&tr.SellUrgency, &tr.SellReason,
			&tr.Sellability, &tr.SellabilityLabel); err != nil {
			return nil, fmt.Errorf("lab repo: scan trend result: %w", err)
		}
		results = append(results, tr)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: trend rows iteration: %w", err)
	}

	return results, nil
}

// LatestTransfigureResults returns the most recent analysis results, optionally filtered by variant.
func (r *Repository) LatestTransfigureResults(ctx context.Context, variant string, limit int) ([]TransfigureResult, error) {
	query := `
		SELECT time, base_name, transfigured_name, variant, base_price, transfigured_price,
		       roi, roi_pct, base_listings, transfigured_listings, COALESCE(gem_color, ''), confidence
		FROM transfigure_results
		WHERE time = (SELECT MAX(time) FROM transfigure_results)`
	args := []any{}

	if variant != "" {
		query += ` AND variant = $1 ORDER BY roi DESC LIMIT $2`
		args = append(args, variant, limit)
	} else {
		query += ` ORDER BY roi DESC LIMIT $1`
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
func (r *Repository) SparklineData(ctx context.Context, names []string, variant string, hours int) (map[string][]SparklinePoint, error) {
	if len(names) == 0 {
		return nil, nil
	}

	query := `
		SELECT name, time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE name = ANY($1) AND is_corrupted = false
		  AND time > NOW() - make_interval(hours => $2)`
	args := []any{names, hours}

	if variant != "" {
		query += ` AND variant = $3`
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

// GemNamesAutocomplete returns distinct transfigured gem names matching all query words (in any order).
func (r *Repository) GemNamesAutocomplete(ctx context.Context, query string, limit int) ([]string, error) {
	if query == "" {
		return nil, nil
	}

	escaper := strings.NewReplacer(`%`, `\%`, `_`, `\_`)
	words := strings.Fields(query)
	conditions := make([]string, len(words))
	args := make([]any, len(words))
	for i, w := range words {
		args[i] = "%" + escaper.Replace(w) + "%"
		conditions[i] = fmt.Sprintf("name ILIKE $%d", i+1)
	}
	args = append(args, limit)

	sql := fmt.Sprintf(`
		SELECT DISTINCT name FROM gem_snapshots
		WHERE is_transfigured = true AND %s
		ORDER BY name LIMIT $%d`,
		strings.Join(conditions, " AND "), len(args))

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
func (r *Repository) SignalHistory(ctx context.Context, name, variant string, limit int) ([]SignalChange, error) {
	rows, err := r.pool.Query(ctx, `
		SELECT time, signal, window_signal, COALESCE(advanced_signal, ''),
		       price_velocity, listing_velocity, current_price, current_listings
		FROM trend_results
		WHERE name = $1 AND variant = $2
		ORDER BY time DESC
		LIMIT $3`, name, variant, limit)
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
func (r *Repository) SaveMarketContext(ctx context.Context, mc MarketContext) error {
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

	_, err = r.pool.Exec(ctx, `
		INSERT INTO market_context
		 (time, price_percentiles, listing_percentiles,
		  velocity_mean, velocity_sigma, listing_vel_mean, listing_vel_sigma,
		  total_gems, total_listings, tier_boundaries,
		  hourly_bias, hourly_volatility, hourly_activity,
		  weekday_bias, weekday_volatility, weekday_activity,
		  temporal_coefficient, temporal_mode, temporal_buckets,
		  variant_stats)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
		ON CONFLICT DO NOTHING`,
		mc.Time, pricePerc, listPerc,
		mc.VelocityMean, mc.VelocitySigma, mc.ListingVelMean, mc.ListingVelSigma,
		mc.TotalGems, mc.TotalListings, tierBounds,
		hourly, hourlyVol, hourlyAct,
		weekday, weekdayVol, weekdayAct,
		mc.TemporalCoefficient, mc.TemporalMode, mc.TemporalBuckets,
		variantStats,
	)
	if err != nil {
		return fmt.Errorf("lab repo: insert market context: %w", err)
	}
	return nil
}

// LatestMarketContext returns the most recent market context snapshot.
// Returns (nil, nil) when no rows exist.
func (r *Repository) LatestMarketContext(ctx context.Context) (*MarketContext, error) {
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
		WHERE time = (SELECT MAX(time) FROM market_context)`).
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
func (r *Repository) SaveGemFeatures(ctx context.Context, features []GemFeature) (int, error) {
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
			 (time, name, variant, chaos, listings, tier, global_tier,
			  vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
			  vel_long_price, vel_long_listing,
			  cv, hist_position, high_7d, low_7d,
			  flood_count, crash_count, listing_elasticity,
			  relative_price, relative_listings,
			  sell_probability_factor, stability_discount,
			  market_depth, market_regime)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
			         $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24,
			         $25, $26)
			 ON CONFLICT DO NOTHING`,
			f.Time, f.Name, f.Variant, f.Chaos, f.Listings, f.Tier, f.GlobalTier,
			f.VelShortPrice, f.VelShortListing, f.VelMedPrice, f.VelMedListing,
			f.VelLongPrice, f.VelLongListing,
			f.CV, f.HistPosition, f.High7d, f.Low7d,
			f.FloodCount, f.CrashCount, f.ListingElasticity,
			f.RelativePrice, f.RelativeListings,
			f.SellProbabilityFactor, f.StabilityDiscount,
			f.MarketDepth, f.MarketRegime,
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
func (r *Repository) LatestGemFeatures(ctx context.Context, variant, tier string, limit int) ([]GemFeature, error) {
	query := `
		SELECT time, name, variant, chaos, listings, tier, COALESCE(global_tier, ''),
		       vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
		       vel_long_price, vel_long_listing,
		       cv, hist_position, high_7d, low_7d,
		       flood_count, crash_count, listing_elasticity,
		       relative_price, relative_listings,
		       sell_probability_factor, stability_discount,
		       COALESCE(market_depth, 0), COALESCE(market_regime, 'TEMPORAL')
		FROM gem_features
		WHERE time = (SELECT MAX(time) FROM gem_features)`
	args := []any{}
	argIdx := 1

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
		if err := rows.Scan(&f.Time, &f.Name, &f.Variant, &f.Chaos, &f.Listings, &f.Tier, &f.GlobalTier,
			&f.VelShortPrice, &f.VelShortListing, &f.VelMedPrice, &f.VelMedListing,
			&f.VelLongPrice, &f.VelLongListing,
			&f.CV, &f.HistPosition, &f.High7d, &f.Low7d,
			&f.FloodCount, &f.CrashCount, &f.ListingElasticity,
			&f.RelativePrice, &f.RelativeListings,
			&f.SellProbabilityFactor, &f.StabilityDiscount,
			&f.MarketDepth, &f.MarketRegime); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem feature: %w", err)
		}
		results = append(results, f)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: gem features rows iteration: %w", err)
	}

	return results, nil
}

// SaveGemSignals batch-inserts pre-computed gem signal rows.
func (r *Repository) SaveGemSignals(ctx context.Context, signals []GemSignal) (int, error) {
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
			 (time, name, variant, signal, confidence,
			  sell_urgency, sell_reason, sellability, sellability_label,
			  window_signal, advanced_signal, phase_modifier,
			  recommendation, tier,
			  risk_adjusted_value, quick_sell_price, sell_confidence)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
			         $15, $16, $17)
			 ON CONFLICT DO NOTHING`,
			s.Time, s.Name, s.Variant, s.Signal, s.Confidence,
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
func (r *Repository) LatestGemSignals(ctx context.Context, variant, tier string, limit int) ([]GemSignal, error) {
	query := `
		SELECT time, name, variant, signal, confidence,
		       sell_urgency, sell_reason, sellability, sellability_label,
		       window_signal, advanced_signal, phase_modifier,
		       recommendation, tier,
		       risk_adjusted_value, quick_sell_price, sell_confidence
		FROM gem_signals
		WHERE time = (SELECT MAX(time) FROM gem_signals)`
	args := []any{}
	argIdx := 1

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
func (r *Repository) AllGemFeaturesInRange(ctx context.Context, hours int) ([]GemFeature, error) {
	query := `
		SELECT time, name, variant, chaos, listings, tier, COALESCE(global_tier, ''),
		       vel_short_price, vel_short_listing, vel_med_price, vel_med_listing,
		       vel_long_price, vel_long_listing,
		       cv, hist_position, high_7d, low_7d,
		       flood_count, crash_count, listing_elasticity,
		       relative_price, relative_listings,
		       sell_probability_factor, stability_discount,
		       COALESCE(market_depth, 0), COALESCE(market_regime, 'TEMPORAL')
		FROM gem_features
		WHERE time > NOW() - make_interval(hours => $1)
		ORDER BY time, name, variant`

	rows, err := r.pool.Query(ctx, query, hours)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query all gem features in range: %w", err)
	}
	defer rows.Close()

	var results []GemFeature
	for rows.Next() {
		var f GemFeature
		if err := rows.Scan(&f.Time, &f.Name, &f.Variant, &f.Chaos, &f.Listings, &f.Tier, &f.GlobalTier,
			&f.VelShortPrice, &f.VelShortListing, &f.VelMedPrice, &f.VelMedListing,
			&f.VelLongPrice, &f.VelLongListing,
			&f.CV, &f.HistPosition, &f.High7d, &f.Low7d,
			&f.FloodCount, &f.CrashCount, &f.ListingElasticity,
			&f.RelativePrice, &f.RelativeListings,
			&f.SellProbabilityFactor, &f.StabilityDiscount,
			&f.MarketDepth, &f.MarketRegime); err != nil {
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
func (r *Repository) SnapshotPricesInRange(ctx context.Context, hours int) ([]SnapshotPrice, error) {
	query := `
		SELECT time, name, variant, COALESCE(chaos, 0)
		FROM gem_snapshots
		WHERE time > NOW() - make_interval(hours => $1)
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND chaos > 5
		ORDER BY time, name, variant`

	rows, err := r.pool.Query(ctx, query, hours)
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
