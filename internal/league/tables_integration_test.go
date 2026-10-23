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
		if !registry[name] {
			t.Errorf("missing registry relation %q: public league foreign key exists", name)
		}
	}
}

func TestScopedTablesReturnsMeasuredWipeOrder(t *testing.T) {
	// D6 adopts the sixteen-table TRUNCATE order in the league migration runbook.
	want := []string{
		"gem_snapshots", "currency_snapshots", "fragment_snapshots", "font_snapshots",
		"transfigure_results", "quality_results", "trend_results", "gem_features",
		"gem_signals", "dedication_snapshots", "trade_lookups", "market_context",
		"currency_exchange_markets", "currency_exchange_cursor", "double_corrupt_snapshots", "item_snapshots",
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
