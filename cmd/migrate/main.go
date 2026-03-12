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
	os.Exit(run())
}

func run() int {
	databaseURL := os.Getenv("DATABASE_URL")
	if databaseURL == "" {
		slog.Error("DATABASE_URL is required")
		fmt.Fprintln(os.Stderr, "DATABASE_URL environment variable must be set")
		return 1
	}

	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: migrate <up|down [N]|force VERSION|version>")
		return 1
	}

	m, err := migrate.New("file://db/migrations", databaseURL)
	if err != nil {
		slog.Error("failed to create migrate instance", "error", err)
		return 1
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

	cmd := os.Args[1]

	switch cmd {
	case "up":
		err = m.Up()
		if err != nil && !errors.Is(err, migrate.ErrNoChange) {
			slog.Error("migration up failed", "error", err)
			return 1
		}
		if errors.Is(err, migrate.ErrNoChange) {
			slog.Info("no new migrations to apply")
		} else {
			slog.Info("migrations applied successfully")
		}

	case "down":
		n := 1
		if len(os.Args) >= 3 {
			n, err = strconv.Atoi(os.Args[2])
			if err != nil {
				slog.Error("invalid step count", "value", os.Args[2], "error", err)
				return 1
			}
		}
		err = m.Steps(-n)
		if err != nil {
			slog.Error("migration down failed", "error", err)
			return 1
		}
		slog.Info("rolled back migrations", "steps", n)

	case "force":
		if len(os.Args) < 3 {
			fmt.Fprintln(os.Stderr, "usage: migrate force VERSION")
			return 1
		}
		version, ferr := strconv.Atoi(os.Args[2])
		if ferr != nil {
			slog.Error("invalid version", "value", os.Args[2], "error", ferr)
			return 1
		}
		if err = m.Force(version); err != nil {
			slog.Error("migration force failed", "error", err)
			return 1
		}
		slog.Info("forced migration version", "version", version)

	case "version":
		version, dirty, verr := m.Version()
		if verr != nil {
			slog.Error("failed to get migration version", "error", verr)
			return 1
		}
		fmt.Printf("version: %d, dirty: %v\n", version, dirty)

	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\nusage: migrate <up|down [N]|force VERSION|version>\n", cmd)
		return 1
	}

	return 0
}
