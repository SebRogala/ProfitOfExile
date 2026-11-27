package lab

import (
	"context"
	"errors"
	"fmt"
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
	gems, err := r.gemsAtSnapshot(ctx, scope.ID(), *snapTime)
	if err != nil {
		return nil, time.Time{}, err
	}
	return gems, *snapTime, nil
}

// GemPricesAsOf returns all gem prices from the newest snapshot at or before asOf.
// Returns (nil, zero-time, nil) when no such snapshot exists.
func (r *Repository) GemPricesAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]GemPrice, time.Time, error) {
	if err := scope.Validate(); err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: gem prices as of: %w", err)
	}
	var snapTime *time.Time
	err := r.pool.QueryRow(ctx,
		"SELECT MAX(time) FROM gem_snapshots WHERE league = $1 AND time <= $2", scope.ID(), asOf).Scan(&snapTime)
	if err != nil {
		return nil, time.Time{}, fmt.Errorf("lab repo: snapshot time as of: %w", err)
	}
	if snapTime == nil {
		return nil, time.Time{}, nil
	}
	gems, err := r.gemsAtSnapshot(ctx, scope.ID(), *snapTime)
	if err != nil {
		return nil, time.Time{}, err
	}
	return gems, *snapTime, nil
}

func (r *Repository) gemsAtSnapshot(ctx context.Context, leagueID string, snapTime time.Time) ([]GemPrice, error) {
	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		       is_transfigured, is_corrupted, COALESCE(gem_color, ''), sample_count, sparkline_empty
		FROM gem_snapshots
		WHERE league = $1 AND time = $2`, leagueID, snapTime)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query latest gems: %w", err)
	}
	defer rows.Close()

	var gems []GemPrice
	for rows.Next() {
		var g GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings,
			&g.IsTransfigured, &g.IsCorrupted, &g.GemColor, &g.SampleCount, &g.SparklineEmpty); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem: %w", err)
		}
		gems = append(gems, g)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: rows iteration: %w", err)
	}
	return gems, nil
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

// LatestNinjaDivinePrice returns the newest poe.ninja divine price in
// currency_snapshots and its snapshot time. No row, or a NULL price, is
// (0, zero time, nil): the caller treats it as no rate.
func (r *Repository) LatestNinjaDivinePrice(ctx context.Context, scope league.Scope) (float64, time.Time, error) {
	chaos, at, err := r.ninjaDivinePrice(ctx, scope, nil)
	if err != nil {
		return 0, time.Time{}, fmt.Errorf("lab repo: latest ninja divine price: %w", err)
	}
	return chaos, at, nil
}

// ninjaDivinePrice reads the newest divine snapshot at or before asOf; a nil
// asOf is unbounded. No row, or a NULL price, is (0, zero time, nil).
func (r *Repository) ninjaDivinePrice(ctx context.Context, scope league.Scope, asOf *time.Time) (float64, time.Time, error) {
	if err := scope.Validate(); err != nil {
		return 0, time.Time{}, err
	}
	var chaos *float64
	var at time.Time
	err := r.pool.QueryRow(ctx, `
		SELECT chaos, time FROM currency_snapshots
		WHERE league = $1 AND currency_id = 'divine'
		  AND ($2::timestamptz IS NULL OR time <= $2)
		ORDER BY time DESC LIMIT 1`, scope.ID(), asOf).Scan(&chaos, &at)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, time.Time{}, nil
	}
	if err != nil {
		return 0, time.Time{}, err
	}
	if chaos == nil {
		return 0, time.Time{}, nil
	}
	return *chaos, at, nil
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
		profit := 0.0 // Deferred D4: NOT NULL profit stores zero for a withheld value; scanFontRows ignores it when ev is NULL.
		if r.Profit != nil {
			profit = *r.Profit
		}
		batch.Queue(
			`INSERT INTO font_snapshots
			 (league, time, color, variant, pool, priced_gems, winners, p_win, avg_win, ev, input_cost, profit,
			  mode, thin_pool_gems, liquidity_risk)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), r.Time, r.Color, r.Variant, r.Pool, r.PricedGems, r.Winners,
			r.PWin, r.AvgWin, r.EV, r.InputCost, profit,
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
		SELECT time, color, variant, pool, priced_gems, winners, p_win, avg_win, ev, input_cost, profit,
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
	return scanFontRows(rows)
}

