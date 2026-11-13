//go:build integration

package retention

import (
	"context"
	"fmt"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/bossloot"
	"profitofexile/internal/device"
	"profitofexile/internal/mercenary"
)

// fixedNow puts the cutoff at 2022-03-01, two calendar years back across the
// 2024 leap day; a 730-day duration would land on 2022-03-02 instead. Every
// timestamp seeded here predates real data, so a purge at this clock touches
// only the rows a test seeds.
var (
	fixedNow = time.Date(2024, 3, 1, 0, 0, 0, 0, time.UTC)
	cutoff   = time.Date(2022, 3, 1, 0, 0, 0, 0, time.UTC)
	old      = time.Date(2021, 6, 1, 0, 0, 0, 0, time.UTC)
	recent   = time.Date(2023, 6, 1, 0, 0, 0, 0, time.UTC)
)

var seq atomic.Int64

// world seeds rows with explicit timestamps and removes them on cleanup. Rows
// are tracked by id because a purge removes the device link they could
// otherwise be found by.
type world struct {
	t       *testing.T
	pool    *pgxpool.Pool
	tag     string
	devices []string
	ids     map[string][]int64
}

func newWorld(t *testing.T) *world {
	t.Helper()
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		t.Skip("DATABASE_URL required")
	}
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES ('Standard','Boss loot synthetic historical league','archived') ON CONFLICT DO NOTHING`); err != nil {
		pool.Close()
		t.Fatal(err)
	}
	w := &world{t: t, pool: pool, tag: fmt.Sprintf("retention-%d-%d", time.Now().UnixNano(), seq.Add(1)), ids: map[string][]int64{}}
	t.Cleanup(func() {
		for _, table := range []string{"font_sessions", "lab_runs", "boss_loot_runs", "merc_icon_templates"} {
			if _, err := pool.Exec(ctx, `DELETE FROM `+table+` WHERE id = ANY($1)`, w.ids[table]); err != nil {
				t.Error(err)
			}
		}
		if _, err := pool.Exec(ctx, `DELETE FROM devices WHERE fingerprint = ANY($1)`, w.devices); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id='Standard' AND display_name='Boss loot synthetic historical league' AND NOT EXISTS (SELECT 1 FROM boss_loot_runs WHERE league='Standard')`); err != nil {
			t.Error(err)
		}
		pool.Close()
	})
	return w
}

func (w *world) purger() *Purger {
	return &Purger{pool: w.pool, now: func() time.Time { return fixedNow }}
}

func (w *world) purge() Counts {
	w.t.Helper()
	c, err := w.purger().Purge(context.Background())
	if err != nil {
		w.t.Fatalf("purge: %v", err)
	}
	return c
}

func (w *world) exec(sql string, args ...any) {
	w.t.Helper()
	if _, err := w.pool.Exec(context.Background(), sql, args...); err != nil {
		w.t.Fatalf("%s: %v", sql, err)
	}
}

func (w *world) insertID(table, sql string, args ...any) int64 {
	w.t.Helper()
	var id int64
	if err := w.pool.QueryRow(context.Background(), sql, args...).Scan(&id); err != nil {
		w.t.Fatalf("seed %s: %v", table, err)
	}
	w.ids[table] = append(w.ids[table], id)
	return id
}

type deviceSpec struct {
	alias  *string
	role   string
	banned bool
}

func (w *world) device(name string, lastSeen time.Time, spec deviceSpec) string {
	w.t.Helper()
	fp := w.tag + "-" + name
	if spec.role == "" {
		spec.role = "user"
	}
	w.devices = append(w.devices, fp)
	w.exec(`INSERT INTO devices (fingerprint, alias, role, banned, app_version, first_seen, last_seen)
		VALUES ($1, $2, $3, $4, '1.2.3', $5, $5)`, fp, spec.alias, spec.role, spec.banned, lastSeen)
	return fp
}

