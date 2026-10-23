//go:build integration

package league

import (
	"context"
	"errors"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// All fixture truncations belong to adminTestTx transactions, which roll back.
// PlanTest* IDs and the exact one-row fixtures are the WI-3 measured cases.
func wipeTestTx(t *testing.T, ctx context.Context, pool *pgxpool.Pool) pgx.Tx {
	t.Helper()
	tx := adminTestTx(t, ctx, pool)
	names := make([]string, 0, len(ScopedTables()))
	for _, table := range ScopedTables() {
		names = append(names, pgx.Identifier{table}.Sanitize())
	}
	if _, err := tx.Exec(ctx, "TRUNCATE "+strings.Join(names, ", ")); err != nil {
		t.Fatalf("truncate rolled-back fixture: %v", err)
	}
	return tx
}

// Legacy shapes mirror insertLegacyScopedRows in migrations_integration_test.go,
// with league added and dedication's mandatory variant from its later migration.
// The final four shapes come from their respective table-creation migrations.
func seedWipeRows(t *testing.T, ctx context.Context, tx pgx.Tx, id string) {
	t.Helper()
	for _, statement := range []string{
		`INSERT INTO gem_snapshots (league,time,name,variant,is_corrupted,chaos,listings) VALUES ($1,NOW(),'WI3 Gem','1/20',FALSE,1,1)`,
		`INSERT INTO currency_snapshots (league,time,currency_id,chaos,volume,sparkline_change) VALUES ($1,NOW(),'WI3 Currency',1,2,3)`,
		`INSERT INTO fragment_snapshots (league,time,fragment_id,chaos,sparkline_change) VALUES ($1,NOW(),'WI3 Fragment',1,2)`,
		`INSERT INTO font_snapshots (league,time,color,variant,mode,pool,winners) VALUES ($1,NOW(),'WI3 Font','1/20','safe',1,1)`,
		`INSERT INTO transfigure_results (league,time,base_name,transfigured_name,variant,base_price,transfigured_price) VALUES ($1,NOW(),'WI3 Base','WI3 Transfigure','1/20',1,2)`,
		`INSERT INTO quality_results (league,time,name,level,buy_price,price_q20) VALUES ($1,NOW(),'WI3 Quality',20,1,2)`,
		`INSERT INTO trend_results (league,time,name,variant,current_price,current_listings) VALUES ($1,NOW(),'WI3 Trend','1/20',1,1)`,
		`INSERT INTO gem_features (league,time,name,variant,chaos,listings) VALUES ($1,NOW(),'WI3 Feature','1/20',1,1)`,
		`INSERT INTO gem_signals (league,time,name,variant,signal,confidence) VALUES ($1,NOW(),'WI3 Signal','1/20','STABLE',1)`,
		`INSERT INTO dedication_snapshots (league,time,variant,color,gem_type,mode,pool,winners) VALUES ($1,NOW(),'21/23c','WI3 Dedication','skill','safe',1,1)`,
		`INSERT INTO trade_lookups (league,time,gem,variant,total_listings,price_floor) VALUES ($1,NOW(),'WI3 Trade','1/20',1,1)`,
		`INSERT INTO market_context (league,time,total_gems,total_listings) VALUES ($1,NOW(),1,1)`,
		`INSERT INTO currency_exchange_markets (league,time,market_id,item_a,item_b,volume_a,volume_b,lowest_stock_a,lowest_stock_b,highest_stock_a,highest_stock_b,lowest_ratio_a,lowest_ratio_b,highest_ratio_a,highest_ratio_b,price_valid) VALUES ($1,NOW(),'WI3 Market','a','b',1,1,1,1,1,1,1,1,1,1,TRUE)`,
		`INSERT INTO currency_exchange_cursor (league,next_hour) VALUES ($1,1)`,
		`INSERT INTO double_corrupt_snapshots (league,time,name,input_variant) VALUES ($1,NOW(),'WI3 Corrupt','20/20')`,
		`INSERT INTO item_snapshots (league,time,category,ninja_id) VALUES ($1,NOW(),'IncursionTemple',1)`,
	} {
		if _, err := tx.Exec(ctx, statement, id); err != nil {
			t.Fatalf("seed %s: %v", statement, err)
		}
	}
}

func seedWipeForeignRow(t *testing.T, ctx context.Context, tx pgx.Tx, table string) {
	t.Helper()
	var statement string
	switch table {
	case "item_snapshots":
		statement = `INSERT INTO item_snapshots (league,time,category,ninja_id) VALUES ('PlanTestNext',NOW(),'IncursionTemple',1)`
	case "gem_snapshots":
		statement = `INSERT INTO gem_snapshots (league,time,name,variant) VALUES ('PlanTestNext',NOW(),'WI3 Gem','1/20')`
	default:
		t.Fatalf("unknown foreign-row fixture %q", table)
	}
	if _, err := tx.Exec(ctx, statement); err != nil {
		t.Fatal(err)
	}
}

func wipeRowCount(t *testing.T, ctx context.Context, source interface {
	QueryRow(context.Context, string, ...any) pgx.Row
}, table, id string) int64 {
	t.Helper()
	var count int64
	query := "SELECT count(*) FROM " + pgx.Identifier{table}.Sanitize()
	var args []any
	if id != "" {
		query += " WHERE league = $1"
		args = append(args, id)
	}
	if err := source.QueryRow(ctx, query, args...).Scan(&count); err != nil {
		t.Fatalf("count %s/%s: %v", table, id, err)
	}
	return count
}

func assertWipeRows(t *testing.T, ctx context.Context, tx pgx.Tx, id string, want int64) {
	t.Helper()
	for _, table := range ScopedTables() {
		if got := wipeRowCount(t, ctx, tx, table, id); got != want {
			t.Errorf("%s/%s rows = %d, want %d", table, id, got, want)
		}
	}
}

func assertWipeCounts(t *testing.T, got map[string]int64, want int64) {
	t.Helper()
	expected := make(map[string]int64)
	for _, table := range ScopedTables() {
		expected[table] = want
	}
	if !reflect.DeepEqual(got, expected) {
		t.Errorf("per-table counts = %v, want %v", got, expected)
	}
}

func wipeArchivedFixture(t *testing.T) (context.Context, *pgxpool.Pool, pgx.Tx) {
	t.Helper()
	ctx, pool, _ := adminTestPools(t)
	tx := wipeTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	seedWipeRows(t, ctx, tx, "Mirage")
	return ctx, pool, tx
}

func TestPreviewWipeExclusiveReportsTruncate(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	got, err := PreviewWipe(ctx, tx, "Mirage", false)
	if err != nil {
		t.Fatal(err)
	}
	assertWipeCounts(t, got.Counts, 1)
	names := make([]string, 0, len(ScopedTables()))
	for _, table := range ScopedTables() {
		names = append(names, pgx.Identifier{table}.Sanitize())
	}
	want := []string{"TRUNCATE " + strings.Join(names, ", ")}
	if got.Mode != "truncate" || !reflect.DeepEqual(got.Statements, want) {
		t.Errorf("preview = %q/%q, want truncate/%q", got.Mode, got.Statements, want)
	}
}

func TestPreviewWipeExclusivePreservesRows(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	if _, err := PreviewWipe(ctx, tx, "Mirage", false); err != nil {
		t.Fatal(err)
	}
	assertWipeRows(t, ctx, tx, "", 1)
}

func TestWipeExclusiveRemovesEveryScopedRow(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	got, err := Wipe(ctx, tx, "Mirage", false)
	if err != nil {
		t.Fatal(err)
	}
	if got.Mode != "truncate" || got.Counts != nil {
		t.Errorf("wipe = %q/%v, want truncate/nil counts", got.Mode, got.Counts)
	}
	assertWipeRows(t, ctx, tx, "", 0)
}

func TestPreviewWipeMixedRefusesForeignRows(t *testing.T) {
	for _, table := range []string{"item_snapshots", "gem_snapshots"} {
		t.Run(table, func(t *testing.T) {
			ctx, _, tx := wipeArchivedFixture(t)
			seedWipeForeignRow(t, ctx, tx, table)
			if _, err := PreviewWipe(ctx, tx, "Mirage", false); !errors.Is(err, ErrForeignRows) {
				t.Errorf("preview with foreign %s row = %v, want ErrForeignRows", table, err)
			}
			assertWipeRows(t, ctx, tx, "Mirage", 1)
			if got := wipeRowCount(t, ctx, tx, table, "PlanTestNext"); got != 1 {
				t.Errorf("foreign row count = %d, want 1", got)
			}
		})
	}
}

func TestWipeMixedRefusesForeignRows(t *testing.T) {
	for _, table := range []string{"item_snapshots", "gem_snapshots"} {
		t.Run(table, func(t *testing.T) {
			ctx, _, tx := wipeArchivedFixture(t)
			seedWipeForeignRow(t, ctx, tx, table)
			if _, err := Wipe(ctx, tx, "Mirage", false); !errors.Is(err, ErrForeignRows) {
				t.Errorf("wipe with foreign %s row = %v, want ErrForeignRows", table, err)
			}
			assertWipeRows(t, ctx, tx, "Mirage", 1)
			if got := wipeRowCount(t, ctx, tx, table, "PlanTestNext"); got != 1 {
				t.Errorf("foreign row count = %d, want 1", got)
			}
		})
	}
}

func TestPreviewWipeMixedReportsDelete(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	seedWipeForeignRow(t, ctx, tx, "item_snapshots")
	got, err := PreviewWipe(ctx, tx, "Mirage", true)
	if err != nil {
		t.Fatal(err)
	}
	assertWipeCounts(t, got.Counts, 1)
	want := make([]string, 0, len(ScopedTables()))
	for _, table := range ScopedTables() {
		want = append(want, "DELETE FROM "+pgx.Identifier{table}.Sanitize()+" WHERE league = $1")
	}
	if got.Mode != "delete" || !reflect.DeepEqual(got.Statements, want) {
		t.Errorf("preview = %q/%q, want delete/%q", got.Mode, got.Statements, want)
	}
}

func TestPreviewWipeMixedPreservesRows(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	seedWipeForeignRow(t, ctx, tx, "item_snapshots")
	if _, err := PreviewWipe(ctx, tx, "Mirage", true); err != nil {
		t.Fatal(err)
	}
	assertWipeRows(t, ctx, tx, "Mirage", 1)
	if got := wipeRowCount(t, ctx, tx, "item_snapshots", "PlanTestNext"); got != 1 {
		t.Errorf("foreign rows = %d, want 1", got)
	}
}

func TestWipeMixedDeletesTargetRows(t *testing.T) {
	ctx, _, tx := wipeArchivedFixture(t)
	seedWipeForeignRow(t, ctx, tx, "item_snapshots")
	// A refusal must leave this same transaction usable for the opt-in retry.
	if _, err := Wipe(ctx, tx, "Mirage", false); !errors.Is(err, ErrForeignRows) {
		t.Fatalf("initial refusal = %v, want ErrForeignRows", err)
	}
	got, err := Wipe(ctx, tx, "Mirage", true)
	if err != nil {
		t.Fatal(err)
	}
	if got.Mode != "delete" {
		t.Errorf("mode = %q, want delete", got.Mode)
	}
	assertWipeCounts(t, got.Counts, 1)
	assertWipeRows(t, ctx, tx, "Mirage", 0)
	for _, table := range ScopedTables() {
		want := int64(0)
		if table == "item_snapshots" {
			want = 1
		}
		if got := wipeRowCount(t, ctx, tx, table, ""); got != want {
			t.Errorf("remaining %s rows = %d, want %d", table, got, want)
		}
	}
	if got := wipeRowCount(t, ctx, tx, "item_snapshots", "PlanTestNext"); got != 1 {
		t.Errorf("foreign rows = %d, want 1", got)
	}
}

func TestWipeNoTargetPreservesForeignRows(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := wipeTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	seedWipeForeignRow(t, ctx, tx, "gem_snapshots")
	got, err := Wipe(ctx, tx, "Mirage", false)
	if err != nil {
		t.Fatal(err)
	}
	if got.Mode != "none" || got.Counts != nil || len(got.Statements) != 0 {
		t.Errorf("empty-target wipe = %+v, want none/nil counts/no statements", got)
	}
	assertWipeRows(t, ctx, tx, "Mirage", 0)
	if got := wipeRowCount(t, ctx, tx, "gem_snapshots", "PlanTestNext"); got != 1 {
		t.Errorf("foreign gem rows = %d, want 1", got)
	}
}

func TestWipeRejectsInvalidLifecycle(t *testing.T) {
	for _, tc := range []struct {
		name, id string
		want     error
	}{{"active", "PlanTestNext", ErrWipeActive}, {"prepared", "PlanTestOther", ErrNotArchived}, {"missing", "PlanTestMissing", ErrLeagueNotFound}} {
		t.Run(tc.name, func(t *testing.T) {
			for _, allowDelete := range []bool{false, true} {
				t.Run(map[bool]string{false: "default", true: "allow_delete"}[allowDelete], func(t *testing.T) {
					ctx, _, tx := wipeArchivedFixture(t)
					if _, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestOther"}); err != nil {
						t.Fatal(err)
					}
					if tc.name != "missing" {
						seedWipeRows(t, ctx, tx, tc.id)
					}
					if _, err := Wipe(ctx, tx, tc.id, allowDelete); !errors.Is(err, tc.want) {
						t.Errorf("Wipe(%q,%t) = %v, want %v", tc.id, allowDelete, err, tc.want)
					}
					assertWipeRows(t, ctx, tx, "Mirage", 1)
					if tc.name != "missing" {
						assertWipeRows(t, ctx, tx, tc.id, 1)
					}
				})
			}
		})
	}
}

