//go:build integration

package handlers

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"profitofexile/internal/bossloot"
	"profitofexile/internal/league"
	"profitofexile/internal/ratelimit"
	"profitofexile/internal/server/middleware"
	"reflect"
	"testing"
	"time"
)

const bossEye = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly"

func TestBossLootHTTPUsesBootScope(t *testing.T) {
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		t.Skip("DATABASE_URL required")
	}
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	boot, err := league.Resolve(ctx, pool)
	if err != nil {
		t.Fatal(err)
	}
	boss := fmt.Sprintf("boot-scope-%d", time.Now().UnixNano())
	next := boss + "-B"
	deviceID := fmt.Sprintf("%x", sha256.Sum256([]byte(boss)))
	// Register cleanup before the first committed fixture or runtime change.
	defer func() {
		if _, err := pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=$2 WHERE singleton=TRUE`, boot.ID(), boot.Revision()); err != nil {
			t.Error(err)
		}
		for _, cleanup := range []struct {
			query string
			arg   string
		}{
			{`DELETE FROM boss_loot_runs WHERE boss=$1`, boss},
			{`DELETE FROM devices WHERE fingerprint=$1`, deviceID},
			{`DELETE FROM leagues WHERE id=$1`, next},
		} {
			if _, err := pool.Exec(ctx, cleanup.query, cleanup.arg); err != nil {
				t.Error(err)
			}
		}
		restored, err := league.Resolve(ctx, pool)
		if err != nil || restored != boot {
			t.Errorf("runtime identity/revision restoration failed: %v", err)
		}
		var remaining int
		if err := pool.QueryRow(ctx, `SELECT (SELECT count(*) FROM boss_loot_runs WHERE boss=$1) + (SELECT count(*) FROM devices WHERE fingerprint=$2) + (SELECT count(*) FROM leagues WHERE id=$3)`, boss, deviceID, next).Scan(&remaining); err != nil || remaining != 0 {
			t.Errorf("fixture restoration: remaining=%d err=%v", remaining, err)
		}
		t.Log("verified runtime identity/revision restoration and zero remaining boot-scope fixtures")
	}()
	if _, err := pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES ($1,$1,'archived')`, next); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO devices(fingerprint) VALUES ($1)`, deviceID); err != nil {
		t.Fatal(err)
	}
	h := NewBossLoot(bossloot.NewRepository(pool), boot, nil)
	if _, err := pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=revision+1 WHERE singleton=TRUE`, next); err != nil {
		t.Fatal(err)
	}
	submit := func(wantLeague string) {
		t.Helper()
		body := bossloot.Submit{
			Profile: bossloot.Profile{Boss: boss, ProfileKey: "Normal", ProfileVersion: 1},
			Mode:    "forceNew", League: boss + "-C",
			Items: bossloot.Items{Quantities: map[string]int64{"Urn": 1}, Counted: []string{"Urn"}},
		}
		raw, err := json.Marshal(body)
		if err != nil {
			t.Fatal(err)
		}
		req := httptest.NewRequest("POST", "/api/boss-loot/runs", bytes.NewReader(raw))
		req.Header.Set("X-Device-ID", deviceID)
		w := httptest.NewRecorder()
		middleware.DeviceMiddleware(bossLootDevice{})(http.HandlerFunc(h.Submit)).ServeHTTP(w, req)
		var out bossloot.SubmitOutcome
		if err := json.Unmarshal(w.Body.Bytes(), &out); err != nil || w.Code != 201 || out.State != "saved" || out.Run.League != wantLeague {
			t.Fatalf("boot scope submission: status=%d body=%s err=%v; want saved league %q", w.Code, w.Body.String(), err, wantLeague)
		}
		var storedLeague string
		if err := pool.QueryRow(ctx, `SELECT league FROM boss_loot_runs WHERE id=$1 AND device_id=$2`, out.Run.ID, deviceID).Scan(&storedLeague); err != nil || storedLeague != wantLeague {
			t.Fatalf("persisted league=%q err=%v; want %q", storedLeague, err, wantLeague)
		}
	}
	submit(boot.ID())
	reboot, err := league.Resolve(ctx, pool)
	if err != nil || reboot.ID() != next || reboot.Revision() != boot.Revision()+1 {
		t.Fatalf("reboot scope did not capture runtime B: %v", err)
	}
	h = NewBossLoot(bossloot.NewRepository(pool), reboot, nil)
	submit(next)
}