// fontSession seeds a session with two rounds.
func (w *world) fontSession(dev string, at time.Time) int64 {
	w.t.Helper()
	id := w.insertID("font_sessions", `INSERT INTO font_sessions (time, lab_type, total_crafts, device_id, pair_code)
		VALUES ($1, 'Uber', 3, $2, 'PAIR-CODE') RETURNING id`, at, dev)
	w.exec(`INSERT INTO font_rounds (session_id, round_number, option_chosen) VALUES ($1, 1, 'a'), ($1, 2, 'b')`, id)
	return id
}

// labRun seeds a run with one room.
func (w *world) labRun(dev string, at time.Time) int64 {
	w.t.Helper()
	id := w.insertID("lab_runs", `INSERT INTO lab_runs (device_id, difficulty, strategy, started_at, elapsed_seconds, room_count, created_at)
		VALUES ($1, 'Uber', 'fast', $2, 300, 1, $2) RETURNING id`, dev, at)
	w.exec(`INSERT INTO lab_run_rooms (run_id, room_number, room_name, entered_at) VALUES ($1, 1, 'Estate Path', $2)`, id, at)
	return id
}

func (w *world) bossRun(dev string, at time.Time) int64 {
	w.t.Helper()
	return w.insertID("boss_loot_runs", `INSERT INTO boss_loot_runs (league, device_id, boss, profile_key, profile_version, run_number, items, created_at)
		VALUES ('Standard', $1, 'catarina', $2, 1, nextval('boss_loot_runs_id_seq'), '{"quantities":{"Urn":2},"counted":["Urn"]}', $3) RETURNING id`,
		dev, w.tag, at)
}

func (w *world) mercTemplate(dev string, at time.Time) int64 {
	w.t.Helper()
	return w.insertID("merc_icon_templates", `INSERT INTO merc_icon_templates (family, tier, format_version, signature, device_id, created_at)
		VALUES ($1, 1, 1, decode(repeat('ab', 576), 'hex'), $2, $3) RETURNING id`, w.tag, dev, at)
}

type deviceRows struct {
	font, lab, boss, merc int64
}

func (w *world) rowsFor(dev string, at time.Time) deviceRows {
	return deviceRows{w.fontSession(dev, at), w.labRun(dev, at), w.bossRun(dev, at), w.mercTemplate(dev, at)}
}

func (r deviceRows) each(fn func(table string, id int64)) {
	fn("font_sessions", r.font)
	fn("lab_runs", r.lab)
	fn("boss_loot_runs", r.boss)
	fn("merc_icon_templates", r.merc)
}

// row reads a row as canonical jsonb text without the given columns.
func (w *world) row(table string, id int64, without ...string) string {
	w.t.Helper()
	if without == nil {
		without = []string{}
	}
	var out string
	err := w.pool.QueryRow(context.Background(),
		`SELECT (to_jsonb(t) - $2::text[])::text FROM `+table+` t WHERE id = $1`, id, without).Scan(&out)
	if err != nil {
		w.t.Fatalf("read %s %d: %v", table, id, err)
	}
	return out
}

// link reads the device columns of a row: device_id, and pair_code for font sessions.
func (w *world) link(table string, id int64) (deviceID, pairCode *string) {
	w.t.Helper()
	pair := "NULL::text"
	if table == "font_sessions" {
		pair = "pair_code"
	}
	err := w.pool.QueryRow(context.Background(), `SELECT device_id, `+pair+` FROM `+table+` WHERE id = $1`, id).Scan(&deviceID, &pairCode)
	if err != nil {
		w.t.Fatalf("read %s %d: %v", table, id, err)
	}
	return deviceID, pairCode
}

func (w *world) children(table string, id int64) int {
	w.t.Helper()
	sql := map[string]string{
		"font_sessions": `SELECT count(*) FROM font_rounds WHERE session_id = $1`,
		"lab_runs":      `SELECT count(*) FROM lab_run_rooms WHERE run_id = $1`,
	}[table]
	if sql == "" {
		return 0
	}
	var n int
	if err := w.pool.QueryRow(context.Background(), sql, id).Scan(&n); err != nil {
		w.t.Fatal(err)
	}
	return n
}

