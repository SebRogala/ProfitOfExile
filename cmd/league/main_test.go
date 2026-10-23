package main

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"profitofexile/internal/league"
)

// Synthetic ids, revisions and timestamps come from WI-4's Measured cases.
func TestParseArgs_preservesCommandValues(t *testing.T) {
	starts := time.Date(2026, time.December, 4, 19, 0, 0, 0, time.UTC)
	cases := []struct {
		name string
		args []string
		want command
	}{
		{"activate_trailing_flag", []string{"activate", "NextLeague", "--dry-run"}, command{name: "activate", id: "NextLeague", dryRun: true, withCounts: true}},
		{"activate_leading_flag", []string{"activate", "--dry-run", "NextLeague"}, command{name: "activate", id: "NextLeague", dryRun: true, withCounts: true}},
		{"wipe_opt_in", []string{"wipe", "OldLeague", "--allow-delete"}, command{name: "wipe", id: "OldLeague", allowDelete: true, withCounts: true}},
		{"prepare_metadata", []string{"prepare", "NextLeague", "--display-name", "Next League", "--starts-at", "2026-12-04T19:00:00Z"}, command{name: "prepare", id: "NextLeague", displayName: "Next League", startsAt: &starts, withCounts: true}},
		{"status_counts", []string{"status"}, command{name: "status", withCounts: true}},
		{"status_without_counts", []string{"status", "--no-counts"}, command{name: "status", withCounts: false}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := parseArgs(tc.args)
			if err != nil {
				t.Fatal(err)
			}
			if !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("parseArgs(%q) = %+v; want %+v", tc.args, got, tc.want)
			}
		})
	}
}

func TestParseArgs_rejectsInvalidArguments(t *testing.T) {
	cases := []struct {
		name    string
		args    []string
		message string
	}{
		{"invalid_start", []string{"prepare", "NextLeague", "--starts-at", "2026-13-01"}, "--starts-at"},
		{"missing_id", []string{"activate"}, "Usage:"},
		{"extra_id", []string{"activate", "A", "B"}, "Usage:"},
		{"unknown_subcommand", []string{"frobnicate"}, "frobnicate"},
		{"inapplicable_flag", []string{"status", "--allow-delete"}, "--allow-delete"},
		{"empty_arguments", nil, "Usage:"},
		{"empty_id", []string{"activate", ""}, "league id"},
		{"padded_id", []string{"activate", " NextLeague"}, "league id"},
		{"missing_flag_value", []string{"prepare", "NextLeague", "--display-name"}, "needs a value"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := parseArgs(tc.args)
			if err == nil || !strings.Contains(err.Error(), tc.message) {
				t.Fatalf("parseArgs(%q) error = %v; want %q", tc.args, err, tc.message)
			}
		})
	}
}

func TestRun_requiresDatabaseURL(t *testing.T) {
	var out, stderr bytes.Buffer
	code := run([]string{"status"}, func(string) string { return "" }, &out, &stderr)
	if code == 0 || !strings.Contains(stderr.String(), "DATABASE_URL") || out.Len() != 0 {
		t.Fatalf("code=%d stdout=%q stderr=%q; want DATABASE_URL refusal without success output", code, out.String(), stderr.String())
	}
}

func TestRenderActivation_reportsLifecycleTransition(t *testing.T) {
	var out bytes.Buffer
	if err := renderActivation(&out, league.Activation{FromID: "OldLeague", FromRevision: 3, ToID: "NextLeague", ToRevision: 4}); err != nil {
		t.Fatal(err)
	}
	want := "before: league \"OldLeague\" revision 3 collecting\nafter: league \"OldLeague\" archived; league \"NextLeague\" revision 4 collecting\n"
	if out.String() != want {
		t.Fatalf("activation output = %q; want %q", out.String(), want)
	}
}

