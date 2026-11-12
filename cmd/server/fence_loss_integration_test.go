//go:build integration

package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/bossloot"
	"profitofexile/internal/league"
)

const fenceChildEnv = "POE_SERVER_FENCE_CHILD"

// TestMain lets the fence-loss tests re-execute this test binary as the real
// server: with fenceChildEnv set it runs main() instead of the tests.
func TestMain(m *testing.M) {
	if os.Getenv(fenceChildEnv) == "1" {
		main()
		os.Exit(0)
	}
	os.Exit(m.Run())
}

// fenceLossExitBound is how long a child may take to exit after its fence
// backend is terminated: one check interval plus one check timeout to detect,
// the 10 s graceful HTTP shutdown bound, and 5 s slack.
const fenceLossExitBound = fenceCheckInterval + fenceCheckTimeout + 10*time.Second + 5*time.Second

type lockedBuffer struct {
	mu  sync.Mutex
	buf bytes.Buffer
}

func (b *lockedBuffer) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.Write(p)
}

func (b *lockedBuffer) String() string {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.buf.String()
}

type fencedChild struct {
	pool      *pgxpool.Pool
	leagueID  string
	deviceID  string
	baseURL   string
	fencePID  int32
	output    *lockedBuffer
	exited    chan error
	exitErr   error
	hasExited bool
}

// waitExit returns the child's exit error once it exits within bound.
func (c *fencedChild) waitExit(t *testing.T, bound time.Duration) error {
	t.Helper()
	if c.hasExited {
		return c.exitErr
	}
	select {
	case err := <-c.exited:
		c.exitErr, c.hasExited = err, true
		return err
	case <-time.After(bound):
		t.Fatalf("server did not exit within %s of fence loss", bound)
		return nil
	}
}

func (c *fencedChild) terminateFenceBackend(t *testing.T) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	var terminated bool
	if err := c.pool.QueryRow(ctx, `SELECT pg_terminate_backend($1)`, c.fencePID).Scan(&terminated); err != nil || !terminated {
		t.Fatalf("pg_terminate_backend(%d) = %v, err=%v", c.fencePID, terminated, err)
	}
}

func (c *fencedChild) healthStatus() (int, error) {
	client := http.Client{Timeout: 2 * time.Second}
	resp, err := client.Get(c.baseURL + "/api/health")
	if err != nil {
		return 0, err
	}
	resp.Body.Close()
	return resp.StatusCode, nil
}

func (c *fencedChild) submitBossLoot() (*http.Response, error) {
	raw, err := json.Marshal(bossloot.Submit{
		Profile: bossloot.Profile{Boss: c.leagueID, ProfileKey: "Normal", ProfileVersion: 1},
		Mode:    "forceNew",
		Items:   bossloot.Items{Quantities: map[string]int64{"Urn": 1}, Counted: []string{"Urn"}},
	})
	if err != nil {
		return nil, err
	}
	req, err := http.NewRequest(http.MethodPost, c.baseURL+"/api/boss-loot/runs", bytes.NewReader(raw))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Device-ID", c.deviceID)
	client := http.Client{Timeout: 5 * time.Second}
	return client.Do(req)
}

// leagueRowCounts counts the test league's rows in every league-scoped table
// plus boss_loot_runs.
func (c *fencedChild) leagueRowCounts(t *testing.T) map[string]int64 {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	counts := map[string]int64{}
	for _, table := range append(league.ScopedTables(), "boss_loot_runs") {
		var n int64
		if err := c.pool.QueryRow(ctx, `SELECT count(*) FROM `+table+` WHERE league = $1`, c.leagueID).Scan(&n); err != nil {
			t.Fatalf("count %s: %v", table, err)
		}
		counts[table] = n
	}
	return counts
}