// deviceRow returns the device as jsonb text, or "" when it does not exist.
func (w *world) deviceRow(fp string) string {
	w.t.Helper()
	var out string
	err := w.pool.QueryRow(context.Background(),
		`SELECT COALESCE((SELECT to_jsonb(d)::text FROM devices d WHERE fingerprint = $1), '')`, fp).Scan(&out)
	if err != nil {
		w.t.Fatal(err)
	}
	return out
}

// snapshot captures every seeded row and device in full.
func (w *world) snapshot() map[string]string {
	w.t.Helper()
	out := map[string]string{}
	for table, ids := range w.ids {
		for _, id := range ids {
			key := fmt.Sprintf("%s/%d", table, id)
			out[key] = w.row(table, id)
			out[key+"/children"] = fmt.Sprint(w.children(table, id))
		}
	}
	for _, fp := range w.devices {
		out["devices/"+fp] = w.deviceRow(fp)
	}
	return out
}

func requireSameSnapshot(t *testing.T, before, after map[string]string) {
	t.Helper()
	for key, want := range before {
		if got := after[key]; got != want {
			t.Errorf("%s changed:\n before %s\n after  %s", key, want, got)
		}
	}
}

// requireUnlinked checks the row lost its device link and nothing else.
func requireUnlinked(t *testing.T, w *world, table string, id int64, before, beforeChildren map[string]string) {
	t.Helper()
	key := fmt.Sprintf("%s/%d", table, id)
	if dev, pair := w.link(table, id); dev != nil || pair != nil {
		t.Errorf("%s: device link kept: device_id=%v pair_code=%v", key, deref(dev), deref(pair))
	}
	if got := w.row(table, id, "device_id", "pair_code"); got != before[key] {
		t.Errorf("%s: other columns changed:\n before %s\n after  %s", key, before[key], got)
	}
	if got := fmt.Sprint(w.children(table, id)); got != beforeChildren[key] {
		t.Errorf("%s: children %s, want %s", key, got, beforeChildren[key])
	}
}

func deref(s *string) string {
	if s == nil {
		return "<nil>"
	}
	return *s
}

// unlinkedState captures rows without their device columns, plus child counts.
func unlinkedState(w *world, rows ...deviceRows) (cols, children map[string]string) {
	cols, children = map[string]string{}, map[string]string{}
	for _, r := range rows {
		r.each(func(table string, id int64) {
			key := fmt.Sprintf("%s/%d", table, id)
			cols[key] = w.row(table, id, "device_id", "pair_code")
			children[key] = fmt.Sprint(w.children(table, id))
		})
	}
	return cols, children
}

func ptr(s string) *string { return &s }

// (a)
func TestPurge_RowsOlderThanTheCutoff_LoseOnlyTheirDeviceLink(t *testing.T) {
	w := newWorld(t)
	dev := w.device("active", recent, deviceSpec{})
	rows := w.rowsFor(dev, old)
	cols, children := unlinkedState(w, rows)
	deviceBefore := w.deviceRow(dev)

	w.purge()

	rows.each(func(table string, id int64) { requireUnlinked(t, w, table, id, cols, children) })
	if got := w.deviceRow(dev); got != deviceBefore {
		t.Errorf("active device changed:\n before %s\n after  %s", deviceBefore, got)
	}
}

// (b)
func TestPurge_NewerRowsOfAnActiveDevice_AreUnchanged(t *testing.T) {
	w := newWorld(t)
	dev := w.device("active", recent, deviceSpec{})
	w.rowsFor(dev, recent)
	before := w.snapshot()

	w.purge()

	requireSameSnapshot(t, before, w.snapshot())
}

