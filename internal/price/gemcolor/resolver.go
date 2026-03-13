// Package gemcolor resolves gem names to their attribute colors (RED/GREEN/BLUE/WHITE).
//
// Transfigured and variant gems often have suffixes or prefixes that differ from
// the base gem name stored in the gem_colors table. The Resolver applies a set of
// heuristic stripping rules (Vaal prefix, Greater prefix, " of X" suffix) to find
// the base gem and inherit its color.
//
// Awakened variants (e.g., "Awakened Empower Support") are expected to be seeded
// directly in the gem_colors table rather than resolved via prefix stripping.
package gemcolor

import (
	"context"
	"fmt"
	"strings"
	"sync"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Color represents a gem attribute color.
type Color string

const (
	ColorRed   Color = "RED"
	ColorGreen Color = "GREEN"
	ColorBlue  Color = "BLUE"
	ColorWhite Color = "WHITE"
)

// Valid returns true if the color is one of the four valid gem colors.
func (c Color) Valid() bool {
	switch c {
	case ColorRed, ColorGreen, ColorBlue, ColorWhite:
		return true
	}
	return false
}

// String returns the string representation of the color.
func (c Color) String() string {
	return string(c)
}

// ParseColor converts a string to a Color, returning an error for invalid values.
func ParseColor(s string) (Color, error) {
	c := Color(s)
	if !c.Valid() {
		return "", fmt.Errorf("invalid gem color %q: must be RED, GREEN, BLUE, or WHITE", s)
	}
	return c, nil
}

// Resolver resolves gem names to colors (RED/GREEN/BLUE/WHITE).
// It loads gem_colors rows into an in-memory map and applies suffix-stripping
// heuristics for transfigured/vaal/greater gem variants.
type Resolver struct {
	pool *pgxpool.Pool

	mu          sync.RWMutex
	colors      map[string]Color // name -> color
	discovered  map[string]Color // newly resolved names not yet in DB
	unresolved  map[string]struct{}
}

// NewResolver creates a resolver pre-loaded from the gem_colors table.
func NewResolver(ctx context.Context, pool *pgxpool.Pool) (*Resolver, error) {
	r := &Resolver{
		pool:       pool,
		colors:     make(map[string]Color),
		discovered: make(map[string]Color),
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
		var name, rawColor string
		if err := rows.Scan(&name, &rawColor); err != nil {
			return fmt.Errorf("scan gem_colors row: %w", err)
		}
		color, err := ParseColor(rawColor)
		if err != nil {
			return fmt.Errorf("validate gem_colors row %q: %w", name, err)
		}
		r.colors[name] = color
	}

	if err := rows.Err(); err != nil {
		return fmt.Errorf("iterate gem_colors rows: %w", err)
	}
	return nil
}

// Resolve returns the color for a gem name.
//
// Resolution order:
//  1. Direct lookup in the loaded map
//  2. Vaal prefix: strip "Vaal ", look up base name, then try transfigured suffix
//  3. Greater prefix: strip "Greater ", look up base name, then try transfigured suffix
//  4. Transfigured suffix: progressively strip rightmost " of X"
//
// Newly resolved names are cached and can be persisted via UpsertDiscoveries.
// Names that cannot be resolved are tracked and returned by UnresolvedGems.
// Callers MUST check UnresolvedGems after batch resolution to detect and log
// gems that could not be mapped to a color.
func (r *Resolver) Resolve(name string) (Color, bool) {
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
func (r *Resolver) resolve(name string) (Color, bool) {
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
		// Greater transfigured: strip Greater prefix then try " of X" stripping.
		if color, ok := resolveTransfigured(base, r.colors); ok {
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
func resolveTransfigured(searchName string, colors map[string]Color) (Color, bool) {
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
	toInsert := make(map[string]Color, len(r.discovered))
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
