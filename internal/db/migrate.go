package db

import (
	"errors"
	"fmt"
	"log/slog"

	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
)

// MigrateUp applies all pending migrations from the given source path against
// the database at databaseURL. It returns nil when migrations are applied
// successfully or when there are no new migrations to apply.
func MigrateUp(migrationsPath, databaseURL string) error {
	m, err := migrate.New(migrationsPath, databaseURL)
	if err != nil {
		return fmt.Errorf("create migrate instance: %w", err)
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
