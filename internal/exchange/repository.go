package exchange

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"strings"
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

// playColumns are the currency_exchange_plays columns ReplacePlays copies, in
// the order playRow emits them.
var playColumns = []string{
	"league", "horizon", "play_key", "served_rank", "mode", "leg_count",
	"side_items", "side_categories", "side_names", "first_quote",
	"investment", "run_investment", "roi", "roi_pct", "turnover", "tick",
	"sort_expected", "sort_roi", "sort_fastest_hours", "play",
}

const deletePlaysSQL = `DELETE FROM currency_exchange_plays WHERE league = $1`

const deletePlaySetsSQL = `DELETE FROM currency_exchange_play_sets WHERE league = $1`

const insertPlaySetSQL = `INSERT INTO currency_exchange_play_sets
	(league, horizon, hours, divine_chaos_rate, window_from, window_to, last_updated, computed_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now())`

// ReplacePlays replaces the scope's stored plays with results, one Result per
// horizon, in one transaction: it deletes every horizon's plays, copies the new
// ones in served order (served_rank is the index in Result.Plays), then writes
// the set headers. A failure at any step rolls back to the previous answer.
//
// Every play is stored with its display (Decorate) whether or not the caller
// decorated it; the filter and sort columns are read from that display.
func (r *Repository) ReplacePlays(ctx context.Context, scope league.Scope, results []Result) error {
	if err := scope.Validate(); err != nil {
		return fmt.Errorf("exchange repo: replace plays: %w", err)
	}

	var rows [][]any
	for _, res := range results {
		for rank, play := range res.Plays {
			row, err := playRow(scope.ID(), res.Horizon, rank, play)
			if err != nil {
				return fmt.Errorf("exchange repo: replace plays: %w", err)
			}
			rows = append(rows, row)
		}
	}

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("exchange repo: replace plays: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	if _, err := tx.Exec(ctx, deletePlaysSQL, scope.ID()); err != nil {
		return fmt.Errorf("exchange repo: replace plays: delete plays: %w", err)
	}
	if _, err := tx.CopyFrom(ctx, pgx.Identifier{"currency_exchange_plays"}, playColumns, pgx.CopyFromRows(rows)); err != nil {
		return fmt.Errorf("exchange repo: replace plays: copy plays: %w", err)
	}
	if _, err := tx.Exec(ctx, deletePlaySetsSQL, scope.ID()); err != nil {
		return fmt.Errorf("exchange repo: replace plays: delete sets: %w", err)
	}
	for _, res := range results {
		var last *time.Time
		if l, ok := LastUpdated(res); ok {
			l = l.UTC()
			last = &l
		}
		if _, err := tx.Exec(ctx, insertPlaySetSQL, scope.ID(), res.Horizon, res.Hours,
			res.DivineChaosRate, res.From, res.To, last); err != nil {
			return fmt.Errorf("exchange repo: replace plays: insert set %q: %w", res.Horizon, err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("exchange repo: replace plays: commit: %w", err)
	}
	return nil
}

// playRow is one play's currency_exchange_plays row.
func playRow(leagueID, horizon string, rank int, play Play) ([]any, error) {
	play = Decorate(play)
	d := play.Display

	var items, categories, names []string
	seen := make(map[string]bool)
	for _, leg := range play.Legs {
		for _, id := range []string{leg.Item, leg.Quote} {
			if seen[id] {
				continue
			}
			seen[id] = true
			item, _ := LookupItem(id)
			items = append(items, id)
			categories = append(categories, item.Category)
			names = append(names, DisplayName(id))
		}
	}
	firstQuote := ""
	if len(play.Legs) > 0 {
		firstQuote = play.Legs[0].Quote
	}

	blob, err := json.Marshal(jsonSafePlay(play))
	if err != nil {
		return nil, fmt.Errorf("encode play %q: %w", play.Key, err)
	}

	return []any{
		leagueID, horizon, play.Key, rank, string(play.Mode), len(play.Legs),
		nonNil(items), nonNil(categories), nonNil(names), firstQuote,
		sqlNumber(play.Investment), sqlNumber(rawRunInvestment(play)),
		sqlNumber(play.Roi), sqlNumber(play.RoiPct), sqlNumber(play.Turnover), sqlNumber(play.Tick),
		sqlNumber(float64(d.SortKeys.Expected)), sqlNumber(float64(d.SortKeys.Roi)),
		sqlNumber(float64(d.SortKeys.FastestHours)), blob,
	}, nil
}

// rawRunInvestment is runInvestment with ±Inf kept: the display sends it as
// null, but the desktop's run-cost bound compares the unbounded value, so an
// Infinity run cost exceeds every ceiling (parity case "run cost bounds").
func rawRunInvestment(play Play) float64 {
	if math.IsNaN(play.ExpectedRoi) || math.IsInf(play.ExpectedRoi, 0) || play.ExpectedRoi <= 0 {
		return play.Investment
	}
	return play.Investment * math.Ceil(ScaleTargetChaos/play.ExpectedRoi)
}

// jsonSafePlay zeroes the play's non-finite top-level numbers so it encodes:
// JSON cannot carry them, and the legacy encoder would fail the whole body on
// one. The filter and sort columns keep the measured value, and the display
// already carries it as null.
func jsonSafePlay(play Play) Play {
	for _, f := range []*float64{
		&play.RoiPct, &play.Edge, &play.RoiPctRaw, &play.Roi, &play.Investment,
		&play.Turnover, &play.Tick, &play.Depth, &play.WindowVolume,
		&play.ExpectedRoi, &play.ExpectedRoiPct,
	} {
		if math.IsNaN(*f) || math.IsInf(*f, 0) {
			*f = 0
		}
	}
	return play
}

// sqlNumber stores NaN as NULL, so a SQL comparison against it is unknown and
// keeps the row, as a NaN comparison keeps it on the desktop (Postgres orders
// NaN above every number instead).
func sqlNumber(f float64) any {
	if math.IsNaN(f) {
		return nil
	}
	return f
}

func nonNil(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}

const selectPlaySetSQL = `SELECT hours, divine_chaos_rate, window_from, window_to, last_updated
FROM currency_exchange_play_sets WHERE league = $1 AND horizon = $2`

// Parameters shared by the stage-count and page statements:
// $1 league, $2 horizon, $3 mode ('all' = every mode), $4 onlyItem,
// $5 hideItem, $6 onlyCategory, $7 hideCategory, $8 any only rule,
// $9–$15 the seven gates, $16 the set's divine rate, $17 investMin,
// $18 investMax, $19 the bound unit's chaos rate, $20 the lowered search
// needle (empty = no search).
const playsServedSQL = `SELECT p.* FROM currency_exchange_plays p
WHERE p.league = $1 AND p.horizon = $2 AND ($3 = 'all' OR p.mode = $3)`

// playsRulesSQL is applyRules: an item rule beats its category's rule, an
// empty category takes no category rule, one hide side drops the play, and any only
// rule makes the play need an only side.
const playsRulesSQL = `(NOT EXISTS (
		SELECT 1 FROM unnest(s.side_items, s.side_categories) AS side(id, category)
		WHERE (side.id = ANY($5) AND NOT side.id = ANY($4))
		   OR (NOT side.id = ANY($4) AND NOT side.id = ANY($5)
		       AND side.category <> '' AND side.category = ANY($7) AND NOT side.category = ANY($6)))
	AND (NOT $8 OR EXISTS (
		SELECT 1 FROM unnest(s.side_items, s.side_categories) AS side(id, category)
		WHERE side.id = ANY($4)
		   OR (NOT side.id = ANY($5) AND side.category <> '' AND side.category = ANY($6)))))`

// playsGatesSQL is applyGates: a gate <= 0 is off, a play is dropped only by a
// comparison that is true (a NULL number is kept), and the divine floor judges
// divine-quoted entries only, while the set has a rate. Every number is cast:
// an untyped "$n > 0" is inferred as an integer.
const playsGatesSQL = `(NOT COALESCE(
	   ($9::float8 > 0 AND s.investment < $9::float8)
	OR ($10::float8 > 0 AND s.first_quote = '` + DivineID + `' AND $16::float8 > 0 AND s.investment / $16::float8 < $10::float8)
	OR ($11::float8 > 0 AND s.roi < $11::float8)
	OR ($12::float8 > 0 AND s.turnover < $12::float8)
	OR ($13::float8 > 0 AND s.tick > $13::float8 / 100)
	OR ($14::float8 > 0 AND s.roi_pct < $14::float8 * s.tick)
	OR ($15::float8 > 0 AND s.roi_pct < $15::float8 / 100), FALSE))`

// playsMatchSQL is applyNumericFilters on the run investment, then
// matchesSearch over the sides' display names.
const playsMatchSQL = `(NOT COALESCE(
	   ($17::float8 IS NOT NULL AND s.run_investment < $17::float8 * $19::float8)
	OR ($18::float8 IS NOT NULL AND s.run_investment > $18::float8 * $19::float8), FALSE)
	AND ($20 = '' OR EXISTS (
		SELECT 1 FROM unnest(s.side_names) AS name WHERE strpos(lower(name), $20) > 0)))`

const playsStagedSQL = `WITH served AS (` + playsServedSQL + `),
staged AS (SELECT s.*, ` + playsRulesSQL + ` AS rules_ok, ` + playsGatesSQL + ` AS gates_ok, ` + playsMatchSQL + ` AS match_ok FROM served s)`

const selectPlayCountsSQL = playsStagedSQL + `
SELECT count(*),
	count(*) FILTER (WHERE rules_ok),
	count(*) FILTER (WHERE rules_ok AND gates_ok),
	count(*) FILTER (WHERE rules_ok AND gates_ok AND match_ok),
	COALESCE(bool_or(leg_count > 2) FILTER (WHERE rules_ok AND gates_ok AND match_ok), FALSE)
FROM staged`

const selectItemUniverseSQL = `SELECT side.id, count(*)
FROM (` + playsServedSQL + `) s, unnest(s.side_items) AS side(id)
GROUP BY side.id ORDER BY side.id`

// playsOrderSQL whitelists the sort keys: the money keys descend, the hours
// ascend, a missing key sorts last, ties keep the served order.
var playsOrderSQL = map[PlaysSort]string{
	SortExpected: `sort_expected DESC NULLS LAST, served_rank`,
	SortRoi:      `sort_roi DESC NULLS LAST, served_rank`,
	SortFastest:  `sort_fastest_hours ASC NULLS LAST, served_rank`,
}

// PlaysPage serves one page of the scope's stored plays for q, with the stage
// counts, the convert flag and the item universe, read in one repeatable-read
// transaction so they describe one recompute. A horizon with no stored set is
// cold: Warm false and nothing else read.
func (r *Repository) PlaysPage(ctx context.Context, scope league.Scope, q PlaysPageQuery) (PlaysPage, error) {
	if err := scope.Validate(); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: %w", err)
	}
	if err := q.Validate(); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: %w", err)
	}
	order, ok := playsOrderSQL[q.Sort]
	if !ok {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: sort %q", q.Sort)
	}
	mode := q.Mode
	if mode == "" {
		mode = "all"
	}

	tx, err := r.pool.BeginTx(ctx, pgx.TxOptions{IsoLevel: pgx.RepeatableRead, AccessMode: pgx.ReadOnly})
	if err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: begin tx: %w", err)
	}
	defer tx.Rollback(ctx)

	page := PlaysPage{League: scope.ID(), Horizon: q.Horizon, Plays: []Play{}, ItemUniverse: []UniverseItem{}}
	var last *time.Time
	err = tx.QueryRow(ctx, selectPlaySetSQL, scope.ID(), string(q.Horizon)).
		Scan(&page.Hours, &page.DivineChaosRate, &page.From, &page.To, &last)
	if errors.Is(err, pgx.ErrNoRows) {
		return page, nil
	}
	if err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: header: %w", err)
	}
	page.Warm = true
	page.From, page.To = page.From.UTC(), page.To.UTC()
	if last != nil {
		page.LastUpdated = last.UTC()
	}

	boundRate := 1.0
	if q.Unit == UnitDivine && page.DivineChaosRate > 0 {
		boundRate = page.DivineChaosRate
	}
	hasOnly := len(q.OnlyItems) > 0
	for _, c := range q.OnlyCategories {
		hasOnly = hasOnly || c != ""
	}
	args := []any{
		scope.ID(), string(q.Horizon), mode,
		nonNil(q.OnlyItems), nonNil(q.HideItems), nonNil(q.OnlyCategories), nonNil(q.HideCategories), hasOnly,
		q.Gates.MinItemPrice, q.Gates.MinItemPriceDiv, q.Gates.MinRoiChaos, q.Gates.MinTurnover,
		q.Gates.MaxTickPct, q.Gates.MinEdgeTickRatio, q.Gates.MinRoiPct,
		page.DivineChaosRate, q.InvestMin, q.InvestMax, boundRate,
		strings.ToLower(strings.TrimSpace(q.Search)),
	}

	if err := tx.QueryRow(ctx, selectPlayCountsSQL, args...).
		Scan(&page.Total, &page.AfterRules, &page.AfterGates, &page.Matched, &page.ShowConvert); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: counts: %w", err)
	}

	universe, err := tx.Query(ctx, selectItemUniverseSQL, scope.ID(), string(q.Horizon), mode)
	if err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: item universe: %w", err)
	}
	for universe.Next() {
		var item UniverseItem
		if err := universe.Scan(&item.ID, &item.PlayCount); err != nil {
			universe.Close()
			return PlaysPage{}, fmt.Errorf("exchange repo: plays page: scan item universe: %w", err)
		}
		page.ItemUniverse = append(page.ItemUniverse, item)
	}
	universe.Close()
	if err := universe.Err(); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: item universe: %w", err)
	}

	// A search bypasses the page size; so does pageSize=all.
	var limit *int
	offset := 0
	if q.Search == "" && q.PageSize != PageSizeAll {
		size := q.PageSize
		limit = &size
		offset = (q.Page - 1) * q.PageSize
	}
	pageSQL := playsStagedSQL + `
SELECT play FROM staged WHERE rules_ok AND gates_ok AND match_ok
ORDER BY ` + order + ` LIMIT $21 OFFSET $22`
	plays, err := tx.Query(ctx, pageSQL, append(args, limit, offset)...)
	if err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: page: %w", err)
	}
	defer plays.Close()
	for plays.Next() {
		var blob []byte
		if err := plays.Scan(&blob); err != nil {
			return PlaysPage{}, fmt.Errorf("exchange repo: plays page: scan play: %w", err)
		}
		var play Play
		if err := json.Unmarshal(blob, &play); err != nil {
			return PlaysPage{}, fmt.Errorf("exchange repo: plays page: decode play: %w", err)
		}
		page.Plays = append(page.Plays, play)
	}
	if err := plays.Err(); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: page: %w", err)
	}
	plays.Close()

	if err := tx.Commit(ctx); err != nil {
		return PlaysPage{}, fmt.Errorf("exchange repo: plays page: commit: %w", err)
	}
	return page, nil
}

// ScopedPlaysPager serves the paged endpoint from the repository for one league
// scope (the process-active league).
type ScopedPlaysPager struct {
	Repo  *Repository
	Scope league.Scope
}

// PlaysPage implements PlaysPager.
func (p ScopedPlaysPager) PlaysPage(ctx context.Context, q PlaysPageQuery) (PlaysPage, error) {
	return p.Repo.PlaysPage(ctx, p.Scope, q)
}

var _ PlaysPager = ScopedPlaysPager{}
