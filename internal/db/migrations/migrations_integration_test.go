//go:build integration

package migrations_test

import (
	"context"
	"errors"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"reflect"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/golang-migrate/migrate/v4"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
	"profitofexile/internal/lab"
	"profitofexile/internal/league"
	"profitofexile/internal/trade"
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

	// POE-399 later replaces this contract (21-day retention, aggregates and
	// trend_results dropped); it is proven at the last version where it holds.
	if err := m.Migrate(preMarketRetentionVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-POE-399 schema: %v", err)
	}

	assertLegacyRowsBackfilledToMirage(t, pool, legacyTime)
	assertLegacyRelationsRemoved(t, pool)
	compressionAfter := scopedPolicyConfigurations(t, pool, "policy_compression", scopedRelations)
	retentionAfter := scopedPolicyConfigurations(t, pool, "policy_retention", retainedRelations)
	refreshAfter := continuousAggregateRefreshConfigurations(t, pool)
	assertPolicyCoverage(t, "compression", compressionAfter, scopedRelations)
	assertPolicyCoverage(t, "continuous aggregate refresh", refreshAfter, continuousAggregates)
	if !reflect.DeepEqual(compressionAfter, compressionBefore) {
		t.Errorf("compression policies after migration = %v, want unchanged %v", compressionAfter, compressionBefore)
	}
	if !reflect.DeepEqual(refreshAfter, refreshBefore) {
		t.Errorf("continuous aggregate refresh policies after migration = %v, want unchanged %v", refreshAfter, refreshBefore)
	}
	if len(retentionAfter) != 0 {
		t.Errorf("retention policies after migration = %v, want none so archived league history survives", retentionAfter)
	}
	if _, ok := compressionAfter["trade_lookups"]; !ok {
		t.Error("trade_lookups lost its compression policy")
	}
}

