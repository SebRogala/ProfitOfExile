package temple

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// THE READ SIDE OF item_snapshots
//
// POE-254 created the table and its writer (internal/collector) and deliberately
// shipped no read path; this is it. Both halves take league.Scope and put
// scope.ID() in every predicate, per ADR-009, whose POE-254 amendment names this
// package alongside the collector as the table's readers.
//
// Every query is bounded by time on top of the (league, category, time DESC)
// index the migration created — an unbounded read of a hypertable is the defect
// POE-150/157 removed a whole endpoint family for.

// newestLookbackHours bounds the search for each category's newest stored
// snapshot.
//
// A week, matching internal/lab's sparkline lookback, and it is a staleness
// ceiling rather than a freshness requirement: the payload reports AsOf and the
// client decides what is too old (POE-258), so the read must still answer after
// a multi-day collector outage instead of blanking the endpoint. Past a week
// there is nothing worth serving and the scan should not pay for it.
const newestLookbackHours = 168

// Repository reads item_snapshots for the temple market.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a temple repository over pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// NewestLines returns every line of the newest stored snapshot of each of the
// seven categories, within newestLookbackHours of now.
//
// One query, not one per category, and the plan's cost is shaped by the SEVEN
// categories rather than by how much sits in the lookback. Unnesting the
// category list and driving a LATERAL MAX(time) off each one plans as seven
// index seeks — a Limit over the (league, category, time DESC) prefix, one row
// each, no heap — and a memoized index scan per category that reads only the
// rows at that category's timestamp.
//
// A GROUP BY CTE is the form that does NOT hold. It plans as a HashAggregate
// over every row in the lookback hash-joined against a second full scan of the
// same rows, so both halves grow with the lookback; measured on the local
// Allflame snapshot it read four times the buffers this form does.
//
// Categories are independent — the collector polls each on its own cadence and
// one 404ing for a league leaves the others collecting — so a category with no
// recent snapshot contributes a NULL max, joins to nothing, and is simply absent
// from the result, which is what Market.CategoriesSeen reports.
//
// No ORDER BY: nothing downstream reads the row order. Build sorts the rooms by
// name, takes medians off sorted values, walks the item names in their own
// order, and breaks a pick tie on ninja_id rather than on position — so an
// ordering here would be a cost with no reader.
func (r *Repository) NewestLines(ctx context.Context, scope league.Scope) ([]Line, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("temple repo: newest lines: %w", err)
	}

	const query = `
		SELECT s.category, s.time, s.ninja_id, s.name, s.chaos, s.divine, s.listings
		FROM unnest($2::text[]) AS c(category)
		CROSS JOIN LATERAL (
		    SELECT MAX(time) AS at
		    FROM item_snapshots
		    WHERE league = $1 AND category = c.category AND time >= $3
		) n
		JOIN item_snapshots s
		  ON s.league = $1 AND s.category = c.category AND s.time = n.at`

	since := time.Now().Add(-newestLookbackHours * time.Hour)
	rows, err := r.pool.Query(ctx, query, scope.ID(), Categories(), since)
	if err != nil {
		return nil, fmt.Errorf("temple repo: newest lines: %w", err)
	}
	return collectLines(rows)
}