// (c)
func TestPurge_InactiveDevices_AreDeletedAndTheirNewerRowsKeptUnlinked(t *testing.T) {
	w := newWorld(t)
	plain := w.device("plain", old, deviceSpec{})
	identified := w.device("identified", old, deviceSpec{alias: ptr("Tester"), role: "beta"})
	// Rows newer than the cutoff: only the device deletion can unlink them.
	plainRows, identifiedRows := w.rowsFor(plain, recent), w.rowsFor(identified, recent)
	cols, children := unlinkedState(w, plainRows, identifiedRows)

	w.purge()

	for _, fp := range []string{plain, identified} {
		if got := w.deviceRow(fp); got != "" {
			t.Errorf("inactive device kept: %s", got)
		}
	}
	for _, r := range []deviceRows{plainRows, identifiedRows} {
		r.each(func(table string, id int64) { requireUnlinked(t, w, table, id, cols, children) })
	}
}

// (c2, order a)
func TestPurge_CandidateRefreshedByUpsertBeforeSelection_IsKeptWithItsRows(t *testing.T) {
	w := newWorld(t)
	ctx := context.Background()
	dev := w.device("identified", old, deviceSpec{alias: ptr("Tester"), role: "beta"})
	w.rowsFor(dev, recent)
	if _, err := device.NewRepository(w.pool).Upsert(ctx, dev, "1.2.3"); err != nil {
		t.Fatal(err)
	}
	rowsBefore := w.snapshot()

	w.purge()

	var alias *string
	var role string
	if err := w.pool.QueryRow(ctx, `SELECT alias, role FROM devices WHERE fingerprint = $1`, dev).Scan(&alias, &role); err != nil {
		t.Fatalf("refreshed device gone: %v", err)
	}
	if deref(alias) != "Tester" || role != "beta" {
		t.Errorf("refreshed device lost its identity: alias=%s role=%s", deref(alias), role)
	}
	requireSameSnapshot(t, rowsBefore, w.snapshot())
}

