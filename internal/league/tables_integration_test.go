//go:build integration

package league

import (
	"context"
	"slices"
	"testing"
)

func TestScopedTablesMatchesPublicLeagueForeignKeys(t *testing.T) {
	pool := newTestPool(t, requireDatabase(t))
	rows, err := pool.Query(context.Background(), `SELECT DISTINCT c.conrelid::regclass::text FROM pg_constraint c JOIN pg_class r ON r.oid = c.conrelid WHERE c.contype = 'f' AND c.confrelid = 'leagues'::regclass AND r.relnamespace = 'public'::regnamespace`)
	if err != nil {
		t.Fatalf("query public league foreign keys: %v", err)
	}
	defer rows.Close()

	schema := make(map[string]bool)
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			t.Fatalf("scan public league relation: %v", err)
		}
		if name != "runtime_config" {
			schema[name] = true
		}
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("read public league relations: %v", err)
	}

	registry := make(map[string]bool)
	for _, name := range ScopedTables() {
		if registry[name] {
			t.Errorf("duplicate registry relation %q", name)
		}
		registry[name] = true
		if !schema[name] {
			t.Errorf("extra registry relation %q: no public league foreign key (excluding runtime_config)", name)
		}
	}
	for name := range schema {
		// ADR-011 retains versioned boss observations across rollover. Keep
		// this sole league-FK exception explicit so new tables still fail closed.
		if name == "boss_loot_runs" {
			if registry[name] {
				t.Error("boss_loot_runs must remain outside the rollover wipe set")
			}
			continue
		}
		if !registry[name] {
			t.Errorf("missing registry relation %q: public league foreign key exists", name)
		}
	}
}

// ADR-034: every league-scoped hypertable carries one 21-day retention job at
// creation, and no other relation (device-keyed tables included) has one.
func TestScopedHypertablesCarryTheMarketRetentionPolicy(t *testing.T) {
	pool := newTestPool(t, requireDatabase(t))
	ctx := context.Background()

	scoped := make(map[string]bool)
	for _, name := range ScopedTables() {
		scoped[name] = true
	}
	hypertables := make(map[string]bool)
	rows, err := pool.Query(ctx, `SELECT hypertable_name FROM timescaledb_information.hypertables WHERE hypertable_schema = 'public'`)
	if err != nil {
		t.Fatalf("query hypertables: %v", err)
	}
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			t.Fatalf("scan hypertable: %v", err)
		}
		if scoped[name] {
			hypertables[name] = true
		}
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		t.Fatalf("read hypertables: %v", err)
	}
	if len(hypertables) == 0 {
		t.Fatal("no ScopedTables() entry is a hypertable; cannot check retention")
	}

	jobs := make(map[string][]bool)
	rows, err = pool.Query(ctx, `SELECT hypertable_name, (config->>'drop_after')::interval = INTERVAL '21 days' FROM timescaledb_information.jobs WHERE proc_name = 'policy_retention'`)
	if err != nil {
		t.Fatalf("query retention jobs: %v", err)
	}
	for rows.Next() {
		var name string
		var is21d bool
		if err := rows.Scan(&name, &is21d); err != nil {
			t.Fatalf("scan retention job: %v", err)
		}
		jobs[name] = append(jobs[name], is21d)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		t.Fatalf("read retention jobs: %v", err)
	}

	for name := range hypertables {
		got := jobs[name]
		if len(got) != 1 || !got[0] {
			t.Errorf("%s retention jobs (true = 21 days) = %v, want exactly one 21-day job", name, got)
		}
	}
	for name := range jobs {
		if !hypertables[name] {
			t.Errorf("%s has a retention job but is not a league-scoped market hypertable", name)
		}
	}
}

func TestScopedTablesReturnsMeasuredWipeOrder(t *testing.T) {
	// D6 adopts the sixteen-table TRUNCATE order in the league migration runbook;
	// ADR-030 appends the two currency-exchange plays tables; ADR-034 drops trend_results.
	want := []string{
		"gem_snapshots", "currency_snapshots", "fragment_snapshots", "font_snapshots",
		"transfigure_results", "quality_results", "gem_features",
		"gem_signals", "dedication_snapshots", "trade_lookups", "market_context",
		"currency_exchange_markets", "currency_exchange_cursor", "double_corrupt_snapshots", "item_snapshots",
		"currency_exchange_plays", "currency_exchange_play_sets",
	}
	if got := ScopedTables(); !slices.Equal(got, want) {
		t.Errorf("ScopedTables() = %q, want measured wipe order %q", got, want)
	}
}

func TestScopedTablesReturnsIndependentSlices(t *testing.T) {
	first := ScopedTables()
	if len(first) == 0 {
		t.Fatal("ScopedTables() returned no tables; cannot exercise caller mutation")
	}
	want := slices.Clone(first)
	for i := range first {
		first[i] = "caller_modified_table"
	}
	if got := ScopedTables(); !slices.Equal(got, want) {
		t.Errorf("caller mutation changed registry: next ScopedTables() = %q, want %q", got, want)
	}
}

func TestScopedTablesProtectsFontCraftColumns(t *testing.T) {
	pool := newTestPool(t, requireDatabase(t))
	rows, err := pool.Query(context.Background(), `SELECT table_name FROM information_schema.columns WHERE table_schema = 'public' AND table_name IN ('font_sessions','font_rounds') AND column_name = 'league'`)
	if err != nil {
		t.Fatalf("query protected font columns: %v", err)
	}
	defer rows.Close()
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			t.Fatalf("scan protected font table: %v", err)
		}
		t.Errorf("protected craft table public.%s has a league column; decide preservation before league scoping", name)
	}
	if err := rows.Err(); err != nil {
		t.Fatalf("read protected font columns: %v", err)
	}
}
