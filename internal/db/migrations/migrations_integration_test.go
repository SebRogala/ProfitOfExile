//go:build integration

package migrations_test

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/golang-migrate/migrate/v4"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
)

func testSetup(t *testing.T) (*pgxpool.Pool, *migrate.Migrate) {
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

	m, err := db.NewMigrate(db.MigrationsFS, dbURL)
	if err != nil {
		pool.Close()
		t.Fatalf("create migrate instance: %v", err)
	}

	return pool, m
}

func migrateUp(t *testing.T, m *migrate.Migrate) {
	t.Helper()
	if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate up: %v", err)
	}
}

// requireTimescaleDB checks that TimescaleDB extension is available and skips
// the test if it is not installed on the database server.
func requireTimescaleDB(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()

	var available bool
	err := pool.QueryRow(context.Background(),
		"SELECT EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'timescaledb')").
		Scan(&available)
	if err != nil {
		t.Fatalf("check timescaledb availability: %v", err)
	}
	if !available {
		t.Skip("TimescaleDB not available, skipping test")
	}
}

func TestMigrationsApply(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	var strategies, index, trigger bool
	for name, dest := range map[string]*bool{
		"strategies":                &strategies,
		"idx_strategies_league":     &index,
		"trg_strategies_updated_at": &trigger,
	} {
		var query string
		switch name {
		case "strategies":
			query = "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)"
		case "idx_strategies_league":
			query = "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = $1)"
		default:
			query = "SELECT EXISTS (SELECT 1 FROM information_schema.triggers WHERE trigger_name = $1)"
		}
		if err := pool.QueryRow(context.Background(), query, name).Scan(dest); err != nil {
			t.Fatalf("check %s: %v", name, err)
		}
		if !*dest {
			t.Errorf("%s should exist after migrations", name)
		}
	}
}

func TestLeagueControlBootstrapSelectsMirage(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	var controlRows int
	if err := pool.QueryRow(context.Background(), "SELECT count(*) FROM runtime_config").Scan(&controlRows); err != nil {
		t.Fatalf("count runtime control rows: %v", err)
	}
	if controlRows != 1 {
		t.Fatalf("runtime control rows = %d, want 1", controlRows)
	}

	var activeLeague, state string
	var revision int64
	err := pool.QueryRow(context.Background(), `
		SELECT config.active_league, config.revision, leagues.collection_state
		FROM runtime_config AS config
		JOIN leagues ON leagues.id = config.active_league
		WHERE config.singleton = TRUE`).Scan(&activeLeague, &revision, &state)
	if err != nil {
		t.Fatalf("query Mirage runtime control: %v", err)
	}
	if activeLeague != "Mirage" || revision != 1 || state != "collecting" {
		t.Errorf("runtime control = (%q, %d, %q), want (Mirage, 1, collecting)", activeLeague, revision, state)
	}
}

func TestCollectedLeagueCanStoreSnapshotsWhileMirageIsActive(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	ctx := context.Background()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatalf("begin transaction: %v", err)
	}
	defer tx.Rollback(ctx)

	const collectedLeague = "POE-119-Test-League"
	if _, err := tx.Exec(ctx, `
		INSERT INTO leagues (id, display_name, collection_state, prepared_at, activated_at)
		VALUES ($1, $1, 'collecting', NOW(), NOW())`, collectedLeague); err != nil {
		t.Fatalf("register collected league: %v", err)
	}
	if _, err := tx.Exec(ctx, `
		INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings)
		VALUES ($1, '2099-01-01 00:00:00+00', 'POE-119 Test Gem', '1/20', FALSE, 1, 1)`, collectedLeague); err != nil {
		t.Fatalf("insert collected-league snapshot: %v", err)
	}

	var activeLeague, storedLeague string
	if err := tx.QueryRow(ctx, "SELECT active_league FROM runtime_config WHERE singleton = TRUE").Scan(&activeLeague); err != nil {
		t.Fatalf("read active league: %v", err)
	}
	if err := tx.QueryRow(ctx, `SELECT league FROM gem_snapshots WHERE name = 'POE-119 Test Gem'`).Scan(&storedLeague); err != nil {
		t.Fatalf("read collected-league snapshot: %v", err)
	}
	if activeLeague != "Mirage" || storedLeague != collectedLeague {
		t.Errorf("active/stored league = (%q, %q), want (Mirage, %q)", activeLeague, storedLeague, collectedLeague)
	}
}