// scanFontRows decodes font_snapshots rows selected in the LatestFontResults
// column order.
func scanFontRows(rows pgx.Rows) ([]FontResult, error) {
	defer rows.Close()
	var results []FontResult
	for rows.Next() {
		var fr FontResult
		var pricedGems *int
		var profit float64
		if err := rows.Scan(&fr.Time, &fr.Color, &fr.Variant, &fr.Pool, &pricedGems, &fr.Winners,
			&fr.PWin, &fr.AvgWin, &fr.EV, &fr.InputCost, &profit,
			&fr.Mode, &fr.ThinPoolGems, &fr.LiquidityRisk); err != nil {
			return nil, fmt.Errorf("lab repo: scan font result: %w", err)
		}
		// An unknown legacy count supplies no SOLID evidence under ADR-032.
		if pricedGems != nil {
			fr.PricedGems = *pricedGems
		}
		if fr.EV != nil {
			fr.Profit = &profit
			evRaw := profit + fr.InputCost
			fr.EVRaw = &evRaw
		}
		withholdUnderFloor(&fr)
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

// FontOverviewRows returns the latest Font snapshot's safe, premium and
// jackpot rows: the FontOverviewMaxLimit most profitable of each variant ×
// mode, every one a top-N any LatestFontResults call could answer.
func (r *Repository) FontOverviewRows(ctx context.Context, scope league.Scope) ([]FontResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: font overview rows: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT time, color, variant, pool, priced_gems, winners, p_win, avg_win, ev, input_cost, profit,
		       mode, thin_pool_gems, liquidity_risk
		FROM (
			SELECT time, color, variant, priced_gems, pool, winners, p_win, avg_win, ev,
			       input_cost, profit, mode, COALESCE(thin_pool_gems, 0) AS thin_pool_gems,
			       COALESCE(liquidity_risk, 'LOW') AS liquidity_risk,
			       row_number() OVER (PARTITION BY variant, mode ORDER BY profit DESC) AS rank
			FROM font_snapshots
			WHERE league = $1 AND time = (SELECT MAX(time) FROM font_snapshots WHERE league = $1)
			  AND mode IN ('safe', 'premium', 'jackpot')
		) ranked
		WHERE rank <= $2
		ORDER BY profit DESC`, scope.ID(), FontOverviewMaxLimit)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query font overview rows: %w", err)
	}
	return scanFontRows(rows)
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
// Only includes analysis variants, and applies sqlNotHeistOnly (eligibility.go).
//
// This is deliberately not the Font pool. These are base gems — non-transfigured
// by definition — read as the base side of the sellUrgency and windowSignal
// classifiers, so isFontOutcome, which requires IsTransfigured, cannot be the
// rule here.
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
		  AND ` + sqlNotHeistOnly + `
		  AND variant = ANY($3)`
	args := []any{scope.ID(), hours, sparklineVariants}

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
		  AND ` + sqlNotHeistOnly + `
		  AND variant = ANY($2)`
	args := []any{scope.ID(), sparklineVariants}

	if variant != "" {
		query = `
		SELECT COALESCE(AVG(COALESCE(listings, 0)), 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
		  AND is_transfigured = false
		  AND is_corrupted = false
		  AND ` + sqlNotHeistOnly + `
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
// within the inclusive [since, asOf] interval, grouped by (name, variant).
// Only includes variants "1", "1/20", "20", "20/20", chaos > 5, and applies
// sqlNotHeistOnly (eligibility.go).
//
// It is wider than isFontOutcome on purpose: this is history, and the caller
// that turns it into features (ComputeGemFeatures) applies the full Font rules
// to the latest snapshot. Narrowing here would only move the same filter
// earlier, at the cost of a colour join the history query does not need.
func (r *Repository) GemPriceHistoryByVariant(ctx context.Context, scope league.Scope, variant string, since, asOf time.Time) ([]GemPriceHistory, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: gem price history by variant: %w", err)
	}
	query := `
		SELECT name, variant, COALESCE(gem_color, ''), time, COALESCE(chaos, 0), COALESCE(listings, 0)
		FROM gem_snapshots
		WHERE league = $1
		  AND time >= $2 AND time <= $3
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND ` + sqlNotHeistOnly + `
		  AND COALESCE(chaos, 0) > 5
		  AND variant = ANY($4)`
	args := []any{scope.ID(), since, asOf, sparklineVariants}

	if variant != "" {
		query += ` AND variant = $5`
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

// gemEvidenceHistory reads the keys' gem_snapshots points inside the hold
// window ending at asOf.
func (r *Repository) gemEvidenceHistory(ctx context.Context, scope league.Scope, keys []GemClassificationKey, asOf time.Time) ([]evidencePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: gem evidence history: %w", err)
	}
	if len(keys) == 0 {
		return nil, nil
	}
	names, variants := make([]string, len(keys)), make([]string, len(keys))
	for i, key := range keys {
		names[i], variants[i] = key.Name, key.Variant
	}
	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, time, chaos, COALESCE(listings, 0), sample_count
		FROM gem_snapshots
		JOIN unnest($2::text[], $3::text[]) AS k(name, variant) USING (name, variant)
		WHERE league = $1 AND time >= $4::timestamptz - $5 * interval '1 second' AND time <= $4
		  AND COALESCE(chaos, 0) > 0`, scope.ID(), names, variants, asOf, GemEvidencePolicy().HoldWindow.Seconds())
	if err != nil {
		return nil, fmt.Errorf("lab repo: query gem evidence history: %w", err)
	}
	defer rows.Close()
	var points []evidencePoint
	for rows.Next() {
		var point evidencePoint
		if err := rows.Scan(&point.Name, &point.Variant, &point.Time, &point.Chaos, &point.Listings, &point.SampleCount); err != nil {
			return nil, fmt.Errorf("lab repo: scan gem evidence history: %w", err)
		}
		points = append(points, point)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: iterate gem evidence history: %w", err)
	}
	return points, nil
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
		SELECT name, time, ` + rawSparklineValues("") + `
		FROM gem_snapshots
		WHERE name = ANY($2) AND ` + rawSparklineFilter("", "$3")
	args := []any{scope.ID(), names, hours}

	if variant != "" {
		query += ` AND variant = $4`
		args = append(args, variant)
	}

	query += fmt.Sprintf(` ORDER BY name, time ASC LIMIT %d`, rawSparklineRowLimit)

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
		result[name] = append(result[name], rawSparklinePoint(t, chaos, listings))
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline rows iteration: %w", err)
	}

	return result, nil
}

// rawSparklineRowLimit bounds one raw sparkline selection: SparklineData's
// whole read, and each variant of a SparklineSeries read.
const rawSparklineRowLimit = 10000

// rawSparklineFilter is the row predicate SparklineData and SparklineSeries
// share: league $1, non-corrupted, strictly newer than hoursParam hours before
// NOW(). alias qualifies the columns ("" for none).
func rawSparklineFilter(alias, hoursParam string) string {
	if alias != "" {
		alias += "."
	}
	return alias + "league = $1 AND " + alias + "is_corrupted = false AND " +
		alias + "time > NOW() - make_interval(hours => " + hoursParam + ")"
}

// rawSparklineValues projects a raw point's price and listings, missing values
// read as zero, as the columns chaos and listings.
func rawSparklineValues(alias string) string {
	if alias != "" {
		alias += "."
	}
	return "COALESCE(" + alias + "chaos, 0) AS chaos, COALESCE(" + alias + "listings, 0) AS listings"
}

// rawSparklinePoint is a served sparkline point: raw chaos, RFC 3339 UTC time.
func rawSparklinePoint(t time.Time, chaos float64, listings int) SparklinePoint {
	return SparklinePoint{Time: t.UTC().Format(time.RFC3339), Price: chaos, Listings: listings}
}

// SparklineSeries reads the raw non-corrupted points of every requested
// (name, variant) series in one statement. Each variant keeps the selection
// SparklineData makes for it: its own first rawSparklineRowLimit window rows by (name, time),
// shared across that variant's requested names, not a per-series cap. The
// primary key makes (name, time) unique within a variant, so that selection is
// deterministic. Series with no points are omitted.
func (r *Repository) SparklineSeries(ctx context.Context, scope league.Scope, keys []CollectiveSparklineKey, hours int) (map[CollectiveSparklineKey][]SparklinePoint, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline series: %w", err)
	}
	if len(keys) == 0 {
		return nil, nil
	}
	names := make([]string, len(keys))
	variants := make([]string, len(keys))
	for i, k := range keys {
		names[i], variants[i] = k.Name, k.Variant
	}

	rows, err := r.pool.Query(ctx, `
		WITH requested AS (
			SELECT DISTINCT name, variant FROM unnest($2::text[], $3::text[]) AS k(name, variant)
		), windowed AS (
			SELECT g.name, g.variant, g.time, `+rawSparklineValues("g")+`,
			       row_number() OVER (PARTITION BY g.variant ORDER BY g.name, g.time) AS rn
			FROM gem_snapshots g
			JOIN requested k ON k.name = g.name AND k.variant = g.variant
			WHERE `+rawSparklineFilter("g", "$4")+`
		)
		SELECT name, variant, time, chaos, listings
		FROM windowed
		WHERE rn <= $5
		ORDER BY variant, name, time`, scope.ID(), names, variants, hours, rawSparklineRowLimit)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query sparkline series: %w", err)
	}
	defer rows.Close()

	result := make(map[CollectiveSparklineKey][]SparklinePoint)
	for rows.Next() {
		var k CollectiveSparklineKey
		var t time.Time
		var chaos float64
		var listings int
		if err := rows.Scan(&k.Name, &k.Variant, &t, &chaos, &listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan sparkline series point: %w", err)
		}
		result[k] = append(result[k], rawSparklinePoint(t, chaos, listings))
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: sparkline series rows iteration: %w", err)
	}
	return result, nil
}

// sparklineVariants are the non-corrupted variants sparkline consumers ask for,
// and the Normal variants BasePriceHistory, MarketAvgBaseListings and
// GemPriceHistoryByVariant bind.
// Prices for different variants are different markets, so the variant is part
// of the series identity rather than something merged away.
var sparklineVariants = []string{"1", "1/20", "20", "20/20"}

// sparklineCorruptedVariants are the corrupted variants sparklines are served
// for — the Dedication pools. Same identity rule as sparklineVariants: each
// variant is its own series, so both selectable Dedication pools are populated.
var sparklineCorruptedVariants = DedicationVariants

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
		    OR (is_corrupted = true AND variant = ANY($4)))`
}

// sparklineIncrementalQuery reads only rows strictly newer than the caller's
// high-water mark ($5). On a running server that mark is one tick old, so the
// result is a single snapshot's worth of rows.
//
// The shared $2-hour bound is what holds when the mark is not one tick old. A
// caller whose series have all decayed carries a mark older than the lookback
// (populateSparklineCache explains why), and this read then degenerates to the
// flat lookback — every row the cache could still retain, which is what lets
// that caller rebuild from an old mark instead of falling back to the cold
// union on every tick. It is the more expensive shape, and it is bounded.
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
	args := []any{scope.ID(), bounds.LookbackHours, sparklineVariants, sparklineCorruptedVariants, since}
	if since.IsZero() {
		query = sparklineColdQuery
		args = []any{
			scope.ID(), bounds.LookbackHours, sparklineVariants, sparklineCorruptedVariants,
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

// GemNamesAutocomplete returns distinct transfigured gem names matching all
// query words (in any order). It backs the Font (normal-mode) gem picker.
//
// The filters are the SQL half of isFontOutcome (eligibility.go): the picker
// offers what the Font of Divine Skill can hand out, which is the *uncorrupted*
// transfigured market minus the support, Heist and WHITE rules. Before POE-142
// this query carried no eligibility rule at all beyond is_transfigured, which
// made it the widest unfiltered name source feeding a gem icon.
//
// Measured 2026-08-05 on the latest local Allflame snapshot: it returned 247
// names, 45 of them poe.ninja's corrupted "Vaal <Base> (<Transfigured>)" market
// identities — Vaal Arc (Arc of Surging) and the like. The Font cannot hand one
// out, and the typed gems map has no entry for that name shape, so each rendered as the
// "?" fallback next to a compare card that can only answer NO_DATA. With the
// filters the query returns 202.
//
// This is also the query that fills the warm pool the picker answers from the
// rest of the time (Cache.SetGemNamePool, called by RunTransfigure), so the two
// halves of the picker cannot offer different gems.
//
// The colour rule is the WHITE one, matching isFontOutcome: nothing on this path
// partitions by colour, so there is no reason to drop a name the gemcolor
// resolver has not met yet. See the colour-rules comment in eligibility.go.
//
// The latest-snapshot bound is what makes the colour rule bite at all.
// gem_color is written per row at insert time from whatever the gemcolor
// resolver knew then (internal/collector/repository.go), not looked up as
// current state, so a name the resolver met late carries NULL on its early rows
// — and over an unbounded league one such row readmits the name for the rest of
// it. Measured 2026-08-05 on the local DB, four Allflame names carry both NULL
// and WHITE rows: Pact of Beidat, Ghorr, K'Tash and Lycia, the 3.29 gems the
// resolver met 8-10 snapshots in. None of the four is transfigured, so the
// unbounded form returned the same 202 names as the bounded one here — the
// readmission is live in the data and this surface escapes it only because this
// league's WHITE gems happen to sit on the other side of is_transfigured.
//
// The bound is the same subquery every other latest-snapshot read in this file
// uses, rather than CorruptedGemNamesAutocomplete's `time > NOW() - INTERVAL '2
// hours'`: this query is also what fills the warm pool, and a pool that empties
// when the collector falls two hours behind would blank the picker while the
// compare card behind it still answers from that same stale snapshot.
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

	// The eligibility fragments are Sprintf ARGUMENTS, not part of the format
	// string: each carries a LIKE pattern whose `%` vet reads as an unknown verb
	// (`% S`) the moment it lands in the format.
	sql := fmt.Sprintf(`
		SELECT DISTINCT name FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1)
		  AND is_transfigured = true
		  AND is_corrupted = false
		  AND %s
		  AND %s
		  AND %s
		  AND %s
		ORDER BY name LIMIT $%d`,
		sqlNotSupportGem, sqlNotHeistOnly, sqlNotColorlessByRule, filter, len(args))

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
	Time     time.Time `json:"time"`
	Signal   string    `json:"signal"`
	Window   string    `json:"window"`
	Advanced string    `json:"advanced"`
	PriceVel float64   `json:"priceVelocity"`
	ListVel  float64   `json:"listingVelocity"`
	Price    float64   `json:"currentPrice"`
	Listings int       `json:"currentListings"`
}

