// Package gemcolor resolves gem names to their attribute colors (RED/GREEN/BLUE/WHITE).
//
// Transfigured and variant gems often have suffixes or prefixes that differ from
// the base gem name stored in the gem_colors table. The Resolver applies a set of
// heuristic stripping rules (Vaal prefix, Greater prefix, " of X" suffix) to find
// the base gem and inherit its color.
package gemcolor

import (
	"context"
	"fmt"
	"strings"
	"sync"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Resolver resolves gem names to colors (RED/GREEN/BLUE/WHITE).
// It loads gem_colors rows into an in-memory map and applies suffix-stripping
// heuristics for transfigured/vaal/greater gem variants.
type Resolver struct {
	pool *pgxpool.Pool

	mu          sync.RWMutex
	colors      map[string]string // name -> color
	discovered  map[string]string // newly resolved names not yet in DB
	unresolved  map[string]struct{}
}

// NewResolver creates a resolver pre-loaded from the gem_colors table.
func NewResolver(ctx context.Context, pool *pgxpool.Pool) (*Resolver, error) {
	r := &Resolver{
		pool:       pool,
		colors:     make(map[string]string),
		discovered: make(map[string]string),
		unresolved: make(map[string]struct{}),
	}

	if err := r.load(ctx); err != nil {
		return nil, fmt.Errorf("load gem colors: %w", err)
	}

	return r, nil
}

// load reads all gem_colors rows into the in-memory map.
func (r *Resolver) load(ctx context.Context) error {
	rows, err := r.pool.Query(ctx, "SELECT name, color FROM gem_colors")
	if err != nil {
		return fmt.Errorf("query gem_colors: %w", err)
	}
	defer rows.Close()

	for rows.Next() {
		var name, color string
		if err := rows.Scan(&name, &color); err != nil {
			return fmt.Errorf("scan gem_colors row: %w", err)
		}
		r.colors[name] = color
	}

	return rows.Err()
}

// Resolve returns the color for a gem name.
//
// Resolution order:
//  1. Direct lookup in the loaded map
//  2. Vaal prefix: strip "Vaal ", look up base name
//  3. Greater prefix: strip "Greater ", look up base name
//  4. Transfigured suffix: progressively strip rightmost " of X"
//
// Newly resolved names are cached and can be persisted via UpsertDiscoveries.
// Names that cannot be resolved are tracked and returned by UnresolvedGems.
func (r *Resolver) Resolve(name string) (string, bool) {
	r.mu.RLock()
	color, inColors := r.colors[name]
	_, inUnresolved := r.unresolved[name]
	r.mu.RUnlock()

	if inColors {
		return color, true
	}
	if inUnresolved {
		return "", false
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// Double-check after acquiring write lock.
	if color, ok := r.colors[name]; ok {
		return color, true
	}
	if _, ok := r.unresolved[name]; ok {
		return "", false
	}

	if color, ok := r.resolve(name); ok {
		r.colors[name] = color
		r.discovered[name] = color
		return color, true
	}

	r.unresolved[name] = struct{}{}
	return "", false
}

// resolve applies heuristic stripping rules to find the base gem color.
func (r *Resolver) resolve(name string) (string, bool) {
	// 1. Vaal prefix: "Vaal Cleave" -> "Cleave"
	if strings.HasPrefix(name, "Vaal ") {
		base := name[5:]
		if color, ok := r.colors[base]; ok {
			return color, true
		}
		// Vaal transfigured: strip Vaal prefix then try " of X" stripping.
		if color, ok := resolveTransfigured(base, r.colors); ok {
			return color, true
		}
	}

	// 2. Greater prefix: "Greater Multiple Projectiles Support" -> "Multiple Projectiles Support"
	if strings.HasPrefix(name, "Greater ") {
		base := name[8:]
		if color, ok := r.colors[base]; ok {
			return color, true
		}
	}

	// 3. Transfigured suffix: progressively strip rightmost " of X"
	if color, ok := resolveTransfigured(name, r.colors); ok {
		return color, true
	}

	return "", false
}

// resolveTransfigured progressively strips the rightmost " of X" suffix from
// searchName until a base gem is found in the color map.
//
// Example: "Rain of Arrows of Saturation"
//   - strip " of Saturation" -> try "Rain of Arrows" -> found!
func resolveTransfigured(searchName string, colors map[string]string) (string, bool) {
	s := searchName
	for {
		pos := strings.LastIndex(s, " of ")
		if pos == -1 {
			return "", false
		}
		s = s[:pos]
		if color, ok := colors[s]; ok {
			return color, true
		}
	}
}

// UpsertDiscoveries writes newly resolved gem colors back to the gem_colors table.
// It uses INSERT ... ON CONFLICT DO NOTHING so concurrent resolvers don't conflict.
func (r *Resolver) UpsertDiscoveries(ctx context.Context) error {
	r.mu.RLock()
	if len(r.discovered) == 0 {
		r.mu.RUnlock()
		return nil
	}

	// Copy under read lock.
	toInsert := make(map[string]string, len(r.discovered))
	for name, color := range r.discovered {
		toInsert[name] = color
	}
	r.mu.RUnlock()

	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("begin transaction: %w", err)
	}
	defer tx.Rollback(ctx) //nolint:errcheck

	for name, color := range toInsert {
		_, err := tx.Exec(ctx,
			"INSERT INTO gem_colors (name, color) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING",
			name, color,
		)
		if err != nil {
			return fmt.Errorf("upsert gem color %q: %w", name, err)
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return fmt.Errorf("commit gem color upserts: %w", err)
	}

	// Clear discovered set after successful persist.
	r.mu.Lock()
	for name := range toInsert {
		delete(r.discovered, name)
	}
	r.mu.Unlock()

	return nil
}

// UnresolvedGems returns gem names that couldn't be resolved to a color.
func (r *Resolver) UnresolvedGems() []string {
	r.mu.RLock()
	defer r.mu.RUnlock()

	names := make([]string, 0, len(r.unresolved))
	for name := range r.unresolved {
		names = append(names, name)
	}
	return names
}
