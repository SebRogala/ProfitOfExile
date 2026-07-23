//go:build integration

package migrations_test

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
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

func TestLeagueMigrationsPreserveLegacySchemaContract(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateToPreLeagueSchema(t, m)

	legacyTime := time.Date(2099, 1, 1, 0, 0, 0, 0, time.UTC)
	insertLegacyScopedRows(t, pool, legacyTime)

	compressionBefore := scopedPolicyConfigurations(t, pool, "policy_compression", scopedRelations)
	retentionBefore := scopedPolicyConfigurations(t, pool, "policy_retention", retainedRelations)
	refreshBefore := continuousAggregateRefreshConfigurations(t, pool)
	assertPolicyCoverage(t, "compression", compressionBefore, scopedRelations)
	assertPolicyCoverage(t, "retention", retentionBefore, retainedRelations)
	assertPolicyCoverage(t, "continuous aggregate refresh", refreshBefore, continuousAggregates)
	if _, ok := retentionBefore["trade_lookups"]; ok {
		t.Fatal("trade_lookups unexpectedly had a retention policy before POE-119")
	}

	migrateUp(t, m)

	assertLegacyRowsBackfilledToMirage(t, pool, legacyTime)
	assertLegacyRelationsRemoved(t, pool)
	compressionAfter := scopedPolicyConfigurations(t, pool, "policy_compression", scopedRelations)
	retentionAfter := scopedPolicyConfigurations(t, pool, "policy_retention", retainedRelations)
	refreshAfter := continuousAggregateRefreshConfigurations(t, pool)
	assertPolicyCoverage(t, "compression", compressionAfter, scopedRelations)
	assertPolicyCoverage(t, "retention", retentionAfter, retainedRelations)
	assertPolicyCoverage(t, "continuous aggregate refresh", refreshAfter, continuousAggregates)
	if !reflect.DeepEqual(compressionAfter, compressionBefore) {
		t.Errorf("compression policies after migration = %v, want unchanged %v", compressionAfter, compressionBefore)
	}
	if !reflect.DeepEqual(retentionAfter, retentionBefore) {
		t.Errorf("retention policies after migration = %v, want unchanged %v", retentionAfter, retentionBefore)
	}
	if !reflect.DeepEqual(refreshAfter, refreshBefore) {
		t.Errorf("continuous aggregate refresh policies after migration = %v, want unchanged %v", refreshAfter, refreshBefore)
	}
	if _, ok := compressionAfter["trade_lookups"]; !ok {
		t.Error("trade_lookups lost its compression policy")
	}
	if _, ok := retentionAfter["trade_lookups"]; ok {
		t.Error("trade_lookups unexpectedly gained a retention policy")
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

			var unknownLeagues int
			query = fmt.Sprintf(`
				SELECT count(*)
				FROM %s AS scoped_rows
				LEFT JOIN leagues ON leagues.id = scoped_rows.league
				WHERE leagues.id IS NULL`, relation)
			if err := pool.QueryRow(context.Background(), query).Scan(&unknownLeagues); err != nil {
				t.Fatalf("count rows with unknown league: %v", err)
			}
			if unknownLeagues != 0 {
				t.Errorf("rows with unknown league = %d, want 0", unknownLeagues)
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

const preLeagueSchemaVersion uint = 20260412224254

var scopedRelations = []string{
	"currency_snapshots", "dedication_snapshots", "font_snapshots", "fragment_snapshots",
	"gem_features", "gem_signals", "gem_snapshots", "market_context", "quality_results",
	"trade_lookups", "transfigure_results", "trend_results",
}

var retainedRelations = []string{
	"currency_snapshots", "dedication_snapshots", "font_snapshots", "fragment_snapshots",
	"gem_features", "gem_signals", "gem_snapshots", "market_context", "quality_results",
	"transfigure_results", "trend_results",
}

var continuousAggregates = []string{"gem_snapshots_hourly", "gem_snapshots_daily"}

func migrateToPreLeagueSchema(t *testing.T, m *migrate.Migrate) {
	t.Helper()
	if err := m.Migrate(preLeagueSchemaVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-POE-119 schema: %v", err)
	}
}

func insertLegacyScopedRows(t *testing.T, pool *pgxpool.Pool, legacyTime time.Time) {
	t.Helper()

	for relation, statement := range map[string]string{
		"currency_snapshots":   `INSERT INTO currency_snapshots (time, currency_id, chaos, volume, sparkline_change) VALUES ($1, 'POE-119 Legacy Currency', 1, 2, 3)`,
		"dedication_snapshots": `INSERT INTO dedication_snapshots (time, color, gem_type, mode, pool, winners) VALUES ($1, 'POE-119 Legacy Dedication', 'skill', 'safe', 1, 1)`,
		"font_snapshots":       `INSERT INTO font_snapshots (time, color, variant, mode, pool, winners) VALUES ($1, 'POE-119 Legacy Font', '1/20', 'safe', 1, 1)`,
		"fragment_snapshots":   `INSERT INTO fragment_snapshots (time, fragment_id, chaos, sparkline_change) VALUES ($1, 'POE-119 Legacy Fragment', 1, 2)`,
		"gem_features":         `INSERT INTO gem_features (time, name, variant, chaos, listings) VALUES ($1, 'POE-119 Legacy Feature', '1/20', 1, 1)`,
		"gem_signals":          `INSERT INTO gem_signals (time, name, variant, signal, confidence) VALUES ($1, 'POE-119 Legacy Signal', '1/20', 'STABLE', 1)`,
		"gem_snapshots":        `INSERT INTO gem_snapshots (time, name, variant, is_corrupted, chaos, listings) VALUES ($1, 'POE-119 Legacy Snapshot', '1/20', FALSE, 1, 1)`,
		"market_context":       `INSERT INTO market_context (time, total_gems, total_listings) VALUES ($1, 1, 1)`,
		"quality_results":      `INSERT INTO quality_results (time, name, level, buy_price, price_q20) VALUES ($1, 'POE-119 Legacy Quality', 20, 1, 2)`,
		"trade_lookups":        `INSERT INTO trade_lookups (time, gem, variant, total_listings, price_floor) VALUES ($1, 'POE-119 Legacy Trade', '1/20', 1, 1)`,
		"transfigure_results":  `INSERT INTO transfigure_results (time, base_name, transfigured_name, variant, base_price, transfigured_price) VALUES ($1, 'POE-119 Legacy Base', 'POE-119 Legacy Transfigure', '1/20', 1, 2)`,
		"trend_results":        `INSERT INTO trend_results (time, name, variant, current_price, current_listings) VALUES ($1, 'POE-119 Legacy Trend', '1/20', 1, 1)`,
	} {
		if _, err := pool.Exec(context.Background(), statement, legacyTime); err != nil {
			t.Fatalf("insert legacy %s row: %v", relation, err)
		}
	}
}

func assertLegacyRowsBackfilledToMirage(t *testing.T, pool *pgxpool.Pool, legacyTime time.Time) {
	t.Helper()

	for _, relation := range scopedRelations {
		t.Run(relation, func(t *testing.T) {
			var total, mirage int
			query := fmt.Sprintf(`SELECT count(*), count(*) FILTER (WHERE league = 'Mirage') FROM %s WHERE time = $1`, relation)
			if err := pool.QueryRow(context.Background(), query, legacyTime).Scan(&total, &mirage); err != nil {
				t.Fatalf("count migrated legacy rows: %v", err)
			}
			if total != 1 || mirage != 1 {
				t.Errorf("Mirage-backed legacy rows = %d of %d, want 1 of 1", mirage, total)
			}
		})
	}
}

func assertLegacyRelationsRemoved(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()

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

func scopedPolicyConfigurations(t *testing.T, pool *pgxpool.Pool, policy string, relations []string) map[string]string {
	t.Helper()

	rows, err := pool.Query(context.Background(), `
		SELECT hypertable_name,
			jsonb_build_object(
				'config', config - 'hypertable_id',
				'schedule_interval', schedule_interval
			)::text
		FROM timescaledb_information.jobs
		WHERE proc_name = $1 AND hypertable_name = ANY($2)
		ORDER BY hypertable_name`, policy, relations)
	if err != nil {
		t.Fatalf("query %s policies: %v", policy, err)
	}
	defer rows.Close()

	policies := make(map[string]string)
	for rows.Next() {
		var relation, configuration string
		if err := rows.Scan(&relation, &configuration); err != nil {
			t.Fatalf("scan %s policy: %v", policy, err)
		}
		policies[relation] = configuration
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate %s policies: %v", policy, err)
	}
	return policies
}

func continuousAggregateRefreshConfigurations(t *testing.T, pool *pgxpool.Pool) map[string]string {
	t.Helper()

	rows, err := pool.Query(context.Background(), `
		SELECT aggregates.view_name,
			jsonb_build_object(
				'config', jobs.config - 'mat_hypertable_id' - 'hypertable_id',
				'schedule_interval', jobs.schedule_interval
			)::text
		FROM timescaledb_information.continuous_aggregates AS aggregates
		JOIN timescaledb_information.jobs AS jobs
			ON jobs.hypertable_schema = aggregates.materialization_hypertable_schema
			AND jobs.hypertable_name = aggregates.materialization_hypertable_name
		WHERE jobs.proc_name = 'policy_refresh_continuous_aggregate'
			AND aggregates.view_name = ANY($1)
		ORDER BY aggregates.view_name`, continuousAggregates)
	if err != nil {
		t.Fatalf("query continuous aggregate refresh policies: %v", err)
	}
	defer rows.Close()

	policies := make(map[string]string)
	for rows.Next() {
		var aggregate, configuration string
		if err := rows.Scan(&aggregate, &configuration); err != nil {
			t.Fatalf("scan continuous aggregate refresh policy: %v", err)
		}
		policies[aggregate] = configuration
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate continuous aggregate refresh policies: %v", err)
	}
	return policies
}

func assertPolicyCoverage(t *testing.T, policy string, got map[string]string, want []string) {
	t.Helper()

	if len(got) != len(want) {
		t.Errorf("%s policy coverage = %v, want %v", policy, got, want)
	}
	for _, relation := range want {
		if _, ok := got[relation]; !ok {
			t.Errorf("missing %s policy for %s; got %v", policy, relation, got)
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
