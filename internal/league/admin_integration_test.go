//go:build integration

package league

import (
	"context"
	"errors"
	"reflect"
	"slices"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// PlanTest* identifiers, display names and the December start time are synthetic
// fixtures from the WI-2 measured cases. Mirage/revision 1 is the migration seed.
func adminTestPools(t *testing.T) (context.Context, *pgxpool.Pool, *pgxpool.Pool) {
	t.Helper()
	url := requireDatabase(t)
	pool, peer := newTestPool(t, url), newTestPool(t, url)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	t.Cleanup(cancel)
	var database string
	if err := pool.QueryRow(ctx, `SELECT current_database()`).Scan(&database); err != nil {
		t.Fatal(err)
	}
	if database != "profitofexile_integration" {
		t.Fatalf("refuse administration tests on database %q", database)
	}
	assertAdminRuntime(t, ctx, pool, "Mirage", 1)
	t.Cleanup(func() { assertAdminRuntime(t, ctx, pool, "Mirage", 1) })
	return ctx, pool, peer
}

func adminTestTx(t *testing.T, ctx context.Context, pool *pgxpool.Pool) pgx.Tx {
	t.Helper()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := tx.Rollback(ctx); err != nil && !errors.Is(err, pgx.ErrTxClosed) {
			t.Errorf("rollback: %v", err)
		}
	})
	return tx
}

func assertAdminRuntime(t *testing.T, ctx context.Context, source interface {
	QueryRow(context.Context, string, ...any) pgx.Row
}, id string, revision int64) {
	t.Helper()
	scope, err := Resolve(ctx, source)
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if scope.ID() != id || scope.Revision() != revision {
		t.Errorf("runtime = %q/%d, want %q/%d", scope.ID(), scope.Revision(), id, revision)
	}
}

func prepareAdminNext(t *testing.T, ctx context.Context, tx pgx.Tx) {
	t.Helper()
	if _, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestNext", DisplayName: "Plan Test Next"}); err != nil {
		t.Fatalf("prepare fixture: %v", err)
	}
}

func activateAdminNext(t *testing.T, ctx context.Context, tx pgx.Tx) Activation {
	t.Helper()
	prepareAdminNext(t, ctx, tx)
	result, err := Activate(ctx, tx, "PlanTestNext")
	if err != nil {
		t.Fatalf("activate fixture: %v", err)
	}
	return result
}

func holdAdminTestLock(t *testing.T, ctx context.Context, peer *pgxpool.Pool, key int64) {
	t.Helper()
	lock, err := AcquireProcessLock(ctx, peer, key)
	if err != nil {
		t.Fatalf("hold peer lock: %v", err)
	}
	t.Cleanup(lock.Release)
}

func assertAdminLocksAvailable(t *testing.T, ctx context.Context, peer *pgxpool.Pool) {
	t.Helper()
	for _, key := range []int64{RuntimeLockKey(), AdministrationLockKey()} {
		lock, err := AcquireProcessLock(ctx, peer, key)
		if err != nil {
			t.Errorf("lock %d still held after transaction: %v", key, err)
			continue
		}
		lock.Release()
	}
}

func TestPrepareReturnsRegisteredValues(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	starts := time.Date(2026, 12, 4, 19, 0, 0, 0, time.UTC)
	row, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestNext", DisplayName: "Plan Test Next", StartsAt: &starts})
	if err != nil {
		t.Fatal(err)
	}
	if row.ID != "PlanTestNext" || row.DisplayName != "Plan Test Next" || row.CollectionState != "prepared" || row.PreparedAt == nil || row.ActivatedAt != nil || row.StartsAt == nil || !row.StartsAt.Equal(starts) {
		t.Errorf("prepared row = %+v, want synthetic id/display, prepared timestamp, exact start and no activation", row)
	}
	var display, state string
	var prepared, activated *time.Time
	var storedStart time.Time
	if err := tx.QueryRow(ctx, `SELECT display_name, collection_state, prepared_at, starts_at, activated_at FROM leagues WHERE id=$1`, "PlanTestNext").Scan(&display, &state, &prepared, &storedStart, &activated); err != nil {
		t.Fatal(err)
	}
	if display != "Plan Test Next" || state != "prepared" || prepared == nil || !storedStart.Equal(starts) || activated != nil {
		t.Errorf("persisted prepare = %q/%q/%v/%v/%v", display, state, prepared, storedStart, activated)
	}
}

