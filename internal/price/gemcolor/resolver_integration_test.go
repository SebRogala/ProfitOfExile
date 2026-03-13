//go:build integration

package gemcolor

import (
	"context"
	"os"
	"sort"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

func integrationPool(t *testing.T) *pgxpool.Pool {
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

	t.Cleanup(func() { pool.Close() })

	// The gem_colors table is created by TimescaleDB migrations.
	// Skip if migrations haven't been applied (e.g., plain Postgres without TimescaleDB).
	var exists bool
	err = pool.QueryRow(ctx,
		"SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'gem_colors')").
		Scan(&exists)
	if err != nil {
		t.Fatalf("check gem_colors table: %v", err)
	}
	if !exists {
		t.Skip("gem_colors table not found, skipping (TimescaleDB migrations not applied)")
	}

	return pool
}

func TestNewResolver_loadsFromDB(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	// The seed migration inserts 750 gem colors.
	// Verify a few known entries from different colors.
	tests := []struct {
		gem       string
		wantColor Color
	}{
		{"Arc", ColorBlue},
		{"Cleave", ColorRed},
		{"Rain of Arrows", ColorGreen},
	}

	for _, tt := range tests {
		t.Run(tt.gem, func(t *testing.T) {
			color, ok := r.Resolve(tt.gem)
			if !ok {
				t.Fatalf("expected %q to be found", tt.gem)
			}
			if color != tt.wantColor {
				t.Errorf("color = %q, want %q", color, tt.wantColor)
			}
		})
	}
}

func TestResolver_VaalPrefix_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	// "Vaal Cleave" should resolve to Cleave's color via Vaal prefix stripping.
	color, ok := r.Resolve("Vaal Cleave")
	if !ok {
		t.Fatal("expected Vaal Cleave to resolve")
	}

	// Verify it matches the base gem.
	baseColor, baseOk := r.Resolve("Cleave")
	if !baseOk {
		t.Fatal("expected Cleave to resolve")
	}
	if color != baseColor {
		t.Errorf("Vaal Cleave color = %q, want %q (same as Cleave)", color, baseColor)
	}
}

func TestResolver_GreaterPrefix_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	// "Greater Multiple Projectiles Support" should resolve via Greater prefix stripping.
	color, ok := r.Resolve("Greater Multiple Projectiles Support")
	if !ok {
		t.Fatal("expected Greater Multiple Projectiles Support to resolve")
	}

	baseColor, baseOk := r.Resolve("Multiple Projectiles Support")
	if !baseOk {
		t.Fatal("expected Multiple Projectiles Support to resolve")
	}
	if color != baseColor {
		t.Errorf("Greater variant color = %q, want %q", color, baseColor)
	}
}

func TestResolver_TransfiguredSuffix_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	// "Rain of Arrows of Saturation" should strip " of Saturation" to find "Rain of Arrows".
	color, ok := r.Resolve("Rain of Arrows of Saturation")
	if !ok {
		t.Fatal("expected Rain of Arrows of Saturation to resolve")
	}

	baseColor, baseOk := r.Resolve("Rain of Arrows")
	if !baseOk {
		t.Fatal("expected Rain of Arrows to resolve")
	}
	if color != baseColor {
		t.Errorf("transfigured color = %q, want %q", color, baseColor)
	}
}

func TestResolver_unknownGem_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	_, ok := r.Resolve("Totally Fake Gem That Does Not Exist")
	if ok {
		t.Fatal("expected unknown gem to not resolve")
	}

	unresolved := r.UnresolvedGems()
	found := false
	for _, name := range unresolved {
		if name == "Totally Fake Gem That Does Not Exist" {
			found = true
			break
		}
	}
	if !found {
		t.Error("expected unknown gem in UnresolvedGems list")
	}
}

func TestResolver_UpsertDiscoveries_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	// Resolve a transfigured gem to create a discovery.
	_, ok := r.Resolve("Arc of Surging")
	if !ok {
		t.Fatal("expected Arc of Surging to resolve")
	}

	// Persist discoveries to DB.
	if err := r.UpsertDiscoveries(ctx); err != nil {
		t.Fatalf("UpsertDiscoveries: %v", err)
	}

	// Clean up the inserted row so test is idempotent.
	// Registered before assertions so it runs even if assertions fail.
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), "DELETE FROM gem_colors WHERE name = $1", "Arc of Surging"); err != nil {
			t.Logf("cleanup warning: failed to delete test row: %v", err)
		}
	})

	// Verify the discovery was written to the database.
	var color string
	err = pool.QueryRow(ctx, "SELECT color FROM gem_colors WHERE name = $1", "Arc of Surging").Scan(&color)
	if err != nil {
		t.Fatalf("query upserted gem: %v", err)
	}
	if color != "BLUE" {
		t.Errorf("upserted color = %q, want BLUE", color)
	}
}

func TestResolver_UnresolvedGems_integration(t *testing.T) {
	pool := integrationPool(t)
	ctx := context.Background()

	r, err := NewResolver(ctx, pool)
	if err != nil {
		t.Fatalf("NewResolver: %v", err)
	}

	fakeGems := []string{"Fake Gem Alpha", "Fake Gem Beta"}
	for _, gem := range fakeGems {
		r.Resolve(gem)
	}

	unresolved := r.UnresolvedGems()
	sort.Strings(unresolved)
	sort.Strings(fakeGems)

	if len(unresolved) != len(fakeGems) {
		t.Fatalf("unresolved count = %d, want %d", len(unresolved), len(fakeGems))
	}
	for i, name := range unresolved {
		if name != fakeGems[i] {
			t.Errorf("unresolved[%d] = %q, want %q", i, name, fakeGems[i])
		}
	}
}
