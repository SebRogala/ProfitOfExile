package lab

import (
	"context"
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
			 (time, color, variant, pool, winners, p_win, avg_win, ev, input_cost, profit, threshold)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.Color, r.Variant, r.Pool, r.Winners,
			r.PWin, r.AvgWin, r.EV, r.InputCost, r.Profit, r.Threshold,
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

// LatestFontResults returns the most recent Font analysis results, optionally filtered by variant.
func (r *Repository) LatestFontResults(ctx context.Context, variant string, limit int) ([]FontResult, error) {
	query := `
		SELECT time, color, variant, pool, winners, p_win, avg_win, ev, input_cost, profit, threshold
		FROM font_snapshots
		WHERE time = (SELECT MAX(time) FROM font_snapshots)`
	args := []any{}

	if variant != "" {
		query += ` AND variant = $1 ORDER BY profit DESC LIMIT $2`
		args = append(args, variant, limit)
	} else {
		query += ` ORDER BY profit DESC LIMIT $1`
		args = append(args, limit)
	}

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query font results: %w", err)
	}
	defer rows.Close()

	var results []FontResult
	for rows.Next() {
		var fr FontResult
		if err := rows.Scan(&fr.Time, &fr.Color, &fr.Variant, &fr.Pool, &fr.Winners,
			&fr.PWin, &fr.AvgWin, &fr.EV, &fr.InputCost, &fr.Profit, &fr.Threshold); err != nil {
			return nil, fmt.Errorf("lab repo: scan font result: %w", err)
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
			  window_score, window_signal, advanced_signal)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
			         $14, $15, $16, $17, $18, $19, $20)
			 ON CONFLICT DO NOTHING`,
			r.Time, r.Name, r.Variant, r.GemColor, r.CurrentPrice, r.CurrentListings,
			r.PriceVelocity, r.ListingVelocity, r.CV, r.Signal, r.HistPosition,
			r.PriceHigh7d, r.PriceLow7d,
			r.BaseListings, r.BaseVelocity, r.RelativeLiquidity, r.LiquidityTier,
			r.WindowScore, r.WindowSignal, r.AdvancedSignal,
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

// LatestTrendResults returns the most recent trend results, optionally filtered by variant, signal, and/or window.
func (r *Repository) LatestTrendResults(ctx context.Context, variant, signal, window, advanced string, limit int) ([]TrendResult, error) {
	query := `
		SELECT time, name, variant, gem_color, current_price, current_listings,
		       price_velocity, listing_velocity, cv, signal, hist_position,
		       price_high_7d, price_low_7d,
		       base_listings, base_velocity, relative_liquidity, liquidity_tier,
		       window_score, window_signal, COALESCE(advanced_signal, '')
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
			&tr.WindowScore, &tr.WindowSignal, &tr.AdvancedSignal); err != nil {
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

// GemNamesAutocomplete returns distinct transfigured gem names matching the query prefix.
func (r *Repository) GemNamesAutocomplete(ctx context.Context, query string, limit int) ([]string, error) {
	if query == "" {
		return nil, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT DISTINCT name FROM gem_snapshots
		WHERE is_transfigured = true AND name ILIKE $1
		ORDER BY name LIMIT $2`,
		strings.NewReplacer(`%`, `\%`, `_`, `\_`).Replace(query)+"%", limit)
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
}

// SignalHistory returns the last N signal snapshots for a gem, used to show transitions.
func (r *Repository) SignalHistory(ctx context.Context, name, variant string, limit int) ([]SignalChange, error) {
	rows, err := r.pool.Query(ctx, `
		SELECT time, signal, window_signal, COALESCE(advanced_signal, ''),
		       price_velocity, listing_velocity
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
			&c.PriceVel, &c.ListVel); err != nil {
			return nil, fmt.Errorf("lab repo: scan signal history: %w", err)
		}
		changes = append(changes, c)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history iteration: %w", err)
	}

	return changes, nil
}
