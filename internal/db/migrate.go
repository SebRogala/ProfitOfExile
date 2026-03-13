package db

import (
	"embed"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"strings"

	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	"github.com/golang-migrate/migrate/v4/source/iofs"
)

//go:embed all:migrations
var MigrationsFS embed.FS

// NewMigrate creates a *migrate.Migrate instance using an embedded filesystem
// as the migration source. Both MigrateUp and cmd/migrate use this to avoid
// duplicating source driver setup.
func NewMigrate(migrationsFS fs.FS, databaseURL string) (*migrate.Migrate, error) {
	// golang-migrate uses lib/pq which defaults to sslmode=require.
	// Append sslmode=disable when not explicitly set, matching pgx behavior.
	if !strings.Contains(databaseURL, "sslmode=") {
		if strings.Contains(databaseURL, "?") {
			databaseURL += "&sslmode=disable"
		} else {
			databaseURL += "?sslmode=disable"
		}
	}

	driver, err := iofs.New(migrationsFS, "migrations")
	if err != nil {
		return nil, fmt.Errorf("create iofs migration source: %w", err)
	}

	m, err := migrate.NewWithSourceInstance("iofs", driver, databaseURL)
	if err != nil {
		return nil, fmt.Errorf("create migrate instance: %w", err)
	}

	return m, nil
}

// MigrateUp applies all pending migrations from the embedded filesystem against
// the database at databaseURL. It returns nil when migrations are applied
// successfully or when there are no new migrations to apply.
func MigrateUp(migrationsFS fs.FS, databaseURL string) error {
	m, err := NewMigrate(migrationsFS, databaseURL)
	if err != nil {
		return err
	}
	defer closeMigrate(m)

	err = m.Up()
	if err != nil && !errors.Is(err, migrate.ErrNoChange) {
		return fmt.Errorf("apply migrations: %w", err)
	}
	if errors.Is(err, migrate.ErrNoChange) {
		slog.Info("migrations: no new migrations to apply")
	} else {
		slog.Info("migrations: applied successfully")
	}

	return nil
}

// closeMigrate closes the migrate source and database connections, logging
// errors at Error level since close failures may indicate resource leaks.
func closeMigrate(m *migrate.Migrate) {
	srcErr, dbErr := m.Close()
	if srcErr != nil {
		slog.Error("failed to close migration source", "error", srcErr)
	}
	if dbErr != nil {
		slog.Error("failed to close migration database", "error", dbErr)
	}
}
