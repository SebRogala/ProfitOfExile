package lab

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"

	"profitofexile/internal/league"
)

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

		variant := dr.Variant
		if variant == "" {
			variant = DefaultDedicationVariant
		}

		batch.Queue(
			`INSERT INTO dedication_snapshots
			 (league, time, variant, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
			  input_cost, profit, fonts_to_hit, mode, thin_pool_gems, liquidity_risk,
			  pool_breakdown, low_confidence_gems)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), dr.Time, variant, dr.Color, dr.GemType, dr.Pool, dr.Winners,
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
// optionally filtered by variant, gemType and/or mode. An empty variant returns
// every analyzed variant.
func (r *Repository) LatestDedicationResults(ctx context.Context, scope league.Scope, variant, gemType, mode string, limit int) ([]DedicationResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest dedication results: %w", err)
	}
	query := `
		SELECT time, variant, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
		       input_cost, profit, fonts_to_hit,
		       COALESCE(mode, 'safe'), COALESCE(thin_pool_gems, 0), COALESCE(liquidity_risk, 'LOW'),
		       COALESCE(pool_breakdown, '[]'::jsonb), COALESCE(low_confidence_gems, '[]'::jsonb)
		FROM dedication_snapshots
		WHERE league = $1 AND time = (
			SELECT MAX(d2.time) FROM dedication_snapshots d2
			WHERE d2.league = $1 AND d2.variant = dedication_snapshots.variant)`
	args := []any{scope.ID()}
	argIdx := 2

	if variant != "" {
		query += fmt.Sprintf(` AND variant = $%d`, argIdx)
		args = append(args, variant)
		argIdx++
	}
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
	return scanDedicationRows(rows)
}

// scanDedicationRows decodes dedication_snapshots rows selected in the
// LatestDedicationResults column order.
func scanDedicationRows(rows pgx.Rows) ([]DedicationResult, error) {
	defer rows.Close()

	var results []DedicationResult
	for rows.Next() {
		var dr DedicationResult
		var poolBreakdownJSON, lowConfJSON []byte
		if err := rows.Scan(&dr.Time, &dr.Variant, &dr.Color, &dr.GemType, &dr.Pool, &dr.Winners,
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

// DedicationOverviewRows returns the latest Dedication snapshot's rows: the
// FontOverviewMaxLimit most profitable of each variant × pool × mode, so every
// top-N across one variant's pool that LatestDedicationResults could answer.
func (r *Repository) DedicationOverviewRows(ctx context.Context, scope league.Scope) ([]DedicationResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: dedication overview rows: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT time, variant, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
		       input_cost, profit, fonts_to_hit, mode, thin_pool_gems, liquidity_risk,
		       pool_breakdown, low_confidence_gems
		FROM (
			SELECT time, variant, color, gem_type, pool, winners, p_win, avg_win_raw, ev_raw,
			       input_cost, profit, fonts_to_hit,
			       COALESCE(mode, 'safe') AS mode, COALESCE(thin_pool_gems, 0) AS thin_pool_gems,
			       COALESCE(liquidity_risk, 'LOW') AS liquidity_risk,
			       COALESCE(pool_breakdown, '[]'::jsonb) AS pool_breakdown,
			       COALESCE(low_confidence_gems, '[]'::jsonb) AS low_confidence_gems,
			       row_number() OVER (PARTITION BY variant, gem_type, COALESCE(mode, 'safe') ORDER BY profit DESC) AS rank
			FROM dedication_snapshots
			WHERE league = $1 AND time = (SELECT MAX(time) FROM dedication_snapshots WHERE league = $1)
		) ranked
		WHERE rank <= $2
		ORDER BY profit DESC`, scope.ID(), FontOverviewMaxLimit)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query dedication overview rows: %w", err)
	}
	return scanDedicationRows(rows)
}

