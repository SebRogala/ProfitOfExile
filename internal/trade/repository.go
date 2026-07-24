package trade

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// Repository handles trade lookup persistence in TimescaleDB.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a trade repository backed by the given connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
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
		`INSERT INTO trade_lookups (league, time, gem, variant, total_listings, price_floor, price_ceiling, median_top10, divine_rate, source, listings)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
		 ON CONFLICT DO NOTHING`,
		scope.ID(), result.FetchedAt, result.Gem, result.Variant,
		result.Total, result.PriceFloor, result.PriceCeiling,
		result.MedianTop10, result.DivinePrice, source, listingsJSON,
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
		        COALESCE(listings, '[]'::jsonb)
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
		if err := rows.Scan(
			&r.FetchedAt, &r.Gem, &r.Variant, &r.Total,
			&r.PriceFloor, &r.PriceCeiling, &r.MedianTop10,
			&r.DivinePrice, &listingsJSON,
		); err != nil {
			return nil, fmt.Errorf("repo: scan latest lookup: %w", err)
		}
		if err := json.Unmarshal(listingsJSON, &r.Listings); err != nil {
			r.Listings = nil // non-fatal: listings may be malformed
		}
		if r.Listings != nil {
			r.Signals = ComputeSignals(r.Listings)
		}
		results = append(results, r)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("repo: iterate latest lookups: %w", err)
	}

	return results, nil
}
