//go:build integration

package exchange

import (
	"context"
	"encoding/json"
	"errors"
	"net/url"
	"os"
	"reflect"
	"sync/atomic"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// integrationScope pins every write and read to the seeded Mirage league. The
// league-control migration registers Mirage, which satisfies the FK on both
// currency-exchange tables.
var integrationScope = league.Historical("Mirage")

// baseHour is the feed hour the tests build their windows from. Whole hours only,
// matching what the runner stores, so TIMESTAMPTZ round-trips exactly.
var baseHour = time.Date(2026, 1, 5, 0, 0, 0, 0, time.UTC)

func integrationPool(t *testing.T) *pgxpool.Pool {
	t.Helper()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, dbURL)
	if err != nil {
		t.Fatalf("connect to database: %v", err)
	}
	t.Cleanup(func() { pool.Close() })

	for _, table := range []string{"currency_exchange_markets", "currency_exchange_cursor"} {
		var exists bool
		if err := pool.QueryRow(ctx,
			"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)", table).
			Scan(&exists); err != nil {
			t.Fatalf("check %s table: %v", table, err)
		}
		if !exists {
			t.Skipf("%s table not found, skipping (currency-exchange migration not applied)", table)
		}
	}

	return pool
}

// newTestRepository returns a repository and registers the cleanup that removes
// the test league's rows from both tables. It is registered before any assertion
// runs, so a failing test still cleans up after itself.
func newTestRepository(t *testing.T, pool *pgxpool.Pool) *Repository {
	t.Helper()
	t.Cleanup(func() {
		ctx := context.Background()
		for _, stmt := range []string{
			"DELETE FROM currency_exchange_markets WHERE league = $1",
			"DELETE FROM currency_exchange_cursor WHERE league = $1",
		} {
			if _, err := pool.Exec(ctx, stmt, integrationScope.ID()); err != nil {
				t.Logf("cleanup warning: %q: %v", stmt, err)
			}
		}
	})
	return NewRepository(pool)
}

// registerSecondLeague adds a second league to the leagues registry — the FK
// target of both currency-exchange tables — and returns its scope. Cleanup runs
// in FK order: the league's rows in both tables first, then the league row.
func registerSecondLeague(t *testing.T, pool *pgxpool.Pool, id string) league.Scope {
	t.Helper()

	t.Cleanup(func() {
		ctx := context.Background()
		for _, stmt := range []string{
			"DELETE FROM currency_exchange_markets WHERE league = $1",
			"DELETE FROM currency_exchange_cursor WHERE league = $1",
			"DELETE FROM leagues WHERE id = $1",
		} {
			if _, err := pool.Exec(ctx, stmt, id); err != nil {
				t.Logf("cleanup warning: %q: %v", stmt, err)
			}
		}
	})

	if _, err := pool.Exec(context.Background(), `
		INSERT INTO leagues (id, display_name, collection_state, prepared_at, activated_at)
		VALUES ($1, $1, 'collecting', now(), now())`, id); err != nil {
		t.Fatalf("register league %q: %v", id, err)
	}
	return league.Historical(id)
}

// pricedRow is a chaos/divine market at 196 chaos per divine, priced on both the
// lowest and the highest ratio pair.
func pricedRow() Row {
	return Row{
		League:        integrationScope.ID(),
		MarketID:      chaosDivineMarket,
		ItemA:         chaosID,
		ItemB:         divineID,
		VolumeA:       1000,
		VolumeB:       5,
		LowestStockA:  40,
		LowestStockB:  2,
		HighestStockA: 50,
		HighestStockB: 3,
		LowestRatioA:  196,
		LowestRatioB:  1,
		HighestRatioA: 210,
		HighestRatioB: 1,
		PriceValid:    true,
	}
}

// unpricedRow is a market whose ratio quantities are all zero: kept, flagged, and
// still carrying usable stock and volume.
func unpricedRow() Row {
	return Row{
		League:        integrationScope.ID(),
		MarketID:      zeroRatioMarket,
		ItemA:         hellID,
		ItemB:         divineID,
		VolumeA:       7,
		VolumeB:       11,
		LowestStockA:  13,
		LowestStockB:  17,
		HighestStockA: 19,
		HighestStockB: 23,
		PriceValid:    false,
		InvalidReason: reasonZeroRatio,
	}
}

// storedByMarketID returns the single stored row for marketID.
func storedByMarketID(t *testing.T, rows []StoredRow, marketID string) StoredRow {
	t.Helper()
	for _, row := range rows {
		if row.MarketID == marketID {
			return row
		}
	}
	t.Fatalf("no stored row for market %q (got %d rows)", marketID, len(rows))
	return StoredRow{}
}

// loadHour reads back exactly one feed hour.
func loadHour(t *testing.T, repo *Repository, hour time.Time) []StoredRow {
	t.Helper()
	rows, err := repo.LoadRows(context.Background(), integrationScope, hour, hour.Add(time.Hour))
	if err != nil {
		t.Fatalf("LoadRows: %v", err)
	}
	return rows
}