// SaveDoubleCorruptResults persists one tick's double-corruption EV results.
//
// ON CONFLICT DO NOTHING makes the write idempotent against the primary key
// (league, time, name, input_variant), which is what lets the tick run twice for
// one snapshot without duplicating or rewriting history — the same contract the
// sibling result writers keep.
func (r *Repository) SaveDoubleCorruptResults(ctx context.Context, scope league.Scope, results []DoubleCorruptResult) (int, error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("lab repo: save double corrupt results: %w", err)
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
	for _, dc := range results {
		outcomesJSON, err := json.Marshal(dc.Outcomes)
		if err != nil {
			slog.Warn("lab repo: marshal double corrupt outcomes failed, using empty array",
				"name", dc.Name, "error", err)
			outcomesJSON = []byte("[]")
		}

		inputVariant := dc.InputVariant
		if inputVariant == "" {
			inputVariant = DefaultDoubleCorruptVariant
		}

		batch.Queue(
			`INSERT INTO double_corrupt_snapshots
			 (league, time, name, input_variant, color, input_cost, temple_overhead_chaos,
			  has_vaal_version, ev, ev_raw, profit, priced_probability, unpriced_probability,
			  thin_outcome_cells, liquidity_risk, model, outcomes)
			 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
			 ON CONFLICT DO NOTHING`,
			scope.ID(), dc.Time, dc.Name, inputVariant, dc.Color, dc.InputCost,
			dc.TempleOverheadChaos, dc.HasVaalVersion, dc.EV, dc.EVRaw, dc.Profit,
			dc.PricedProbability, dc.UnpricedProbability, dc.ThinOutcomeCells,
			dc.LiquidityRisk, dc.Model, outcomesJSON,
		)
	}

	br := tx.SendBatch(ctx, batch)
	inserted := 0
	for range results {
		ct, err := br.Exec()
		if err != nil {
			br.Close()
			return 0, fmt.Errorf("lab repo: insert double corrupt result: %w", err)
		}
		inserted += int(ct.RowsAffected())
	}
	if err := br.Close(); err != nil {
		return 0, fmt.Errorf("lab repo: close batch: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("lab repo: commit double corrupt results: %w", err)
	}

	return inserted, nil
}

// LatestDoubleCorruptResults returns the most recent double-corruption results,
// profit-descending. An empty inputVariant returns every analyzed input variant;
// pass one to keep a caller inside a single market (the per-variant rule).
//
// This is the cold path behind the ranking view and the compare tiebreaker — the
// warm path reads the cache. It is bounded by the latest tick's timestamp rather
// than by a time window, so it never scans the hypertable.
func (r *Repository) LatestDoubleCorruptResults(ctx context.Context, scope league.Scope, inputVariant string, limit int) ([]DoubleCorruptResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: latest double corrupt results: %w", err)
	}
	query := `
		SELECT time, name, input_variant, color, input_cost, temple_overhead_chaos,
		       has_vaal_version, ev, ev_raw, profit, priced_probability,
		       unpriced_probability, thin_outcome_cells, liquidity_risk, model,
		       COALESCE(outcomes, '[]'::jsonb)
		FROM double_corrupt_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM double_corrupt_snapshots WHERE league = $1)`
	args := []any{scope.ID()}
	argIdx := 2

	if inputVariant != "" {
		query += fmt.Sprintf(` AND input_variant = $%d`, argIdx)
		args = append(args, inputVariant)
		argIdx++
	}

	query += fmt.Sprintf(` ORDER BY profit DESC, name ASC LIMIT $%d`, argIdx)
	args = append(args, limit)

	rows, err := r.pool.Query(ctx, query, args...)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query double corrupt results: %w", err)
	}
	defer rows.Close()

	var results []DoubleCorruptResult
	for rows.Next() {
		var dc DoubleCorruptResult
		var outcomesJSON []byte
		if err := rows.Scan(&dc.Time, &dc.Name, &dc.InputVariant, &dc.Color, &dc.InputCost,
			&dc.TempleOverheadChaos, &dc.HasVaalVersion, &dc.EV, &dc.EVRaw, &dc.Profit,
			&dc.PricedProbability, &dc.UnpricedProbability, &dc.ThinOutcomeCells,
			&dc.LiquidityRisk, &dc.Model, &outcomesJSON); err != nil {
			return nil, fmt.Errorf("lab repo: scan double corrupt result: %w", err)
		}
		if len(outcomesJSON) > 0 {
			if err := json.Unmarshal(outcomesJSON, &dc.Outcomes); err != nil {
				return nil, fmt.Errorf("lab repo: unmarshal double corrupt outcomes: %w", err)
			}
		}
		results = append(results, dc)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: double corrupt rows iteration: %w", err)
	}

	return results, nil
}

// DoubleCorruptResultsByNames returns the latest results for named gems at one
// input variant — the compare path's cold join, answering only for the two or
// three gems a Font offered rather than loading the whole corpus.
//
// inputVariant is required here, unlike LatestDoubleCorruptResults: a compare
// request is always about one market, and letting it default would mix input
// variants under one gem name.
func (r *Repository) DoubleCorruptResultsByNames(ctx context.Context, scope league.Scope, inputVariant string, names []string) ([]DoubleCorruptResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: double corrupt results by names: %w", err)
	}
	if inputVariant == "" {
		return nil, fmt.Errorf("lab repo: double corrupt results by names: input variant is required")
	}
	if len(names) == 0 {
		return nil, nil
	}

	rows, err := r.pool.Query(ctx, `
		SELECT time, name, input_variant, color, input_cost, temple_overhead_chaos,
		       has_vaal_version, ev, ev_raw, profit, priced_probability,
		       unpriced_probability, thin_outcome_cells, liquidity_risk, model,
		       COALESCE(outcomes, '[]'::jsonb)
		FROM double_corrupt_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM double_corrupt_snapshots WHERE league = $1)
		  AND input_variant = $2
		  AND name = ANY($3)
		ORDER BY profit DESC, name ASC`, scope.ID(), inputVariant, names)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query double corrupt results by names: %w", err)
	}
	defer rows.Close()

	var results []DoubleCorruptResult
	for rows.Next() {
		var dc DoubleCorruptResult
		var outcomesJSON []byte
		if err := rows.Scan(&dc.Time, &dc.Name, &dc.InputVariant, &dc.Color, &dc.InputCost,
			&dc.TempleOverheadChaos, &dc.HasVaalVersion, &dc.EV, &dc.EVRaw, &dc.Profit,
			&dc.PricedProbability, &dc.UnpricedProbability, &dc.ThinOutcomeCells,
			&dc.LiquidityRisk, &dc.Model, &outcomesJSON); err != nil {
			return nil, fmt.Errorf("lab repo: scan double corrupt result: %w", err)
		}
		if len(outcomesJSON) > 0 {
			if err := json.Unmarshal(outcomesJSON, &dc.Outcomes); err != nil {
				return nil, fmt.Errorf("lab repo: unmarshal double corrupt outcomes: %w", err)
			}
		}
		results = append(results, dc)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("lab repo: double corrupt by-names rows iteration: %w", err)
	}

	return results, nil
}