func TestPrepareDefaultsDisplayNameToID(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	row, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestOther"})
	if err != nil {
		t.Fatal(err)
	}
	var display string
	if err := tx.QueryRow(ctx, `SELECT display_name FROM leagues WHERE id=$1`, "PlanTestOther").Scan(&display); err != nil {
		t.Fatal(err)
	}
	if row.DisplayName != "PlanTestOther" || display != "PlanTestOther" {
		t.Errorf("display returned/stored = %q/%q, want PlanTestOther", row.DisplayName, display)
	}
}

func TestPrepareRejectsInvalidID(t *testing.T) {
	for _, tc := range []struct{ name, id string }{{"empty", ""}, {"leading_whitespace", " PlanTestNext"}} {
		t.Run(tc.name, func(t *testing.T) {
			ctx, pool, _ := adminTestPools(t)
			tx := adminTestTx(t, ctx, pool)
			if _, err := Prepare(ctx, tx, PrepareInput{ID: tc.id}); !errors.Is(err, ErrInvalidLeagueID) {
				t.Errorf("Prepare(%q) error = %v, want ErrInvalidLeagueID", tc.id, err)
			}
			var count int
			if err := tx.QueryRow(ctx, `SELECT count(*) FROM leagues WHERE id=$1`, tc.id).Scan(&count); err != nil {
				t.Fatal(err)
			}
			if count != 0 {
				t.Errorf("invalid id inserted %d rows", count)
			}
		})
	}
}

func TestPrepareDuplicateLeavesTransactionUsable(t *testing.T) {
	for _, tc := range []struct {
		name  string
		input PrepareInput
	}{{"id", PrepareInput{ID: "PlanTestNext"}}, {"display_name", PrepareInput{ID: "PlanTestDup", DisplayName: "Plan Test Next"}}} {
		t.Run(tc.name, func(t *testing.T) {
			ctx, pool, _ := adminTestPools(t)
			tx := adminTestTx(t, ctx, pool)
			prepareAdminNext(t, ctx, tx)
			if _, err := Prepare(ctx, tx, tc.input); !errors.Is(err, ErrLeagueExists) {
				t.Errorf("duplicate error = %v, want ErrLeagueExists", err)
			}
			row, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestOther"})
			if err != nil {
				t.Fatalf("Prepare following duplicate: %v", err)
			}
			var state string
			if err := tx.QueryRow(ctx, `SELECT collection_state FROM leagues WHERE id=$1`, "PlanTestOther").Scan(&state); err != nil {
				t.Fatal(err)
			}
			if row.ID != "PlanTestOther" || state != "prepared" {
				t.Errorf("following prepare = %q/%q, want PlanTestOther/prepared", row.ID, state)
			}
		})
	}
}

func TestActivateReturnsRevisionTransition(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	got := activateAdminNext(t, ctx, tx)
	want := Activation{FromID: "Mirage", FromRevision: 1, ToID: "PlanTestNext", ToRevision: 2}
	if got != want {
		t.Errorf("Activation = %+v, want %+v", got, want)
	}
}

func TestActivatePersistsRuntimeSelection(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	assertAdminRuntime(t, ctx, tx, "PlanTestNext", 2)
}

func TestActivatePromotesPreparedLeague(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	var state string
	var activated *time.Time
	if err := tx.QueryRow(ctx, `SELECT collection_state, activated_at FROM leagues WHERE id=$1`, "PlanTestNext").Scan(&state, &activated); err != nil {
		t.Fatal(err)
	}
	if state != "collecting" || activated == nil {
		t.Errorf("target state/timestamp = %q/%v, want collecting/activation", state, activated)
	}
}

func TestActivateArchivesOutgoingLeague(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	var state string
	var archived, ends *time.Time
	if err := tx.QueryRow(ctx, `SELECT collection_state, archived_at, ends_at FROM leagues WHERE id=$1`, "Mirage").Scan(&state, &archived, &ends); err != nil {
		t.Fatal(err)
	}
	if state != "archived" || archived == nil || ends == nil {
		t.Errorf("outgoing state/archive/end = %q/%v/%v, want archived with both timestamps", state, archived, ends)
	}
}

func TestActivateRejectsInvalidTransition(t *testing.T) {
	for _, tc := range []struct {
		name, id string
		flip     bool
		want     error
	}{{"already_active", "Mirage", false, ErrAlreadyActive}, {"missing", "PlanTestMissing", false, ErrLeagueNotFound}, {"archived", "Mirage", true, ErrNotPrepared}} {
		t.Run(tc.name, func(t *testing.T) {
			ctx, pool, _ := adminTestPools(t)
			tx := adminTestTx(t, ctx, pool)
			id, rev := "Mirage", int64(1)
			if tc.flip {
				activateAdminNext(t, ctx, tx)
				id, rev = "PlanTestNext", 2
			}
			if _, err := Activate(ctx, tx, tc.id); !errors.Is(err, tc.want) {
				t.Errorf("Activate(%q) error = %v, want %v", tc.id, err, tc.want)
			}
			assertAdminRuntime(t, ctx, tx, id, rev)
		})
	}
}