// startFencedServer activates a unique test league and boots the real server
// main() in a child process against it, returning once the child is healthy
// and its fence backend is located.
func startFencedServer(t *testing.T) *fencedChild {
	t.Helper()
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		t.Fatal("DATABASE_URL required")
	}
	ctx := context.Background()
	pool := newTestPool(t, databaseURL)

	// (1) Refuse anything but a disposable integration database, through the
	// shared guard the rollover test uses, before any write.
	var database string
	if err := pool.QueryRow(ctx, `SELECT current_database()`).Scan(&database); err != nil {
		t.Fatalf("read current database: %v", err)
	}
	root, err := filepath.Abs("../..")
	if err != nil {
		t.Fatal(err)
	}
	databaseGuard := exec.Command("bash", "-c", `source "$1"`, "integration database guard", filepath.Join(root, "scripts/lib/integration-env.sh"))
	databaseGuard.Env = append(os.Environ(), "INTEGRATION_DB="+database)
	if output, err := databaseGuard.CombinedOutput(); err != nil {
		t.Fatalf("fence-loss tests require a supported disposable integration database: %v\n%s", err, output)
	}

	// (2) Save runtime and registry state.
	var savedRuntime, savedRegistry string
	if err := pool.QueryRow(ctx, `SELECT to_jsonb(r)::text FROM runtime_config r WHERE singleton`).Scan(&savedRuntime); err != nil {
		t.Fatalf("save runtime_config: %v", err)
	}
	if err := pool.QueryRow(ctx, `SELECT jsonb_agg(to_jsonb(l) ORDER BY id)::text FROM leagues l`).Scan(&savedRegistry); err != nil {
		t.Fatalf("save leagues: %v", err)
	}

	leagueID := fmt.Sprintf("POE118 Fence %d", time.Now().UnixNano())
	child := &fencedChild{
		pool:     pool,
		leagueID: leagueID,
		deviceID: fmt.Sprintf("%x", sha256.Sum256([]byte(leagueID))),
		output:   &lockedBuffer{},
		exited:   make(chan error, 1),
	}
	var cmd *exec.Cmd

	// (3) Restore on every exit path, registered before the first write.
	t.Cleanup(func() {
		if cmd != nil && cmd.Process != nil && !child.hasExited {
			_ = cmd.Process.Kill()
			select {
			case err := <-child.exited:
				child.exitErr, child.hasExited = err, true
			case <-time.After(30 * time.Second):
				t.Error("killed server child did not exit")
			}
		}
		if t.Failed() {
			t.Logf("server child output:\n%s", child.output.String())
		}
		restoreFenceFixture(t, pool, savedRuntime, savedRegistry, leagueID, child.deviceID)
	})

	// (4) Activate the test league while both fences are free.
	adminCtx, adminCancel := context.WithTimeout(ctx, 30*time.Second)
	defer adminCancel()
	if err := league.RunAdmin(adminCtx, pool, false, func(tx pgx.Tx) error {
		if _, err := league.Prepare(adminCtx, tx, league.PrepareInput{ID: leagueID}); err != nil {
			return err
		}
		_, err := league.Activate(adminCtx, tx, leagueID)
		return err
	}); err != nil {
		t.Fatalf("prepare and activate %q: %v", leagueID, err)
	}

	// (5) Boot the real main() as a child on an OS-chosen port.
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	port := listener.Addr().(*net.TCPAddr).Port
	listener.Close()
	child.baseURL = fmt.Sprintf("http://127.0.0.1:%d", port)

	cmd = exec.Command(os.Args[0], "-test.run=^$")
	var env []string
	for _, kv := range os.Environ() {
		name, _, _ := strings.Cut(kv, "=")
		switch name {
		case "DATABASE_URL", "PORT", "MERCURE_URL", "TRADE_ENABLED", "EXPECTED_LEAGUE", "ICON_CACHE_DIR", fenceChildEnv:
			continue
		}
		env = append(env, kv)
	}
	cmd.Env = append(env,
		fenceChildEnv+"=1",
		"DATABASE_URL="+databaseURL,
		fmt.Sprintf("PORT=%d", port),
		"MERCURE_URL=",
		"ICON_CACHE_DIR="+t.TempDir(),
	)
	cmd.Stdout, cmd.Stderr = child.output, child.output
	if err := cmd.Start(); err != nil {
		t.Fatalf("start server child: %v", err)
	}
	go func() { child.exited <- cmd.Wait() }()

	healthDeadline := time.Now().Add(fenceAcquireMaxWait + 15*time.Second)
	for {
		if status, err := child.healthStatus(); err == nil && status == http.StatusOK {
			break
		}
		select {
		case err := <-child.exited:
			child.exitErr, child.hasExited = err, true
			t.Fatalf("server child exited before healthy: %v\n%s", err, child.output.String())
		default:
		}
		if time.Now().After(healthDeadline) {
			t.Fatalf("server child not healthy within %s\n%s", fenceAcquireMaxWait+15*time.Second, child.output.String())
		}
		time.Sleep(100 * time.Millisecond)
	}

	// pg_locks is cluster-wide and ServerLockKey is the same in every database,
	// so filter to this database: another lane's or the dev server's fence must
	// neither break the one-row assertion nor be terminated.
	key := league.ServerLockKey()
	rows, err := pool.Query(ctx, `SELECT pid, pid <> pg_backend_pid() FROM pg_locks
		WHERE locktype = 'advisory'
		  AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
		  AND classid = $1 AND objid = $2 AND objsubid = 1 AND granted`,
		uint32(uint64(key)>>32), uint32(uint64(key)))
	if err != nil {
		t.Fatalf("locate fence backend: %v", err)
	}
	var pids []int32
	for rows.Next() {
		var pid int32
		var foreign bool
		if err := rows.Scan(&pid, &foreign); err != nil {
			t.Fatal(err)
		}
		if !foreign {
			t.Fatalf("server fence is held by the test's own backend %d", pid)
		}
		pids = append(pids, pid)
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if len(pids) != 1 {
		t.Fatalf("server fence holders in this database = %v, want exactly one", pids)
	}
	child.fencePID = pids[0]
	return child
}