// CorruptedGemNamesAutocomplete returns distinct corrupted gem names for the
// Dedication picker. When isTransfigured is true, returns only transfigured
// corrupted gems; when false, only non-transfigured corrupted gems.
// Restricts to the last 2 hours to avoid full hypertable scans.
//
// The filters are the SQL half of isDedicationOutcome (eligibility.go): a picker
// for the Dedication compare surface must not offer gems the craft can never
// hand out. Before POE-142 it applied the support and Heist rules and no colour
// rule at all, so — measured 2026-08-05 on the latest local Allflame snapshot —
// it offered Pact of Beidat, Ghorr, K'Tash and Lycia, the four WHITE 3.29 gems
// that isDedicationOutcome excludes from the pool, from tier classification,
// from the rankings, and that the compare path can only ever render as AVOID.
// The non-transfigured half went from 339 names to 335.
//
// The colour rule applied here is the WHITE one (sqlNotColorlessByRule), not the
// three-colour one. A typeahead over names does not partition the market by
// colour, so it has no reason to pay hasAttributeColor's cost of dropping every
// gem the gemcolor resolver has not met yet — the collector writes a NULL colour
// for those (internal/collector/repository.go), and a player typing a name the
// resolver is behind on would get no results at all. Measured 2026-08-05 on the
// 05:28 Allflame snapshot, where the resolver was still behind: the three-colour
// form returned 333 names, dropping Dark Bargain and Mana-Infused Staff — real
// BLUE skill gems by the 12:39 snapshot — alongside the Pact gems.
//
// Vaal gems are deliberately still offered. They are a legal Dedication feed,
// and BuildDedicationCorpus documents the compare path as answering for them and
// marking them not-an-outcome — so the picker keeps the one non-outcome a player
// has a reason to look up.
func (r *Repository) CorruptedGemNamesAutocomplete(ctx context.Context, scope league.Scope, isTransfigured bool, limit int) ([]string, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem names autocomplete: %w", err)
	}
	rows, err := r.pool.Query(ctx, `
		SELECT DISTINCT name FROM gem_snapshots
		WHERE league = $1
		  AND is_corrupted = true
		  AND is_transfigured = $2
		  AND `+sqlNotSupportGem+`
		  AND `+sqlNotHeistOnly+`
		  AND `+sqlNotColorlessByRule+`
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

// CorruptedGemPricesByNames returns the latest snapshot prices for specific corrupted gems
// at one Dedication variant. Used by the Dedication compare endpoint.
func (r *Repository) CorruptedGemPricesByNames(ctx context.Context, scope league.Scope, names []string, variant string) ([]GemPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("lab repo: corrupted gem prices by names: %w", err)
	}
	if len(names) == 0 {
		return nil, nil
	}
	if variant == "" {
		variant = DefaultDedicationVariant
	}

	rows, err := r.pool.Query(ctx, `
		SELECT name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		       is_transfigured, is_corrupted, COALESCE(gem_color, ''), sample_count, sparkline_empty
		FROM gem_snapshots
		WHERE league = $1
		  AND time = (SELECT MAX(time) FROM gem_snapshots WHERE league = $1 AND variant = $3 AND is_corrupted = true)
		  AND is_corrupted = true
		  AND variant = $3
		  AND name = ANY($2)`, scope.ID(), names, variant)
	if err != nil {
		return nil, fmt.Errorf("lab repo: query corrupted gem prices by names: %w", err)
	}
	defer rows.Close()

	var gems []GemPrice
	for rows.Next() {
		var g GemPrice
		if err := rows.Scan(&g.Name, &g.Variant, &g.Chaos, &g.Listings,
			&g.IsTransfigured, &g.IsCorrupted, &g.GemColor, &g.SampleCount, &g.SparklineEmpty); err != nil {
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