func TestWipeRefusesRunningCollector(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	holdAdminTestLock(t, ctx, peer, RuntimeLockKey())
	tx := wipeTestTx(t, ctx, pool)
	seedWipeRows(t, ctx, tx, "Mirage")
	if _, err := Wipe(ctx, tx, "Mirage", false); !errors.Is(err, ErrCollectorRunning) {
		t.Errorf("wipe with collector = %v, want ErrCollectorRunning", err)
	}
	assertWipeRows(t, ctx, tx, "", 1)
}

func TestWipeAdministrationContentionRefusesOperations(t *testing.T) {
	for _, operation := range []struct {
		name string
		call func(context.Context, pgx.Tx, string, bool) (WipeResult, error)
	}{{"Wipe", Wipe}, {"PreviewWipe", PreviewWipe}} {
		t.Run(operation.name, func(t *testing.T) {
			ctx, pool, peer := adminTestPools(t)
			holdAdminTestLock(t, ctx, peer, AdministrationLockKey())
			tx := wipeTestTx(t, ctx, pool)
			seedWipeRows(t, ctx, tx, "Mirage")
			if _, err := operation.call(ctx, tx, "Mirage", false); !errors.Is(err, ErrAdministrationBusy) {
				t.Errorf("%s with administrator = %v, want ErrAdministrationBusy", operation.name, err)
			}
			assertWipeRows(t, ctx, tx, "", 1)
		})
	}
}