// SignalHistory returns the last N signal snapshots within signalHistorySeedMaxDays
// for a gem, matching the ring cache's seed window.
// Queries gem_signals joined with gem_features for velocity and price data.
func (r *Repository) SignalHistory(ctx context.Context, scope league.Scope, name, variant string, limit int) ([]SignalChange, error) {
	key := SignalHistoryKey{Name: name, Variant: variant}
	history, err := r.SignalHistoryBulk(ctx, scope, []SignalHistoryKey{key}, limit)
	if err != nil {
		return nil, err
	}
	return history[key], nil
}

// SignalHistoryBulk returns each key's newest limit signal snapshots within
// signalHistorySeedMaxDays in one statement: each key runs the per-series
// predicate, ordering and limit as a lateral subquery. Keys without rows are
// absent from the result. SignalHistory is its one-key form.
func (r *Repository) SignalHistoryBulk(ctx context.Context, scope league.Scope, keys []SignalHistoryKey, limit int) (map[SignalHistoryKey][]SignalChange, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history bulk: %w", err)
	}
	if limit <= 0 || len(keys) == 0 {
		return nil, nil
	}
	names, variants := make([]string, 0, len(keys)), make([]string, 0, len(keys))
	seen := make(map[SignalHistoryKey]bool, len(keys))
	for _, k := range keys {
		if !seen[k] {
			seen[k] = true
			names, variants = append(names, k.Name), append(variants, k.Variant)
		}
	}
	rows, err := r.pool.Query(ctx, `
		SELECT k.name, k.variant, h.time, h.signal, h.window_signal, h.advanced,
		       h.price_vel, h.listing_vel, h.chaos, h.listings
		FROM unnest($2::text[], $3::text[]) AS k(name, variant)
		CROSS JOIN LATERAL (
			SELECT s.time, s.signal, s.window_signal, COALESCE(s.advanced_signal, '') AS advanced,
			       COALESCE(f.vel_long_price, 0) AS price_vel, COALESCE(f.vel_long_listing, 0) AS listing_vel,
			       COALESCE(f.chaos, 0) AS chaos, COALESCE(f.listings, 0) AS listings
			FROM gem_signals s
			LEFT JOIN gem_features f ON f.league = s.league AND f.time = s.time AND f.name = s.name AND f.variant = s.variant
			  AND f.time > NOW() - make_interval(days => $5)
			WHERE s.league = $1 AND s.name = k.name AND s.variant = k.variant
			  AND s.time > NOW() - make_interval(days => $5)
			ORDER BY s.time DESC
			LIMIT $4
		) h`, scope.ID(), names, variants, limit, signalHistorySeedMaxDays)
	if err != nil {
		return nil, fmt.Errorf("lab repo: signal history bulk: %w", err)
	}
	defer rows.Close()

	out := make(map[SignalHistoryKey][]SignalChange)
	for rows.Next() {
		var k SignalHistoryKey
		var c SignalChange
		if err := rows.Scan(&k.Name, &k.Variant, &c.Time, &c.Signal, &c.Window, &c.Advanced,
			&c.PriceVel, &c.ListVel, &c.Price, &c.Listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan signal history bulk: %w", err)
		}
		out[k] = append(out[k], c)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history bulk iteration: %w", err)
	}
	// The lateral rows arrive per key in its ORDER BY, but the outer query
	// does not promise to keep it.
	for _, series := range out {
		sort.SliceStable(series, func(i, j int) bool { return series[i].Time.After(series[j].Time) })
	}
	return out, nil
}