func TestRenderActivationGuidance_preservesRolloverOrder(t *testing.T) {
	var out bytes.Buffer
	if err := renderActivationGuidance(&out, league.Activation{FromID: "OldLeague", FromRevision: 3, ToID: "NextLeague", ToRevision: 4}); err != nil {
		t.Fatal(err)
	}
	assertRolloverGuidance(t, out.String())
}

func assertRolloverGuidance(t *testing.T, output string) {
	t.Helper()
	// WI-5's ordered procedure, substituting WI-4's synthetic outgoing/new ids.
	steps := []string{
		"Verify committed league \"NextLeague\" against poe.ninja's exact league id before wiping.",
		"Keep the collector stopped with auto-restart disabled throughout the wipe and server restart.",
		"  /league wipe \"OldLeague\" --dry-run\n",
		"  /league wipe \"OldLeague\"\n",
		"Set EXPECTED_LEAGUE to \"NextLeague\" or unset it for both the server and collector services.",
		"Restart the server and wait until healthy.",
		"Run the counted stray-row check in the server container:\n  /league status\n",
		"If outgoing league \"OldLeague\" rows remain, run:",
		"  /league wipe \"OldLeague\" --allow-delete --dry-run\n",
		"  /league wipe \"OldLeague\" --allow-delete\n",
		"Only then re-enable auto-restart and start the collector.",
		"Check /health and /latest report league \"NextLeague\".",
	}
	previous := -1
	for _, step := range steps {
		at := strings.Index(output, step)
		if at < 0 {
			t.Fatalf("guidance missing %q:\n%s", step, output)
		}
		if at <= previous {
			t.Fatalf("guidance step %q is out of order:\n%s", step, output)
		}
		previous = at
	}
}

func TestRenderWipe_reportsModeAndCounts(t *testing.T) {
	var out bytes.Buffer
	if err := renderWipe(&out, league.WipeResult{Mode: "truncate", Counts: map[string]int64{"gem_snapshots": 2}}); err != nil {
		t.Fatal(err)
	}
	if got := strings.Fields(out.String()); !reflect.DeepEqual(got, []string{"mode:", "truncate", "gem_snapshots", "2"}) {
		t.Fatalf("wipe output = %q; want truncate and gem_snapshots count 2", out.String())
	}
}

func TestRenderWipe_reportsNoChange(t *testing.T) {
	var out bytes.Buffer
	if err := renderWipe(&out, league.WipeResult{Mode: "none"}); err != nil {
		t.Fatal(err)
	}
	if out.String() != "mode: none\nnothing was changed\n" {
		t.Fatalf("no-change output = %q", out.String())
	}
}

// All default operations fail closed. A dispatch test installs only its allowed
// operation; selecting another operation fails before any database can be used.
func memoryDeps(t *testing.T, wantDry bool) cliDeps {
	t.Helper()
	adminCalled := false
	deps := cliDeps{
		openPool:  func(context.Context, string) (*pgxpool.Pool, error) { return nil, nil },
		closePool: func(*pgxpool.Pool) {},
		runAdmin: func(_ context.Context, _ *pgxpool.Pool, dry bool, op func(pgx.Tx) error) error {
			adminCalled = true
			if dry != wantDry {
				t.Errorf("RunAdmin dryRun = %v; want %v", dry, wantDry)
			}
			return op(nil)
		},
		prepare: func(context.Context, pgx.Tx, league.PrepareInput) (league.LeagueRow, error) {
			t.Fatal("forbidden Prepare")
			return league.LeagueRow{}, nil
		},
		activate: func(context.Context, pgx.Tx, string) (league.Activation, error) {
			t.Fatal("forbidden Activate")
			return league.Activation{}, nil
		},
		readStatus: func(context.Context, pgx.Tx, bool) (league.Status, error) {
			t.Fatal("forbidden ReadStatus")
			return league.Status{}, nil
		},
		previewWipe: func(context.Context, pgx.Tx, string, bool) (league.WipeResult, error) {
			t.Fatal("forbidden PreviewWipe")
			return league.WipeResult{}, nil
		},
		wipe: func(context.Context, pgx.Tx, string, bool) (league.WipeResult, error) {
			t.Fatal("forbidden Wipe")
			return league.WipeResult{}, nil
		},
	}
	t.Cleanup(func() {
		if !adminCalled {
			t.Error("RunAdmin was not invoked")
		}
	})
	return deps
}

