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
	"profitofexile/internal/server/middleware"
	"reflect"
	"testing"
	"time"
)

const bossEye = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly"

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
	if _, err = pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES ('Standard','Boss loot synthetic historical league','archived') ON CONFLICT DO NOTHING`); err != nil {
		t.Fatal(err)
	}
	for _, dev := range []string{a, b} {
		if _, err = pool.Exec(ctx, `INSERT INTO devices(fingerprint) VALUES ($1) ON CONFLICT DO NOTHING`, dev); err != nil {
			t.Fatal(err)
		}
	}
	defer func() {
		if _, err := pool.Exec(ctx, `DELETE FROM boss_loot_runs WHERE boss=$1`, boss); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM devices WHERE fingerprint=ANY($1)`, []string{a, b}); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id='Standard' AND display_name='Boss loot synthetic historical league' AND NOT EXISTS (SELECT 1 FROM boss_loot_runs WHERE league='Standard')`); err != nil {
			t.Error(err)
		}
	}()
	h := NewBossLoot(pool)
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
	var forced bossloot.SubmitOutcome
	request(a, "POST", path, bossloot.Submit{Profile: profile, Mode: "forceNew", Seed: original.Seed, Items: bossloot.Items{Quantities: map[string]int64{bossEye: 1, "Urn": 1}, Counted: []string{"Urn", "Cane", bossEye}}}, 201, &forced)
	stats(a, "own", bossloot.ItemStats{Drops: 1, EligibleRuns: 1})
	stats(a, "collective", bossloot.ItemStats{Drops: 2, EligibleRuns: 2})
	// Switch the committed runtime selection so HTTP requests observe rollover.
	// Restore the exact selection/revision before the fixture leaves this test.
	var previousLeague string
	var previousRevision int64
	if err = pool.QueryRow(ctx, `SELECT active_league,revision FROM runtime_config WHERE singleton=TRUE`).Scan(&previousLeague, &previousRevision); err != nil {
		t.Fatal(err)
	}
	next := "Standard"
	if original.League == next {
		next = "Mirage"
	}
	if _, err = pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=revision+1 WHERE singleton=TRUE`, next); err != nil {
		t.Fatal(err)
	}
	defer pool.Exec(ctx, `UPDATE runtime_config SET active_league=$1,revision=$2 WHERE singleton=TRUE`, previousLeague, previousRevision)
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
}
func bossString(s string) *string { return &s }