func restoreFenceFixture(t *testing.T, pool *pgxpool.Pool, savedRuntime, savedRegistry, leagueID, deviceID string) {
	t.Helper()
	restoreCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	tx, err := pool.Begin(restoreCtx)
	if err != nil {
		t.Error("begin restoration:", err)
		return
	}
	defer tx.Rollback(restoreCtx)
	if _, err := tx.Exec(restoreCtx, `UPDATE runtime_config r SET active_league=s.active_league, revision=s.revision, updated_at=s.updated_at FROM jsonb_populate_record(NULL::runtime_config,$1::jsonb) s WHERE r.singleton`, savedRuntime); err != nil {
		t.Error("restore runtime:", err)
		return
	}
	if _, err := tx.Exec(restoreCtx, `UPDATE leagues l SET display_name=s.display_name, starts_at=s.starts_at, ends_at=s.ends_at, collection_state=s.collection_state, prepared_at=s.prepared_at, activated_at=s.activated_at, archived_at=s.archived_at, created_at=s.created_at, updated_at=s.updated_at FROM jsonb_populate_recordset(NULL::leagues,$1::jsonb) s WHERE l.id=s.id`, savedRegistry); err != nil {
		t.Error("restore registry:", err)
		return
	}
	for _, table := range append(league.ScopedTables(), "boss_loot_runs") {
		if _, err := tx.Exec(restoreCtx, `DELETE FROM `+table+` WHERE league = $1`, leagueID); err != nil {
			t.Errorf("remove %s fixtures: %v", table, err)
			return
		}
	}
	if _, err := tx.Exec(restoreCtx, `DELETE FROM leagues WHERE id = $1`, leagueID); err != nil {
		t.Error("remove test league:", err)
		return
	}
	if _, err := tx.Exec(restoreCtx, `DELETE FROM devices WHERE fingerprint = $1`, deviceID); err != nil {
		t.Error("remove test device:", err)
		return
	}
	if err := tx.Commit(restoreCtx); err != nil {
		t.Error("commit restoration:", err)
		return
	}
	var restored bool
	if err := pool.QueryRow(restoreCtx, `SELECT (SELECT to_jsonb(r) FROM runtime_config r WHERE singleton)=$1::jsonb
		AND (SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM leagues l)=$2::jsonb
		AND NOT EXISTS (SELECT 1 FROM boss_loot_runs WHERE league=$3)
		AND NOT EXISTS (SELECT 1 FROM devices WHERE fingerprint=$4)`, savedRuntime, savedRegistry, leagueID, deviceID).Scan(&restored); err != nil || !restored {
		t.Errorf("runtime/registry and fixture restoration: restored=%v err=%v", restored, err)
		return
	}
	for _, key := range []int64{league.ServerLockKey(), league.RuntimeLockKey()} {
		lock, err := league.AcquireProcessLock(restoreCtx, pool, key)
		if err != nil {
			t.Error("fence still held after cleanup:", err)
			continue
		}
		lock.Release()
	}
}