func executeMemory(t *testing.T, args []string, deps cliDeps) string {
	t.Helper()
	var out, stderr bytes.Buffer
	code := runWithDeps(args, func(key string) string {
		if key != "DATABASE_URL" {
			t.Fatalf("unexpected environment lookup %q", key)
		}
		return "synthetic-in-memory"
	}, &out, &stderr, deps)
	if code != 0 || stderr.Len() != 0 {
		t.Fatalf("runWithDeps(%q): code=%d stderr=%q stdout=%q", args, code, stderr.String(), out.String())
	}
	return out.String()
}

func TestPrepareDispatch_forwardsMetadata(t *testing.T) {
	for _, dry := range []bool{false, true} {
		t.Run(fmt.Sprintf("dry_run_%t", dry), func(t *testing.T) {
			deps := memoryDeps(t, dry)
			starts := time.Date(2026, time.December, 4, 19, 0, 0, 0, time.UTC)
			deps.prepare = func(_ context.Context, _ pgx.Tx, input league.PrepareInput) (league.LeagueRow, error) {
				want := league.PrepareInput{ID: "NextLeague", DisplayName: "Next League", StartsAt: &starts}
				if !reflect.DeepEqual(input, want) {
					t.Fatalf("Prepare input = %+v; want %+v", input, want)
				}
				return league.LeagueRow{ID: "NextLeague", DisplayName: "Next League", StartsAt: &starts, CollectionState: "prepared"}, nil
			}
			args := []string{"prepare", "NextLeague", "--display-name", "Next League", "--starts-at", "2026-12-04T19:00:00Z"}
			if dry {
				args = append(args, "--dry-run")
			}
			output := executeMemory(t, args, deps)
			want := "league \"NextLeague\": prepared (display name \"Next League\")\nstarts at: 2026-12-04T19:00:00Z\n"
			if dry {
				want += "dry run: rolled back\n"
			}
			if output != want {
				t.Fatalf("prepare output = %q; want %q", output, want)
			}
		})
	}
}

func activationDeps(t *testing.T, dry bool) cliDeps {
	t.Helper()
	deps := memoryDeps(t, dry)
	deps.activate = func(_ context.Context, _ pgx.Tx, id string) (league.Activation, error) {
		if id != "NextLeague" {
			t.Fatalf("Activate id = %q; want NextLeague", id)
		}
		return league.Activation{FromID: "OldLeague", FromRevision: 3, ToID: "NextLeague", ToRevision: 4}, nil
	}
	return deps
}

func TestActivateDispatch_dryRunRollsBack(t *testing.T) {
	output := executeMemory(t, []string{"activate", "NextLeague", "--dry-run"}, activationDeps(t, true))
	want := "before: league \"OldLeague\" revision 3 collecting\nafter: league \"OldLeague\" archived; league \"NextLeague\" revision 4 collecting\ndry run: rolled back\n"
	if output != want {
		t.Fatalf("dry activation output = %q; want %q (without committed guidance)", output, want)
	}
}

func TestActivateDispatch_commitPrintsGuidance(t *testing.T) {
	output := executeMemory(t, []string{"activate", "NextLeague"}, activationDeps(t, false))
	assertRolloverGuidance(t, output)
	if strings.Contains(output, "dry run:") {
		t.Fatalf("committed activation claims a dry run: %q", output)
	}
}