func TestScopedRelationsHaveLeagueIdentityAndPrimaryKeys(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	relations := map[string][]string{
		"currency_snapshots":   {"league", "time", "currency_id"},
		"dedication_snapshots": {"league", "time", "color", "gem_type", "mode"},
		"font_snapshots":       {"league", "time", "color", "variant", "mode"},
		"fragment_snapshots":   {"league", "time", "fragment_id"},
		"gem_features":         {"league", "time", "name", "variant"},
		"gem_signals":          {"league", "time", "name", "variant"},
		"gem_snapshots":        {"league", "time", "name", "variant", "is_corrupted"},
		"market_context":       {"league", "time"},
		"quality_results":      {"league", "time", "name", "level"},
		"trade_lookups":        {"league", "time", "gem", "variant"},
		"transfigure_results":  {"league", "time", "transfigured_name", "variant"},
		"trend_results":        {"league", "time", "name", "variant"},
	}

	for relation, wantPrimaryKey := range relations {
		t.Run(relation, func(t *testing.T) {
			var nullable string
			err := pool.QueryRow(context.Background(), `
				SELECT is_nullable
				FROM information_schema.columns
				WHERE table_schema = 'public' AND table_name = $1 AND column_name = 'league'`, relation).Scan(&nullable)
			if err != nil {
				t.Fatalf("find league column: %v", err)
			}
			if nullable != "NO" {
				t.Errorf("league column nullable = %q, want NO", nullable)
			}

			rows, err := pool.Query(context.Background(), `
				SELECT key_columns.column_name
				FROM information_schema.table_constraints AS constraints
				JOIN information_schema.key_column_usage AS key_columns
					ON key_columns.constraint_name = constraints.constraint_name
					AND key_columns.table_schema = constraints.table_schema
				WHERE constraints.table_schema = 'public'
					AND constraints.table_name = $1
					AND constraints.constraint_type = 'PRIMARY KEY'
				ORDER BY key_columns.ordinal_position`, relation)
			if err != nil {
				t.Fatalf("query primary key: %v", err)
			}
			defer rows.Close()

			var gotPrimaryKey []string
			for rows.Next() {
				var column string
				if err := rows.Scan(&column); err != nil {
					t.Fatalf("scan primary key column: %v", err)
				}
				gotPrimaryKey = append(gotPrimaryKey, column)
			}
			if err := rows.Err(); err != nil {
				t.Fatalf("iterate primary key columns: %v", err)
			}
			if fmt.Sprint(gotPrimaryKey) != fmt.Sprint(wantPrimaryKey) {
				t.Errorf("primary key = %v, want %v", gotPrimaryKey, wantPrimaryKey)
			}

			var nullLeagues int
			query := fmt.Sprintf("SELECT count(*) FROM %s WHERE league IS NULL", relation)
			if err := pool.QueryRow(context.Background(), query).Scan(&nullLeagues); err != nil {
				t.Fatalf("count rows without league: %v", err)
			}
			if nullLeagues != 0 {
				t.Errorf("rows without league = %d, want 0", nullLeagues)
			}
		})
	}
}

