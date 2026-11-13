package trade

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
	"profitofexile/internal/price/resolve"
)

// Repository handles trade lookup persistence in TimescaleDB.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a trade repository backed by the given connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// KnownGem checks exact membership in the raw gem-name sources. Unlike the OCR
// dictionary, these retain support, Heist and corrupted names; a trade lookup
// is not limited to gems eligible for Labyrinth outcomes. Snapshot membership
// is league-scoped; gem_colors is the shared name registry.
func (r *Repository) KnownGem(ctx context.Context, scope league.Scope, name string) (bool, error) {
	if err := scope.Validate(); err != nil {
		return false, fmt.Errorf("repo: known gem: %w", err)
	}
	var known bool
	err := r.pool.QueryRow(ctx,
		`SELECT EXISTS (
		    SELECT 1 FROM gem_colors WHERE name = $2
		    UNION ALL
		    SELECT 1 FROM gem_snapshots WHERE league = $1 AND name = $2
		)`, scope.ID(), name).Scan(&known)
	if err != nil {
		return false, fmt.Errorf("repo: known gem: %w", err)
	}
	return known, nil
}

// InsertTradeLookup persists a single trade lookup result. Uses ON CONFLICT DO
// NOTHING to deduplicate rows with the same (time, gem, variant) key.
func (r *Repository) InsertTradeLookup(ctx context.Context, scope league.Scope, result *TradeLookupResult, source string) error {
	if err := scope.Validate(); err != nil {
		return fmt.Errorf("repo: insert trade lookup: %w", err)
	}

	listingsJSON, err := json.Marshal(result.Listings)
	if err != nil {
		return fmt.Errorf("repo: marshal listings: %w", err)
	}

	_, err = r.pool.Exec(ctx,
		`INSERT INTO trade_lookups (league, time, gem, variant, total_listings, price_floor, price_ceiling, median_top10, divine_rate, source, listings, unique_accounts)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
		 ON CONFLICT DO NOTHING`,
		scope.ID(), result.FetchedAt, result.Gem, result.Variant,
		result.Total, result.PriceFloor, result.PriceCeiling,
		result.MedianTop10, result.DivinePrice, source, listingsJSON,
		result.Signals.UniqueAccounts,
	)
	if err != nil {
		return fmt.Errorf("repo: insert trade lookup: %w", err)
	}

	return nil
}

// LatestLookups returns the most recent trade lookup per gem+variant,
// limited to entries within the given hour window. Used to warm the
// in-memory TradeCache on server startup.
func (r *Repository) LatestLookups(ctx context.Context, scope league.Scope, hours int) ([]TradeLookupResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("repo: latest lookups: %w", err)
	}

	rows, err := r.pool.Query(ctx,
		`SELECT DISTINCT ON (gem, variant)
		        time, gem, variant, COALESCE(total_listings, 0),
		        COALESCE(price_floor, 0), COALESCE(price_ceiling, 0),
		        COALESCE(median_top10, 0), COALESCE(divine_rate, 0),
		        COALESCE(listings, '[]'::jsonb), COALESCE(unique_accounts, 0)
		 FROM trade_lookups
		 WHERE league = $1
		   AND time > NOW() - make_interval(hours => $2)
		 ORDER BY gem, variant, time DESC`,
		scope.ID(), hours,
	)
	if err != nil {
		return nil, fmt.Errorf("repo: query latest lookups: %w", err)
	}
	defer rows.Close()

	var results []TradeLookupResult
	for rows.Next() {
		var r TradeLookupResult
		var listingsJSON []byte
		var uniqueAccounts int
		if err := rows.Scan(
			&r.FetchedAt, &r.Gem, &r.Variant, &r.Total,
			&r.PriceFloor, &r.PriceCeiling, &r.MedianTop10,
			&r.DivinePrice, &listingsJSON, &uniqueAccounts,
		); err != nil {
			return nil, fmt.Errorf("repo: scan latest lookup: %w", err)
		}
		if err := json.Unmarshal(listingsJSON, &r.Listings); err != nil {
			r.Listings = nil // non-fatal: listings may be malformed
		}
		if r.Listings != nil {
			r.Signals = ComputeSignals(r.Listings, uniqueAccounts)
		}
		results = append(results, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("repo: iterate latest lookups: %w", err)
	}

	return results, nil
}

// LookupsAsOf returns the newest stored lookup per gem variant at or before
// asOf in the scope's league, with no age cap. Only identity, time and the
// listings are read; malformed listings JSON leaves that row's Listings nil.
func (r *Repository) LookupsAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]TradeLookupResult, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("repo: lookups as of: %w", err)
	}

	rows, err := r.pool.Query(ctx,
		`SELECT DISTINCT ON (gem, variant)
		        time, gem, variant, COALESCE(listings, '[]'::jsonb)
		 FROM trade_lookups
		 WHERE league = $1
		   AND time <= $2
		 ORDER BY gem, variant, time DESC`,
		scope.ID(), asOf,
	)
	if err != nil {
		return nil, fmt.Errorf("repo: lookups as of: %w", err)
	}
	defer rows.Close()

	var results []TradeLookupResult
	for rows.Next() {
		var r TradeLookupResult
		var listingsJSON []byte
		if err := rows.Scan(&r.FetchedAt, &r.Gem, &r.Variant, &listingsJSON); err != nil {
			return nil, fmt.Errorf("repo: lookups as of: %w", err)
		}
		if err := json.Unmarshal(listingsJSON, &r.Listings); err != nil {
			r.Listings = nil
		}
		results = append(results, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("repo: lookups as of: %w", err)
	}
	return results, nil
}

// ObservationsAsOf is the trade adapter's read: the stored lookups as of asOf
// turned into native trade observations for the scope's league.
func (r *Repository) ObservationsAsOf(ctx context.Context, scope league.Scope, asOf time.Time) ([]resolve.Observation, error) {
	rows, err := r.LookupsAsOf(ctx, scope, asOf)
	if err != nil {
		return nil, err
	}
	return Observations(scope.ID(), rows), nil
}
