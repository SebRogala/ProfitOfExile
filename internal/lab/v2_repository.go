package lab

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/league"
	"profitofexile/internal/price/tier"
)

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

// previousGemTiers returns the gem_features tiers of the latest snapshot
// strictly before asOf and no older than labSnapshotGapMax, with that
// snapshot's time; found is false when there is none.
func (r *Repository) previousGemTiers(ctx context.Context, scope league.Scope, asOf time.Time) (GemClassificationMap, time.Time, bool, error) {
	if err := scope.Validate(); err != nil {
		return nil, time.Time{}, false, fmt.Errorf("lab repo: previous gem tiers: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT time, name, variant, tier
		FROM gem_features
		WHERE league = $1 AND time = (
			SELECT MAX(time) FROM gem_features
			WHERE league = $1 AND time < $2 AND time >= $2::timestamptz - $3 * interval '1 second')`,
		scope.ID(), asOf, labSnapshotGapMax.Seconds())
	if err != nil {
		return nil, time.Time{}, false, fmt.Errorf("lab repo: query previous gem tiers: %w", err)
	}
	defer rows.Close()
	tiers := make(GemClassificationMap)
	var at time.Time
	found := false
	for rows.Next() {
		var key GemClassificationKey
		var c GemClassification
		if err := rows.Scan(&at, &key.Name, &key.Variant, &c.Tier); err != nil {
			return nil, time.Time{}, false, fmt.Errorf("lab repo: scan previous gem tier: %w", err)
		}
		tiers[key] = c
		found = true
	}
	if err := rows.Err(); err != nil {
		return nil, time.Time{}, false, fmt.Errorf("lab repo: iterate previous gem tiers: %w", err)
	}
	return tiers, at, found, nil
}

// previousTierBoundaries returns, per variant except "all", the tier
// boundaries of the latest market_context row strictly before asOf whose
// boundaries name a HIGH, at any age. A variant without one is absent.
func (r *Repository) previousTierBoundaries(ctx context.Context, scope league.Scope, asOf time.Time) (map[string]tier.Boundaries, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: previous tier boundaries: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT DISTINCT ON (v.key) v.key, v.value->'tiers'
		FROM market_context mc,
		     jsonb_each(CASE WHEN jsonb_typeof(mc.variant_stats) = 'object' THEN mc.variant_stats ELSE '{}'::jsonb END) AS v
		WHERE mc.league = $1 AND mc.time < $2 AND v.key <> 'all'
		  AND v.value->'tiers'->'names' @> jsonb_build_array($3::text)
		ORDER BY v.key, mc.time DESC`, scope.ID(), asOf, tier.High)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query previous tier boundaries: %w", err)
	}
	defer rows.Close()
	result := make(map[string]tier.Boundaries)
	for rows.Next() {
		var variant string
		var raw []byte
		if err := rows.Scan(&variant, &raw); err != nil {
			return nil, fmt.Errorf("lab repo: scan previous tier boundaries: %w", err)
		}
		var b tier.Boundaries
		if err := json.Unmarshal(raw, &b); err != nil {
			return nil, fmt.Errorf("lab repo: unmarshal previous tier boundaries for %s: %w", variant, err)
		}
		result[variant] = b
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: iterate previous tier boundaries: %w", err)
	}
	return result, nil
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