func TestReadStatusReportsActivatedLeague(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	status, err := ReadStatus(ctx, tx, true)
	if err != nil {
		t.Fatal(err)
	}
	if status.Active != "PlanTestNext" || status.Revision != 2 {
		t.Errorf("status = %q/%d, want PlanTestNext/2", status.Active, status.Revision)
	}
	states := map[string]string{}
	ids := make([]string, 0, len(status.Leagues))
	for _, row := range status.Leagues {
		ids = append(ids, row.ID)
		states[row.ID] = row.CollectionState
	}
	if !slices.IsSorted(ids) {
		t.Errorf("league ids not sorted: %q", ids)
	}
	if states["Mirage"] != "archived" || states["PlanTestNext"] != "collecting" {
		t.Errorf("required status members = %v", states)
	}
}

func TestReadStatusCountsEveryScopedTable(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	status, err := ReadStatus(ctx, tx, true)
	if err != nil {
		t.Fatal(err)
	}
	tables := ScopedTables()
	if len(status.Rows) != len(tables) {
		t.Errorf("count table entries = %d, want %d", len(status.Rows), len(tables))
	}
	for _, table := range tables {
		if _, ok := status.Rows[table]; !ok {
			t.Errorf("missing counts for %s", table)
		}
	}
}

func TestReadStatusOmitsUnrequestedCounts(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	status, err := ReadStatus(ctx, tx, false)
	if err != nil {
		t.Fatal(err)
	}
	if status.Active != "PlanTestNext" || status.Revision != 2 || status.Rows != nil {
		t.Errorf("uncounted status = %+v, want PlanTestNext/2 and nil Rows", status)
	}
}

func TestReadStatusCountsGemRowsByLeague(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	before, err := ReadStatus(ctx, tx, true)
	if err != nil {
		t.Fatal(err)
	}
	if before.Rows["gem_snapshots"]["PlanTestNext"] != 0 {
		t.Fatalf("synthetic league initial gem count = %d, want 0", before.Rows["gem_snapshots"]["PlanTestNext"])
	}
	if _, err := tx.Exec(ctx, `INSERT INTO gem_snapshots (league,time,name,variant) VALUES ($1,NOW(),$2,$3)`, "PlanTestNext", "Synthetic WI2 Gem", "1"); err != nil {
		t.Fatal(err)
	}
	after, err := ReadStatus(ctx, tx, true)
	if err != nil {
		t.Fatal(err)
	}
	if after.Rows["gem_snapshots"]["PlanTestNext"] != 1 {
		t.Errorf("synthetic league gem count = %d, want 1", after.Rows["gem_snapshots"]["PlanTestNext"])
	}
	for id, count := range before.Rows["gem_snapshots"] {
		if id != "PlanTestNext" && after.Rows["gem_snapshots"][id] != count {
			t.Errorf("other league %q count changed: %d -> %d", id, count, after.Rows["gem_snapshots"][id])
		}
	}
}

func TestActivateRefusesRunningCollector(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	holdAdminTestLock(t, ctx, peer, RuntimeLockKey())
	tx := adminTestTx(t, ctx, pool)
	prepareAdminNext(t, ctx, tx)
	if _, err := Activate(ctx, tx, "PlanTestNext"); !errors.Is(err, ErrCollectorRunning) {
		t.Errorf("Activate with collector error = %v, want ErrCollectorRunning", err)
	}
	assertAdminRuntime(t, ctx, tx, "Mirage", 1)
}

func TestPrepareSucceedsWithRunningCollector(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	holdAdminTestLock(t, ctx, peer, RuntimeLockKey())
	tx := adminTestTx(t, ctx, pool)
	row, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestNext"})
	if err != nil {
		t.Fatal(err)
	}
	if row.ID != "PlanTestNext" || row.CollectionState != "prepared" {
		t.Errorf("Prepare with collector = %+v", row)
	}
}