func TestGemSnapshotsAllowSameIdentityAcrossLeaguesAndRejectDuplicatesWithinOneLeague(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	ctx := context.Background()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatalf("begin transaction: %v", err)
	}
	defer tx.Rollback(ctx)

	const secondLeague = "POE-119-Duplicate-Test"
	if _, err := tx.Exec(ctx, `
		INSERT INTO leagues (id, display_name, collection_state)
		VALUES ($1, $1, 'collecting')`, secondLeague); err != nil {
		t.Fatalf("create second league: %v", err)
	}

	insert := `
		INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings)
		VALUES ($1, '2099-01-02 00:00:00+00', 'POE-119 Duplicate Gem', '1/20', FALSE, 1, 1)`
	for _, leagueID := range []string{"Mirage", secondLeague} {
		if _, err := tx.Exec(ctx, insert, leagueID); err != nil {
			t.Fatalf("insert raw identity for %s: %v", leagueID, err)
		}
	}
	_, err = tx.Exec(ctx, insert, secondLeague)
	if err == nil {
		t.Fatal("duplicate raw gem identity within one league was accepted")
	}
	var dbErr *pgconn.PgError
	if !errors.As(err, &dbErr) || dbErr.Code != "23505" {
		t.Errorf("duplicate insert error = %v, want PostgreSQL unique violation", err)
	}
}

func TestGemContinuousAggregatesPreserveLeagueAndCorruption(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	ctx := context.Background()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatalf("begin transaction: %v", err)
	}
	defer tx.Rollback(ctx)

	const secondLeague = "POE-119-Aggregate-Test"
	if _, err := tx.Exec(ctx, `INSERT INTO leagues (id, display_name, collection_state) VALUES ($1, $1, 'collecting')`, secondLeague); err != nil {
		t.Fatalf("create aggregate test league: %v", err)
	}
	const snapshotName = "POE-119 Aggregate Gem"
	snapshotTime := time.Date(2099, 1, 3, 1, 15, 0, 0, time.UTC)
	for _, row := range []struct {
		league      string
		isCorrupted bool
		chaos       int
	}{
		{"Mirage", false, 10},
		{"Mirage", true, 20},
		{secondLeague, false, 30},
	} {
		if _, err := tx.Exec(ctx, `
			INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings)
			VALUES ($1, $2, $3, '1/20', $4, $5, 1)`, row.league, snapshotTime, snapshotName, row.isCorrupted, row.chaos); err != nil {
			t.Fatalf("insert aggregate input for %s corrupted=%t: %v", row.league, row.isCorrupted, err)
		}
	}

	for _, aggregate := range []string{"gem_snapshots_hourly", "gem_snapshots_daily"} {
		t.Run(aggregate, func(t *testing.T) {
			rows, err := tx.Query(ctx, fmt.Sprintf(`
				SELECT league, is_corrupted, avg_chaos::float8
				FROM %s
				WHERE name = $1 AND variant = '1/20'
					AND bucket = time_bucket(CASE WHEN $2 = 'gem_snapshots_hourly' THEN '1 hour'::interval ELSE '1 day'::interval END, $3)
				ORDER BY league, is_corrupted`, aggregate), snapshotName, aggregate, snapshotTime)
			if err != nil {
				t.Fatalf("query %s: %v", aggregate, err)
			}
			defer rows.Close()

			got := map[string]float64{}
			for rows.Next() {
				var league string
				var corrupted bool
				var chaos float64
				if err := rows.Scan(&league, &corrupted, &chaos); err != nil {
					t.Fatalf("scan aggregate row: %v", err)
				}
				got[fmt.Sprintf("%s/%t", league, corrupted)] = chaos
			}
			if err := rows.Err(); err != nil {
				t.Fatalf("iterate aggregate rows: %v", err)
			}
			want := map[string]float64{
				"Mirage/false":          10,
				"Mirage/true":           20,
				secondLeague + "/false": 30,
			}
			if fmt.Sprint(got) != fmt.Sprint(want) {
				t.Errorf("aggregate rows = %v, want %v", got, want)
			}
		})
	}
}