// TestServerExitsNonZeroAfterFenceBackendTerminated pins that losing the fence
// connection, and only that, makes the server exit 1.
func TestServerExitsNonZeroAfterFenceBackendTerminated(t *testing.T) {
	child := startFencedServer(t)

	// The child must outlive several fence checks while the fence is held, so
	// an exit observed below is caused by the drop, not by a timer.
	stayUp := fenceCheckInterval + fenceCheckTimeout + time.Second
	select {
	case err := <-child.exited:
		child.exitErr, child.hasExited = err, true
		t.Fatalf("server exited with its fence held: %v\n%s", err, child.output.String())
	case <-time.After(stayUp):
	}
	if status, err := child.healthStatus(); err != nil || status != http.StatusOK {
		t.Fatalf("health after %s with fence held = %d, err=%v; want 200", stayUp, status, err)
	}

	child.terminateFenceBackend(t)
	err := child.waitExit(t, fenceLossExitBound)
	var exitErr *exec.ExitError
	if !errors.As(err, &exitErr) || exitErr.ExitCode() != 1 {
		t.Fatalf("server exit after fence loss = %v, want exit code 1\n%s", err, child.output.String())
	}
	if !strings.Contains(child.output.String(), "server fence lost") {
		t.Fatalf("server output lacks the fence-loss log line:\n%s", child.output.String())
	}
}

// TestServerWritesNoLeagueRowsAfterFenceLoss pins that writes cease by the time
// the process exits after fence loss: the per-table counts of the test league
// stay constant after exit, and the endpoint no longer accepts connections.
// It does not prove that nothing commits between the drop and the exit. Two
// windows are accepted (ADR-009, POE-118 amendment): drop → detection, up to
// fenceCheckInterval + fenceCheckTimeout (closed only by a per-write fencing
// token), and detection → exit, the graceful srv.Shutdown drain, in which
// requests already in flight keep their own request context and can commit.
// The post-exit baseline taken here cannot see writes from either window.
func TestServerWritesNoLeagueRowsAfterFenceLoss(t *testing.T) {
	child := startFencedServer(t)

	// Positive control: the counter sees this server's writes.
	before := child.leagueRowCounts(t)["boss_loot_runs"]
	resp, err := child.submitBossLoot()
	if err != nil {
		t.Fatalf("boss-loot submit with fence held: %v", err)
	}
	resp.Body.Close()
	if resp.StatusCode != http.StatusCreated {
		t.Fatalf("boss-loot submit with fence held = %d, want 201", resp.StatusCode)
	}
	countDeadline := time.Now().Add(10 * time.Second)
	for child.leagueRowCounts(t)["boss_loot_runs"] != before+1 {
		if time.Now().After(countDeadline) {
			t.Fatalf("boss_loot_runs for %q did not rise from %d to %d", child.leagueID, before, before+1)
		}
		time.Sleep(100 * time.Millisecond)
	}

	child.terminateFenceBackend(t)
	child.waitExit(t, fenceLossExitBound)
	atExit := child.leagueRowCounts(t)
	time.Sleep(3 * time.Second) // post-exit grace window: the observation itself
	afterGrace := child.leagueRowCounts(t)
	for table, n := range atExit {
		if afterGrace[table] != n {
			t.Errorf("%s rows for %q changed after exit: %d → %d", table, child.leagueID, n, afterGrace[table])
		}
	}
	if resp, err := child.submitBossLoot(); err == nil {
		resp.Body.Close()
		t.Fatalf("boss-loot submit after exit reached a server (status %d), want connection failure", resp.StatusCode)
	}
}