func TestMigrationsApply(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	var strategies, index, gemIndex, trigger bool
	for name, dest := range map[string]*bool{
		"strategies":            &strategies,
		"idx_strategies_league": &index,
		"idx_gem_snapshots_league_transfigured_name": &gemIndex,
		"trg_strategies_updated_at":                  &trigger,
	} {
		var query string
		switch name {
		case "strategies":
			query = "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)"
		case "idx_strategies_league", "idx_gem_snapshots_league_transfigured_name":
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
		"boss_loot_runs": {"league", "id"},
		// currency_exchange_cursor is the one entry that is not a hypertable: one
		// row per league, keyed by the league alone.
		"currency_exchange_cursor":  {"league"},
		"currency_exchange_markets": {"league", "time", "market_id"},

		"currency_snapshots":       {"league", "time", "currency_id"},
		"dedication_snapshots":     {"league", "time", "variant", "color", "gem_type", "mode"},
		"double_corrupt_snapshots": {"league", "time", "name", "input_variant"},
		"font_snapshots":           {"league", "time", "color", "variant", "mode"},
		"fragment_snapshots":       {"league", "time", "fragment_id"},
		"gem_features":             {"league", "time", "name", "variant"},
		"gem_signals":              {"league", "time", "name", "variant"},
		"gem_snapshots":            {"league", "time", "name", "variant", "is_corrupted"},
		"item_snapshots":           {"league", "time", "category", "ninja_id"},
		"market_context":           {"league", "time"},
		"quality_results":          {"league", "time", "name", "level"},
		"trade_lookups":            {"league", "time", "gem", "variant"},
		"transfigure_results":      {"league", "time", "transfigured_name", "variant"},
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

func TestScopedRelationsReferenceTheLeagueRegistry(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	migrateUp(t, m)

	for _, relation := range leagueRegistryRelations {
		t.Run(relation, func(t *testing.T) {
			var referenced, column string
			err := pool.QueryRow(context.Background(), `
				SELECT confrelid::REGCLASS::TEXT, attributes.attname
				FROM pg_constraint AS constraints
				JOIN pg_attribute AS attributes
					ON attributes.attrelid = constraints.conrelid
					AND attributes.attnum = constraints.conkey[1]
				WHERE constraints.conrelid = $1::REGCLASS
					AND constraints.contype = 'f'
					AND attributes.attname = 'league'
					AND array_length(constraints.conkey, 1) = 1`, relation).Scan(&referenced, &column)
			if err != nil {
				t.Fatalf("find foreign key on %s: %v", relation, err)
			}
			if referenced != "leagues" || column != "league" {
				t.Errorf("foreign key = %s(%s), want leagues(league)", referenced, column)
			}
		})
	}
}

func TestGemSnapshotsRejectAnUnregisteredLeague(t *testing.T) {
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

	_, err = tx.Exec(ctx, `
		INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings)
		VALUES ('POE-119-Never-Registered', '2099-01-03 00:00:00+00', 'POE-119 Unregistered Gem', '1/20', FALSE, 1, 1)`)
	if err == nil {
		t.Fatal("snapshot for an unregistered league was accepted")
	}
	var dbErr *pgconn.PgError
	if !errors.As(err, &dbErr) || dbErr.Code != "23503" {
		t.Errorf("unregistered-league insert error = %v, want PostgreSQL foreign key violation", err)
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

const preLeagueSchemaVersion uint = 20260412224254

// preMarketRetentionVersion is the last version before POE-399's 21-day
// market retention migration.
const preMarketRetentionVersion uint = 20261113140000

// scopedRelations is the set of league-scoped relations that PREDATE the league
// migration: every one of them existed at preLeagueSchemaVersion, was backfilled
// to Mirage by the up-walk, and is a hypertable carrying a compression policy.
// The legacy-contract test asserts all three of those properties, so a relation
// created after that migration does not belong here — put it in
// leagueRegistryRelations, which asserts only the foreign key.
var scopedRelations = []string{
	"currency_snapshots", "dedication_snapshots", "font_snapshots", "fragment_snapshots",
	"gem_features", "gem_signals", "gem_snapshots", "market_context", "quality_results",
	"trade_lookups", "transfigure_results", "trend_results",
}

// leagueRegistryRelations is every relation whose league column is a foreign key
// into the leagues registry: the legacy set plus the two currency-exchange tables
// added by POE-173, double_corrupt_snapshots added by POE-125, item_snapshots
// added by POE-254 and retained boss_loot_runs added by POE-317.
// currency_exchange_cursor is a plain table and the other three are younger than
// the backfill, so none of the four can join scopedRelations — the FK is the
// one contract all of them share. trend_results is left out: POE-399 drops it.
var leagueRegistryRelations = append(slices.DeleteFunc(slices.Clone(scopedRelations),
	func(relation string) bool { return relation == "trend_results" }),
	"currency_exchange_markets", "currency_exchange_cursor",
	"double_corrupt_snapshots", "item_snapshots", "boss_loot_runs")

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
		JOIN _timescaledb_catalog.continuous_agg AS continuous_aggregate_catalog
			ON continuous_aggregate_catalog.user_view_schema = aggregates.view_schema
			AND continuous_aggregate_catalog.user_view_name = aggregates.view_name
		JOIN timescaledb_information.jobs AS jobs
			ON (jobs.config->>'mat_hypertable_id')::integer = continuous_aggregate_catalog.mat_hypertable_id
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

// Keep this last: TestLeagueMigrationsPreserveLegacySchemaContract must run
// before any test applies the forward-only corruption-identity migration.
func TestGemSnapshotNinjaSampleCompressedHistoryMigration(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	ctx := context.Background()
	const previousVersion uint = 20260906024420
	const name = "Ninja sample compressed history"
	// Isolate the fixture from the existing 2099 migration fixtures' chunks.
	snapshotTime := time.Date(2101, 1, 1, 0, 0, 0, 0, time.UTC)
	var chunk string
	defer func() {
		if chunk != "" {
			if _, err := pool.Exec(ctx, `SELECT decompress_chunk($1::regclass, if_compressed => true)`, chunk); err != nil {
				t.Errorf("decompress fixture chunk: %v", err)
			}
		}
		if _, err := pool.Exec(ctx, `DELETE FROM gem_snapshots WHERE league = 'Mirage' AND time = $1 AND name = $2`, snapshotTime, name); err != nil {
			t.Errorf("delete historical fixture: %v", err)
		}
		if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
			t.Errorf("restore fully migrated schema: %v", err)
		}
	}()
	if err := m.Migrate(previousVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-sample version: %v", err)
	}
	if err := pool.QueryRow(ctx, `
		INSERT INTO gem_snapshots (league, time, name, variant, is_corrupted, chaos, listings)
		VALUES ('Mirage', $1, $2, '20/20', false, 1817, 9)
		RETURNING tableoid::regclass::text`, snapshotTime, name).Scan(&chunk); err != nil {
		t.Fatalf("insert pre-sample row: %v", err)
	}
	if _, err := pool.Exec(ctx, `SELECT compress_chunk($1::regclass)`, chunk); err != nil {
		t.Fatalf("compress historical chunk: %v", err)
	}
	var compressed bool
	if err := pool.QueryRow(ctx, `
		SELECT is_compressed FROM timescaledb_information.chunks
		WHERE format('%I.%I', chunk_schema, chunk_name)::regclass = $1::regclass`, chunk).Scan(&compressed); err != nil {
		t.Fatalf("check historical compression: %v", err)
	}
	if !compressed {
		t.Fatal("historical row's chunk is not compressed before migration")
	}
	migrateUp(t, m)
	for _, column := range []string{"sample_count", "sparkline_empty"} {
		t.Run(column+"_historical_null", func(t *testing.T) {
			var isNull bool
			query := fmt.Sprintf(`SELECT %s IS NULL FROM gem_snapshots WHERE league = 'Mirage' AND time = $1 AND name = $2`, column)
			if err := pool.QueryRow(ctx, query, snapshotTime, name).Scan(&isNull); err != nil {
				t.Fatalf("read historical %s: %v", column, err)
			}
			if !isNull {
				t.Errorf("historical %s is non-NULL, want unknown (SQL NULL)", column)
			}
		})
	}
	if err := m.Migrate(previousVersion); err != nil {
		t.Fatalf("roll back sample migration: %v", err)
	}
	for _, column := range []string{"sample_count", "sparkline_empty"} {
		t.Run(column+"_removed_on_rollback", func(t *testing.T) {
			var columns int
			if err := pool.QueryRow(ctx, `
				SELECT count(*) FROM information_schema.columns
				WHERE table_schema = 'public' AND table_name = 'gem_snapshots' AND column_name = $1`, column).Scan(&columns); err != nil {
				t.Fatalf("lookup rolled-back %s: %v", column, err)
			}
			if columns != 0 {
				t.Errorf("%s columns after rollback = %d, want 0", column, columns)
			}
		})
	}
}

// POE-394: the unique_accounts migration keeps each stored row's seller
// concentration while removing every seller name from trade_lookups.listings.
func TestTradeLookupsUniqueAccountsMigrationStripsNamesAndKeepsConcentration(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	ctx := context.Background()
	const previousVersion uint = 20261113002101
	const gem = "POE394 Seller Count Fixture"
	const variant = "20/20"
	scope := league.Historical("Mirage")
	// Isolated from the 2099 and 2101 migration fixtures' chunks.
	lookupTime := time.Date(2102, 1, 1, 0, 0, 0, 0, time.UTC)
	// Ten listings, cheapest first. Sellers: A, B, C, D, E, "" (an explicit "",
	// a JSON null and a missing key all count as the one seller "" under the
	// pre-change Go rule) = 6 distinct.
	listings := `[
		{"price":10,"currency":"chaos","chaosPrice":10,"account":"SellerA","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":11,"currency":"chaos","chaosPrice":11,"account":"SellerB","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":12,"currency":"chaos","chaosPrice":12,"account":null,"indexedAt":"2026-01-01T00:00:00Z"},
		{"price":13,"currency":"chaos","chaosPrice":13,"account":"SellerC","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":14,"currency":"chaos","chaosPrice":14,"account":"SellerD","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":15,"currency":"chaos","chaosPrice":15,"account":"SellerE","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":16,"currency":"chaos","chaosPrice":16,"account":"","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":17,"currency":"chaos","chaosPrice":17,"indexedAt":"2026-01-01T00:00:00Z"},
		{"price":18,"currency":"chaos","chaosPrice":18,"account":"SellerB","indexedAt":"2026-01-01T00:00:00Z"},
		{"price":19,"currency":"chaos","chaosPrice":19,"account":"SellerA","indexedAt":"2026-01-01T00:00:00Z"}
	]`
	var chunk string
	defer func() {
		if chunk != "" {
			if _, err := pool.Exec(ctx, `SELECT decompress_chunk($1::regclass, if_compressed => true)`, chunk); err != nil {
				t.Errorf("decompress fixture chunk: %v", err)
			}
		}
		if _, err := pool.Exec(ctx, `DELETE FROM trade_lookups WHERE league = 'Mirage' AND time = $1 AND gem = $2`, lookupTime, gem); err != nil {
			t.Errorf("delete fixture row: %v", err)
		}
		if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
			t.Errorf("restore fully migrated schema: %v", err)
		}
	}()
	if err := m.Migrate(previousVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-unique-accounts version: %v", err)
	}
	if err := pool.QueryRow(ctx, `
		INSERT INTO trade_lookups (league, time, gem, variant, total_listings, price_floor, price_ceiling, median_top10, divine_rate, source, listings)
		VALUES ('Mirage', $1, $2, $3, 10, 10, 19, 14.5, 200, 'desktop', $4::jsonb)
		RETURNING tableoid::regclass::text`, lookupTime, gem, variant, listings).Scan(&chunk); err != nil {
		t.Fatalf("insert pre-migration row: %v", err)
	}
	if _, err := pool.Exec(ctx, `SELECT compress_chunk($1::regclass)`, chunk); err != nil {
		t.Fatalf("compress fixture chunk: %v", err)
	}
	migrateUp(t, m)

	t.Run("stored_row", func(t *testing.T) {
		var uniqueAccounts, withAccount int
		var prices []float64
		if err := pool.QueryRow(ctx, `
			SELECT unique_accounts,
			       (SELECT count(*) FROM jsonb_array_elements(listings) e WHERE e ? 'account'),
			       (SELECT array_agg((e->>'price')::float8 ORDER BY ord) FROM jsonb_array_elements(listings) WITH ORDINALITY AS t(e, ord))
			FROM trade_lookups WHERE league = 'Mirage' AND time = $1 AND gem = $2`, lookupTime, gem).Scan(&uniqueAccounts, &withAccount, &prices); err != nil {
			t.Fatalf("read migrated row: %v", err)
		}
		if uniqueAccounts != 6 {
			t.Errorf("unique_accounts = %d, want 6", uniqueAccounts)
		}
		if withAccount != 0 {
			t.Errorf("%d listings still carry an account key, want 0", withAccount)
		}
		want := []float64{10, 11, 12, 13, 14, 15, 16, 17, 18, 19}
		if !reflect.DeepEqual(prices, want) {
			t.Errorf("listing prices in stored order = %v, want %v", prices, want)
		}
	})

	t.Run("warm_cache_and_lab_features", func(t *testing.T) {
		results, err := trade.NewRepository(pool).LatestLookups(ctx, scope, 24)
		if err != nil {
			t.Fatalf("LatestLookups: %v", err)
		}
		var row []trade.TradeLookupResult
		for _, r := range results {
			if r.Gem == gem && r.Variant == variant {
				row = append(row, r)
			}
		}
		if len(row) != 1 {
			t.Fatalf("LatestLookups returned %d fixture rows, want 1", len(row))
		}
		cache := trade.NewTradeCache(10, scope)
		cache.Warm(row)
		cached, ok := cache.Get(trade.CacheKey(gem, variant))
		if !ok {
			t.Fatal("warm cache lost the fixture row")
		}
		// Pre-change rule on this row: 6 distinct sellers -> CONCENTRATED.
		if cached.Signals.SellerConcentration != "CONCENTRATED" || cached.Signals.UniqueAccounts != 6 {
			t.Errorf("warm signals = %s/%d, want CONCENTRATED/6", cached.Signals.SellerConcentration, cached.Signals.UniqueAccounts)
		}
		features := lab.ComputeGemFeatures(lookupTime.Add(time.Minute),
			[]lab.GemPrice{{Name: gem, Variant: variant, Chaos: 15, Listings: 10, IsTransfigured: true, GemColor: "BLUE"}}, nil, lab.MarketContext{}, nil, cache)
		if len(features) != 1 {
			t.Fatalf("ComputeGemFeatures returned %d features, want 1", len(features))
		}
		if features[0].TradeSellerConcentration != "CONCENTRATED" {
			t.Errorf("TradeSellerConcentration = %q, want CONCENTRATED", features[0].TradeSellerConcentration)
		}
		// Base 0.5+0.5*5/45 for 10 listings, then CONCENTRATED (x0.9) and STALE
		// (x0.9) = 0.45; a count read as 0 (MONOPOLY, x0.8) gives 0.40.
		const wantSellProbabilityFactor = 0.45
		if got := features[0].SellProbabilityFactor; math.Abs(got-wantSellProbabilityFactor) > 1e-9 {
			t.Errorf("SellProbabilityFactor = %v, want %v", got, wantSellProbabilityFactor)
		}
	})

	if err := m.Migrate(previousVersion); err != nil {
		t.Fatalf("roll back unique_accounts migration: %v", err)
	}
	var columns int
	if err := pool.QueryRow(ctx, `
		SELECT count(*) FROM information_schema.columns
		WHERE table_schema = 'public' AND table_name = 'trade_lookups' AND column_name = 'unique_accounts'`).Scan(&columns); err != nil {
		t.Fatalf("lookup rolled-back unique_accounts: %v", err)
	}
	if columns != 0 {
		t.Errorf("unique_accounts columns after rollback = %d, want 0", columns)
	}
}

func TestDeviceRetentionMigrationAllowsAnonymisedRows(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	ctx := context.Background()
	const previousVersion uint = 20261113130000
	const device = "poe396-retention-fixture"
	pastStart := time.Date(2025, 3, 1, 12, 0, 0, 0, time.UTC)
	futureStart := time.Now().Add(365 * 24 * time.Hour)

	var fontID, pastLabID, futureLabID, bossID, mercID int64
	cleanup := func() {
		for _, q := range []struct {
			sql string
			id  *int64
		}{
			{`DELETE FROM font_sessions WHERE id = $1`, &fontID},
			{`DELETE FROM lab_runs WHERE id = $1`, &pastLabID},
			{`DELETE FROM lab_runs WHERE id = $1`, &futureLabID},
			{`DELETE FROM boss_loot_runs WHERE id = $1`, &bossID},
			{`DELETE FROM merc_icon_templates WHERE id = $1`, &mercID},
		} {
			if _, err := pool.Exec(ctx, q.sql, *q.id); err != nil {
				t.Errorf("cleanup %q: %v", q.sql, err)
			}
		}
		if _, err := pool.Exec(ctx, `DELETE FROM devices WHERE fingerprint = $1`, device); err != nil {
			t.Errorf("cleanup device: %v", err)
		}
	}
	defer func() {
		cleanup()
		if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
			t.Errorf("restore fully migrated schema: %v", err)
		}
	}()

	if err := m.Migrate(previousVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-retention version: %v", err)
	}
	seed := []struct {
		name string
		sql  string
		args []any
		id   *int64
	}{
		{"device", `INSERT INTO devices (fingerprint) VALUES ($1) RETURNING 0`, []any{device}, new(int64)},
		{"font_sessions", `INSERT INTO font_sessions (lab_type, total_crafts, device_id, pair_code) VALUES ('Uber', 3, $1, 'P') RETURNING id`, []any{device}, &fontID},
		{"lab_runs past", `INSERT INTO lab_runs (device_id, difficulty, strategy, started_at, elapsed_seconds, room_count) VALUES ($1, 'Uber', 's', $2, 60, 5) RETURNING id`, []any{device, pastStart}, &pastLabID},
		{"lab_runs future", `INSERT INTO lab_runs (device_id, difficulty, strategy, started_at, elapsed_seconds, room_count) VALUES ($1, 'Uber', 's', $2, 60, 5) RETURNING id`, []any{device, futureStart}, &futureLabID},
		{"boss_loot_runs", `INSERT INTO boss_loot_runs (league, device_id, boss, profile_key, profile_version, run_number, items) VALUES ('Mirage', $1, 'b', 'k', 1, 1, '{"quantities":{},"counted":[]}') RETURNING id`, []any{device}, &bossID},
		{"merc_icon_templates", `INSERT INTO merc_icon_templates (family, tier, format_version, signature, device_id) VALUES ('f', 1, 1, decode(repeat('00', 576), 'hex'), $1) RETURNING id`, []any{device}, &mercID},
	}
	for _, s := range seed {
		if err := pool.QueryRow(ctx, s.sql, s.args...).Scan(s.id); err != nil {
			t.Fatalf("seed %s: %v", s.name, err)
		}
	}
	migrateBefore := time.Now()
	migrateUp(t, m)

	t.Run("lab_runs_created_at_backfills_past_start", func(t *testing.T) {
		var createdAt time.Time
		if err := pool.QueryRow(ctx, `SELECT created_at FROM lab_runs WHERE id = $1`, pastLabID).Scan(&createdAt); err != nil {
			t.Fatalf("read created_at: %v", err)
		}
		if !createdAt.Equal(pastStart) {
			t.Errorf("created_at = %v, want started_at %v", createdAt, pastStart)
		}
	})

	t.Run("lab_runs_created_at_caps_future_start_at_migration_time", func(t *testing.T) {
		var createdAt time.Time
		if err := pool.QueryRow(ctx, `SELECT created_at FROM lab_runs WHERE id = $1`, futureLabID).Scan(&createdAt); err != nil {
			t.Fatalf("read created_at: %v", err)
		}
		if createdAt.After(time.Now()) || createdAt.Before(migrateBefore.Add(-time.Minute)) {
			t.Errorf("created_at = %v, want the migration time (started_at was %v)", createdAt, futureStart)
		}
	})

	t.Run("boss_loot_runs_created_at_backfilled", func(t *testing.T) {
		var createdAt time.Time
		if err := pool.QueryRow(ctx, `SELECT created_at FROM boss_loot_runs WHERE id = $1`, bossID).Scan(&createdAt); err != nil {
			t.Fatalf("read created_at: %v", err)
		}
		if createdAt.After(time.Now()) || createdAt.Before(migrateBefore.Add(-time.Minute)) {
			t.Errorf("created_at = %v, want the migration time", createdAt)
		}
	})

	t.Run("boss_loot_fk_sets_null_on_device_delete", func(t *testing.T) {
		var delType string
		if err := pool.QueryRow(ctx, `
			SELECT confdeltype::text FROM pg_constraint
			WHERE conrelid = 'boss_loot_runs'::regclass AND contype = 'f' AND confrelid = 'devices'::regclass`).Scan(&delType); err != nil {
			t.Fatalf("read FK: %v", err)
		}
		if delType != "n" {
			t.Errorf("confdeltype = %q, want n (SET NULL)", delType)
		}
		tx, err := pool.Begin(ctx)
		if err != nil {
			t.Fatalf("begin: %v", err)
		}
		defer tx.Rollback(ctx)
		if _, err := tx.Exec(ctx, `DELETE FROM devices WHERE fingerprint = $1`, device); err != nil {
			t.Fatalf("delete device with boss-loot rows: %v", err)
		}
		var deviceID *string
		if err := tx.QueryRow(ctx, `SELECT device_id FROM boss_loot_runs WHERE id = $1`, bossID).Scan(&deviceID); err != nil {
			t.Fatalf("read boss-loot row: %v", err)
		}
		if deviceID != nil {
			t.Errorf("boss_loot_runs.device_id = %q after device delete, want NULL", *deviceID)
		}
	})

	anonymise := []struct {
		table, sql string
		id         *int64
	}{
		{"font_sessions", `UPDATE font_sessions SET device_id = NULL, pair_code = NULL WHERE id = $1`, &fontID},
		{"lab_runs", `UPDATE lab_runs SET device_id = NULL WHERE id = $1`, &pastLabID},
		{"boss_loot_runs", `UPDATE boss_loot_runs SET device_id = NULL WHERE id = $1`, &bossID},
		{"merc_icon_templates", `UPDATE merc_icon_templates SET device_id = NULL WHERE id = $1`, &mercID},
	}
	t.Run("device_columns_accept_null", func(t *testing.T) {
		for _, a := range anonymise {
			if _, err := pool.Exec(ctx, a.sql, *a.id); err != nil {
				t.Errorf("%s: store anonymised row: %v", a.table, err)
			}
		}
	})

	t.Run("down_refuses_while_anonymised_rows_exist", func(t *testing.T) {
		err := m.Migrate(previousVersion)
		if err == nil {
			t.Fatal("down succeeded with NULL device_id rows present, want failure")
		}
		if !strings.Contains(err.Error(), "font_sessions") {
			t.Errorf("down error = %v, want it to name font_sessions", err)
		}
		if _, err := pool.Exec(ctx, `DELETE FROM font_sessions WHERE id = $1`, fontID); err != nil {
			t.Fatalf("clear font_sessions: %v", err)
		}
		forceRetentionVersion(t, m)
		err = m.Migrate(previousVersion)
		if err == nil || !strings.Contains(err.Error(), "lab_runs") {
			t.Errorf("down error after clearing font_sessions = %v, want it to name lab_runs", err)
		}
		forceRetentionVersion(t, m)
	})

	t.Run("down_restores_schema_once_rows_are_cleared", func(t *testing.T) {
		for _, a := range anonymise[1:] {
			if _, err := pool.Exec(ctx, `DELETE FROM `+a.table+` WHERE id = $1`, *a.id); err != nil {
				t.Fatalf("clear %s: %v", a.table, err)
			}
		}
		if err := m.Migrate(previousVersion); err != nil {
			t.Fatalf("down after clearing NULL rows: %v", err)
		}
		var nullable []string
		rows, err := pool.Query(ctx, `
			SELECT table_name || '.' || column_name FROM information_schema.columns
			WHERE table_schema = 'public' AND is_nullable = 'YES'
			  AND (table_name, column_name) IN (('font_sessions','device_id'), ('font_sessions','pair_code'),
			      ('lab_runs','device_id'), ('boss_loot_runs','device_id'), ('merc_icon_templates','device_id'))`)
		if err != nil {
			t.Fatalf("read nullability: %v", err)
		}
		for rows.Next() {
			var c string
			if err := rows.Scan(&c); err != nil {
				t.Fatalf("scan: %v", err)
			}
			nullable = append(nullable, c)
		}
		rows.Close()
		if len(nullable) != 0 {
			t.Errorf("columns still nullable after down: %v", nullable)
		}
		var createdAtCols int
		if err := pool.QueryRow(ctx, `
			SELECT count(*) FROM information_schema.columns
			WHERE table_schema = 'public' AND column_name = 'created_at' AND table_name IN ('lab_runs', 'boss_loot_runs')`).Scan(&createdAtCols); err != nil {
			t.Fatalf("read created_at columns: %v", err)
		}
		if createdAtCols != 0 {
			t.Errorf("created_at columns after down = %d, want 0", createdAtCols)
		}
		var delType string
		if err := pool.QueryRow(ctx, `
			SELECT confdeltype::text FROM pg_constraint
			WHERE conrelid = 'boss_loot_runs'::regclass AND contype = 'f' AND confrelid = 'devices'::regclass`).Scan(&delType); err != nil {
			t.Fatalf("read FK: %v", err)
		}
		if delType != "a" {
			t.Errorf("confdeltype after down = %q, want a (NO ACTION)", delType)
		}
	})
}

// forceRetentionVersion clears the dirty flag a failed down migration leaves;
// the down's guard raises before any DDL, so the schema is still at the version.
func forceRetentionVersion(t *testing.T, m *migrate.Migrate) {
	t.Helper()
	if err := m.Force(20261113140000); err != nil {
		t.Fatalf("force retention version after failed down: %v", err)
	}
}

const marketRetentionUpFile = "migrations/20261113150000_market_data_retention.up.sql"

// marketRetentionTables are the 14 league-scoped hypertables POE-399 gives a
// 21-day retention policy; the registry's plain tables are not among them.
var marketRetentionTables = []string{
	"currency_exchange_markets", "currency_snapshots", "dedication_snapshots", "double_corrupt_snapshots",
	"font_snapshots", "fragment_snapshots", "gem_features", "gem_signals", "gem_snapshots",
	"item_snapshots", "market_context", "quality_results", "trade_lookups", "transfigure_results",
}

func TestMarketDataRetentionMigration(t *testing.T) {
	pool, m := testSetup(t)
	defer pool.Close()
	defer m.Close()
	requireTimescaleDB(t, pool)
	ctx := context.Background()
	defer func() {
		if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
			t.Errorf("restore fully migrated schema: %v", err)
		}
	}()

	if err := m.Migrate(preMarketRetentionVersion); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		t.Fatalf("migrate to pre-POE-399 schema: %v", err)
	}
	legacySchema := marketRetentionSchemaFacts(t, pool)
	legacyRefresh := continuousAggregateRefreshConfigurations(t, pool)
	legacyCompression := scopedPolicyConfigurations(t, pool, "policy_compression", []string{"trend_results"})
	// A surviving policy with another period must be replaced, not kept.
	if _, err := pool.Exec(ctx, `SELECT add_retention_policy('gem_snapshots', INTERVAL '90 days')`); err != nil {
		t.Fatalf("install stray 90-day retention policy: %v", err)
	}
	migrateUp(t, m)

	t.Run("each_market_hypertable_has_one_21_day_policy", func(t *testing.T) {
		assertMarketRetentionJobs(t, pool)
	})

	t.Run("aggregates_and_trend_results_are_dropped", func(t *testing.T) {
		assertMarketRetentionDrops(t, pool)
	})

	t.Run("rerunning_the_up_sql_keeps_one_21_day_policy", func(t *testing.T) {
		upSQL, err := db.MigrationsFS.ReadFile(marketRetentionUpFile)
		if err != nil {
			t.Fatalf("read %s: %v", marketRetentionUpFile, err)
		}
		if _, err := pool.Exec(ctx, string(upSQL)); err != nil {
			t.Fatalf("re-run %s: %v", marketRetentionUpFile, err)
		}
		assertMarketRetentionJobs(t, pool)
		assertMarketRetentionDrops(t, pool)
	})

	t.Run("down_restores_the_legacy_schema_without_retention", func(t *testing.T) {
		if err := m.Migrate(preMarketRetentionVersion); err != nil {
			t.Fatalf("migrate down through POE-399: %v", err)
		}
		if got := retentionJobDropAfter(t, pool); len(got) != 0 {
			t.Errorf("retention jobs after down = %v, want none", got)
		}
		if got := marketRetentionSchemaFacts(t, pool); !slices.Equal(got, legacySchema) {
			t.Errorf("schema after down =\n%s\nwant pre-POE-399\n%s", strings.Join(got, "\n"), strings.Join(legacySchema, "\n"))
		}
		if got := continuousAggregateRefreshConfigurations(t, pool); !reflect.DeepEqual(got, legacyRefresh) {
			t.Errorf("aggregate refresh policies after down = %v, want %v", got, legacyRefresh)
		}
		if got := scopedPolicyConfigurations(t, pool, "policy_compression", []string{"trend_results"}); !reflect.DeepEqual(got, legacyCompression) {
			t.Errorf("trend_results compression policy after down = %v, want %v", got, legacyCompression)
		}
	})
}