func TestLeagueMigrationsRestoreRequiredPoliciesAndRemoveLegacyRelations(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	compressedRelations := []string{
		"currency_snapshots", "dedication_snapshots", "font_snapshots", "fragment_snapshots",
		"gem_features", "gem_signals", "gem_snapshots", "market_context", "quality_results",
		"trade_lookups", "transfigure_results", "trend_results",
	}
	rows, err := pool.Query(context.Background(), `
		SELECT hypertable_name
		FROM timescaledb_information.jobs
		WHERE proc_name = 'policy_compression'
		ORDER BY hypertable_name`)
	if err != nil {
		t.Fatalf("query compression policies: %v", err)
	}
	defer rows.Close()
	var gotCompressed []string
	for rows.Next() {
		var relation string
		if err := rows.Scan(&relation); err != nil {
			t.Fatalf("scan compression policy: %v", err)
		}
		gotCompressed = append(gotCompressed, relation)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate compression policies: %v", err)
	}
	for _, relation := range compressedRelations {
		if !contains(gotCompressed, relation) {
			t.Errorf("missing compression policy for %s; got %v", relation, gotCompressed)
		}
	}

	rows, err = pool.Query(context.Background(), `
		SELECT aggregates.view_name
		FROM timescaledb_information.continuous_aggregates AS aggregates
		JOIN timescaledb_information.jobs AS jobs
			ON jobs.hypertable_schema = aggregates.materialization_hypertable_schema
			AND jobs.hypertable_name = aggregates.materialization_hypertable_name
		WHERE jobs.proc_name = 'policy_refresh_continuous_aggregate'
			AND aggregates.view_name IN ('gem_snapshots_hourly', 'gem_snapshots_daily')`)
	if err != nil {
		t.Fatalf("query continuous aggregate refresh policies: %v", err)
	}
	defer rows.Close()
	var gotRefreshPolicies []string
	for rows.Next() {
		var aggregate string
		if err := rows.Scan(&aggregate); err != nil {
			t.Fatalf("scan continuous aggregate refresh policy: %v", err)
		}
		gotRefreshPolicies = append(gotRefreshPolicies, aggregate)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate continuous aggregate refresh policies: %v", err)
	}
	for _, aggregate := range []string{"gem_snapshots_hourly", "gem_snapshots_daily"} {
		if !contains(gotRefreshPolicies, aggregate) {
			t.Errorf("missing refresh policy for %s; got %v", aggregate, gotRefreshPolicies)
		}
	}

	for _, relation := range []string{"exchange_snapshots", "gcp_snapshots"} {
		var exists bool
		if err := pool.QueryRow(context.Background(), `
			SELECT EXISTS (
				SELECT 1 FROM information_schema.tables
				WHERE table_schema = 'public' AND table_name = $1)`, relation).Scan(&exists); err != nil {
			t.Fatalf("check removed relation %s: %v", relation, err)
		}
		if exists {
			t.Errorf("removed relation %s still exists", relation)
		}
	}
}

func TestLeagueScopedSeedScriptsInsertMirageRows(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	ctx := context.Background()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatalf("begin transaction: %v", err)
	}
	defer tx.Rollback(ctx)

	seeds := []struct {
		file     string
		relation string
	}{
		{"002_gem_snapshots.sql", "gem_snapshots"},
		{"003_font_snapshots.sql", "font_snapshots"},
		{"004_currency_snapshots.sql", "currency_snapshots"},
	}
	for _, seed := range seeds {
		t.Run(seed.file, func(t *testing.T) {
			content, err := os.ReadFile(filepath.Join("..", "..", "..", "db", "seeds", seed.file))
			if err != nil {
				t.Fatalf("read seed script: %v", err)
			}
			if _, err := tx.Exec(ctx, string(content)); err != nil {
				t.Fatalf("execute seed script: %v", err)
			}

			var total, mirage int
			query := fmt.Sprintf("SELECT count(*), count(*) FILTER (WHERE league = 'Mirage') FROM %s", seed.relation)
			if err := tx.QueryRow(ctx, query).Scan(&total, &mirage); err != nil {
				t.Fatalf("count seeded rows: %v", err)
			}
			if total == 0 || total != mirage {
				t.Errorf("Mirage-scoped rows = %d of %d, want all seeded rows scoped to Mirage", mirage, total)
			}
		})
	}
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
