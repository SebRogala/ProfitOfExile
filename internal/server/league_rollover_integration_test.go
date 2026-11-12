//go:build integration

package server

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/collector"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
)

// TestCommittedRolloverRejectsStaleSubmits exercises committed CLI transactions
// and rebuilt production consumers. It deliberately retains historical fixtures;
// destructive wipe/archive rehearsal belongs to the operational runbook.
func TestCommittedRolloverRejectsStaleSubmits(t *testing.T) {
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		t.Skip("DATABASE_URL required")
	}
	root, err := filepath.Abs("../..")
	rolloverMust(t, err)
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, databaseURL)
	if err != nil {
		t.Fatal("create integration pool")
	}
	defer pool.Close()
	var database string
	rolloverMust(t, pool.QueryRow(ctx, `SELECT current_database()`).Scan(&database))
	// Broad integration discovery uses the shared runner's disposable DB rule.
	// Validate the actual connected database through that rule's owner, without
	// executing any database command. The dedicated runner keeps its lane lease.
	databaseGuard := exec.Command("bash", "-c", `source "$1"`, "integration database guard", filepath.Join(root, "scripts/lib/integration-env.sh"))
	databaseGuard.Env = append(os.Environ(), "INTEGRATION_DB="+database)
	if output, err := databaseGuard.CombinedOutput(); err != nil {
		t.Fatalf("rollover requires a supported disposable integration database: %v\n%s", err, output)
	}
	savedScope, err := league.Resolve(ctx, pool)
	rolloverMust(t, err)
	var savedRuntime, savedRegistry string
	rolloverMust(t, pool.QueryRow(ctx, `SELECT to_jsonb(r)::text FROM runtime_config r WHERE singleton`).Scan(&savedRuntime))
	rolloverMust(t, pool.QueryRow(ctx, `SELECT jsonb_agg(to_jsonb(l) ORDER BY id)::text FROM leagues l`).Scan(&savedRegistry))
	outgoing := fmt.Sprintf("WI7 Outgoing %d", time.Now().UnixNano())
	incoming := outgoing + " Incoming"
	var locks []*league.ProcessLock
	release := func() {
		t.Helper()
		for _, lock := range locks {
			lock.Release()
		}
		locks = nil
	}
	// Register restoration before any committed fixture, even CLI preparation.
	defer func() {
		release()
		cleanupCtx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		tx, err := pool.Begin(cleanupCtx)
		if err != nil {
			t.Error("begin restoration:", err)
			return
		}
		defer tx.Rollback(cleanupCtx)
		// Restore timestamps as well as identity/revision and all lifecycle fields.
		_, err = tx.Exec(cleanupCtx, `UPDATE runtime_config r SET active_league=s.active_league, revision=s.revision, updated_at=s.updated_at FROM jsonb_populate_record(NULL::runtime_config,$1::jsonb) s WHERE r.singleton`, savedRuntime)
		if err != nil {
			t.Error("restore runtime:", err)
			return
		}
		_, err = tx.Exec(cleanupCtx, `UPDATE leagues l SET display_name=s.display_name, starts_at=s.starts_at, ends_at=s.ends_at, collection_state=s.collection_state, prepared_at=s.prepared_at, activated_at=s.activated_at, archived_at=s.archived_at, created_at=s.created_at, updated_at=s.updated_at FROM jsonb_populate_recordset(NULL::leagues,$1::jsonb) s WHERE l.id=s.id`, savedRegistry)
		if err != nil {
			t.Error("restore registry:", err)
			return
		}
		for _, table := range []string{"trade_lookups", "gem_snapshots", "leagues"} {
			column := "league"
			if table == "leagues" {
				column = "id"
			}
			if _, err := tx.Exec(cleanupCtx, `DELETE FROM `+table+` WHERE `+column+` IN ($1,$2)`, outgoing, incoming); err != nil {
				t.Error("remove fixtures:", err)
				return
			}
		}
		if err := tx.Commit(cleanupCtx); err != nil {
			t.Error("commit restoration:", err)
			return
		}
		var restored bool
		err = pool.QueryRow(cleanupCtx, `SELECT (SELECT to_jsonb(r) FROM runtime_config r WHERE singleton)=$1::jsonb AND (SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM leagues l)=$2::jsonb AND NOT EXISTS (SELECT 1 FROM gem_snapshots WHERE league IN ($3,$4)) AND NOT EXISTS (SELECT 1 FROM trade_lookups WHERE league IN ($3,$4))`, savedRuntime, savedRegistry, outgoing, incoming).Scan(&restored)
		if err != nil || !restored {
			t.Errorf("exact runtime/registry and fixture restoration: restored=%v err=%v", restored, err)
			return
		}
		// A separate connection can acquire both fences after cleanup.
		for _, key := range []int64{league.ServerLockKey(), league.RuntimeLockKey()} {
			lock, err := league.AcquireProcessLock(cleanupCtx, pool, key)
			if err != nil {
				t.Error("cleanup fence still held:", err)
				continue
			}
			lock.Release()
		}
		t.Log("RESTORED: exact runtime identity/revision/timestamp and registry; zero fixture rows; both fences released")
	}()

	// Build the real CLI, rather than substituting its dispatch dependencies.
	binary := filepath.Join(t.TempDir(), "league")
	build := exec.Command("go", "build", "-buildvcs=false", "-o", binary, "./cmd/league")
	build.Dir = root
	if output, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build league CLI: %v\n%s", err, output)
	}
	cli := func(wantSuccess bool, args ...string) string {
		t.Helper()
		commandCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		output, err := exec.CommandContext(commandCtx, binary, args...).CombinedOutput()
		if (err == nil) != wantSuccess {
			t.Fatalf("league %s: success=%v, want %v\n%s", args[0], err == nil, wantSuccess, output)
		}
		t.Logf("CLI %s: %s", args[0], strings.TrimSpace(string(output)))
		return string(output)
	}
	cli(true, "prepare", outgoing)
	cli(true, "activate", outgoing)

	boot := func(wantID string, wantRevision int64) (league.Scope, league.Scope, http.Handler, *trade.TradeCache, *collector.Repository, *lab.Repository, *LeagueEventGuard) {
		t.Helper()
		serverScope, err := league.Resolve(ctx, pool)
		rolloverMust(t, err)
		serverLock, err := league.AcquireProcessLock(ctx, pool, league.ServerLockKey())
		rolloverMust(t, err)
		locks = append(locks, serverLock)
		collectorScope, err := league.Resolve(ctx, pool)
		rolloverMust(t, err)
		collectorLock, err := league.AcquireProcessLock(ctx, pool, league.RuntimeLockKey())
		rolloverMust(t, err)
		locks = append(locks, collectorLock)
		if serverScope.ID() != wantID || serverScope.Revision() != wantRevision || collectorScope != serverScope {
			t.Fatalf("reconstructed scopes: server=%v collector=%v want=%s revision=%d", serverScope, collectorScope, wantID, wantRevision)
		}
		tradeRepo := trade.NewRepository(pool)
		cache := trade.NewTradeCache(32, serverScope)
		warm, err := tradeRepo.LatestLookups(ctx, serverScope, 24)
		rolloverMust(t, err)
		cache.Warm(warm)
		labRepo := lab.NewRepository(pool)
		router := NewRouter(pool, nil, RouterConfig{Pool: pool, LabRepo: labRepo, LabCache: lab.NewCache(serverScope), TradeCache: cache, TradeRepo: tradeRepo, KnownGem: tradeRepo.KnownGem, League: serverScope, FenceChecker: serverLock, IconCacheDir: t.TempDir()})
		t.Logf("BOOT: server and fixture collector resolved %s revision %d with actual process fences", wantID, wantRevision)
		return serverScope, collectorScope, router, cache, collector.NewRepository(pool), labRepo, NewLeagueEventGuard(serverScope)
	}
	oldScope, oldCollectorScope, oldRouter, oldCache, oldCollector, _, _ := boot(outgoing, savedScope.Revision()+1)
	gem := "Arc of Surging"
	key := trade.CacheKey(gem, "20/20")
	oldTime := time.Now().UTC().Add(-time.Minute).Truncate(time.Microsecond)
	oldTrade := trade.TradeLookupResult{Gem: gem, Variant: "20/20", League: outgoing, PriceFloor: 811, Total: 11, FetchedAt: oldTime}
	rolloverMust(t, trade.NewRepository(pool).InsertTradeLookup(ctx, oldScope, &oldTrade, "fixture"))
	oldCache.Set(key, &oldTrade)
	oldGem := collector.GemSnapshot{Name: gem, Variant: "20/20", Chaos: 911, Listings: 17, IsTransfigured: true, GemColor: "BLUE"}
	count, err := oldCollector.InsertGemSnapshots(ctx, oldCollectorScope, oldTime, []collector.GemSnapshot{oldGem})
	rolloverMust(t, err)
	if count != 1 {
		t.Fatalf("outgoing collector inserted %d rows, want 1", count)
	}
	var oldMarketRow, oldTradeRow string
	rolloverMust(t, pool.QueryRow(ctx, `SELECT to_jsonb(g)::text FROM gem_snapshots g WHERE league=$1`, outgoing).Scan(&oldMarketRow))
	rolloverMust(t, pool.QueryRow(ctx, `SELECT to_jsonb(l)::text FROM trade_lookups l WHERE league=$1`, outgoing).Scan(&oldTradeRow))
	status := rolloverRequest(t, oldRouter, "GET", "/api/analysis/status", nil)
	if status.Code != http.StatusOK || !strings.Contains(status.Body.String(), `"league":"`+outgoing+`"`) {
		t.Fatalf("outgoing status: %d %s", status.Code, status.Body)
	}

	blocked := cli(false, "activate", incoming)
	if !strings.Contains(blocked, league.ErrCollectorRunning.Error()) {
		t.Fatalf("activation refusal was not the runtime fence: %s", blocked)
	}
	stillOld, err := league.Resolve(ctx, pool)
	rolloverMust(t, err)
	if stillOld != oldScope {
		t.Fatalf("blocked activation changed committed runtime: %v", stillOld)
	}
	release()
	cli(true, "prepare", incoming)
	cli(true, "activate", incoming)
	scope, collectorScope, router, cache, writer, reader, guard := boot(incoming, oldScope.Revision()+1)
	status = rolloverRequest(t, router, "GET", "/api/analysis/status", nil)
	var health struct {
		League      string `json:"league"`
		Cached      bool   `json:"cached"`
		Transfigure int    `json:"transfigure"`
	}
	rolloverMust(t, json.Unmarshal(status.Body.Bytes(), &health))
	if status.Code != http.StatusOK || health.League != incoming || health.Cached || health.Transfigure != 0 {
		t.Fatalf("new status is not cold incoming scope: %d %s", status.Code, status.Body)
	}
	cold, coldTime, err := reader.LatestGemPrices(ctx, scope)
	rolloverMust(t, err)
	rolling, err := writer.QueryGemSnapshots(ctx, collectorScope, 24)
	rolloverMust(t, err)
	if len(cold) != 0 || !coldTime.IsZero() || len(rolling) != 0 || cache.Len() != 0 {
		t.Fatalf("cold incoming scope leaked outgoing data: prices=%v time=%v rolling=%v cache=%v", cold, coldTime, rolling, cache.GetSnapshot())
	}
	compare := func(wantPrice float64) {
		t.Helper()
		response := rolloverRequest(t, router, "GET", "/api/analysis/compare?gems="+url.QueryEscape(gem)+"&variant=20%2F20", nil)
		var body struct {
			Count int `json:"count"`
			Data  []struct {
				Name  string                   `json:"transfiguredName"`
				Trade *trade.TradeLookupResult `json:"trade"`
			} `json:"data"`
		}
		rolloverMust(t, json.Unmarshal(response.Body.Bytes(), &body))
		if response.Code != http.StatusOK || body.Count != 1 || len(body.Data) != 1 || body.Data[0].Name != gem {
			t.Fatalf("compare fixture missing: %d %s", response.Code, response.Body)
		}
		enrichment := body.Data[0].Trade
		if wantPrice == 0 && enrichment != nil {
			t.Fatalf("outgoing trade enrichment leaked: %+v", enrichment)
		}
		if wantPrice != 0 && (enrichment == nil || enrichment.League != incoming || enrichment.PriceFloor != wantPrice) {
			t.Fatalf("incoming enrichment: %+v, want price %v league %s", enrichment, wantPrice, incoming)
		}
	}
	compare(0)
	t.Log("ASSERT: incoming status and scoped market reads cold; compare has no outgoing enrichment")

	before := cache.GetSnapshot()
	stale := oldTrade
	stale.League = "" // Installed clients convey identity only through the raw-space URL.
	stale.TradeURL = "https://www.pathofexile.com/trade/search/" + outgoing + "/old-query"
	stale.PriceFloor = 733
	stale.FetchedAt = time.Now().UTC().Truncate(time.Microsecond)
	response := rolloverRequest(t, router, "POST", "/api/trade/submit", stale)
	expected, err := json.Marshal(map[string]string{"error": "league_mismatch", "league": incoming})
	rolloverMust(t, err)
	if response.Code != http.StatusConflict || strings.TrimSpace(response.Body.String()) != string(expected) {
		t.Fatalf("legacy stale submit: %d %s, want 409 %s", response.Code, response.Body, expected)
	}
	if !reflect.DeepEqual(cache.GetSnapshot(), before) {
		t.Fatal("stale submit mutated incoming cache")
	}
	compare(0)
	if guard.AcceptRaw([]byte(fmt.Sprintf(`{"topic":"poe/collector/gems","league":%q,"leagueRevision":%d}`, outgoing, oldScope.Revision()))) {
		t.Fatal("new event guard accepted outgoing event")
	}
	if guard.Accept(incoming, true, oldScope.Revision(), true) {
		t.Fatal("new event guard accepted outgoing revision")
	}
	if !guard.Accept(incoming, true, scope.Revision(), true) || guard.Rejected() != 2 {
		t.Fatalf("guard stamp acceptance/rejection count = %d, want 2", guard.Rejected())
	}

	fresh := stale
	fresh.League = incoming
	fresh.TradeURL = "https://www.pathofexile.com/trade/search/" + incoming + "/new-query"
	fresh.PriceFloor = 29
	fresh.FetchedAt = stale.FetchedAt.Add(time.Microsecond)
	response = rolloverRequest(t, router, "POST", "/api/trade/submit", fresh)
	if response.Code != http.StatusNoContent || response.Body.Len() != 0 {
		t.Fatalf("incoming submit: %d %s", response.Code, response.Body)
	}
	compare(29)
	// A late stale contribution must also preserve an already populated cache.
	before = cache.GetSnapshot()
	response = rolloverRequest(t, router, "POST", "/api/trade/submit", stale)
	if response.Code != http.StatusConflict || strings.TrimSpace(response.Body.String()) != string(expected) || !reflect.DeepEqual(cache.GetSnapshot(), before) {
		t.Fatalf("late stale submit changed populated cache: %d %s cache=%v", response.Code, response.Body, cache.GetSnapshot())
	}
	compare(29)
	// Persistence is asynchronous in TradeSubmit. Wait for its real durable outcome,
	// then observe absence of the rejected row through the entire insert deadline.
	deadline := time.Now().Add(7 * time.Second)
	var persisted int
	for time.Now().Before(deadline) {
		rolloverMust(t, pool.QueryRow(ctx, `SELECT count(*) FROM trade_lookups WHERE league=$1 AND gem=$2 AND variant='20/20' AND time=$3 AND price_floor=29 AND source='desktop'`, incoming, gem, fresh.FetchedAt).Scan(&persisted))
		if persisted == 1 {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	if persisted != 1 {
		t.Fatal("accepted incoming contribution never persisted under incoming identity")
	}
	staleDeadline := time.Now().Add(6 * time.Second)
	for {
		var staleRows int
		rolloverMust(t, pool.QueryRow(ctx, `SELECT count(*) FROM trade_lookups WHERE league IN ($1,$2) AND price_floor=733`, outgoing, incoming).Scan(&staleRows))
		if staleRows != 0 {
			t.Fatalf("rejected stale contribution persisted: %d rows", staleRows)
		}
		if time.Now().After(staleDeadline) {
			break
		}
		time.Sleep(50 * time.Millisecond)
	}
	t.Log("ASSERT: legacy stale submit exact 409, cache unchanged, no durable stale row; incoming submit enriched and persisted; outgoing events rejected")

	// Keep the outgoing snapshot newer: dropping the inner MAX(time) league
	// predicate in LatestGemPrices must lose the incoming row and fail here.
	newTime := oldTime.Add(-30 * time.Second)
	newGem := oldGem
	newGem.Chaos, newGem.Listings = 31, 23
	count, err = writer.InsertGemSnapshots(ctx, collectorScope, newTime, []collector.GemSnapshot{newGem})
	rolloverMust(t, err)
	if count != 1 {
		t.Fatalf("incoming collector inserted %d rows, want 1", count)
	}
	rolling, err = writer.QueryGemSnapshots(ctx, collectorScope, 24)
	rolloverMust(t, err)
	if len(rolling) != 1 || rolling[0].Name != gem || rolling[0].Chaos != 31 || rolling[0].Listings != 23 || !rolling[0].Time.Equal(newTime) {
		t.Fatalf("incoming rolling reads mixed leagues: %+v", rolling)
	}
	prices, latestTime, err := reader.LatestGemPrices(ctx, scope)
	rolloverMust(t, err)
	if len(prices) != 1 || prices[0].Name != gem || prices[0].Chaos != 31 || !latestTime.Equal(newTime) {
		t.Fatalf("incoming latest price identity/value/time: %+v at %v", prices, latestTime)
	}
	history, err := writer.QueryGemSnapshots(ctx, league.Historical(outgoing), 24)
	rolloverMust(t, err)
	if len(history) != 1 || history[0].Chaos != 911 || history[0].Listings != 17 || !history[0].Time.Equal(oldTime) {
		t.Fatalf("outgoing historical market fixture changed: %+v", history)
	}
	trades, err := trade.NewRepository(pool).LatestLookups(ctx, league.Historical(outgoing), 24)
	rolloverMust(t, err)
	if len(trades) != 1 || trades[0].Gem != gem || trades[0].PriceFloor != 811 || !trades[0].FetchedAt.Equal(oldTime) {
		t.Fatalf("outgoing historical trade fixture changed: %+v", trades)
	}
	var fixturesUnchanged bool
	rolloverMust(t, pool.QueryRow(ctx, `SELECT (SELECT to_jsonb(g) FROM gem_snapshots g WHERE league=$1)=$2::jsonb AND (SELECT to_jsonb(l) FROM trade_lookups l WHERE league=$1)=$3::jsonb`, outgoing, oldMarketRow, oldTradeRow).Scan(&fixturesUnchanged))
	if !fixturesUnchanged {
		t.Fatal("outgoing historical fixtures changed outside the projected repository fields")
	}
	t.Log("ASSERT: committed incoming collector writes and latest/rolling reads exclude outgoing prices; explicit historical fixtures unchanged")
}

func rolloverMust(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func rolloverRequest(t *testing.T, router http.Handler, method, path string, payload any) *httptest.ResponseRecorder {
	t.Helper()
	var body []byte
	if payload != nil {
		var err error
		body, err = json.Marshal(payload)
		rolloverMust(t, err)
	}
	request := httptest.NewRequest(method, path, bytes.NewReader(body))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	return response
}