func assertMarketRetentionJobs(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	got := retentionJobDropAfter(t, pool)
	want := make(map[string][]string, len(marketRetentionTables))
	for _, table := range marketRetentionTables {
		want[table] = []string{"21 days"}
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("retention jobs (drop_after per table) = %v, want %v", got, want)
	}
}

func assertMarketRetentionDrops(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	for _, relation := range []string{"gem_snapshots_hourly", "gem_snapshots_daily", "trend_results"} {
		var present bool
		if err := pool.QueryRow(context.Background(), `SELECT to_regclass($1) IS NOT NULL`, relation).Scan(&present); err != nil {
			t.Fatalf("look up %s: %v", relation, err)
		}
		if present {
			t.Errorf("%s still exists", relation)
		}
	}
	var refreshJobs int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM timescaledb_information.jobs WHERE proc_name = 'policy_refresh_continuous_aggregate'`).Scan(&refreshJobs); err != nil {
		t.Fatalf("count aggregate refresh jobs: %v", err)
	}
	if refreshJobs != 0 {
		t.Errorf("continuous aggregate refresh jobs = %d, want 0", refreshJobs)
	}
}

func retentionJobDropAfter(t *testing.T, pool *pgxpool.Pool) map[string][]string {
	t.Helper()
	rows, err := pool.Query(context.Background(), `
		SELECT hypertable_name, config->>'drop_after'
		FROM timescaledb_information.jobs
		WHERE proc_name = 'policy_retention'
		ORDER BY hypertable_name, job_id`)
	if err != nil {
		t.Fatalf("query retention jobs: %v", err)
	}
	defer rows.Close()
	jobs := make(map[string][]string)
	for rows.Next() {
		var table, dropAfter string
		if err := rows.Scan(&table, &dropAfter); err != nil {
			t.Fatalf("scan retention job: %v", err)
		}
		jobs[table] = append(jobs[table], dropAfter)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate retention jobs: %v", err)
	}
	return jobs
}

// marketRetentionSchemaFacts lists what POE-399's down must restore: the
// trend_results columns, constraints, indexes and compression settings, and both
// aggregates' definitions and indexes.
func marketRetentionSchemaFacts(t *testing.T, pool *pgxpool.Pool) []string {
	t.Helper()
	rows, err := pool.Query(context.Background(), `
		SELECT format('column %s %s not_null=%s default=%s', a.attname, format_type(a.atttypid, a.atttypmod), a.attnotnull, pg_get_expr(d.adbin, d.adrelid))
		FROM pg_attribute AS a
		LEFT JOIN pg_attrdef AS d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
		WHERE a.attrelid = to_regclass('trend_results') AND a.attnum > 0 AND NOT a.attisdropped
		UNION ALL
		SELECT format('constraint %s %s', conname, pg_get_constraintdef(oid))
		FROM pg_constraint WHERE conrelid = to_regclass('trend_results')
		UNION ALL
		SELECT 'index ' || indexdef FROM pg_indexes WHERE schemaname = 'public' AND tablename = 'trend_results'
		UNION ALL
		SELECT 'index ' || indexname FROM pg_indexes
		WHERE indexname IN ('idx_gem_snapshots_hourly_league_name_variant', 'idx_gem_snapshots_daily_league_name_variant')
		UNION ALL
		SELECT format('compression %s segmentby=%s orderby=%s asc=%s', attname, segmentby_column_index, orderby_column_index, orderby_asc)
		FROM timescaledb_information.compression_settings WHERE hypertable_name = 'trend_results'
		UNION ALL
		SELECT format('aggregate %s materialized_only=%s %s', view_name, materialized_only, view_definition)
		FROM timescaledb_information.continuous_aggregates
		WHERE view_name IN ('gem_snapshots_hourly', 'gem_snapshots_daily')
		ORDER BY 1`)
	if err != nil {
		t.Fatalf("query POE-399 schema facts: %v", err)
	}
	defer rows.Close()
	var facts []string
	for rows.Next() {
		var fact string
		if err := rows.Scan(&fact); err != nil {
			t.Fatalf("scan schema fact: %v", err)
		}
		facts = append(facts, fact)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("iterate schema facts: %v", err)
	}
	if len(facts) == 0 {
		t.Fatal("no POE-399 schema facts found; trend_results and the aggregates are missing")
	}
	return facts
}
