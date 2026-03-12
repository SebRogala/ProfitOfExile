package main

import (
	"errors"
	"fmt"
	"log/slog"
	"os"
	"strconv"

	"github.com/golang-migrate/migrate/v4"
	_ "github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
)

func main() {
	databaseURL := os.Getenv("DATABASE_URL")
	if err := run(os.Args[1:], databaseURL); err != nil {
		slog.Error("migrate", "error", err)
		os.Exit(1)
	}
}

// run executes the migrate CLI with the given arguments and database URL.
// It validates inputs before attempting any database operations.
func run(args []string, databaseURL string) error {
	if databaseURL == "" {
		return fmt.Errorf("database URL is required")
	}

	if len(args) == 0 {
		return fmt.Errorf("usage: migrate <up|down [N]|force VERSION|version>")
	}

	cmd := args[0]

	// Validate command and its arguments before creating the migrate instance.
	var downSteps int
	var forceVersion int
	switch cmd {
	case "up", "version":
		// no additional arguments to validate
	case "down":
		downSteps = 1
		if len(args) >= 2 {
			n, parseErr := strconv.Atoi(args[1])
			if parseErr != nil {
				return fmt.Errorf("invalid step count %q: %w", args[1], parseErr)
			}
			if n <= 0 {
				return fmt.Errorf("step count must be a positive integer, got %d", n)
			}
			downSteps = n
		}
	case "force":
		if len(args) < 2 {
			return fmt.Errorf("usage: migrate force VERSION")
		}
		fv, parseErr := strconv.Atoi(args[1])
		if parseErr != nil {
			return fmt.Errorf("invalid version %q: %w", args[1], parseErr)
		}
		forceVersion = fv
	default:
		return fmt.Errorf("unknown command: %s", cmd)
	}

	m, err := migrate.New("file://db/migrations", databaseURL)
	if err != nil {
		return fmt.Errorf("create migrate instance: %w", err)
	}
	defer func() {
		srcErr, dbErr := m.Close()
		if srcErr != nil {
			slog.Error("failed to close migration source", "error", srcErr)
		}
		if dbErr != nil {
			slog.Error("failed to close migration database", "error", dbErr)
		}
	}()

	switch cmd {
	case "up":
		err = m.Up()
		if err != nil && !errors.Is(err, migrate.ErrNoChange) {
			return fmt.Errorf("migration up: %w", err)
		}
		if errors.Is(err, migrate.ErrNoChange) {
			slog.Info("no new migrations to apply")
		} else {
			slog.Info("migrations applied successfully")
		}

	case "down":
		err = m.Steps(-downSteps)
		if err != nil {
			return fmt.Errorf("migration down: %w", err)
		}
		slog.Info("rolled back migrations", "steps", downSteps)

	case "force":
		if err = m.Force(forceVersion); err != nil {
			return fmt.Errorf("migration force: %w", err)
		}
		slog.Info("forced migration version", "version", forceVersion)

	case "version":
		version, dirty, verr := m.Version()
		if verr != nil {
			if errors.Is(verr, migrate.ErrNilVersion) {
				fmt.Println("no migrations applied yet")
				return nil
			}
			return fmt.Errorf("get migration version: %w", verr)
		}
		fmt.Printf("version: %d, dirty: %v\n", version, dirty)
	}

	return nil
}
