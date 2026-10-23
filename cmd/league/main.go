// Command league administers league rollover through internal/league.
// DATABASE_URL is required; league identifiers always come from the operator.
package main

import (
	"bytes"
	"context"
	"flag"
	"fmt"
	"io"
	"os"
	"sort"
	"strings"
	"text/tabwriter"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/db"
	"profitofexile/internal/league"
)

const usage = `Usage:
  league status [--no-counts]
  league prepare <id> [--display-name <name>] [--starts-at <RFC3339>] [--dry-run]
  league activate <id> [--dry-run]
  league wipe <id> [--allow-delete] [--dry-run]

Environment: DATABASE_URL (required)
`

type command struct {
	name        string
	id          string
	displayName string
	startsAt    *time.Time
	dryRun      bool
	allowDelete bool
	withCounts  bool
}

func parseArgs(args []string) (command, error) {
	var cmd command
	if len(args) == 0 {
		return cmd, fmt.Errorf("%s", usage)
	}
	cmd.name = args[0]
	cmd.withCounts = true
	fs := flag.NewFlagSet(cmd.name, flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	var startsAt string
	var noCounts bool
	switch cmd.name {
	case "status":
		fs.BoolVar(&noCounts, "no-counts", false, "omit scoped row counts")
	case "prepare":
		fs.StringVar(&cmd.displayName, "display-name", "", "display name (defaults to id)")
		fs.StringVar(&startsAt, "starts-at", "", "start time in RFC3339")
		fs.BoolVar(&cmd.dryRun, "dry-run", false, "execute and roll back")
	case "activate":
		fs.BoolVar(&cmd.dryRun, "dry-run", false, "execute and roll back")
	case "wipe":
		fs.BoolVar(&cmd.dryRun, "dry-run", false, "preview without deleting rows")
		fs.BoolVar(&cmd.allowDelete, "allow-delete", false, "allow targeted deletion when other leagues have rows")
	default:
		return cmd, fmt.Errorf("unknown subcommand %q\n%s", cmd.name, usage)
	}

	// Separate positionals while retaining string flag values. FlagSet.Parse
	// otherwise stops at the id and silently leaves trailing flags unparsed.
	var flags, positionals []string
	for i := 1; i < len(args); i++ {
		arg := args[i]
		if arg == "--" {
			positionals = append(positionals, args[i+1:]...)
			break
		}
		if arg == "-" || !strings.HasPrefix(arg, "-") {
			positionals = append(positionals, arg)
			continue
		}
		name, _, hasValue := strings.Cut(strings.TrimPrefix(strings.TrimPrefix(arg, "-"), "-"), "=")
		f := fs.Lookup(name)
		if f == nil {
			return cmd, fmt.Errorf("unknown flag %q for %s\n%s", arg, cmd.name, usage)
		}
		flags = append(flags, arg)
		boolean, isBool := f.Value.(interface{ IsBoolFlag() bool })
		if !hasValue && !(isBool && boolean.IsBoolFlag()) {
			if i+1 == len(args) {
				return cmd, fmt.Errorf("flag --%s needs a value", name)
			}
			i++
			flags = append(flags, args[i])
		}
	}
	if err := fs.Parse(flags); err != nil {
		return cmd, fmt.Errorf("%s: %w", cmd.name, err)
	}
	wantIDs := 1
	if cmd.name == "status" {
		wantIDs = 0
	}
	if len(positionals) != wantIDs {
		return cmd, fmt.Errorf("%s expects %d league id(s)\n%s", cmd.name, wantIDs, usage)
	}
	if wantIDs == 1 {
		cmd.id = positionals[0]
		if cmd.id == "" || cmd.id != strings.TrimSpace(cmd.id) {
			return cmd, league.ErrInvalidLeagueID
		}
	}
	if fs.Lookup("starts-at") != nil {
		var supplied bool
		fs.Visit(func(f *flag.Flag) { supplied = supplied || f.Name == "starts-at" })
		if supplied {
			parsed, err := time.Parse(time.RFC3339, startsAt)
			if err != nil {
				return cmd, fmt.Errorf("--starts-at must be RFC3339: %w", err)
			}
			cmd.startsAt = &parsed
		}
	}
	cmd.withCounts = !noCounts
	return cmd, nil
}

// cliDeps retains the existing API signatures so tests can exercise the same
// dispatch and transaction callbacks without connecting to a database.
type cliDeps struct {
	openPool    func(context.Context, string) (*pgxpool.Pool, error)
	closePool   func(*pgxpool.Pool)
	runAdmin    func(context.Context, *pgxpool.Pool, bool, func(pgx.Tx) error) error
	prepare     func(context.Context, pgx.Tx, league.PrepareInput) (league.LeagueRow, error)
	activate    func(context.Context, pgx.Tx, string) (league.Activation, error)
	readStatus  func(context.Context, pgx.Tx, bool) (league.Status, error)
	previewWipe func(context.Context, pgx.Tx, string, bool) (league.WipeResult, error)
	wipe        func(context.Context, pgx.Tx, string, bool) (league.WipeResult, error)
}

func main() {
	os.Exit(run(os.Args[1:], os.Getenv, os.Stdout, os.Stderr))
}

func run(args []string, getenv func(string) string, stdout, stderr io.Writer) int {
	return runWithDeps(args, getenv, stdout, stderr, cliDeps{
		openPool: db.NewPool, closePool: (*pgxpool.Pool).Close,
		runAdmin: league.RunAdmin, prepare: league.Prepare, activate: league.Activate,
		readStatus: league.ReadStatus, previewWipe: league.PreviewWipe, wipe: league.Wipe,
	})
}

func runWithDeps(args []string, getenv func(string) string, stdout, stderr io.Writer, deps cliDeps) int {
	fail := func(err error) int {
		fmt.Fprintf(stderr, "error: %v\n", err)
		return 1
	}
	cmd, err := parseArgs(args)
	if err != nil {
		return fail(err)
	}
	databaseURL := getenv("DATABASE_URL")
	if strings.TrimSpace(databaseURL) == "" {
		return fail(fmt.Errorf("DATABASE_URL is required"))
	}
	ctx := context.Background()
	pool, err := deps.openPool(ctx, databaseURL)
	if err != nil {
		// Keep connection details out of operator captures.
		return fail(fmt.Errorf("database connection failed; check DATABASE_URL and database availability"))
	}
	defer deps.closePool(pool)

	var prepared league.LeagueRow
	var activation league.Activation
	var status league.Status
	var wipe league.WipeResult
	err = deps.runAdmin(ctx, pool, cmd.dryRun, func(tx pgx.Tx) error {
		var opErr error
		switch cmd.name {
		case "prepare":
			prepared, opErr = deps.prepare(ctx, tx, league.PrepareInput{
				ID: cmd.id, DisplayName: cmd.displayName, StartsAt: cmd.startsAt,
			})
		case "activate":
			activation, opErr = deps.activate(ctx, tx, cmd.id)
		case "status":
			status, opErr = deps.readStatus(ctx, tx, cmd.withCounts)
		case "wipe":
			if cmd.dryRun {
				wipe, opErr = deps.previewWipe(ctx, tx, cmd.id, cmd.allowDelete)
			} else {
				wipe, opErr = deps.wipe(ctx, tx, cmd.id, cmd.allowDelete)
			}
		}
		return opErr
	})
	if err != nil {
		return fail(err)
	}

	// Render only after RunAdmin confirms commit or rollback, so a refusal
	// cannot leave success output or committed rollover guidance behind.
	var output bytes.Buffer
	switch cmd.name {
	case "prepare":
		err = renderPrepare(&output, prepared)
	case "activate":
		err = renderActivation(&output, activation)
		if !cmd.dryRun && err == nil {
			err = renderActivationGuidance(&output, activation)
		}
	case "status":
		err = renderStatus(&output, status)
	case "wipe":
		fmt.Fprintf(&output, "wipe %q\n", cmd.id)
		err = renderWipe(&output, wipe)
	}
	if err != nil {
		return fail(err)
	}
	if cmd.dryRun {
		if cmd.name == "wipe" {
			fmt.Fprintln(&output, "dry run: preview only; no rows changed")
		} else {
			fmt.Fprintln(&output, "dry run: rolled back")
		}
	}
	if _, err := io.Copy(stdout, &output); err != nil {
		return fail(fmt.Errorf("write output: %w", err))
	}
	return 0
}

func renderPrepare(w io.Writer, row league.LeagueRow) error {
	_, err := fmt.Fprintf(w, "league %q: %s (display name %q)\n", row.ID, row.CollectionState, row.DisplayName)
	if err == nil && row.StartsAt != nil {
		_, err = fmt.Fprintf(w, "starts at: %s\n", row.StartsAt.Format(time.RFC3339))
	}
	return err
}

func renderActivation(w io.Writer, result league.Activation) error {
	_, err := fmt.Fprintf(w, "before: league %q revision %d collecting\nafter: league %q archived; league %q revision %d collecting\n",
		result.FromID, result.FromRevision, result.FromID, result.ToID, result.ToRevision)
	return err
}

func renderActivationGuidance(w io.Writer, result league.Activation) error {
	// This order mirrors the retained WI-5 rollover procedure: verify before
	// wiping, restart the server before the counted stray-row check, and keep
	// the collector stopped until any resulting stray rows have been removed.
	_, err := fmt.Fprintf(w, `Verify committed league %q against poe.ninja's exact league id before wiping.
Keep the collector stopped with auto-restart disabled throughout the wipe and server restart.
Run in the server container:
  /league wipe %[3]s --dry-run
  /league wipe %[3]s
Set EXPECTED_LEAGUE to %[1]q or unset it for both the server and collector services.
Restart the server and wait until healthy.
Run the counted stray-row check in the server container:
  /league status
If outgoing league %[2]q rows remain, run:
  /league wipe %[3]s --allow-delete --dry-run
  /league wipe %[3]s --allow-delete
Only then re-enable auto-restart and start the collector.
Check /health and /latest report league %[1]q.
`, result.ToID, result.FromID, shellQuoteID(result.FromID))
	return err
}

// Quote for the shell in the suggested commands, including expansion tokens
// that fmt's Go-string quoting leaves literal.
func shellQuoteID(id string) string {
	return `"` + strings.NewReplacer(`\`, `\\`, `"`, `\"`, `$`, `\$`, "`", "\\`").Replace(id) + `"`
}

func renderStatus(w io.Writer, status league.Status) error {
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', 0)
	fmt.Fprintf(tw, "active league: %q\nrevision: %d\n", status.Active, status.Revision)
	fmt.Fprintln(tw, "LEAGUE\tDISPLAY NAME\tSTATE\tSTARTS AT")
	for _, row := range status.Leagues {
		startsAt := "-"
		if row.StartsAt != nil {
			startsAt = row.StartsAt.Format(time.RFC3339)
		}
		fmt.Fprintf(tw, "%q\t%q\t%s\t%s\n", row.ID, row.DisplayName, row.CollectionState, startsAt)
	}
	if status.Rows == nil {
		fmt.Fprintln(tw, "row counts omitted")
	} else {
		fmt.Fprintln(tw, "TABLE\tLEAGUE\tROWS")
		for _, table := range sortedKeys(status.Rows) {
			counts := status.Rows[table]
			if len(counts) == 0 {
				fmt.Fprintf(tw, "%s\t-\t0\n", table)
			}
			for _, id := range sortedKeys(counts) {
				fmt.Fprintf(tw, "%s\t%q\t%d\n", table, id, counts[id])
			}
		}
	}
	return tw.Flush()
}

func renderWipe(w io.Writer, result league.WipeResult) error {
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', 0)
	fmt.Fprintf(tw, "mode: %s\n", result.Mode)
	if result.Mode == "none" {
		fmt.Fprintln(tw, "nothing was changed")
	}
	for _, table := range sortedKeys(result.Counts) {
		fmt.Fprintf(tw, "%s\t%d\n", table, result.Counts[table])
	}
	for _, statement := range result.Statements {
		fmt.Fprintln(tw, statement)
	}
	return tw.Flush()
}

func sortedKeys[V any](values map[string]V) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