// waitForLockWait polls until a backend running a statement containing
// fragment is waiting on a lock.
func waitForLockWait(t *testing.T, pool *pgxpool.Pool, fragment string) {
	t.Helper()
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		var n int
		err := pool.QueryRow(context.Background(), `SELECT count(*) FROM pg_stat_activity
			WHERE datname = current_database() AND wait_event_type = 'Lock' AND strpos(query, $1) > 0`, fragment).Scan(&n)
		if err != nil {
			t.Fatal(err)
		}
		if n > 0 {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatalf("no backend waiting on a lock while running %q", fragment)
}

// (c2, order b)
func TestPurge_UpsertWaitingOnTheCandidateLock_CreatesAFreshDeviceWithNoRows(t *testing.T) {
	w := newWorld(t)
	ctx := context.Background()
	dev := w.device("identified", old, deviceSpec{alias: ptr("Tester"), role: "beta"})
	rows := w.rowsFor(dev, recent)

	// Hold the candidate's merc row so Purge stops after locking the device.
	blocker, err := w.pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer blocker.Rollback(ctx)
	if _, err := blocker.Exec(ctx, `SELECT 1 FROM merc_icon_templates WHERE id = $1 FOR UPDATE`, rows.merc); err != nil {
		t.Fatal(err)
	}

	purgeDone := make(chan error, 1)
	go func() {
		_, err := w.purger().Purge(ctx)
		purgeDone <- err
	}()
	waitForLockWait(t, w.pool, "UPDATE merc_icon_templates SET device_id = NULL WHERE device_id = ANY")

	type upsertResult struct {
		d   *device.Device
		err error
	}
	upsertDone := make(chan upsertResult, 1)
	go func() {
		d, err := device.NewRepository(w.pool).Upsert(ctx, dev, "9.9.9")
		upsertDone <- upsertResult{d, err}
	}()
	waitForLockWait(t, w.pool, "INSERT INTO devices")

	if err := blocker.Commit(ctx); err != nil {
		t.Fatal(err)
	}
	if err := <-purgeDone; err != nil {
		t.Fatalf("purge: %v", err)
	}
	up := <-upsertDone
	if up.err != nil {
		t.Fatalf("upsert: %v", up.err)
	}

	var alias *string
	var role string
	var firstSeen time.Time
	if err := w.pool.QueryRow(ctx, `SELECT alias, role, first_seen FROM devices WHERE fingerprint = $1`, dev).Scan(&alias, &role, &firstSeen); err != nil {
		t.Fatal(err)
	}
	if alias != nil || role != "user" || !firstSeen.After(cutoff) {
		t.Errorf("device after purge+upsert: alias=%s role=%s first_seen=%s, want a fresh default device", deref(alias), role, firstSeen)
	}
	rows.each(func(table string, id int64) {
		if d, pair := w.link(table, id); d != nil || pair != nil {
			t.Errorf("%s/%d relinked: device_id=%s pair_code=%s", table, id, deref(d), deref(pair))
		}
	})
}

// (boundary)
func TestPurge_RowsAndDevicesExactlyAtTheCutoff_AreUnchanged(t *testing.T) {
	w := newWorld(t)
	dev := w.device("at-cutoff", cutoff, deviceSpec{alias: ptr("Edge"), role: "beta"})
	w.device("banned-at-cutoff", cutoff, deviceSpec{alias: ptr("Bad"), banned: true})
	w.rowsFor(dev, cutoff)
	before := w.snapshot()

	w.purge()

	requireSameSnapshot(t, before, w.snapshot())
}

// (boundary)
func TestPurge_RowsAndDevicesJustOlderThanTheCutoff_FollowTheRule(t *testing.T) {
	w := newWorld(t)
	justOlder := cutoff.Add(-time.Microsecond)
	active := w.device("active", recent, deviceSpec{})
	rows := w.rowsFor(active, justOlder)
	inactive := w.device("inactive", justOlder, deviceSpec{})
	banned := w.device("banned", justOlder, deviceSpec{alias: ptr("Bad"), banned: true})
	cols, children := unlinkedState(w, rows)

	w.purge()

	rows.each(func(table string, id int64) { requireUnlinked(t, w, table, id, cols, children) })
	if got := w.deviceRow(inactive); got != "" {
		t.Errorf("device just past the cutoff kept: %s", got)
	}
	var alias *string
	if err := w.pool.QueryRow(context.Background(), `SELECT alias FROM devices WHERE fingerprint = $1`, banned).Scan(&alias); err != nil {
		t.Fatal(err)
	}
	if alias != nil {
		t.Errorf("banned device just past the cutoff kept alias %s", *alias)
	}
}

// (rollback)
func TestPurge_FailingLaterStatement_RollsBackEveryChange(t *testing.T) {
	w := newWorld(t)
	ctx := context.Background()
	active := w.device("active", recent, deviceSpec{})
	w.rowsFor(active, old)
	inactive := w.device("inactive", old, deviceSpec{alias: ptr("Tester"), role: "beta"})
	w.rowsFor(inactive, recent)
	w.device("banned", old, deviceSpec{alias: ptr("Bad"), banned: true})

	fn, trigger := strings.ReplaceAll(w.tag, "-", "_")+"_fail", strings.ReplaceAll(w.tag, "-", "_")+"_trg"
	w.exec(`CREATE FUNCTION ` + fn + `() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected delete failure'; END $$`)
	t.Cleanup(func() {
		if _, err := w.pool.Exec(ctx, `DROP TRIGGER IF EXISTS `+trigger+` ON devices`); err != nil {
			t.Error(err)
		}
		if _, err := w.pool.Exec(ctx, `DROP FUNCTION IF EXISTS `+fn+`()`); err != nil {
			t.Error(err)
		}
	})
	w.exec(`CREATE TRIGGER ` + trigger + ` BEFORE DELETE ON devices FOR EACH ROW WHEN (OLD.fingerprint = '` + inactive + `') EXECUTE FUNCTION ` + fn + `()`)
	before := w.snapshot()

	_, err := w.purger().Purge(ctx)

	if err == nil || !strings.Contains(err.Error(), "injected delete failure") {
		t.Fatalf("Purge error = %v, want the injected failure", err)
	}
	requireSameSnapshot(t, before, w.snapshot())
}

// (d)
func TestPurge_InactiveBannedDevice_KeepsOnlyItsBanRecord(t *testing.T) {
	w := newWorld(t)
	ctx := context.Background()
	fp := w.device("banned", old, deviceSpec{alias: ptr("Bad"), role: "beta", banned: true})

	w.purge()

	var alias, appVersion *string
	var role string
	var banned bool
	var firstSeen, lastSeen time.Time
	err := w.pool.QueryRow(ctx, `SELECT alias, app_version, role, banned, first_seen, last_seen FROM devices WHERE fingerprint = $1`, fp).
		Scan(&alias, &appVersion, &role, &banned, &firstSeen, &lastSeen)
	if err != nil {
		t.Fatalf("inactive banned device gone: %v", err)
	}
	if alias != nil || appVersion != nil || role != "user" || !banned || !firstSeen.Equal(old) || !lastSeen.Equal(old) {
		t.Errorf("banned device = alias %s app_version %s role %s banned %v first_seen %s last_seen %s; want only fingerprint, ban and timestamps",
			deref(alias), deref(appVersion), role, banned, firstSeen, lastSeen)
	}
}

// (d)
func TestPurge_ActiveBannedDevice_IsUntouched(t *testing.T) {
	w := newWorld(t)
	fp := w.device("banned", recent, deviceSpec{alias: ptr("Bad"), role: "beta", banned: true})
	before := w.deviceRow(fp)

	w.purge()

	if got := w.deviceRow(fp); got != before {
		t.Errorf("active banned device changed:\n before %s\n after  %s", before, got)
	}
}

// (e)
func TestPurge_SecondRun_ChangesNothing(t *testing.T) {
	w := newWorld(t)
	active := w.device("active", recent, deviceSpec{})
	w.rowsFor(active, old)
	inactive := w.device("inactive", old, deviceSpec{})
	w.rowsFor(inactive, recent)
	w.device("banned", old, deviceSpec{alias: ptr("Bad"), banned: true})
	if first := w.purge(); first == (Counts{}) {
		t.Fatal("first purge changed nothing; the seed does not exercise the policy")
	}
	before := w.snapshot()

	second := w.purge()

	if second != (Counts{}) {
		t.Errorf("second purge counts = %+v, want zero", second)
	}
	requireSameSnapshot(t, before, w.snapshot())
}

// (f)
func TestPurge_AnonymisedBossLootRun_StillCountsInCollectiveStats(t *testing.T) {
	w := newWorld(t)
	dev := w.device("inactive", old, deviceSpec{})
	w.bossRun(dev, old)

	w.purge()

	stats, err := bossloot.NewRepository(w.pool).StatsCumulative(context.Background(), "",
		bossloot.Profile{Boss: "catarina", ProfileKey: w.tag, ProfileVersion: 1}, true)
	if err != nil {
		t.Fatal(err)
	}
	if stats.Runs != 1 || stats.Items["Urn"].Drops != 2 || stats.Items["Urn"].EligibleRuns != 1 {
		t.Errorf("collective stats = %+v, want the anonymised run counted", stats)
	}
}

// (f)
func TestPurge_AnonymisedMercTemplate_IsStillInTheCorpus(t *testing.T) {
	w := newWorld(t)
	dev := w.device("inactive", old, deviceSpec{})
	w.mercTemplate(dev, old)

	w.purge()

	corpus, err := mercenary.NewRepository(w.pool).Corpus(context.Background(), 1)
	if err != nil {
		t.Fatal(err)
	}
	found := 0
	for _, s := range corpus.Templates {
		if s.Key.Family == w.tag {
			found++
		}
	}
	if found != 1 {
		t.Errorf("corpus serves %d templates of the anonymised family, want 1", found)
	}
}

// (f)
func TestPurge_DeviceStats_CountTheKeptBannedDevice(t *testing.T) {
	w := newWorld(t)
	w.device("banned", old, deviceSpec{alias: ptr("Bad"), banned: true})
	w.device("inactive", old, deviceSpec{})
	repo := device.NewRepository(w.pool)
	before, err := repo.Stats(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	w.purge()

	after, err := repo.Stats(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if after.Banned != before.Banned || after.Total != before.Total-1 {
		t.Errorf("stats before %+v after %+v: want the banned device kept and the inactive one gone", before, after)
	}
}
