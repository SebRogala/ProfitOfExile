//go:build integration

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/bossloot"
	"profitofexile/internal/device"
	"profitofexile/internal/league"
)

func importFixture(t *testing.T) (*pgxpool.Pool, string, league.Scope) {
	t.Helper()
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		t.Fatal("DATABASE_URL required; integration tests must not skip")
	}
	ctx := context.Background()
	cfg, err := pgxpool.ParseConfig(url)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasSuffix(cfg.ConnConfig.Database, "_integration") {
		t.Fatal("integration database required")
	}
	cfg.MaxConns = 1
	pool, err := pgxpool.NewWithConfig(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	id := fmt.Sprintf("boss-import-synthetic-%d", time.Now().UnixNano())
	scope := league.Historical(id)
	if _, err = pool.Exec(ctx, `INSERT INTO leagues(id,display_name,collection_state) VALUES($1,$1,'archived')`, scope.ID()); err != nil {
		pool.Close()
		t.Fatal(err)
	}
	t.Cleanup(func() {
		for _, query := range []string{`DELETE FROM boss_loot_runs WHERE device_id=$1`, `DELETE FROM devices WHERE fingerprint=$1`, `DELETE FROM leagues WHERE id=$1`} {
			if _, err := pool.Exec(ctx, query, id); err != nil {
				t.Error(err)
			}
		}
		pool.Close()
	})
	if _, err = device.NewRepository(pool).Upsert(ctx, id, "test"); err != nil {
		t.Fatal(err)
	}
	return pool, id, scope
}

func persistedCount(t *testing.T, pool *pgxpool.Pool, dev string) int {
	t.Helper()
	var n int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM boss_loot_runs WHERE device_id=$1`, dev).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n
}

func TestImportPersistsAnonymousSourceRows(t *testing.T) {
	pool, dev, scope := importFixture(t)
	ctx := context.Background()
	source := fixtureRows(t)
	if err := importRuns(ctx, pool, scope, dev, source); err != nil {
		t.Fatal(err)
	}
	rows, err := pool.Query(ctx, `SELECT league,run_number,date,seed,items FROM boss_loot_runs WHERE device_id=$1 ORDER BY run_number`, dev)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	n := 0
	for rows.Next() {
		var storedLeague string
		var run int
		var date time.Time
		var seed *string
		var raw []byte
		if err := rows.Scan(&storedLeague, &run, &date, &seed, &raw); err != nil {
			t.Fatal(err)
		}
		var payload struct {
			bossloot.Items
			Source struct {
				Table         string `json:"table"`
				Row           int    `json:"row"`
				Date          string `json:"date"`
				DatePrecision string `json:"datePrecision"`
			} `json:"source"`
		}
		if err := json.Unmarshal(raw, &payload); err != nil {
			t.Fatal(err)
		}
		if n >= 78 {
			t.Fatal("more than 78 source rows")
		}
		want := source[n]
		if storedLeague != scope.ID() || run != n+1 || seed != nil || date.Format(time.RFC3339) != want.Date+"T00:00:00Z" || !reflect.DeepEqual(payload.Items, want.Items) || payload.Source.Table != "CatarinaRuns" || payload.Source.Row != want.SourceRow || payload.Source.Date != want.Date || payload.Source.DatePrecision != "calendar" {
			t.Fatalf("persisted run %d: league=%q date=%v seed=%v items=%s", run, storedLeague, date, seed, raw)
		}
		n++
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if n != 78 {
		t.Fatalf("persisted rows=%d; want 78", n)
	}
}

func TestImportStatisticsUseOnlyRetainedColumns(t *testing.T) {
	pool, dev, scope := importFixture(t)
	ctx := context.Background()
	if err := importRuns(ctx, pool, scope, dev, fixtureRows(t)); err != nil {
		t.Fatal(err)
	}
	stats, err := bossloot.NewRepository(pool).StatsCumulative(ctx, dev, bossloot.Profile{Boss: "catarina", ProfileKey: "Normal", ProfileVersion: 1}, false)
	if err != nil {
		t.Fatal(err)
	}
	if stats.Runs != 78 || stats.Devices != 1 || stats.NextRun != 79 || len(stats.Items) != 9 {
		t.Fatalf("import stats=%+v", stats)
	}
	for i, want := range []int64{26, 33, 22, 24, 14, 16, 7, 6, 11} {
		if stats.Items[expectedKeys[i]] != (bossloot.ItemStats{Drops: want, EligibleRuns: 78}) {
			t.Errorf("%s stats=%+v; want %d/78", expectedKeys[i], stats.Items[expectedKeys[i]], want)
		}
	}
}

func TestImportRerunLeavesOriginalRows(t *testing.T) {
	pool, dev, scope := importFixture(t)
	ctx := context.Background()
	source := fixtureRows(t)
	if err := importRuns(ctx, pool, scope, dev, source); err != nil {
		t.Fatal(err)
	}
	var before, after string
	snapshot := `SELECT jsonb_agg(to_jsonb(r) ORDER BY run_number)::text FROM boss_loot_runs r WHERE device_id=$1`
	if err := pool.QueryRow(ctx, snapshot, dev).Scan(&before); err != nil {
		t.Fatal(err)
	}
	if err := importRuns(ctx, pool, scope, dev, source); err == nil {
		t.Fatal("repeat import accepted")
	}
	if err := pool.QueryRow(ctx, snapshot, dev).Scan(&after); err != nil {
		t.Fatal(err)
	}
	if before != after || persistedCount(t, pool, dev) != 78 {
		t.Fatal("repeat import changed persisted rows")
	}
}

func TestImportRollsBackAfterDatabaseFailure(t *testing.T) {
	pool, dev, scope := importFixture(t)
	ctx := context.Background()
	// This connection-local trigger rejects row 40 after rows 1–39 have been
	// inserted. A single-connection pool keeps the test seam on the importer.
	_, err := pool.Exec(ctx, `CREATE FUNCTION pg_temp.reject_seed_row() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.device_id = TG_ARGV[0] AND NEW.run_number=40 THEN RAISE EXCEPTION 'synthetic row 40 failure'; END IF; RETURN NEW; END $$`)
	if err != nil {
		t.Fatal(err)
	}
	// Trigger names are test-owned, and the argument is quoted by PostgreSQL.
	var statement string
	if err = pool.QueryRow(ctx, `SELECT format('CREATE TRIGGER %I BEFORE INSERT ON boss_loot_runs FOR EACH ROW EXECUTE FUNCTION pg_temp.reject_seed_row(%L)', $1::text, $2::text)`, dev, dev).Scan(&statement); err != nil {
		t.Fatal(err)
	}
	if _, err = pool.Exec(ctx, statement); err != nil {
		t.Fatal(err)
	}
	defer func() {
		var drop string
		if err := pool.QueryRow(ctx, `SELECT format('DROP TRIGGER %I ON boss_loot_runs',$1::text)`, dev).Scan(&drop); err != nil {
			t.Error(err)
			return
		}
		if _, err := pool.Exec(ctx, drop); err != nil {
			t.Error(err)
		}
	}()
	if err = importRuns(ctx, pool, scope, dev, fixtureRows(t)); err == nil || !strings.Contains(err.Error(), "synthetic row 40 failure") {
		t.Fatalf("expected database failure after 39 inserts; got %v", err)
	}
	if n := persistedCount(t, pool, dev); n != 0 {
		t.Fatalf("partial rows=%d; want zero", n)
	}
}
