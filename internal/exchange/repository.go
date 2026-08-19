package exchange

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// insertMarketSQL writes one normalized market row for one feed hour.
//
// Seventeen columns: the league, the feed hour and the fifteen persisted Row
// fields. Row.LowestPriceBInA and Row.HighestPriceBInA are absent on purpose —
// LoadRows derives them again with Ratio so the prices have exactly one source
// of truth (see internal/exchange/doc.go).
//
// ON CONFLICT DO NOTHING against the (league, time, market_id) primary key is
// what makes re-ingesting an hour after a crash idempotent.
const insertMarketSQL = `INSERT INTO currency_exchange_markets (
	league, time, market_id, item_a, item_b,
	volume_a, volume_b,
	lowest_stock_a, lowest_stock_b, highest_stock_a, highest_stock_b,
	lowest_ratio_a, lowest_ratio_b, highest_ratio_a, highest_ratio_b,
	price_valid, invalid_reason
) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
ON CONFLICT DO NOTHING`

// upsertCursorSQL advances the league's ingest cursor. It runs in the same
// transaction as the hour's inserts, so the cursor can never point past an hour
// whose rows were not committed.
const upsertCursorSQL = `INSERT INTO currency_exchange_cursor (league, next_hour, updated_at)
VALUES ($1, $2, now())
ON CONFLICT (league) DO UPDATE SET next_hour = EXCLUDED.next_hour, updated_at = now()`

// selectRowsSQL reads back the persisted columns for a half-open hour window.
const selectRowsSQL = `SELECT time, market_id, item_a, item_b,
	volume_a, volume_b,
	lowest_stock_a, lowest_stock_b, highest_stock_a, highest_stock_b,
	lowest_ratio_a, lowest_ratio_b, highest_ratio_a, highest_ratio_b,
	price_valid, invalid_reason
FROM currency_exchange_markets
WHERE league = $1 AND time >= $2 AND time < $3
ORDER BY time, market_id`

// selectCursorSQL reads the league's next unfetched hour.
const selectCursorSQL = `SELECT next_hour FROM currency_exchange_cursor WHERE league = $1`

// selectNewestHourSQL reads the newest feed hour the league has rows for.
//
// This is deliberately not the cursor: the cursor is where the walk goes NEXT,
// and it advances past hours that stored zero rows for this league (see
// internal/exchange/doc.go). A recompute needs the newest hour that actually
// carries data, so it aggregates a window that ends on real rows.
//
// MAX over the hypertable's partitioning column is a chunk-bounded scan rather
// than a full read, and the predicate narrows it to one league.
const selectNewestHourSQL = `SELECT MAX(time) FROM currency_exchange_markets WHERE league = $1`

// Repository persists currency-exchange hours in TimescaleDB. It is the only
// reader and the only writer of currency_exchange_markets and
// currency_exchange_cursor: downstream engines go through NewestHour and
// LoadRows rather than writing their own SQL.
//
// Cursor and InsertHour serve the ingest walk; NewestHour and LoadRows serve the
// read side (the server's recompute, see service.go).
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a currency-exchange repository backed by the given
// connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// StoredRow is a Row read back from storage together with the feed hour it
// belongs to. The embedded Row carries fully populated prices: LoadRows derives
// them from the stored quantities with Ratio.
type StoredRow struct {
	Hour time.Time
	Row
}

// Cursor returns the next unfetched feed hour for the scope as unix seconds.
// found is false when the league has no cursor row yet, which is the runner's
// signal to bootstrap.
func (r *Repository) Cursor(ctx context.Context, scope league.Scope) (nextHour int64, found bool, err error) {
	if err := scope.Validate(); err != nil {
		return 0, false, fmt.Errorf("exchange repo: cursor: %w", err)
	}

	err = r.pool.QueryRow(ctx, selectCursorSQL, scope.ID()).Scan(&nextHour)
	if errors.Is(err, pgx.ErrNoRows) {
		return 0, false, nil
	}
	if err != nil {
		return 0, false, fmt.Errorf("exchange repo: cursor: %w", err)
	}
	return nextHour, true, nil
}

