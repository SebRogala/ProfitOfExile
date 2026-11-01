//go:build integration

package bossloot

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"github.com/jackc/pgx/v5/pgxpool"
	"os"
	"profitofexile/internal/device"
	"profitofexile/internal/league"
	"reflect"
	"sync"
	"testing"
	"time"
)

var testProfile = Profile{Boss: "catarina", ProfileKey: "Normal", ProfileVersion: 1}

const eye = "Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly"

func repoFixture(t *testing.T) (*Repository, *pgxpool.Pool, string) {
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
	id := fmt.Sprintf("bossloot-%d", time.Now().UnixNano())
	if _, err = pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES ('Standard','Boss loot synthetic historical league','archived') ON CONFLICT DO NOTHING`); err != nil {
		pool.Close()
		t.Fatal(err)
	}
	if _, err = device.NewRepository(pool).Upsert(ctx, id, "test"); err != nil {
		pool.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if _, err := pool.Exec(ctx, `DELETE FROM boss_loot_runs WHERE device_id=$1`, id); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM devices WHERE fingerprint=$1`, id); err != nil {
			t.Error(err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM leagues WHERE id='Standard' AND display_name='Boss loot synthetic historical league' AND NOT EXISTS (SELECT 1 FROM boss_loot_runs WHERE league='Standard')`); err != nil {
			t.Error(err)
		}
		pool.Close()
	})
	return NewRepository(pool), pool, id
}
func submitFixture(t *testing.T, r *Repository, dev string, scope league.Scope, quantities map[string]int64, counted []string) Run {
	t.Helper()
	out, err := r.Submit(context.Background(), scope, dev, Submit{Profile: testProfile, Mode: "forceNew", Seed: ptr("same"), Items: Items{Quantities: quantities, Counted: counted}})
	if err != nil {
		t.Fatal(err)
	}
	return out.Run
}
func mergeFixture(run Run, q map[string]int64, positive ...string) Submit {
	return Submit{Profile: run.Profile, Mode: "merge", Seed: run.Seed, Target: &Target{ID: run.ID, ExpectedVersion: run.Version}, Items: Items{Quantities: q, Counted: run.Items.Counted}, IncomingPositiveKeys: positive}
}
func TestRepositoryMergePersistsCorrection(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	original := submitFixture(t, r, dev, scope, map[string]int64{"Cinderswallow Urn": 1}, []string{"Cinderswallow Urn", "Cane of Kulemak"})
	request := mergeFixture(original, map[string]int64{"Cane of Kulemak": 1}, "Cane of Kulemak")
	out, err := r.Submit(ctx, scope, dev, request)
	if err != nil {
		t.Fatal(err)
	}
	request.Target.ExpectedVersion = out.Run.Version
	if _, err = r.Submit(ctx, scope, dev, request); err != nil {
		t.Fatal(err)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if page.Total != 1 || !reflect.DeepEqual(page.Items[0].Items.Quantities, map[string]int64{"Cane of Kulemak": 1}) || page.Items[0].Version != 3 {
		t.Fatalf("persisted correction: %+v", page)
	}
}
func TestRepositoryConcurrentMergeRejectsStalePick(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	original := submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1}, []string{"Urn", "Cane", "Diadem"})
	var wg sync.WaitGroup
	results := make(chan error, 2)
	for _, pick := range []string{"Cane", "Diadem"} {
		wg.Add(1)
		go func(pick string) {
			defer wg.Done()
			_, err := r.Submit(ctx, scope, dev, mergeFixture(original, map[string]int64{pick: 1}, pick))
			results <- err
		}(pick)
	}
	wg.Wait()
	close(results)
	success, conflict := 0, 0
	for err := range results {
		if err == nil {
			success++
		} else if errors.Is(err, ErrConflict) {
			conflict++
		} else {
			t.Fatal(err)
		}
	}
	if success != 1 || conflict != 1 {
		t.Fatalf("success/conflict %d/%d", success, conflict)
	}
	page, _ := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if page.Total != 1 || len(page.Items[0].Items.Quantities) != 1 || page.Items[0].Version != 2 {
		t.Fatalf("stale overwrite: %+v", page)
	}
	_, err := r.Submit(ctx, scope, dev, mergeFixture(page.Items[0], map[string]int64{"Urn": 1}, "Urn"))
	if err != nil {
		t.Fatal(err)
	}
	page, _ = r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if !reflect.DeepEqual(page.Items[0].Items.Quantities, map[string]int64{"Urn": 1}) {
		t.Fatalf("re-read correction: %+v", page)
	}
}
func TestRepositoryMergePreservesOriginalEligibility(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	original := submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1}, []string{"Urn", "Cane"})
	req := mergeFixture(original, map[string]int64{"Cane": 1}, "Cane", eye)
	out, err := r.Submit(ctx, scope, dev, req)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(out.DroppedPositiveKeys, []string{eye}) || !reflect.DeepEqual(out.Run.Items.Counted, original.Items.Counted) {
		t.Fatalf("merge outcome %+v", out)
	}
	own, err := r.StatsCumulative(ctx, dev, testProfile, false)
	if err != nil {
		t.Fatal(err)
	}
	if own.Items[eye] != (ItemStats{}) || own.Runs != 1 {
		t.Fatalf("own %+v", own)
	}
	req = Submit{Profile: testProfile, Mode: "forceNew", Seed: ptr("same"), Items: Items{Quantities: map[string]int64{eye: 1}, Counted: []string{eye}}}
	fresh, err := r.Submit(ctx, scope, dev, req)
	if err != nil {
		t.Fatal(err)
	}
	if fresh.Run.Run != 2 {
		t.Fatalf("force-new %+v", fresh)
	}
	own, _ = r.StatsCumulative(ctx, dev, testProfile, false)
	if own.Items[eye] != (ItemStats{Drops: 1, EligibleRuns: 1}) || own.Runs != 2 {
		t.Fatalf("own %+v", own)
	}
	page, _ := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if !reflect.DeepEqual(page.Items[1].Items.Counted, original.Items.Counted) {
		t.Fatal("force-new changed original eligibility")
	}
}
func TestRepositoryRejectsInterveningMergeTarget(t *testing.T) {
	r, _, dev := repoFixture(t)
	scope := league.Historical("Mirage")
	ctx := context.Background()
	first := submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1}, []string{"Urn"})
	submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1}, []string{"Urn"})
	if _, err := r.Submit(ctx, scope, dev, mergeFixture(first, map[string]int64{"Urn": 1}, "Urn")); !errors.Is(err, ErrConflict) {
		t.Fatalf("intervening target: %v", err)
	}
	if _, err := r.Submit(ctx, league.Historical("Standard"), dev, mergeFixture(first, map[string]int64{"Urn": 1}, "Urn")); !errors.Is(err, ErrConflict) {
		t.Fatalf("cross-league target: %v", err)
	}
}
func TestRepositoryHistoricalMutationsPreserveHiddenQuantity(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	first := submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1, eye: 2}, []string{"Urn", "Cane", eye})
	submitFixture(t, r, dev, league.Historical("Standard"), map[string]int64{}, []string{eye})
	if _, err := r.EditOwned(ctx, "other-device", first.ID, first.Version, first.Items); !errors.Is(err, ErrNotFound) {
		t.Fatalf("foreign edit %v", err)
	}
	if err := r.DeleteOwned(ctx, "other-device", first.ID, first.Version); !errors.Is(err, ErrNotFound) {
		t.Fatalf("foreign delete %v", err)
	}
	first.Items.Quantities = map[string]int64{"Cane": 1, eye: 2}
	updated, err := r.EditOwned(ctx, dev, first.ID, first.Version, first.Items)
	if err != nil {
		t.Fatal(err)
	}
	if updated.League != "Mirage" || updated.Items.Quantities[eye] != 2 {
		t.Fatalf("historical edit %+v", updated)
	}
	if _, err = r.EditOwned(ctx, dev, first.ID, first.Version, first.Items); !errors.Is(err, ErrConflict) {
		t.Fatalf("stale edit %v", err)
	}
	if err = r.DeleteOwned(ctx, dev, first.ID, first.Version); !errors.Is(err, ErrConflict) {
		t.Fatalf("stale delete %v", err)
	}
	stats, _ := r.StatsCumulative(ctx, dev, testProfile, false)
	if stats.Items[eye] != (ItemStats{Drops: 2, EligibleRuns: 2}) || stats.NextRun != 3 {
		t.Fatalf("historical stats %+v", stats)
	}
	if err = r.DeleteOwned(ctx, dev, updated.ID, updated.Version); err != nil {
		t.Fatal(err)
	}
	stats, _ = r.StatsCumulative(ctx, dev, testProfile, false)
	if stats.Runs != 1 || stats.Items[eye] != (ItemStats{EligibleRuns: 1}) {
		t.Fatalf("delete stats %+v", stats)
	}
}
func TestRepositoryPaginationSeparatesProfileVersions(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	for i := 0; i < 3; i++ {
		submitFixture(t, r, dev, scope, map[string]int64{}, []string{})
	}
	_, err := r.Submit(ctx, scope, dev, Submit{Profile: Profile{Boss: "catarina", ProfileKey: "Normal", ProfileVersion: 2}, Mode: "new", Items: Items{Quantities: map[string]int64{}, Counted: []string{}}})
	if err != nil {
		t.Fatal(err)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 2, 1)
	if err != nil {
		t.Fatal(err)
	}
	if page.Total != 3 || len(page.Items) != 1 || page.Items[0].Run != 2 || page.Page != 2 || page.PageSize != 1 {
		t.Fatalf("page %+v", page)
	}
	other, _ := r.ListCumulative(ctx, "other-device", testProfile, 1, 10)
	if other.Total != 0 || other.Items == nil {
		t.Fatalf("other device %+v", other)
	}
}

func TestRepositoryConcurrentSameSeedNewRequiresMerge(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	req := Submit{Profile: testProfile, Mode: "new", Seed: ptr("concurrent"), Items: Items{Quantities: map[string]int64{"Urn": 1}, Counted: []string{"Urn"}}}
	results := make(chan error, 2)
	var wg sync.WaitGroup
	for i := 0; i < 2; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); _, err := r.Submit(ctx, scope, dev, req); results <- err }()
	}
	wg.Wait()
	close(results)
	successes, conflicts := 0, 0
	for err := range results {
		if err == nil {
			successes++
		} else if errors.Is(err, ErrConflict) {
			conflicts++
		} else {
			t.Fatal(err)
		}
	}
	if successes != 1 || conflicts != 1 {
		t.Fatalf("same seed success/conflict = %d/%d; want 1/1", successes, conflicts)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if page.Total != 1 {
		t.Fatalf("duplicate counted runs: %+v", page)
	}
	req.Mode = "forceNew"
	out, err := r.Submit(ctx, scope, dev, req)
	if err != nil {
		t.Fatal(err)
	}
	if out.Run.Run != 2 {
		t.Fatalf("force-new number %+v", out)
	}
	_, err = r.Submit(ctx, scope, dev, mergeFixture(out.Run, map[string]int64{"Urn": 1}, "Urn"))
	if err != nil {
		t.Fatal(err)
	}
	page, err = r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if page.Total != 2 || page.Items[0].Version != 2 {
		t.Fatalf("force-new merge %+v", page)
	}
}

func TestRepositoryRejectsMergeAcrossLeagues(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	original := submitFixture(t, r, dev, league.Historical("Mirage"), map[string]int64{"Urn": 1}, []string{"Urn", "Cane"})
	_, err := r.Submit(ctx, league.Historical("Standard"), dev, mergeFixture(original, map[string]int64{"Cane": 1}, "Cane"))
	if !errors.Is(err, ErrConflict) {
		t.Fatalf("equal-seed cross-league merge: %v", err)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if page.Total != 1 || page.Items[0].League != "Mirage" || page.Items[0].Items.Quantities["Urn"] != 1 || page.Items[0].Version != 1 {
		t.Fatalf("cross-league write %+v", page)
	}
}
func TestRepositoryCountedSetCannotBeRewritten(t *testing.T) {
	r, _, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	original := submitFixture(t, r, dev, scope, map[string]int64{"Urn": 1}, []string{"Urn"})
	changed := Items{Quantities: map[string]int64{"Urn": 1, eye: 1}, Counted: []string{"Urn", eye}}
	if _, err := r.EditOwned(ctx, dev, original.ID, 1, changed); !errors.Is(err, ErrInvalid) {
		t.Fatalf("counted-set edit: %v", err)
	}
	merge := mergeFixture(original, changed.Quantities, eye)
	merge.Items.Counted = changed.Counted
	if _, err := r.Submit(ctx, scope, dev, merge); !errors.Is(err, ErrInvalid) {
		t.Fatalf("counted-set merge: %v", err)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 1, 20)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(page.Items[0].Items, original.Items) || page.Items[0].Version != 1 {
		t.Fatalf("eligibility changed %+v", page)
	}
}
func TestRepositoryCumulativeStatsUseEachDevicesEligibility(t *testing.T) {
	r, pool, a := repoFixture(t)
	_, _, b := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	submitFixture(t, r, a, scope, map[string]int64{}, []string{})
	submitFixture(t, r, b, scope, map[string]int64{eye: 2}, []string{eye})
	submitFixture(t, r, b, league.Historical("Standard"), map[string]int64{}, []string{eye})
	own, err := r.StatsCumulative(ctx, a, testProfile, false)
	if err != nil {
		t.Fatal(err)
	}
	collective, err := r.StatsCumulative(ctx, a, testProfile, true)
	if err != nil {
		t.Fatal(err)
	}
	if own.Runs != 1 || own.Devices != 1 || own.Items[eye] != (ItemStats{}) || collective.Runs != 3 || collective.Devices != 2 || collective.Items[eye] != (ItemStats{Drops: 2, EligibleRuns: 2}) || collective.NextRun != 2 {
		t.Fatalf("own=%+v collective=%+v", own, collective)
	}
	var stored []byte
	if err = pool.QueryRow(ctx, `SELECT items FROM boss_loot_runs WHERE device_id=$1 ORDER BY run_number LIMIT 1`, b).Scan(&stored); err != nil {
		t.Fatal(err)
	}
	var items Items
	if err = json.Unmarshal(stored, &items); err != nil {
		t.Fatal(err)
	}
	if items.Quantities[eye] != 2 {
		t.Fatalf("stored quantities %+v", items)
	}
}
func TestRepositoryListOrdersEqualDatesByRunWithNullDatesLast(t *testing.T) {
	r, pool, dev := repoFixture(t)
	ctx := context.Background()
	scope := league.Historical("Mirage")
	for i := 0; i < 3; i++ {
		submitFixture(t, r, dev, scope, map[string]int64{}, []string{})
	}
	if _, err := pool.Exec(ctx, `UPDATE boss_loot_runs SET date=CASE WHEN run_number=3 THEN NULL ELSE '2026-01-01'::timestamptz END WHERE device_id=$1`, dev); err != nil {
		t.Fatal(err)
	}
	page, err := r.ListCumulative(ctx, dev, testProfile, 1, 10)
	if err != nil {
		t.Fatal(err)
	}
	if len(page.Items) != 3 || page.Items[0].Run != 2 || page.Items[1].Run != 1 || page.Items[2].Run != 3 || page.Items[2].Date != nil {
		t.Fatalf("stable legacy order %+v", page)
	}
}

func TestRepositoryStatsIsolateProfileVersions(t *testing.T) {
	ctx := context.Background()
	r, _, a := repoFixture(t)
	_, _, b := repoFixture(t)
	_, _, c := repoFixture(t)
	_, _, d := repoFixture(t)
	const v1Only = "category:version-test:1:eligible"
	const v2Only = "category:version-test:2:eligible"
	cases := []struct {
		name       string
		version    int
		collective bool
		want       Stats
	}{
		{"v1 own", 1, false, Stats{Runs: 1, Devices: 1, NextRun: 2, Items: map[string]ItemStats{eye: {Drops: 2, EligibleRuns: 1}, v1Only: {EligibleRuns: 1}}}},
		{"v1 collective", 1, true, Stats{Runs: 3, Devices: 2, NextRun: 2, Items: map[string]ItemStats{eye: {Drops: 5, EligibleRuns: 2}, v1Only: {EligibleRuns: 2}, "Urn": {Drops: 1, EligibleRuns: 1}}}},
		{"v2 own", 2, false, Stats{Runs: 2, Devices: 1, NextRun: 3, Items: map[string]ItemStats{eye: {Drops: 7, EligibleRuns: 1}, v2Only: {Drops: 2, EligibleRuns: 1}}}},
		{"v2 collective", 2, true, Stats{Runs: 6, Devices: 3, NextRun: 3, Items: map[string]ItemStats{eye: {Drops: 18, EligibleRuns: 3}, v2Only: {Drops: 7, EligibleRuns: 4}}}},
	}
	profile := testProfile
	profile.Boss = "version-stats-" + a
	for _, seed := range []struct {
		version    int
		device     string
		quantities map[string]int64
		counted    []string
		run        int64
	}{
		{1, a, map[string]int64{eye: 2}, []string{eye, v1Only}, 1},
		{1, b, map[string]int64{eye: 3}, []string{eye, v1Only}, 1},
		{1, b, map[string]int64{"Urn": 1}, []string{"Urn"}, 2},
		{2, a, map[string]int64{eye: 7}, []string{eye}, 1},
		{2, a, map[string]int64{v2Only: 2}, []string{v2Only}, 2},
		{2, c, map[string]int64{eye: 11}, []string{eye, v2Only}, 1},
		{2, c, map[string]int64{}, []string{v2Only}, 2},
		{2, c, map[string]int64{}, []string{eye}, 3},
		{2, d, map[string]int64{v2Only: 5}, []string{v2Only}, 1},
	} {
		profile.ProfileVersion = seed.version
		out, err := r.Submit(ctx, league.Historical("Mirage"), seed.device, Submit{Profile: profile, Mode: "forceNew", Items: Items{Quantities: seed.quantities, Counted: seed.counted}})
		if err != nil {
			t.Fatal(err)
		}
		if out.Run.Run != seed.run {
			t.Fatalf("version %d device %s: stored run %d, want %d", seed.version, seed.device, out.Run.Run, seed.run)
		}
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			requested := profile
			requested.ProfileVersion = tc.version
			got, err := r.StatsCumulative(ctx, a, requested, tc.collective)
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("version %d collective=%t: stats %+v, want %+v", tc.version, tc.collective, got, tc.want)
			}
		})
	}
}