func TestWipeTablesBusyReturnsLockTimeout(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	original := wipeLockTimeout
	wipeLockTimeout = 200 * time.Millisecond
	t.Cleanup(func() { wipeLockTimeout = original })
	// This case deliberately takes the peer lock before any fixture TRUNCATE.
	blocker := adminTestTx(t, ctx, peer)
	if _, err := blocker.Exec(ctx, "LOCK TABLE gem_snapshots IN ACCESS SHARE MODE"); err != nil {
		t.Fatal(err)
	}
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	_, err := Wipe(ctx, tx, "Mirage", false)
	if !errors.Is(err, ErrTablesBusy) {
		t.Errorf("blocked wipe = %v, want ErrTablesBusy", err)
	}
	var pgErr *pgconn.PgError
	if !errors.As(err, &pgErr) || pgErr.Code != "55P03" {
		t.Errorf("blocked wipe = %v, want SQLSTATE 55P03", err)
	}
	// The SQL error aborts tx: cleanup only rolls it back, then the blocker.
}

func TestWipeRollbackRestoresGemRows(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	before := wipeRowCount(t, ctx, pool, "gem_snapshots", "")
	tx := wipeTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	seedWipeRows(t, ctx, tx, "Mirage")
	if _, err := Wipe(ctx, tx, "Mirage", false); err != nil {
		t.Fatal(err)
	}
	assertWipeRows(t, ctx, tx, "", 0)
	if err := tx.Rollback(ctx); err != nil {
		t.Fatal(err)
	}
	if after := wipeRowCount(t, ctx, pool, "gem_snapshots", ""); after != before {
		t.Errorf("gem pool count after rollback = %d, want original %d", after, before)
	}
}