// InsertHour stores one feed hour's rows and advances the cursor to nextHour in
// a single transaction, and returns how many market rows were actually inserted
// (conflicts excluded).
//
// hour is the feed hour itself, not the collection time. Every row must already
// belong to the scope; a foreign row is rejected before the transaction opens,
// because scope.ID() is bound as the league column and a mismatched Row.League
// would be silently rewritten otherwise.
//
// An hour with no rows for this league still advances the cursor: that is the
// case a MAX(time) cursor could not express (see internal/exchange/doc.go).
func (r *Repository) InsertHour(ctx context.Context, scope league.Scope, hour time.Time, rows []Row, nextHour int64) (inserted int, err error) {
	if err := scope.Validate(); err != nil {
		return 0, fmt.Errorf("exchange repo: insert hour: %w", err)
	}
	for i := range rows {
		if rows[i].League != scope.ID() {
			return 0, fmt.Errorf("exchange repo: insert hour: row %q belongs to league %q, scope is %q",
				rows[i].MarketID, rows[i].League, scope.ID())
		}
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, fmt.Errorf("exchange repo: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	if len(rows) > 0 {
		batch := &pgx.Batch{}
		for i := range rows {
			row := &rows[i]
			batch.Queue(insertMarketSQL,
				scope.ID(), hour, row.MarketID, row.ItemA, row.ItemB,
				row.VolumeA, row.VolumeB,
				row.LowestStockA, row.LowestStockB, row.HighestStockA, row.HighestStockB,
				row.LowestRatioA, row.LowestRatioB, row.HighestRatioA, row.HighestRatioB,
				row.PriceValid, row.InvalidReason,
			)
		}

		br := tx.SendBatch(ctx, batch)
		for range rows {
			ct, execErr := br.Exec()
			if execErr != nil {
				br.Close()
				return 0, fmt.Errorf("exchange repo: insert market row: %w", execErr)
			}
			inserted += int(ct.RowsAffected())
		}
		if err := br.Close(); err != nil {
			return 0, fmt.Errorf("exchange repo: insert hour: close batch: %w", err)
		}
	}

	if _, err := tx.Exec(ctx, upsertCursorSQL, scope.ID(), nextHour); err != nil {
		return 0, fmt.Errorf("exchange repo: upsert cursor: %w", err)
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, fmt.Errorf("exchange repo: commit hour: %w", err)
	}

	return inserted, nil
}

// NewestHour returns the newest feed hour the scope has stored rows for. found
// is false when the league has no rows at all — a fresh league, or one whose
// hours were wiped at rollover — which callers read as "nothing to aggregate"
// rather than as an error.
//
// The returned time is pinned to UTC for the same reason LoadRows pins its
// hours: pgx hands a TIMESTAMPTZ back in time.Local, and the feed hour is a UTC
// identity that callers render into caches and event payloads.
//
// This is the read side's window anchor: recompute asks for the newest stored
// hour and then loads backwards from it, so a stalled feed narrows the window
// instead of returning nothing.
func (r *Repository) NewestHour(ctx context.Context, scope league.Scope) (time.Time, bool, error) {
	if err := scope.Validate(); err != nil {
		return time.Time{}, false, fmt.Errorf("exchange repo: newest hour: %w", err)
	}

	// MAX over zero rows is one row holding NULL, not pgx.ErrNoRows, so the
	// scan target has to be nullable — a plain time.Time would fail to scan
	// exactly in the empty-league case this reports as found == false.
	var newest *time.Time
	if err := r.pool.QueryRow(ctx, selectNewestHourSQL, scope.ID()).Scan(&newest); err != nil {
		return time.Time{}, false, fmt.Errorf("exchange repo: newest hour: %w", err)
	}
	if newest == nil {
		return time.Time{}, false, nil
	}
	return newest.UTC(), true, nil
}

// LoadRows returns the scope's stored rows for the half-open hour window
// [from, to), ordered by hour then market id.
//
// This is the read API for downstream engines. Prices are not stored, so each
// returned Row has LowestPriceBInA and HighestPriceBInA derived from its
// quantities with Ratio — the same function Normalize used on ingest, never an
// inline division.
func (r *Repository) LoadRows(ctx context.Context, scope league.Scope, from, to time.Time) ([]StoredRow, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("exchange repo: load rows: %w", err)
	}

	queryRows, err := r.pool.Query(ctx, selectRowsSQL, scope.ID(), from, to)
	if err != nil {
		return nil, fmt.Errorf("exchange repo: query rows: %w", err)
	}
	defer queryRows.Close()

	var stored []StoredRow
	for queryRows.Next() {
		s := StoredRow{Row: Row{League: scope.ID()}}
		if err := queryRows.Scan(
			&s.Hour, &s.MarketID, &s.ItemA, &s.ItemB,
			&s.VolumeA, &s.VolumeB,
			&s.LowestStockA, &s.LowestStockB, &s.HighestStockA, &s.HighestStockB,
			&s.LowestRatioA, &s.LowestRatioB, &s.HighestRatioA, &s.HighestRatioB,
			&s.PriceValid, &s.InvalidReason,
		); err != nil {
			return nil, fmt.Errorf("exchange repo: scan row: %w", err)
		}

		// pgx hands a TIMESTAMPTZ back in time.Local, so the same stored hour
		// formats differently depending on the process TZ. The feed hour is a UTC
		// identity, not a local wall clock, and callers key caches and event
		// payloads on its rendering — pin it back to UTC.
		s.Hour = s.Hour.UTC()

		// Re-derive both prices exactly as Normalize did: the price of one ItemB
		// in ItemA units, from each ratio pair. Both stay 0 when either pair is
		// unusable, which is the same state PriceValid == false records.
		lowest, lowestOK := Ratio(s.LowestRatioA, s.LowestRatioB)
		highest, highestOK := Ratio(s.HighestRatioA, s.HighestRatioB)
		if lowestOK && highestOK {
			s.LowestPriceBInA = lowest
			s.HighestPriceBInA = highest
		}

		stored = append(stored, s)
	}
	if err := queryRows.Err(); err != nil {
		return nil, fmt.Errorf("exchange repo: iterate rows: %w", err)
	}

	return stored, nil
}