// WindowPrices returns the prints of each named line over the trailing
// WindowHours, keyed by category and name.
//
// The caller passes the thin lines ThinLines picked, so this is one bounded
// query over a handful of names rather than one query per item. Both the
// category list and the name list are predicates so the scan stays on the
// index's leading columns; a name that is served under two categories comes back
// under both keys and the aggregation reads only the one it priced from.
func (r *Repository) WindowPrices(ctx context.Context, scope league.Scope, lines []Line) (map[string][]WindowPrice, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("temple repo: window prices: %w", err)
	}
	if len(lines) == 0 {
		return map[string][]WindowPrice{}, nil
	}

	names := make([]string, 0, len(lines))
	categories := make([]string, 0, len(lines))
	seenName := make(map[string]struct{}, len(lines))
	seenCategory := make(map[string]struct{}, len(lines))
	for _, line := range lines {
		if _, dup := seenName[line.Name]; !dup {
			seenName[line.Name] = struct{}{}
			names = append(names, line.Name)
		}
		if _, dup := seenCategory[line.Category]; !dup {
			seenCategory[line.Category] = struct{}{}
			categories = append(categories, line.Category)
		}
	}

	const query = `
		SELECT category, name, chaos, divine
		FROM item_snapshots
		WHERE league = $1 AND category = ANY($2) AND time >= $3 AND name = ANY($4)`

	since := time.Now().Add(-WindowHours * time.Hour)
	rows, err := r.pool.Query(ctx, query, scope.ID(), categories, since, names)
	if err != nil {
		return nil, fmt.Errorf("temple repo: window prices: %w", err)
	}
	defer rows.Close()

	window := make(map[string][]WindowPrice, len(names))
	for rows.Next() {
		var category, name string
		var price WindowPrice
		if err := rows.Scan(&category, &name, &price.Chaos, &price.Divine); err != nil {
			return nil, fmt.Errorf("temple repo: window prices: scan: %w", err)
		}
		key := lineKey(Line{Category: category, Name: name})
		window[key] = append(window[key], price)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("temple repo: window prices: %w", err)
	}
	return window, nil
}

// RoomWindowMedians returns each room line's median chaos over the trailing
// WindowHours, keyed by category and name (lineKey), for Build's above-floor gate.
//
// One query, aggregated in SQL: every room line is gated, so the alternative is
// fetching ~86 lines times every print in the window to take a median in Go.
// The predicate is the same index-friendly shape WindowPrices uses — league,
// category and a time lower bound on the (league, category, time DESC) prefix —
// with the category pinned to RoomCategory.
//
// percentile_cont interpolates between the two middle prints on an even count,
// which differs from medianOf's upper-middle convention. The gate compares this
// number against a multiple of the floor and does not pair it with any other
// axis, so an interpolated value is a fair reading of the day.
//
// A room line with no print inside the window is absent from the result, and
// Build then gates it on its newest print.
func (r *Repository) RoomWindowMedians(ctx context.Context, scope league.Scope) (map[string]float64, error) {
	if err := scope.Validate(); err != nil {
		return nil, fmt.Errorf("temple repo: room window medians: %w", err)
	}

	const query = `
		SELECT name, percentile_cont(0.5) WITHIN GROUP (ORDER BY chaos)
		FROM item_snapshots
		WHERE league = $1 AND category = $2 AND time >= $3
		GROUP BY name`

	since := time.Now().Add(-WindowHours * time.Hour)
	rows, err := r.pool.Query(ctx, query, scope.ID(), RoomCategory, since)
	if err != nil {
		return nil, fmt.Errorf("temple repo: room window medians: %w", err)
	}
	defer rows.Close()

	medians := make(map[string]float64)
	for rows.Next() {
		var name string
		var median float64
		if err := rows.Scan(&name, &median); err != nil {
			return nil, fmt.Errorf("temple repo: room window medians: scan: %w", err)
		}
		medians[lineKey(Line{Category: RoomCategory, Name: name})] = median
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("temple repo: room window medians: %w", err)
	}
	return medians, nil
}

func collectLines(rows pgx.Rows) ([]Line, error) {
	defer rows.Close()

	var lines []Line
	for rows.Next() {
		var line Line
		if err := rows.Scan(&line.Category, &line.Time, &line.NinjaID, &line.Name, &line.Chaos, &line.Divine, &line.Listings); err != nil {
			return nil, fmt.Errorf("temple repo: scan line: %w", err)
		}
		lines = append(lines, line)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("temple repo: read lines: %w", err)
	}
	return lines, nil
}