// signalHistorySeedMaxDays bounds the seed and cold-cache fallback, and it is the
// endpoint's semantics rather than an implementation detail: a gem whose last
// signal predates it holds no ring, and /api/analysis/history answers empty for
// it against a warm cache. That is deliberate — the per-request query this seed
// replaced had no time bound at all, so a gem that stopped trading kept serving
// transitions from arbitrarily far back, which the desktop renders as a
// time-of-day with no date.
//
// 14 days is chosen to keep that horizon well clear of anything a user would
// still call current, while staying cheap enough to pay once per process: it
// prunes to the two most recent hypertable chunks. Measured on production
// 2026-08-05 (Allflame, 284,954 signal rows in the window): 1.63 s, dominated by
// the per-series sort, against 13,452 rows returned.
//
// It is NOT a per-gem row cap. That is SignalHistoryDepth's job, applied per
// series below.
const signalHistorySeedMaxDays = 14

// SignalHistoryWindow returns the newest depth transitions per gem and variant,
// keyed for the ring cache. It is the one-off seed populateSignalHistory takes
// on a cold cache, replacing the per-request query for every gem at once.
//
// The depth cap is applied PER SERIES, not to a league-wide set of snapshot
// times, because the endpoint is addressed per series. Ranking by the newest N
// distinct times league-wide instead seeds nothing for any gem that missed those
// times — measured on production 2026-08-05, that was 142 of 721 (name, variant)
// series in Allflame, all of them at the 1/0, 1/20 and 20/0 variants the
// comparator offers, and each one then answered empty by a warm cache with rows
// sitting in the database. It also short-changes a series that merely skipped
// some of those times: 22 further series, 161 entries.
//
// Bounded twice over: to depth rows per series, and to signalHistorySeedMaxDays
// so the time predicate prunes chunks — see that constant for what the window
// means to a caller.
//
// The gem_features join repeats that time bound, though f.time = r.time already
// implies it. Without it the planner scans every gem_features chunk, and on
// production 2026-10-14 (TimescaleDB 2.25.2) it chose an ordered ChunkAppend
// merge join that failed every time with "mergejoin input data is out of order"
// (SQLSTATE XX000). Reading the chunks in primary-key order found no corruption,
// and the query succeeded with timescaledb.enable_chunk_append off. With the
// bound it pruned to three chunks, passed under custom and generic plans, and
// returned the same rows; replayed locally on production's rows, 1.6 s against
// 4-6 s.
func (r *Repository) SignalHistoryWindow(ctx context.Context, scope league.Scope, depth int) (map[signalKey][]SignalChange, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history window: %w", err)
	}
	if depth <= 0 {
		return nil, nil
	}

	rows, err := r.pool.Query(ctx, `
		WITH ranked AS (
			SELECT s.name, s.variant, s.time, s.signal, s.window_signal, s.advanced_signal,
			       row_number() OVER (PARTITION BY s.name, s.variant ORDER BY s.time DESC) AS rn
			FROM gem_signals s
			WHERE s.league = $1 AND s.time > NOW() - make_interval(days => $3)
		)
		SELECT r.name, r.variant, r.time, r.signal, r.window_signal,
		       COALESCE(r.advanced_signal, ''),
		       COALESCE(f.vel_long_price, 0), COALESCE(f.vel_long_listing, 0),
		       COALESCE(f.chaos, 0), COALESCE(f.listings, 0)
		FROM ranked r
		LEFT JOIN gem_features f
		  ON f.league = $1 AND f.time = r.time AND f.name = r.name AND f.variant = r.variant
		 AND f.time > NOW() - make_interval(days => $3)
		WHERE r.rn <= $2`, scope.ID(), depth, signalHistorySeedMaxDays)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query signal history window: %w", err)
	}
	defer rows.Close()

	out := make(map[signalKey][]SignalChange)
	for rows.Next() {
		var k signalKey
		var c SignalChange
		if err := rows.Scan(&k.name, &k.variant, &c.Time, &c.Signal, &c.Window, &c.Advanced,
			&c.PriceVel, &c.ListVel, &c.Price, &c.Listings); err != nil {
			return nil, fmt.Errorf("lab repo: scan signal history window: %w", err)
		}
		out[k] = append(out[k], c)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: signal history window rows iteration: %w", err)
	}

	// Newest first per series, sorted here rather than by an ORDER BY time DESC
	// on the query: with that ORDER BY, TimescaleDB 2.25.2 planned an ordered
	// ChunkAppend over gem_features with its chunks newest first while labelling
	// it ascending for the merge join, and the join failed with "mergejoin input
	// data is out of order" (local and production, 2026-10-13). Without it the
	// planner does not build that path; a series holds at most depth rows.
	for _, ring := range out {
		sort.Slice(ring, func(i, j int) bool { return ring[i].Time.After(ring[j].Time) })
	}

	return out, nil
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

	// The Heist rule holds on both halves — see FilterGemDictionary. Measured
	// 2026-08-05 the local gem_snapshots carries 0 Trarthus rows, so this guards
	// a league whose feed reintroduces them rather than an open instance.
	kept := fromSnapshots[:0]
	for _, n := range fromSnapshots {
		if isHeistOnlyGemName(n) {
			continue
		}
		kept = append(kept, n)
	}
	fromSnapshots = kept

	// The snapshot half needs the support-gem strip too, but ONLY the strip.
	// FilterGemDictionary re-derives transfigured-ness from whatever list it is
	// handed — its `known` set is built from its own argument — so running it
	// over fromSnapshots would re-classify, against the snapshot half as the
	// universe of base names, names the market already classified authoritatively
	// in the is_transfigured column. Applying the strip directly keeps the
	// market's verdict.
	//
	// Measured 2026-08-04, local DB, league Allflame: doing that would be a no-op
	// on this skill half (0 of 341 stripped names lost from the final dictionary)
	// but would drop 45 of 247 names from the transfigured half, which has no
	// second source — those are exactly the names the union exists to rescue.
	// The guard for that lives on the transfigured half, in
	// TestGemNameDictionary_includesLeagueNamesMissingFromGemColors; nothing pins
	// the hazard on this half, because it is inert here today.
	if !transfigured {
		kept = fromSnapshots[:0]
		for _, n := range fromSnapshots {
			if isSupportGemName(n) {
				continue
			}
			kept = append(kept, n)
		}
		fromSnapshots = kept
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
//
// The Heist rule (isHeistOnlyGemName) applies to BOTH halves. gem_colors — the
// list this is normally handed — still carries 12 seeded "of Trarthus" names
// whose base gem is also seeded, so every one of them lands in the transfigured
// half; the collector refuses to store a Trarthus snapshot and every analysis
// path excludes them, which makes each a name the OCR matcher can hit and
// nothing can price (POE-142).
//
// The support rule (isSupportGemName) applies to the skill half only. That
// asymmetry is deliberate and predates this split: is_transfigured comes from
// poe.ninja's alt_ discriminator (convertGemLines in
// internal/collector/ninja.go), which is authoritative, and a name-shape
// heuristic layered on top of an authoritative flag can only subtract from it.
// Measured 2026-08-04 on the local DB the un-stripped half is inert anyway —
// SELECT name FROM gem_snapshots WHERE name LIKE '% Support' AND is_transfigured
// returns 0 rows. TestGemNameDictionary_transfiguredPoolIsNotSupportFiltered
// pins the pass-through.
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
		if isHeistOnlyGemName(n) {
			continue
		}
		if !transfigured && isSupportGemName(n) {
			continue
		}
		names = append(names, n)
	}
	return names
}

// NinjaDivinePriceAsOf is LatestNinjaDivinePrice as of asOf: the newest
// poe.ninja divine snapshot taken at or before asOf, with its time. No such
// row, or a NULL price, is (0, zero time, nil).
func (r *Repository) NinjaDivinePriceAsOf(ctx context.Context, scope league.Scope, asOf time.Time) (float64, time.Time, error) {
	chaos, at, err := r.ninjaDivinePrice(ctx, scope, &asOf)
	if err != nil {
		return 0, time.Time{}, fmt.Errorf("lab repo: ninja divine price as of: %w", err)
	}
	return chaos, at, nil
}