func TestCursor_leagueWithNoCursorRow_reportsNotFound(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)

	nextHour, found, err := repo.Cursor(context.Background(), integrationScope)

	if err != nil {
		t.Fatalf("Cursor: %v", err)
	}
	if found {
		t.Errorf("found = true, want false — a league with no cursor row must bootstrap")
	}
	if nextHour != 0 {
		t.Errorf("nextHour = %d, want 0", nextHour)
	}
}

func TestInsertHour_storesEveryRowOfTheHour(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	hour := baseHour

	inserted, err := repo.InsertHour(context.Background(), integrationScope, hour,
		[]Row{pricedRow(), unpricedRow()}, hour.Add(time.Hour).Unix())
	if err != nil {
		t.Fatalf("InsertHour: %v", err)
	}
	if inserted != 2 {
		t.Errorf("inserted = %d, want 2", inserted)
	}

	stored := loadHour(t, repo, hour)
	if len(stored) != 2 {
		t.Fatalf("loaded %d rows, want 2", len(stored))
	}
}

func TestInsertHour_roundTripsEveryQuantityOfAPricedRow(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	hour := baseHour
	want := pricedRow()

	if _, err := repo.InsertHour(context.Background(), integrationScope, hour,
		[]Row{want}, hour.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	got := storedByMarketID(t, loadHour(t, repo, hour), chaosDivineMarket)

	if !got.Hour.Equal(hour) {
		t.Errorf("Hour = %s, want %s", got.Hour, hour)
	}
	if got.League != integrationScope.ID() {
		t.Errorf("League = %q, want %q", got.League, integrationScope.ID())
	}
	if got.ItemA != want.ItemA || got.ItemB != want.ItemB {
		t.Errorf("items = %q/%q, want %q/%q", got.ItemA, got.ItemB, want.ItemA, want.ItemB)
	}
	quantities := []struct {
		name      string
		got, want int64
	}{
		{"VolumeA", got.VolumeA, want.VolumeA},
		{"VolumeB", got.VolumeB, want.VolumeB},
		{"LowestStockA", got.LowestStockA, want.LowestStockA},
		{"LowestStockB", got.LowestStockB, want.LowestStockB},
		{"HighestStockA", got.HighestStockA, want.HighestStockA},
		{"HighestStockB", got.HighestStockB, want.HighestStockB},
		{"LowestRatioA", got.LowestRatioA, want.LowestRatioA},
		{"LowestRatioB", got.LowestRatioB, want.LowestRatioB},
		{"HighestRatioA", got.HighestRatioA, want.HighestRatioA},
		{"HighestRatioB", got.HighestRatioB, want.HighestRatioB},
	}
	for _, q := range quantities {
		if q.got != q.want {
			t.Errorf("%s = %d, want %d", q.name, q.got, q.want)
		}
	}
	if !got.PriceValid {
		t.Error("PriceValid = false, want true")
	}
	if got.InvalidReason != "" {
		t.Errorf("InvalidReason = %q, want empty", got.InvalidReason)
	}
}

func TestLoadRows_derivesBothPricesFromTheStoredRatios(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	hour := baseHour

	if _, err := repo.InsertHour(context.Background(), integrationScope, hour,
		[]Row{pricedRow()}, hour.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	got := storedByMarketID(t, loadHour(t, repo, hour), chaosDivineMarket)

	// Prices are not columns: 196:1 and 210:1 are re-derived from the stored
	// ratio quantities.
	if got.LowestPriceBInA != 196 {
		t.Errorf("LowestPriceBInA = %v, want 196", got.LowestPriceBInA)
	}
	if got.HighestPriceBInA != 210 {
		t.Errorf("HighestPriceBInA = %v, want 210", got.HighestPriceBInA)
	}
}

func TestLoadRows_returnsTheFeedHourInUTC(t *testing.T) {
	// pgx scans a TIMESTAMPTZ into time.Local, so without an explicit conversion
	// the same stored hour renders differently per process time zone — and the
	// hour is an identity that ends up in cache keys and event payloads.
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	hour := baseHour

	if _, err := repo.InsertHour(context.Background(), integrationScope, hour,
		[]Row{pricedRow()}, hour.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	stored := loadHour(t, repo, hour)
	if len(stored) != 1 {
		t.Fatalf("loaded %d rows, want 1", len(stored))
	}
	if loc := stored[0].Hour.Location(); loc != time.UTC {
		t.Errorf("Hour location = %v, want UTC", loc)
	}
}

func TestLoadRows_zeroRatioRow_keepsItsFlagAndReasonWithNoPrice(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	hour := baseHour
	want := unpricedRow()

	if _, err := repo.InsertHour(context.Background(), integrationScope, hour,
		[]Row{want}, hour.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	got := storedByMarketID(t, loadHour(t, repo, hour), zeroRatioMarket)

	if got.PriceValid {
		t.Error("PriceValid = true, want false")
	}
	if got.InvalidReason != reasonZeroRatio {
		t.Errorf("InvalidReason = %q, want %q", got.InvalidReason, reasonZeroRatio)
	}
	if got.LowestPriceBInA != 0 || got.HighestPriceBInA != 0 {
		t.Errorf("prices = %v/%v, want 0/0 — zero ratios cannot produce a price",
			got.LowestPriceBInA, got.HighestPriceBInA)
	}
	// The unusable ratios must not cost the row its usable quantities.
	if got.VolumeA != want.VolumeA || got.HighestStockB != want.HighestStockB {
		t.Errorf("VolumeA/HighestStockB = %d/%d, want %d/%d",
			got.VolumeA, got.HighestStockB, want.VolumeA, want.HighestStockB)
	}
}

func TestInsertHour_advancesTheCursorToTheNextHour(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()
	hour := baseHour
	nextHour := hour.Add(time.Hour).Unix()

	if _, err := repo.InsertHour(ctx, integrationScope, hour, []Row{pricedRow()}, nextHour); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	got, found, err := repo.Cursor(ctx, integrationScope)
	if err != nil {
		t.Fatalf("Cursor: %v", err)
	}
	if !found {
		t.Fatal("found = false, want true")
	}
	if got != nextHour {
		t.Errorf("nextHour = %d, want %d", got, nextHour)
	}
}

func TestInsertHour_hourWithNoRows_stillAdvancesTheCursor(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()
	hour := baseHour
	nextHour := hour.Add(time.Hour).Unix()

	inserted, err := repo.InsertHour(ctx, integrationScope, hour, nil, nextHour)
	if err != nil {
		t.Fatalf("InsertHour: %v", err)
	}
	if inserted != 0 {
		t.Errorf("inserted = %d, want 0", inserted)
	}

	got, found, err := repo.Cursor(ctx, integrationScope)
	if err != nil {
		t.Fatalf("Cursor: %v", err)
	}
	if !found || got != nextHour {
		t.Errorf("cursor = (%d, %t), want (%d, true) — an hour with no rows for this league must not be re-fetched",
			got, found, nextHour)
	}
}

func TestInsertHour_replayedHour_insertsNothingAndKeepsOneRowPerMarket(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()
	hour := baseHour
	nextHour := hour.Add(time.Hour).Unix()
	rows := []Row{pricedRow(), unpricedRow()}

	if _, err := repo.InsertHour(ctx, integrationScope, hour, rows, nextHour); err != nil {
		t.Fatalf("first InsertHour: %v", err)
	}

	inserted, err := repo.InsertHour(ctx, integrationScope, hour, rows, nextHour)
	if err != nil {
		t.Fatalf("second InsertHour: %v", err)
	}
	if inserted != 0 {
		t.Errorf("inserted = %d on replay, want 0", inserted)
	}

	stored := loadHour(t, repo, hour)
	if len(stored) != 2 {
		t.Fatalf("loaded %d rows after the replay, want 2", len(stored))
	}
	got, found, err := repo.Cursor(ctx, integrationScope)
	if err != nil {
		t.Fatalf("Cursor: %v", err)
	}
	if !found || got != nextHour {
		t.Errorf("cursor = (%d, %t), want (%d, true)", got, found, nextHour)
	}
}

func TestLoadRows_excludesHoursOutsideTheHalfOpenWindow(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()

	earlier := baseHour.Add(-time.Hour)
	inside := baseHour
	atUpperBound := baseHour.Add(time.Hour)
	for _, hour := range []time.Time{earlier, inside, atUpperBound} {
		if _, err := repo.InsertHour(ctx, integrationScope, hour,
			[]Row{pricedRow()}, hour.Add(time.Hour).Unix()); err != nil {
			t.Fatalf("InsertHour %s: %v", hour, err)
		}
	}

	stored, err := repo.LoadRows(ctx, integrationScope, inside, atUpperBound)
	if err != nil {
		t.Fatalf("LoadRows: %v", err)
	}

	if len(stored) != 1 {
		t.Fatalf("loaded %d rows, want 1 — [from, to) includes from and excludes to", len(stored))
	}
	if !stored[0].Hour.Equal(inside) {
		t.Errorf("Hour = %s, want %s", stored[0].Hour, inside)
	}
}

func TestLoadRows_ordersByHourThenMarketID(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()

	first, second := baseHour, baseHour.Add(time.Hour)
	for _, hour := range []time.Time{second, first} { // inserted out of order on purpose
		if _, err := repo.InsertHour(ctx, integrationScope, hour,
			[]Row{pricedRow(), unpricedRow()}, hour.Add(time.Hour).Unix()); err != nil {
			t.Fatalf("InsertHour %s: %v", hour, err)
		}
	}

	stored, err := repo.LoadRows(ctx, integrationScope, first, second.Add(time.Hour))
	if err != nil {
		t.Fatalf("LoadRows: %v", err)
	}
	if len(stored) != 4 {
		t.Fatalf("loaded %d rows, want 4", len(stored))
	}

	// zeroRatioMarket sorts before chaosDivineMarket (…CurrencyH… < …CurrencyR…).
	want := []struct {
		hour     time.Time
		marketID string
	}{
		{first, zeroRatioMarket},
		{first, chaosDivineMarket},
		{second, zeroRatioMarket},
		{second, chaosDivineMarket},
	}
	for i, w := range want {
		if !stored[i].Hour.Equal(w.hour) || stored[i].MarketID != w.marketID {
			t.Errorf("row %d = (%s, %s), want (%s, %s)",
				i, stored[i].Hour, stored[i].MarketID, w.hour, w.marketID)
		}
	}
}

func TestLoadRows_hourHeldByTwoLeagues_returnsOnlyTheScopesRows(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	otherScope := registerSecondLeague(t, pool, "ExchangeScopeTest")
	ctx := context.Background()
	hour := baseHour
	nextHour := hour.Add(time.Hour).Unix()

	// Same market, same feed hour, one row per league — which is what the feed
	// actually produces, since one payload carries every league. The league
	// column is therefore the only thing that can separate them, and volume_a is
	// what proves which row came back: LoadRows fills Row.League from the scope
	// argument, so asserting on that field would assert on the input.
	mine := pricedRow()
	theirs := pricedRow()
	theirs.League = otherScope.ID()
	theirs.VolumeA = 4242

	if _, err := repo.InsertHour(ctx, integrationScope, hour, []Row{mine}, nextHour); err != nil {
		t.Fatalf("InsertHour %s: %v", integrationScope.ID(), err)
	}
	if _, err := repo.InsertHour(ctx, otherScope, hour, []Row{theirs}, nextHour); err != nil {
		t.Fatalf("InsertHour %s: %v", otherScope.ID(), err)
	}

	for _, tc := range []struct {
		scope       league.Scope
		wantVolumeA int64
	}{
		{integrationScope, mine.VolumeA},
		{otherScope, theirs.VolumeA},
	} {
		t.Run(tc.scope.ID(), func(t *testing.T) {
			stored, err := repo.LoadRows(ctx, tc.scope, hour, hour.Add(time.Hour))
			if err != nil {
				t.Fatalf("LoadRows: %v", err)
			}
			if len(stored) != 1 {
				t.Fatalf("loaded %d rows, want 1 — the other league's row for the same hour must not come back", len(stored))
			}
			if stored[0].VolumeA != tc.wantVolumeA {
				t.Errorf("VolumeA = %d, want %d — this is the other league's row", stored[0].VolumeA, tc.wantVolumeA)
			}
		})
	}
}

func TestInsertHour_rowFromAnotherLeague_isRejectedAndNothingIsWritten(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()
	hour := baseHour

	foreign := pricedRow()
	foreign.League = "Hardcore Mirage"

	inserted, err := repo.InsertHour(ctx, integrationScope, hour,
		[]Row{unpricedRow(), foreign}, hour.Add(time.Hour).Unix())

	if err == nil {
		t.Fatal("error = nil, want a rejection — a foreign row would be silently relabelled")
	}
	if inserted != 0 {
		t.Errorf("inserted = %d, want 0", inserted)
	}
	if stored := loadHour(t, repo, hour); len(stored) != 0 {
		t.Errorf("loaded %d rows, want 0 — the well-scoped row must not be written either", len(stored))
	}
	if _, found, cursorErr := repo.Cursor(ctx, integrationScope); cursorErr != nil {
		t.Fatalf("Cursor: %v", cursorErr)
	} else if found {
		t.Error("cursor row exists, want none — a rejected hour must not advance the walk")
	}
}

func TestRepositoryMethods_unscopedScope_areRejectedBeforeTouchingTheDatabase(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()
	unscoped := league.Scope{}

	operations := []struct {
		name string
		call func() error
	}{
		{"Cursor", func() error {
			_, _, err := repo.Cursor(ctx, unscoped)
			return err
		}},
		{"InsertHour", func() error {
			_, err := repo.InsertHour(ctx, unscoped, baseHour, []Row{pricedRow()}, baseHour.Add(time.Hour).Unix())
			return err
		}},
		{"LoadRows", func() error {
			_, err := repo.LoadRows(ctx, unscoped, baseHour, baseHour.Add(time.Hour))
			return err
		}},
		{"NewestHour", func() error {
			_, _, err := repo.NewestHour(ctx, unscoped)
			return err
		}},
	}

	for _, op := range operations {
		t.Run(op.name, func(t *testing.T) {
			if err := op.call(); !errors.Is(err, league.ErrUnscoped) {
				t.Fatalf("error = %v, want it to wrap %v", err, league.ErrUnscoped)
			}
		})
	}

	// Nothing reached storage: no rows and no cursor row for the scoped league.
	if stored := loadHour(t, repo, baseHour); len(stored) != 0 {
		t.Errorf("loaded %d rows, want 0", len(stored))
	}
	if _, found, err := repo.Cursor(ctx, integrationScope); err != nil {
		t.Fatalf("Cursor: %v", err)
	} else if found {
		t.Error("cursor row exists, want none")
	}
}

func TestNewestHour_leagueWithNoRows_reportsNotFound(t *testing.T) {
	// A fresh league, or one whose hours were wiped at rollover. MAX over zero
	// rows is one row holding NULL rather than pgx.ErrNoRows, so this is also
	// where a non-nullable scan target would fail instead of reporting empty.
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)

	newest, found, err := repo.NewestHour(context.Background(), integrationScope)

	if err != nil {
		t.Fatalf("NewestHour: %v", err)
	}
	if found {
		t.Errorf("found = true, want false — the league has stored no hour")
	}
	if !newest.IsZero() {
		t.Errorf("newest = %s, want the zero time", newest)
	}
}

func TestNewestHour_twoStoredHours_returnsTheLaterOne(t *testing.T) {
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	ctx := context.Background()

	earlier, later := baseHour, baseHour.Add(time.Hour)
	for _, hour := range []time.Time{later, earlier} { // stored out of order on purpose
		if _, err := repo.InsertHour(ctx, integrationScope, hour,
			[]Row{pricedRow()}, hour.Add(time.Hour).Unix()); err != nil {
			t.Fatalf("InsertHour %s: %v", hour, err)
		}
	}

	newest, found, err := repo.NewestHour(ctx, integrationScope)

	if err != nil {
		t.Fatalf("NewestHour: %v", err)
	}
	if !found {
		t.Fatal("found = false, want true")
	}
	if !newest.Equal(later) {
		t.Errorf("newest = %s, want %s — the recompute anchors its window on the latest stored hour",
			newest, later)
	}
}

func TestNewestHour_returnsTheFeedHourInUTC(t *testing.T) {
	// pgx scans a TIMESTAMPTZ into time.Local; the anchor is rendered into the
	// cache and the update payload, where the feed hour is a UTC identity.
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)

	if _, err := repo.InsertHour(context.Background(), integrationScope, baseHour,
		[]Row{pricedRow()}, baseHour.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour: %v", err)
	}

	newest, _, err := repo.NewestHour(context.Background(), integrationScope)
	if err != nil {
		t.Fatalf("NewestHour: %v", err)
	}

	if loc := newest.Location(); loc != time.UTC {
		t.Errorf("location = %v, want UTC", loc)
	}
}

func TestNewestHour_anotherLeaguesLaterHour_isIgnored(t *testing.T) {
	// One feed payload carries every league, so a neighbouring league routinely
	// holds hours this one does not. Anchoring on its hour would slide the window
	// past this league's own newest rows and serve an empty ranking.
	pool := integrationPool(t)
	repo := newTestRepository(t, pool)
	otherScope := registerSecondLeague(t, pool, "ExchangeNewestHourTest")
	ctx := context.Background()

	mine, theirs := baseHour, baseHour.Add(5*time.Hour)
	if _, err := repo.InsertHour(ctx, integrationScope, mine,
		[]Row{pricedRow()}, mine.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour %s: %v", integrationScope.ID(), err)
	}
	theirRow := pricedRow()
	theirRow.League = otherScope.ID()
	if _, err := repo.InsertHour(ctx, otherScope, theirs,
		[]Row{theirRow}, theirs.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("InsertHour %s: %v", otherScope.ID(), err)
	}

	newest, found, err := repo.NewestHour(ctx, integrationScope)

	if err != nil {
		t.Fatalf("NewestHour: %v", err)
	}
	if !found {
		t.Fatal("found = false, want true")
	}
	if !newest.Equal(mine) {
		t.Errorf("newest = %s, want %s — the other league's later hour must not anchor this window",
			newest, mine)
	}
}

// newPlaysRepository returns a repository and registers the cleanup of the
// scope's stored plays and set headers.
func newPlaysRepository(t *testing.T, pool *pgxpool.Pool, scopes ...league.Scope) *Repository {
	t.Helper()
	requirePlaysTables(t, pool)
	scopes = append(scopes, integrationScope)
	t.Cleanup(func() {
		ctx := context.Background()
		for _, scope := range scopes {
			for _, stmt := range []string{
				"DELETE FROM currency_exchange_plays WHERE league = $1",
				"DELETE FROM currency_exchange_play_sets WHERE league = $1",
			} {
				if _, err := pool.Exec(ctx, stmt, scope.ID()); err != nil {
					t.Logf("cleanup warning: %q: %v", stmt, err)
				}
			}
		}
	})
	return NewRepository(pool)
}

func requirePlaysTables(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	var exists bool
	if err := pool.QueryRow(context.Background(),
		"SELECT to_regclass('currency_exchange_plays') IS NOT NULL").Scan(&exists); err != nil {
		t.Fatalf("check currency_exchange_plays: %v", err)
	}
	if !exists {
		t.Fatal("currency_exchange_plays table not found (migration not applied)")
	}
}

// keyedPlays is a horizon Result holding one direct play per key, in order.
func keyedPlays(horizon Horizon, keys ...string) Result {
	res := Result{
		League: integrationScope.ID(), Horizon: string(horizon),
		From: baseHour, To: baseHour.Add(6 * time.Hour), Hours: 6, DivineChaosRate: 196,
		Plays: []Play{},
	}
	for i, key := range keys {
		res.Plays = append(res.Plays, Play{
			Key: key, Mode: ModeDirect,
			Legs:       []Leg{{Action: "buy", Item: cardID, Quote: ChaosID, PriceItemQty: 1, PriceQuoteQty: 10}},
			Investment: 10, Roi: float64(len(keys) - i), ExpectedRoi: 1, SimEntries: 1, Depth: 1,
		})
	}
	return res
}

func storedKeys(t *testing.T, pool *pgxpool.Pool, scope league.Scope, horizon Horizon) []string {
	t.Helper()
	rows, err := pool.Query(context.Background(),
		`SELECT play_key FROM currency_exchange_plays WHERE league = $1 AND horizon = $2 ORDER BY served_rank`,
		scope.ID(), string(horizon))
	if err != nil {
		t.Fatalf("read stored keys: %v", err)
	}
	defer rows.Close()
	keys := []string{}
	for rows.Next() {
		var k string
		if err := rows.Scan(&k); err != nil {
			t.Fatal(err)
		}
		keys = append(keys, k)
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	return keys
}

type storedHeader struct {
	Hours int
	Rate  float64
	From  time.Time
}

func storedHeaders(t *testing.T, pool *pgxpool.Pool, scope league.Scope) map[string]storedHeader {
	t.Helper()
	rows, err := pool.Query(context.Background(),
		`SELECT horizon, hours, divine_chaos_rate, window_from FROM currency_exchange_play_sets WHERE league = $1`, scope.ID())
	if err != nil {
		t.Fatalf("read headers: %v", err)
	}
	defer rows.Close()
	out := map[string]storedHeader{}
	for rows.Next() {
		var h string
		var v storedHeader
		if err := rows.Scan(&h, &v.Hours, &v.Rate, &v.From); err != nil {
			t.Fatal(err)
		}
		v.From = v.From.UTC()
		out[h] = v
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	return out
}

func TestReplacePlays_StoresEveryPlayInServedOrderPerHorizon(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)

	recent := keyedPlays(HorizonRecent, "c", "a", "b")
	day := keyedPlays(HorizonDay, "z", "y")
	if err := repo.ReplacePlays(context.Background(), integrationScope, []Result{recent, day}); err != nil {
		t.Fatalf("ReplacePlays: %v", err)
	}

	if got := storedKeys(t, pool, integrationScope, HorizonRecent); !reflect.DeepEqual(got, []string{"c", "a", "b"}) {
		t.Errorf("recent keys = %q, want [c a b]", got)
	}
	if got := storedKeys(t, pool, integrationScope, HorizonDay); !reflect.DeepEqual(got, []string{"z", "y"}) {
		t.Errorf("day keys = %q, want [z y]", got)
	}
}

func TestReplacePlays_SecondCallReplacesTheFirstAnswer(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	ctx := context.Background()
	if err := repo.ReplacePlays(ctx, integrationScope, []Result{keyedPlays(HorizonRecent, "old1", "old2"), keyedPlays(HorizonDay, "oldday")}); err != nil {
		t.Fatal(err)
	}

	if err := repo.ReplacePlays(ctx, integrationScope, []Result{keyedPlays(HorizonRecent, "new1"), keyedPlays(HorizonDay)}); err != nil {
		t.Fatalf("ReplacePlays: %v", err)
	}

	if got := storedKeys(t, pool, integrationScope, HorizonRecent); !reflect.DeepEqual(got, []string{"new1"}) {
		t.Errorf("recent keys = %q, want [new1]", got)
	}
	if got := storedKeys(t, pool, integrationScope, HorizonDay); len(got) != 0 {
		t.Errorf("day keys = %q, want none", got)
	}
}

func TestReplacePlays_ReplacesOnlyTheScopedLeague(t *testing.T) {
	pool := integrationPool(t)
	// Registered first so its cleanup runs after the plays cleanup (FK order).
	other := registerSecondLeague(t, pool, "PlaysOtherLeague")
	repo := newPlaysRepository(t, pool, other)
	ctx := context.Background()
	if err := repo.ReplacePlays(ctx, other, []Result{keyedPlays(HorizonRecent, "kept")}); err != nil {
		t.Fatal(err)
	}

	if err := repo.ReplacePlays(ctx, integrationScope, []Result{keyedPlays(HorizonRecent, "mine")}); err != nil {
		t.Fatalf("ReplacePlays: %v", err)
	}

	if got := storedKeys(t, pool, other, HorizonRecent); !reflect.DeepEqual(got, []string{"kept"}) {
		t.Errorf("other league keys = %q, want [kept]", got)
	}
	if _, ok := storedHeaders(t, pool, other)[string(HorizonRecent)]; !ok {
		t.Error("other league's recent header was removed")
	}
}

func TestReplacePlays_FailureAfterTheDeleteKeepsBothPriorHorizonSets(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	ctx := context.Background()
	if err := repo.ReplacePlays(ctx, integrationScope, []Result{keyedPlays(HorizonRecent, "r1", "r2"), keyedPlays(HorizonDay, "d1")}); err != nil {
		t.Fatal(err)
	}
	wantHeaders := storedHeaders(t, pool, integrationScope)
	if len(wantHeaders) != 2 {
		t.Fatalf("prior headers = %v, want both horizons", wantHeaders)
	}

	// The empty key violates the plays CHECK during the COPY, after the delete.
	broken := keyedPlays(HorizonRecent, "n1", "")
	broken.Hours = 99
	err := repo.ReplacePlays(ctx, integrationScope, []Result{broken, keyedPlays(HorizonDay, "n2")})

	if err == nil {
		t.Fatal("ReplacePlays succeeded, want the CHECK violation")
	}
	if got := storedKeys(t, pool, integrationScope, HorizonRecent); !reflect.DeepEqual(got, []string{"r1", "r2"}) {
		t.Errorf("recent keys = %q, want the prior [r1 r2]", got)
	}
	if got := storedKeys(t, pool, integrationScope, HorizonDay); !reflect.DeepEqual(got, []string{"d1"}) {
		t.Errorf("day keys = %q, want the prior [d1]", got)
	}
	if got := storedHeaders(t, pool, integrationScope); !reflect.DeepEqual(got, wantHeaders) {
		t.Errorf("headers = %v, want the prior %v", got, wantHeaders)
	}
}

// storeParity stores the parity fixture's plays, in fixture order, as the
// recent horizon at the fixture's divine rate.
func storeParity(t *testing.T, repo *Repository) parityFixture {
	t.Helper()
	f := loadParity(t)
	res := Result{
		League: integrationScope.ID(), Horizon: string(HorizonRecent),
		From: baseHour, To: baseHour.Add(6 * time.Hour), Hours: 6, DivineChaosRate: f.DivineChaosRate,
	}
	for _, p := range f.Plays {
		play := p.Play
		applyNonFinite(t, &play, p.NonFinite)
		res.Plays = append(res.Plays, play)
	}
	if err := repo.ReplacePlays(context.Background(), integrationScope, []Result{res}); err != nil {
		t.Fatalf("store parity plays: %v", err)
	}
	return f
}

type parityCase struct {
	Name         string              `json:"name"`
	Params       map[string][]string `json:"params"`
	Total        int                 `json:"total"`
	AfterRules   int                 `json:"afterRules"`
	AfterGates   int                 `json:"afterGates"`
	Matched      int                 `json:"matched"`
	ShowConvert  bool                `json:"showConvert"`
	OrderedKeys  []string            `json:"orderedKeys"`
	PageKeys     []string            `json:"pageKeys"`
	ItemUniverse []struct {
		ID        string `json:"id"`
		PlayCount int    `json:"playCount"`
	} `json:"itemUniverse"`
}

func loadParityCases(t *testing.T) []parityCase {
	t.Helper()
	raw, err := os.ReadFile("testdata/page-parity.json")
	if err != nil {
		t.Fatal(err)
	}
	var f struct {
		Cases []parityCase `json:"cases"`
	}
	if err := json.Unmarshal(raw, &f); err != nil {
		t.Fatal(err)
	}
	if len(f.Cases) == 0 {
		t.Fatal("parity fixture carries no cases")
	}
	return f.Cases
}

func parityQuery(t *testing.T, params map[string][]string, pageSize string) PlaysPageQuery {
	t.Helper()
	values := url.Values{}
	for k, v := range params {
		values[k] = v
	}
	if pageSize != "" {
		values.Set("pageSize", pageSize)
	} else if !values.Has("pageSize") {
		values.Set("pageSize", "all")
	}
	q, err := ParsePlaysPageQuery(values)
	if err != nil {
		t.Fatalf("parse %v: %v", values, err)
	}
	q.Mode, q.Horizon = "all", HorizonRecent
	return q
}

func pageKeys(page PlaysPage) []string {
	keys := []string{}
	for _, p := range page.Plays {
		keys = append(keys, p.Key)
	}
	return keys
}

func TestPlaysPage_MatchesTheClientChainOnEveryParityCase(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	storeParity(t, repo)
	ctx := context.Background()

	for _, tc := range loadParityCases(t) {
		t.Run(tc.Name, func(t *testing.T) {
			page, err := repo.PlaysPage(ctx, integrationScope, parityQuery(t, tc.Params, ""))
			if err != nil {
				t.Fatalf("PlaysPage: %v", err)
			}
			got := [4]int{page.Total, page.AfterRules, page.AfterGates, page.Matched}
			want := [4]int{tc.Total, tc.AfterRules, tc.AfterGates, tc.Matched}
			if got != want {
				t.Errorf("total/afterRules/afterGates/matched = %v, want %v", got, want)
			}
			if page.ShowConvert != tc.ShowConvert {
				t.Errorf("showConvert = %v, want %v", page.ShowConvert, tc.ShowConvert)
			}
			if keys := pageKeys(page); !reflect.DeepEqual(keys, tc.PageKeys) {
				t.Errorf("page keys = %q\nwant %q", keys, tc.PageKeys)
			}
			all, err := repo.PlaysPage(ctx, integrationScope, parityQuery(t, tc.Params, "all"))
			if err != nil {
				t.Fatalf("PlaysPage all: %v", err)
			}
			if keys := pageKeys(all); !reflect.DeepEqual(keys, tc.OrderedKeys) {
				t.Errorf("ordered keys = %q\nwant %q", keys, tc.OrderedKeys)
			}
			wantUniverse := map[string]int{}
			for _, item := range tc.ItemUniverse {
				wantUniverse[item.ID] = item.PlayCount
			}
			gotUniverse := map[string]int{}
			for _, item := range page.ItemUniverse {
				gotUniverse[item.ID] = item.PlayCount
			}
			if !reflect.DeepEqual(gotUniverse, wantUniverse) {
				t.Errorf("item universe = %v\nwant %v", gotUniverse, wantUniverse)
			}
		})
	}
}

func TestPlaysPage_TotalIsServedAfterModeNotAfterFilters(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	f := storeParity(t, repo)
	direct := 0
	for _, p := range f.Plays {
		if p.Play.Mode == ModeDirect {
			direct++
		}
	}
	q := parityQuery(t, map[string][]string{"onlyCategory": {"Divination Cards"}}, "25")
	q.Mode = string(ModeDirect)

	page, err := repo.PlaysPage(context.Background(), integrationScope, q)

	if err != nil {
		t.Fatalf("PlaysPage: %v", err)
	}
	if page.Total != direct {
		t.Errorf("total = %d, want the %d direct plays", page.Total, direct)
	}
	if page.Matched >= page.Total {
		t.Errorf("matched = %d, want fewer than total %d (the rule cuts)", page.Matched, page.Total)
	}
}

func TestPlaysPage_PageTwoContinuesTheOrder(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	storeParity(t, repo)
	all := loadParityCases(t)[0].OrderedKeys
	q := parityQuery(t, nil, "25")
	q.Page = 2

	page, err := repo.PlaysPage(context.Background(), integrationScope, q)

	if err != nil {
		t.Fatalf("PlaysPage: %v", err)
	}
	if keys := pageKeys(page); !reflect.DeepEqual(keys, all[25:]) {
		t.Errorf("page 2 keys = %q\nwant %q", keys, all[25:])
	}
}

func TestPlaysPage_ServesTheDisplayOfEveryPlay(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	f := storeParity(t, repo)
	want := map[string]PlayDisplay{}
	for _, p := range f.Plays {
		want[p.Play.Key] = p.Display
	}

	page, err := repo.PlaysPage(context.Background(), integrationScope, parityQuery(t, nil, "all"))

	if err != nil {
		t.Fatalf("PlaysPage: %v", err)
	}
	if len(page.Plays) != len(f.Plays) {
		t.Fatalf("served %d plays, want %d", len(page.Plays), len(f.Plays))
	}
	for _, p := range page.Plays {
		if p.Display == nil {
			t.Errorf("%s: no display", p.Key)
			continue
		}
		if d := displayDiff(*p.Display, want[p.Key]); len(d) > 0 {
			t.Errorf("%s display: %v", p.Key, d)
		}
	}
}

func TestPlaysPage_HorizonWithoutAStoredSetIsCold(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)

	page, err := repo.PlaysPage(context.Background(), integrationScope, parityQuery(t, nil, "25"))

	if err != nil {
		t.Fatalf("PlaysPage: %v", err)
	}
	if page.Warm || len(page.Plays) != 0 || page.Total != 0 {
		t.Errorf("page = warm %v, %d plays, total %d; want cold and empty", page.Warm, len(page.Plays), page.Total)
	}
}

// countingTracer counts the statements a pool sends, BEGIN and COMMIT included.
type countingTracer struct{ n atomic.Int64 }

func (c *countingTracer) TraceQueryStart(ctx context.Context, _ *pgx.Conn, _ pgx.TraceQueryStartData) context.Context {
	c.n.Add(1)
	return ctx
}

func (c *countingTracer) TraceQueryEnd(context.Context, *pgx.Conn, pgx.TraceQueryEndData) {}

// playsPageStatements is the documented budget: BEGIN, header, stage counts,
// item universe, page, COMMIT.
const playsPageStatements = 6

func TestPlaysPage_StatementCountIsIndependentOfPageSize(t *testing.T) {
	pool := integrationPool(t)
	storeParity(t, newPlaysRepository(t, pool))
	cfg, err := pgxpool.ParseConfig(os.Getenv("DATABASE_URL"))
	if err != nil {
		t.Fatal(err)
	}
	tracer := &countingTracer{}
	cfg.ConnConfig.Tracer = tracer
	traced, err := pgxpool.NewWithConfig(context.Background(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(traced.Close)
	repo := NewRepository(traced)
	if err := traced.Ping(context.Background()); err != nil {
		t.Fatal(err)
	}

	counts := map[string]int64{}
	for _, size := range []string{"25", "all"} {
		before := tracer.n.Load()
		page, err := repo.PlaysPage(context.Background(), integrationScope, parityQuery(t, nil, size))
		if err != nil {
			t.Fatalf("PlaysPage %s: %v", size, err)
		}
		if len(page.Plays) == 0 {
			t.Fatalf("PlaysPage %s served nothing", size)
		}
		counts[size] = tracer.n.Load() - before
	}

	for size, n := range counts {
		if n != playsPageStatements {
			t.Errorf("pageSize %s sent %d statements, want %d", size, n, playsPageStatements)
		}
	}
}

func TestPlaysPage_OverflowingPageIsEmptyWithUnchangedCounts(t *testing.T) {
	pool := integrationPool(t)
	repo := newPlaysRepository(t, pool)
	f := storeParity(t, repo)
	ctx := context.Background()

	for _, tc := range []struct{ page, size string }{
		{"2305843009213693953", "200"}, // (page-1)*size wraps to 0
		{"9223372036854775807", "50"},  // (page-1)*size wraps negative
	} {
		t.Run(tc.page+"x"+tc.size, func(t *testing.T) {
			q := parityQuery(t, map[string][]string{"page": {tc.page}}, tc.size)

			page, err := repo.PlaysPage(ctx, integrationScope, q)

			if err != nil {
				t.Fatalf("PlaysPage: %v", err)
			}
			if len(page.Plays) != 0 {
				t.Errorf("served %d plays, want an empty page", len(page.Plays))
			}
			if page.Total != len(f.Plays) || page.Matched != len(f.Plays) {
				t.Errorf("total/matched = %d/%d, want %d/%d", page.Total, page.Matched, len(f.Plays), len(f.Plays))
			}
		})
	}
}
