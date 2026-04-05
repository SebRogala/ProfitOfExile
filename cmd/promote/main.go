// cmd/promote is a CLI tool for promoting device roles.
//
// Usage:
//
//	promote list                         — list all identified devices
//	promote <prefix> <role>              — set role for device matching fingerprint prefix
//	promote <prefix> <role> "New Alias"  — set role + alias
//
// Valid roles: user, editor, admin.
// The tool connects directly to the database via DATABASE_URL.
package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"text/tabwriter"
	"time"

	"profitofexile/internal/db"
	"profitofexile/internal/device"
)

var validRoles = map[string]bool{
	"user":   true,
	"editor": true,
	"admin":  true,
}

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	ctx := context.Background()

	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		dbURL = "postgresql://profitofexile:profitofexile@postgres:5432/profitofexile"
	}

	pool, err := db.NewPool(ctx, dbURL)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: database connection failed: %v\n", err)
		os.Exit(1)
	}
	defer pool.Close()

	repo := device.NewRepository(pool)

	switch os.Args[1] {
	case "list":
		if err := runList(ctx, repo); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
	case "stats":
		if err := runStats(ctx, repo); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
	case "help", "--help", "-h":
		printUsage()
	default:
		if err := runPromote(ctx, repo, os.Args[1:]); err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
	}
}

func runList(ctx context.Context, repo *device.Repository) error {
	devices, err := repo.ListIdentified(ctx)
	if err != nil {
		return fmt.Errorf("list identified devices: %w", err)
	}

	if len(devices) == 0 {
		fmt.Println("No identified devices found.")
		return nil
	}

	w := tabwriter.NewWriter(os.Stdout, 0, 4, 2, ' ', 0)
	fmt.Fprintln(w, "PREFIX\tALIAS\tROLE\tBANNED\tVERSION\tLAST SEEN")
	fmt.Fprintln(w, "------\t-----\t----\t------\t-------\t---------")
	for _, d := range devices {
		prefix := d.Fingerprint
		if len(prefix) > 8 {
			prefix = prefix[:8]
		}

		alias := "-"
		if d.Alias != nil {
			alias = *d.Alias
		}

		banned := "no"
		if d.Banned {
			banned = "YES"
		}

		version := "-"
		if d.AppVersion != nil {
			version = *d.AppVersion
		}

		lastSeen := d.LastSeen.Format(time.RFC3339)

		fmt.Fprintf(w, "%s\t%s\t%s\t%s\t%s\t%s\n", prefix, alias, d.Role, banned, version, lastSeen)
	}
	return w.Flush()
}

func runPromote(ctx context.Context, repo *device.Repository, args []string) error {
	if len(args) < 2 {
		return fmt.Errorf("usage: promote <prefix> <role> [alias]\n\nRun 'promote help' for details")
	}

	prefix := args[0]
	role := args[1]
	alias := ""
	if len(args) >= 3 {
		alias = args[2]
	}

	if !validRoles[role] {
		return fmt.Errorf("invalid role %q — must be one of: user, editor, admin", role)
	}

	d, err := repo.GetByPrefix(ctx, prefix)
	if err != nil {
		if errors.Is(err, device.ErrNotFound) {
			return fmt.Errorf("no device found with fingerprint prefix %q", prefix)
		}
		if errors.Is(err, device.ErrAmbiguousPrefix) {
			return fmt.Errorf("prefix %q matches multiple devices — use a longer prefix", prefix)
		}
		return fmt.Errorf("lookup device: %w", err)
	}

	if err := repo.SetRole(ctx, d.Fingerprint, role, alias); err != nil {
		return fmt.Errorf("set role: %w", err)
	}

	displayAlias := "-"
	if alias != "" {
		displayAlias = alias
	} else if d.Alias != nil {
		displayAlias = *d.Alias
	}

	shortFP := d.Fingerprint
	if len(shortFP) > 8 {
		shortFP = shortFP[:8]
	}

	fmt.Printf("Device %s promoted to %s (alias: %s)\n", shortFP, role, displayAlias)
	return nil
}

func runStats(ctx context.Context, repo *device.Repository) error {
	s, err := repo.Stats(ctx)
	if err != nil {
		return err
	}

	fmt.Printf("Total devices:  %d\n", s.Total)
	fmt.Printf("Active (24h):   %d\n", s.Active24h)
	fmt.Printf("Active (7d):    %d\n", s.Active7d)
	fmt.Printf("Identified:     %d\n", s.Identified)
	fmt.Printf("Banned:         %d\n", s.Banned)

	fmt.Print("\nBy role: ")
	first := true
	for role, count := range s.ByRole {
		if !first {
			fmt.Print(", ")
		}
		fmt.Printf("%s=%d", role, count)
		first = false
	}
	fmt.Println()

	fmt.Print("By version: ")
	first = true
	for version, count := range s.ByVersion {
		if !first {
			fmt.Print(", ")
		}
		fmt.Printf("%s=%d", version, count)
		first = false
	}
	fmt.Println()

	return nil
}

func printUsage() {
	fmt.Fprintf(os.Stderr, `Usage: promote <command>

Commands:
  list                         List all identified devices
  stats                        Show device statistics (total, active, by role/version)
  <prefix> <role>              Set role for device matching fingerprint prefix
  <prefix> <role> "alias"      Set role and alias

Valid roles: user, editor, admin

Environment:
  DATABASE_URL    PostgreSQL connection string (defaults to local Docker dev DB)

Examples:
  promote list
  promote a1b2c3d4 admin
  promote a1b2 editor "Seb's PC"
`)
}
