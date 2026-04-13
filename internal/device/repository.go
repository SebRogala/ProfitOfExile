package device

import (
	"context"
	"errors"
	"fmt"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// ErrAmbiguousPrefix is returned by GetByPrefix when the prefix matches
// more than one device fingerprint.
var ErrAmbiguousPrefix = errors.New("device: ambiguous fingerprint prefix — matches multiple devices")

// ErrNotFound is returned when no device matches the query.
var ErrNotFound = errors.New("device: not found")

// Repository handles device persistence in PostgreSQL.
type Repository struct {
	pool *pgxpool.Pool
}

// NewRepository creates a device repository backed by the given connection pool.
func NewRepository(pool *pgxpool.Pool) *Repository {
	return &Repository{pool: pool}
}

// Upsert inserts a new device or updates last_seen and app_version for an
// existing one. Returns the full device record after the upsert.
func (r *Repository) Upsert(ctx context.Context, fingerprint, appVersion string) (*Device, error) {
	var d Device
	err := r.pool.QueryRow(ctx,
		`INSERT INTO devices (fingerprint, app_version)
		 VALUES ($1, $2)
		 ON CONFLICT (fingerprint) DO UPDATE
		   SET last_seen = NOW(), app_version = COALESCE(EXCLUDED.app_version, devices.app_version)
		 RETURNING fingerprint, alias, role, banned, app_version, first_seen, last_seen`,
		fingerprint, nilIfEmpty(appVersion),
	).Scan(&d.Fingerprint, &d.Alias, &d.Role, &d.Banned, &d.AppVersion, &d.FirstSeen, &d.LastSeen)
	if err != nil {
		return nil, fmt.Errorf("device upsert %q: %w", fingerprint, err)
	}
	return &d, nil
}

// GetByPrefix finds a device whose fingerprint starts with the given prefix.
// Returns ErrAmbiguousPrefix if more than one device matches, and ErrNotFound
// if none match. Used by the promote CLI for short-prefix lookups.
func (r *Repository) GetByPrefix(ctx context.Context, prefix string) (*Device, error) {
	rows, err := r.pool.Query(ctx,
		`SELECT fingerprint, alias, role, banned, app_version, first_seen, last_seen
		 FROM devices
		 WHERE LEFT(fingerprint, LENGTH($1)) = $1
		 LIMIT 2`,
		prefix,
	)
	if err != nil {
		return nil, fmt.Errorf("device get by prefix %q: %w", prefix, err)
	}
	defer rows.Close()

	var devices []Device
	for rows.Next() {
		var d Device
		if err := rows.Scan(&d.Fingerprint, &d.Alias, &d.Role, &d.Banned, &d.AppVersion, &d.FirstSeen, &d.LastSeen); err != nil {
			return nil, fmt.Errorf("device scan: %w", err)
		}
		devices = append(devices, d)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("device rows: %w", err)
	}

	switch len(devices) {
	case 0:
		return nil, ErrNotFound
	case 1:
		return &devices[0], nil
	default:
		return nil, ErrAmbiguousPrefix
	}
}

// ListIdentified returns all devices that have an alias set (alias IS NOT NULL).
// Used by the promote CLI to show known devices.
func (r *Repository) ListIdentified(ctx context.Context) ([]Device, error) {
	rows, err := r.pool.Query(ctx,
		`SELECT fingerprint, alias, role, banned, app_version, first_seen, last_seen
		 FROM devices
		 WHERE alias IS NOT NULL
		 ORDER BY last_seen DESC`,
	)
	if err != nil {
		return nil, fmt.Errorf("device list identified: %w", err)
	}
	defer rows.Close()

	return collectDevices(rows)
}

// SetRole updates the role and optionally the alias for a device.
// An empty alias string leaves the existing alias unchanged.
func (r *Repository) SetRole(ctx context.Context, fingerprint, role, alias string) error {
	var ct pgconn.CommandTag
	var err error

	if alias != "" {
		ct, err = r.pool.Exec(ctx,
			`UPDATE devices SET role = $2, alias = $3 WHERE fingerprint = $1`,
			fingerprint, role, alias,
		)
		if err != nil {
			return fmt.Errorf("device set role+alias %q: %w", fingerprint, err)
		}
	} else {
		ct, err = r.pool.Exec(ctx,
			`UPDATE devices SET role = $2 WHERE fingerprint = $1`,
			fingerprint, role,
		)
		if err != nil {
			return fmt.Errorf("device set role %q: %w", fingerprint, err)
		}
	}
	if ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return nil
}

// SetAlias updates just the alias for a device. Used by the identify endpoint.
func (r *Repository) SetAlias(ctx context.Context, fingerprint, alias string) error {
	ct, err := r.pool.Exec(ctx,
		`UPDATE devices SET alias = $2 WHERE fingerprint = $1`,
		fingerprint, alias,
	)
	if err != nil {
		return fmt.Errorf("device set alias %q: %w", fingerprint, err)
	}
	if ct.RowsAffected() == 0 {
		return ErrNotFound
	}
	return nil
}

// List returns all devices ordered by last_seen descending.
// Used by the admin devices endpoint.
func (r *Repository) List(ctx context.Context) ([]Device, error) {
	rows, err := r.pool.Query(ctx,
		`SELECT fingerprint, alias, role, banned, app_version, first_seen, last_seen
		 FROM devices
		 ORDER BY last_seen DESC`,
	)
	if err != nil {
		return nil, fmt.Errorf("device list: %w", err)
	}
	defer rows.Close()

	return collectDevices(rows)
}

// DeviceStats holds aggregate device statistics.
type DeviceStats struct {
	Total      int
	Active8h  int
	Active24h  int
	Active7d   int
	Identified int
	Banned     int
	ByRole     map[string]int
	ByVersion  map[string]int
}

// Stats returns aggregate device statistics.
func (r *Repository) Stats(ctx context.Context) (*DeviceStats, error) {
	s := &DeviceStats{ByRole: make(map[string]int), ByVersion: make(map[string]int)}

	// Exclusive activity buckets: each device counted in exactly one window.
	// Active8h + Active24h + Active7d + (implicit inactive) = Total.
	err := r.pool.QueryRow(ctx, `SELECT
		COUNT(*),
		COUNT(*) FILTER (WHERE last_seen > NOW() - INTERVAL '8 hours'),
		COUNT(*) FILTER (WHERE last_seen > NOW() - INTERVAL '24 hours' AND last_seen <= NOW() - INTERVAL '8 hours'),
		COUNT(*) FILTER (WHERE last_seen > NOW() - INTERVAL '7 days' AND last_seen <= NOW() - INTERVAL '24 hours'),
		COUNT(*) FILTER (WHERE alias IS NOT NULL),
		COUNT(*) FILTER (WHERE banned = true)
		FROM devices`).Scan(&s.Total, &s.Active8h, &s.Active24h, &s.Active7d, &s.Identified, &s.Banned)
	if err != nil {
		return nil, fmt.Errorf("device stats: %w", err)
	}

	// By role
	rows, err := r.pool.Query(ctx, `SELECT role, COUNT(*) FROM devices GROUP BY role`)
	if err != nil {
		return nil, fmt.Errorf("device stats by role: %w", err)
	}
	defer rows.Close()
	for rows.Next() {
		var role string
		var count int
		if err := rows.Scan(&role, &count); err != nil {
			return nil, fmt.Errorf("device stats scan role: %w", err)
		}
		s.ByRole[role] = count
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("device stats rows role: %w", err)
	}

	// By version
	rows2, err := r.pool.Query(ctx, `SELECT COALESCE(app_version, 'unknown'), COUNT(*) FROM devices GROUP BY app_version`)
	if err != nil {
		return nil, fmt.Errorf("device stats by version: %w", err)
	}
	defer rows2.Close()
	for rows2.Next() {
		var version string
		var count int
		if err := rows2.Scan(&version, &count); err != nil {
			return nil, fmt.Errorf("device stats scan version: %w", err)
		}
		s.ByVersion[version] = count
	}
	if err := rows2.Err(); err != nil {
		return nil, fmt.Errorf("device stats rows version: %w", err)
	}

	return s, nil
}

// collectDevices scans all rows into a Device slice.
func collectDevices(rows pgx.Rows) ([]Device, error) {
	var devices []Device
	for rows.Next() {
		var d Device
		if err := rows.Scan(&d.Fingerprint, &d.Alias, &d.Role, &d.Banned, &d.AppVersion, &d.FirstSeen, &d.LastSeen); err != nil {
			return nil, fmt.Errorf("device scan: %w", err)
		}
		devices = append(devices, d)
	}
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("device rows: %w", err)
	}
	return devices, nil
}

func nilIfEmpty(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}