func TestWipeDispatch_selectsSafeOperation(t *testing.T) {
	for _, dry := range []bool{false, true} {
		for _, allow := range []bool{false, true} {
			t.Run(fmt.Sprintf("dry_run_%t_allow_delete_%t", dry, allow), func(t *testing.T) {
				deps := memoryDeps(t, dry)
				operation := func(_ context.Context, _ pgx.Tx, id string, allowDelete bool) (league.WipeResult, error) {
					if id != "OldLeague" || allowDelete != allow {
						t.Fatalf("wipe arguments = (%q, %v); want (OldLeague, %v)", id, allowDelete, allow)
					}
					return league.WipeResult{Mode: "truncate", Counts: map[string]int64{"gem_snapshots": 2}}, nil
				}
				if dry {
					deps.previewWipe = operation
				} else {
					deps.wipe = operation
				}
				args := []string{"wipe", "OldLeague"}
				if dry {
					args = append(args, "--dry-run")
				}
				if allow {
					args = append(args, "--allow-delete")
				}
				output := executeMemory(t, args, deps)
				want := []string{"wipe", `"OldLeague"`, "mode:", "truncate", "gem_snapshots", "2"}
				if dry {
					want = append(want, "dry", "run:", "preview", "only;", "no", "rows", "changed")
				}
				if !reflect.DeepEqual(strings.Fields(output), want) {
					t.Fatalf("wipe output = %q; want fields %q", output, want)
				}
			})
		}
	}
}

func TestStatusDispatch_forwardsCountSelection(t *testing.T) {
	for _, counts := range []bool{true, false} {
		t.Run(fmt.Sprintf("counts_%t", counts), func(t *testing.T) {
			deps := memoryDeps(t, false)
			deps.readStatus = func(_ context.Context, _ pgx.Tx, withCounts bool) (league.Status, error) {
				if withCounts != counts {
					t.Fatalf("ReadStatus withCounts = %v; want %v", withCounts, counts)
				}
				status := league.Status{Active: "NextLeague", Revision: 4}
				if counts {
					status.Rows = map[string]map[string]int64{"gem_snapshots": {"OldLeague": 2}}
				}
				return status, nil
			}
			args := []string{"status"}
			if !counts {
				args = append(args, "--no-counts")
			}
			output := executeMemory(t, args, deps)
			want := []string{"active", "league:", `"NextLeague"`, "revision:", "4", "LEAGUE", "DISPLAY", "NAME", "STATE", "STARTS", "AT"}
			if counts {
				want = append(want, "TABLE", "LEAGUE", "ROWS", "gem_snapshots", `"OldLeague"`, "2")
			} else {
				want = append(want, "row", "counts", "omitted")
			}
			if !reflect.DeepEqual(strings.Fields(output), want) {
				t.Fatalf("status output = %q; want fields %q", output, want)
			}
		})
	}
}

func TestAdministrationRefusal_emitsNoSuccess(t *testing.T) {
	for _, stage := range []string{"operation", "transaction_completion"} {
		t.Run(stage, func(t *testing.T) {
			deps := activationDeps(t, false)
			refusal := errors.New("synthetic administration refusal")
			if stage == "operation" {
				deps.activate = func(context.Context, pgx.Tx, string) (league.Activation, error) { return league.Activation{}, refusal }
			} else {
				original := deps.runAdmin
				deps.runAdmin = func(ctx context.Context, pool *pgxpool.Pool, dry bool, op func(pgx.Tx) error) error {
					if err := original(ctx, pool, dry, op); err != nil {
						return err
					}
					return refusal
				}
			}
			var out, stderr bytes.Buffer
			code := runWithDeps([]string{"activate", "NextLeague"}, func(string) string { return "synthetic-in-memory" }, &out, &stderr, deps)
			if code == 0 || stderr.String() != "error: synthetic administration refusal\n" || out.Len() != 0 {
				t.Fatalf("refusal: code=%d stdout=%q stderr=%q; want nonzero, refusal and no success/guidance", code, out.String(), stderr.String())
			}
		})
	}
}