func TestAdministrationContentionRefusesOperations(t *testing.T) {
	for _, operation := range []string{"Prepare", "Activate", "ReadStatus"} {
		t.Run(operation, func(t *testing.T) {
			ctx, pool, peer := adminTestPools(t)
			holdAdminTestLock(t, ctx, peer, AdministrationLockKey())
			tx := adminTestTx(t, ctx, pool)
			var err error
			switch operation {
			case "Prepare":
				_, err = Prepare(ctx, tx, PrepareInput{ID: "PlanTestNext"})
			case "Activate":
				_, err = Activate(ctx, tx, "Mirage")
			case "ReadStatus":
				_, err = ReadStatus(ctx, tx, true)
			}
			if !errors.Is(err, ErrAdministrationBusy) {
				t.Errorf("%s with competing administrator = %v, want ErrAdministrationBusy", operation, err)
			}
		})
	}
}

func TestActivateRollbackReleasesLocks(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	activateAdminNext(t, ctx, tx)
	if err := tx.Rollback(ctx); err != nil {
		t.Fatal(err)
	}
	assertAdminLocksAvailable(t, ctx, peer)
}

func adminTestRowCount(t *testing.T, ctx context.Context, pool *pgxpool.Pool, id string) int {
	t.Helper()
	var count int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM leagues WHERE id=$1`, id).Scan(&count); err != nil {
		t.Fatal(err)
	}
	return count
}

func TestRunAdminDryRunRollsBackPrepare(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	var row LeagueRow
	err := RunAdmin(ctx, pool, true, func(tx pgx.Tx) error {
		var err error
		row, err = Prepare(ctx, tx, PrepareInput{ID: "PlanTestDry"})
		return err
	})
	if err != nil {
		t.Fatal(err)
	}
	if row.ID != "PlanTestDry" || row.CollectionState != "prepared" {
		t.Errorf("dry run operation result = %+v", row)
	}
	if got := adminTestRowCount(t, ctx, pool, "PlanTestDry"); got != 0 {
		t.Errorf("dry run persisted %d rows, want 0", got)
	}
}

func TestRunAdminCommitPersistsPrepare(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id=$1`, "PlanTestCommit"); err != nil {
			t.Errorf("clean committed prepare: %v", err)
		}
	})
	err := RunAdmin(ctx, pool, false, func(tx pgx.Tx) error { _, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestCommit"}); return err })
	if err != nil {
		t.Fatal(err)
	}
	if got := adminTestRowCount(t, ctx, pool, "PlanTestCommit"); got != 1 {
		t.Errorf("committed rows = %d, want 1", got)
	}
	var state string
	if err := pool.QueryRow(ctx, `SELECT collection_state FROM leagues WHERE id=$1`, "PlanTestCommit").Scan(&state); err != nil {
		t.Fatal(err)
	}
	if state != "prepared" {
		t.Errorf("committed state = %q, want prepared", state)
	}
}

func TestRunAdminCommitReleasesLocks(t *testing.T) {
	ctx, pool, peer := adminTestPools(t)
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id=$1`, "PlanTestCommit"); err != nil {
			t.Errorf("clean committed prepare: %v", err)
		}
	})
	if err := RunAdmin(ctx, pool, false, func(tx pgx.Tx) error { _, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestCommit"}); return err }); err != nil {
		t.Fatal(err)
	}
	assertAdminLocksAvailable(t, ctx, peer)
}

func TestRunAdminOperationErrorRollsBackWrites(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	want := errors.New("synthetic operation failure")
	err := RunAdmin(ctx, pool, false, func(tx pgx.Tx) error {
		if _, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestDry"}); err != nil {
			return err
		}
		return want
	})
	if !errors.Is(err, want) {
		t.Errorf("RunAdmin error = %v, want original operation failure", err)
	}
	if got := adminTestRowCount(t, ctx, pool, "PlanTestDry"); got != 0 {
		t.Errorf("failed operation persisted %d rows, want 0", got)
	}
}

// Keep the expected prepare snapshot comparison independent of production scanners.
func TestPreparePersistsReturnedRow(t *testing.T) {
	ctx, pool, _ := adminTestPools(t)
	tx := adminTestTx(t, ctx, pool)
	row, err := Prepare(ctx, tx, PrepareInput{ID: "PlanTestNext"})
	if err != nil {
		t.Fatal(err)
	}
	var stored LeagueRow
	if err := tx.QueryRow(ctx, `SELECT id,display_name,starts_at,ends_at,collection_state,prepared_at,activated_at,archived_at,created_at,updated_at FROM leagues WHERE id=$1`, "PlanTestNext").Scan(&stored.ID, &stored.DisplayName, &stored.StartsAt, &stored.EndsAt, &stored.CollectionState, &stored.PreparedAt, &stored.ActivatedAt, &stored.ArchivedAt, &stored.CreatedAt, &stored.UpdatedAt); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(row, stored) {
		t.Errorf("returned row = %+v, persisted row = %+v", row, stored)
	}
}
