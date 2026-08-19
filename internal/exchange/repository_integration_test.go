//go:build integration

package exchange

import (
	"context"
	"errors"
	"os"
	"testing"
	"time"

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
