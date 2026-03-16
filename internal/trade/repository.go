package trade

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// TradeLookupSummary is a lightweight row returned by TradeLookupHistory,
// suitable for sparkline/trend overlays.
type TradeLookupSummary struct {
	Time          time.Time `json:"time"`
	PriceFloor    float64   `json:"priceFloor"`
	PriceCeiling  float64   `json:"priceCeiling"`
	MedianTop10   float64   `json:"medianTop10"`
	TotalListings int       `json:"totalListings"`
	DivineRate    float64   `json:"divineRate"`
}

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
func (r *Repository) InsertTradeLookup(ctx context.Context, result *TradeLookupResult, source string) error {
	listingsJSON, err := json.Marshal(result.Listings)
	if err != nil {
		return fmt.Errorf("repo: marshal listings: %w", err)
	}

	_, err = r.pool.Exec(ctx,
		`INSERT INTO trade_lookups (time, gem, variant, total_listings, price_floor, price_ceiling, median_top10, divine_rate, source, listings)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
		 ON CONFLICT DO NOTHING`,
		result.FetchedAt, result.Gem, result.Variant,
		result.Total, result.PriceFloor, result.PriceCeiling,
		result.MedianTop10, result.DivinePrice, source, listingsJSON,
	)
	if err != nil {
		return fmt.Errorf("repo: insert trade lookup: %w", err)
	}

	return nil
}

// TradeLookupHistory returns recent trade lookups for a gem+variant pair within
// the given hour window, ordered by time descending. Used for sparkline/trend
// overlays on the frontend.
func (r *Repository) TradeLookupHistory(ctx context.Context, gem, variant string, hours int) ([]TradeLookupSummary, error) {
	rows, err := r.pool.Query(ctx,
		`SELECT time, COALESCE(price_floor, 0), COALESCE(price_ceiling, 0),
		        COALESCE(median_top10, 0), COALESCE(total_listings, 0), COALESCE(divine_rate, 0)
		 FROM trade_lookups
		 WHERE gem = $1 AND variant = $2
		   AND time > NOW() - make_interval(hours => $3)
		 ORDER BY time DESC`,
		gem, variant, hours,
	)
	if err != nil {
		return nil, fmt.Errorf("repo: query trade lookup history: %w", err)
	}
	defer rows.Close()

	var summaries []TradeLookupSummary
	for rows.Next() {
		var s TradeLookupSummary
		if err := rows.Scan(&s.Time, &s.PriceFloor, &s.PriceCeiling, &s.MedianTop10, &s.TotalListings, &s.DivineRate); err != nil {
			return nil, fmt.Errorf("repo: scan trade lookup: %w", err)
		}
		summaries = append(summaries, s)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("repo: iterate trade lookups: %w", err)
	}

	return summaries, nil
}