func TestBossLootHTTPRetainsHistoricalObservation(t *testing.T) {
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		t.Skip("DATABASE_URL required")
	}
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	// The HTTP server uses actual handlers/repository; only identity lookup is a
	// fixed middleware boundary, independent of body-supplied attribution.
	boss := fmt.Sprintf("http-%d", time.Now().UnixNano())
	a, b := fmt.Sprintf("%x", sha256.Sum256([]byte(boss+"a"))), fmt.Sprintf("%x", sha256.Sum256([]byte(boss+"b")))
	boot, err := league.Resolve(ctx, pool)
	if err != nil {
		t.Fatal(err)
	}
	insertedStandard := false
	defer func() {
		if _, err := pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=$2 WHERE singleton=TRUE`, boot.ID(), boot.Revision()); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM boss_loot_runs WHERE boss=$1`, boss); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM devices WHERE fingerprint=ANY($1)`, []string{a, b}); err != nil {
			t.Error(err)
		}
		if insertedStandard {
			if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id='Standard'`); err != nil {
				t.Error(err)
			}
		}
		restored, err := league.Resolve(ctx, pool)
		if err != nil || restored != boot {
			t.Errorf("historical runtime identity/revision restoration failed: %v", err)
		}
		var remaining int
		if err := pool.QueryRow(ctx, `SELECT (SELECT count(*) FROM boss_loot_runs WHERE boss=$1) + (SELECT count(*) FROM devices WHERE fingerprint=ANY($2)) + (SELECT count(*) FROM leagues WHERE id='Standard' AND $3::boolean)`, boss, []string{a, b}, insertedStandard).Scan(&remaining); err != nil || remaining != 0 {
			t.Errorf("historical fixture restoration: remaining=%d err=%v", remaining, err)
		}
		t.Log("verified runtime identity/revision restoration and zero remaining historical fixtures")
	}()
	insert, err := pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES ('Standard','Boss loot synthetic historical league','archived') ON CONFLICT DO NOTHING`)
	if err != nil {
		t.Fatal(err)
	}
	insertedStandard = insert.RowsAffected() == 1
	for _, dev := range []string{a, b} {
		if _, err = pool.Exec(ctx, `INSERT INTO devices(fingerprint) VALUES ($1)`, dev); err != nil {
			t.Fatal(err)
		}
	}
	h := NewBossLoot(bossloot.NewRepository(pool), boot, ratelimit.NewRateLimiter(DefaultBossLootWriteBudget, DefaultBossLootWriteWindow, DefaultBossLootRateLimitDevices))
	router := chi.NewRouter()
	router.Use(middleware.DeviceMiddleware(bossLootDevice{}))
	router.Post("/api/boss-loot/runs", h.Submit)
	router.Get("/api/boss-loot/runs", h.List)
	router.Patch("/api/boss-loot/runs/{id}", h.Edit)
	router.Post("/api/boss-loot/runs/{id}/delete", h.Delete)
	router.Get("/api/boss-loot/stats", h.Stats)
	server := httptest.NewServer(router)
	defer server.Close()
	request := func(dev, method, path string, body any, status int, output any) {
		t.Helper()
		var data []byte
		if body != nil {
			data, _ = json.Marshal(body)
		}
		req, e := http.NewRequest(method, server.URL+path, bytes.NewReader(data))
		if e != nil {
			t.Fatal(e)
		}
		req.Header.Set("X-Device-ID", dev)
		resp, e := server.Client().Do(req)
		if e != nil {
			t.Fatal(e)
		}
		defer resp.Body.Close()
		raw, _ := io.ReadAll(resp.Body)
		if resp.StatusCode != status {
			t.Fatalf("%s %s: %d %s", method, path, resp.StatusCode, raw)
		}
		if status == http.StatusTooManyRequests {
			var refusal map[string]string
			if e = json.Unmarshal(raw, &refusal); e != nil || len(refusal) != 1 || refusal["error"] != "upload rate limit exceeded" || resp.Header.Get("Retry-After") == "" {
				t.Fatalf("rate refusal: %s headers=%v", raw, resp.Header)
			}
		}
		if output != nil {
			// Each HTTP response is a fresh value. Reusing a decoded map would
			// retain keys omitted by later responses and fabricate stored drops.
			reflect.ValueOf(output).Elem().SetZero()
			if e = json.Unmarshal(raw, output); e != nil {
				t.Fatalf("decode %s: %v", raw, e)
			}
		} else if status >= 400 && !bytes.Contains(raw, []byte(`"error"`)) {
			t.Fatalf("missing error: %s", raw)
		}
	}
	profile := bossloot.Profile{Boss: boss, ProfileKey: "Normal", ProfileVersion: 1}
	path := "/api/boss-loot/runs"
	query := "?boss=" + boss + "&profileKey=Normal&profileVersion=1"
	makeRun := func(dev string, items bossloot.Items) bossloot.Run {
		t.Helper()
		var out bossloot.SubmitOutcome
		request(dev, "POST", path, bossloot.Submit{Profile: profile, Mode: "new", Seed: bossString("seed"), Items: items}, 201, &out)
		if out.State != "saved" || out.Run.ID == "" {
			t.Fatalf("submit %+v", out)
		}
		return out.Run
	}
	original := makeRun(a, bossloot.Items{Quantities: map[string]int64{"Urn": 1}, Counted: []string{"Urn", "Cane"}})
	makeRun(b, bossloot.Items{Quantities: map[string]int64{bossEye: 1}, Counted: []string{bossEye}})
	stats := func(dev, scope string, want bossloot.ItemStats) {
		t.Helper()
		var out bossloot.Stats
		request(dev, "GET", "/api/boss-loot/stats"+query+"&scope="+scope, nil, 200, &out)
		if out.Items[bossEye] != want {
			t.Fatalf("%s Eye %+v want %+v", scope, out, want)
		}
	}
	stats(a, "own", bossloot.ItemStats{})
	stats(a, "collective", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	merge := bossloot.Submit{Profile: profile, Mode: "merge", Seed: original.Seed, Target: &bossloot.Target{ID: original.ID, ExpectedVersion: 1}, IncomingPositiveKeys: []string{"Cane", bossEye}, Items: bossloot.Items{Quantities: map[string]int64{"Cane": 1}, Counted: original.Items.Counted}}
	var merged bossloot.SubmitOutcome
	request(a, "POST", path, merge, 200, &merged)
	if !reflect.DeepEqual(merged.DroppedPositiveKeys, []string{bossEye}) || !reflect.DeepEqual(merged.Run.Items.Quantities, map[string]int64{"Cane": 1}) {
		t.Fatalf("merge %+v", merged)
	}
	stats(a, "own", bossloot.ItemStats{})
	stats(a, "collective", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	request(a, "POST", path, merge, 409, nil)
	var page bossloot.Page
	request(a, "GET", path+query, nil, 200, &page)
	if page.Total != 1 || page.Items[0].Version != 2 || page.Items[0].Items.Quantities["Cane"] != 1 || !reflect.DeepEqual(page.Items[0].Items.Counted, original.Items.Counted) {
		t.Fatalf("stale merge persisted %+v", page)
	}
	merge.Target.ExpectedVersion = page.Items[0].Version
	merge.Items.Quantities = map[string]int64{"Urn": 1}
	merge.IncomingPositiveKeys = []string{"Urn"}
	request(a, "POST", path, merge, 200, &merged)
	correctedItems := bossloot.Items{Quantities: map[string]int64{"Urn": 1}, Counted: original.Items.Counted}
	correctedVersion := merge.Target.ExpectedVersion + 1
	if merged.State != "saved" || merged.Run.ID != original.ID || merged.Run.Run != original.Run ||
		merged.Run.Profile != original.Profile || merged.Run.Version != correctedVersion ||
		merged.Run.League != original.League || !reflect.DeepEqual(merged.Run.Items, correctedItems) {
		t.Fatalf("corrected merge response: %+v; want original identity, version %d and items %+v", merged, correctedVersion, correctedItems)
	}
	request(a, "GET", path+query, nil, 200, &page)
	if page.Total != 1 || len(page.Items) != 1 {
		t.Fatalf("corrected merge must retain one row: %+v", page)
	}
	corrected := page.Items[0]
	if corrected.ID != original.ID || corrected.Run != original.Run || corrected.Profile != original.Profile ||
		corrected.Version != correctedVersion || corrected.League != original.League ||
		!reflect.DeepEqual(corrected.Items, correctedItems) {
		t.Fatalf("persisted corrected merge: %+v; want original identity, version %d and items %+v", corrected, correctedVersion, correctedItems)
	}
	var forced bossloot.SubmitOutcome
	request(a, "POST", path, bossloot.Submit{Profile: profile, Mode: "forceNew", Seed: original.Seed, Items: bossloot.Items{Quantities: map[string]int64{bossEye: 1, "Urn": 1}, Counted: []string{"Urn", "Cane", bossEye}}}, 201, &forced)
	stats(a, "own", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	stats(a, "collective", bossloot.ItemStats{Drops: 2, EligibleRuns: 2})
	// Change the committed selection while the handler retains its boot scope.
	next := "Standard"
	if original.League == next {
		next = "Mirage"
	}
	if _, err = pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=revision+1 WHERE singleton=TRUE`, next); err != nil {
		t.Fatal(err)
	}
	request(a, "GET", path+query, nil, 200, &page)
	edited := page.Items[0]
	if edited.ID != forced.Run.ID {
		t.Fatalf("latest run %+v", page)
	}
	edited.Items.Quantities = map[string]int64{"Cane": 1, bossEye: 1}
	body := map[string]any{"expectedVersion": edited.Version, "items": edited.Items, "device_id": b, "league": next}
	request(b, "PATCH", path+"/"+edited.ID, body, 404, nil)
	var saved bossloot.Run
	request(a, "PATCH", path+"/"+edited.ID, body, 200, &saved)
	if saved.League != edited.League || saved.Items.Quantities[bossEye] != 1 || !reflect.DeepEqual(saved.Items.Counted, edited.Items.Counted) {
		t.Fatalf("hidden history %+v", saved)
	}
	request(a, "GET", path+query, nil, 200, &page)
	if !reflect.DeepEqual(page.Items[0].Items, saved.Items) || page.Items[0].League != edited.League {
		t.Fatalf("reread %+v", page)
	}
	stats(a, "own", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	stats(a, "collective", bossloot.ItemStats{Drops: 2, EligibleRuns: 2})
	var persistedLeague string
	var raw []byte
	if err = pool.QueryRow(ctx, `SELECT league,items FROM boss_loot_runs WHERE id=$1 AND device_id=$2`, saved.ID, a).Scan(&persistedLeague, &raw); err != nil {
		t.Fatal(err)
	}
	var persisted bossloot.Items
	if err = json.Unmarshal(raw, &persisted); err != nil {
		t.Fatal(err)
	}
	if persistedLeague != edited.League || !reflect.DeepEqual(persisted, saved.Items) {
		t.Fatalf("stored history %s %+v", persistedLeague, persisted)
	}
	request(a, "PATCH", path+"/"+edited.ID, body, 409, nil)
	request(b, "POST", path+"/"+edited.ID+"/delete", map[string]any{"expectedVersion": saved.Version}, 404, nil)
	request(a, "POST", path+"/"+edited.ID+"/delete", map[string]any{"expectedVersion": edited.Version}, 409, nil)
	request(a, "POST", path+"/"+edited.ID+"/delete", map[string]any{"expectedVersion": saved.Version}, 200, &map[string]any{})
	stats(a, "own", bossloot.ItemStats{})
	stats(a, "collective", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	var count int
	if err = pool.QueryRow(ctx, `SELECT count(*) FROM boss_loot_runs WHERE id=$1`, saved.ID).Scan(&count); err != nil || count != 0 {
		t.Fatalf("deleted rows %d: %v", count, err)
	}
	t.Run("SuccessfulWritesShareBudget", func(t *testing.T) {
		h.limiter = ratelimit.NewRateLimiter(3, time.Hour, 8)
		var created bossloot.SubmitOutcome
		items := bossloot.Items{Quantities: map[string]int64{"Urn": 1}, Counted: []string{"Urn", "Cane"}}
		request(a, "POST", path, bossloot.Submit{Profile: profile, Mode: "forceNew", Items: items}, 201, &created)
		items.Quantities = map[string]int64{"Cane": 1}
		var updated bossloot.Run
		request(a, "PATCH", path+"/"+created.Run.ID, map[string]any{"expectedVersion": created.Run.Version, "items": items}, 200, &updated)
		if updated.ID != created.Run.ID || updated.Version != created.Run.Version+1 || !reflect.DeepEqual(updated.Items, items) {
			t.Fatalf("budgeted edit: %+v", updated)
		}
		var deleted struct {
			Deleted bool   `json:"deleted"`
			ID      string `json:"id"`
		}
		request(a, "POST", path+"/"+updated.ID+"/delete", map[string]any{"expectedVersion": updated.Version}, 200, &deleted)
		if !deleted.Deleted || deleted.ID != updated.ID {
			t.Fatalf("budgeted deletion: %+v", deleted)
		}
		for _, write := range []struct{ method, route string }{{"POST", path}, {"PATCH", path + "/" + original.ID}, {"POST", path + "/" + original.ID + "/delete"}} {
			request(a, write.method, write.route, map[string]any{}, 429, nil)
		}
		var remaining bossloot.Page
		request(a, "GET", path+query, nil, 200, &remaining)
		if remaining.Total != 1 || len(remaining.Items) != 1 || remaining.Items[0].ID != original.ID || remaining.Items[0].Items.Quantities["Urn"] != 1 {
			t.Fatalf("read after exhausted budget: %+v", remaining)
		}
		for _, scope := range []string{"own", "collective"} {
			var out bossloot.Stats
			request(a, "GET", "/api/boss-loot/stats"+query+"&scope="+scope, nil, 200, &out)
			wantRuns := int64(1)
			if scope == "collective" {
				wantRuns = 2
			}
			if out.Runs != wantRuns || out.Items["Urn"] != (bossloot.ItemStats{Drops: 1, EligibleRuns: 1}) {
				t.Fatalf("%s read after exhaustion: %+v", scope, out)
			}
		}
		request(a, "GET", path+query+"&page=51&pageSize=200", nil, 200, &remaining)
		if remaining.Total != 1 || remaining.Page != 51 || remaining.PageSize != 200 || len(remaining.Items) != 0 || remaining.Items == nil {
			t.Fatalf("offset boundary: %+v", remaining)
		}
		request(a, "GET", path+query+"&page=52&pageSize=200", nil, 400, nil)
		var independent bossloot.SubmitOutcome
		request(b, "POST", path, bossloot.Submit{Profile: profile, Mode: "forceNew", Items: items}, 201, &independent)
		if independent.State != "saved" || independent.Run.ID == "" || !reflect.DeepEqual(independent.Run.Items, items) {
			t.Fatalf("independent device: %+v", independent)
		}
	})
}
func bossString(s string) *string { return &s }
