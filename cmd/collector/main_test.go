package main

import (
	"bytes"
	"context"
	"io"
	"log/slog"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/collector"
	"profitofexile/internal/league"
)

// EXPECTED_LEAGUE is the deploy-time guard that stops a collector from writing
// under a league the operator did not intend. These tests pin the three
// behaviors the guard must have: opt-in absence, pass-on-match, fail-on-mismatch.

func TestCheckExpectedLeague_emptyExpectedIsNoOp(t *testing.T) {
	// Absence of EXPECTED_LEAGUE must not block startup — the runtime_config
	// selection stands on its own.
	if err := checkExpectedLeague("", "Mirage"); err != nil {
		t.Fatalf("empty expected should be a no-op, got error: %v", err)
	}
}

func TestCheckExpectedLeague_matchPasses(t *testing.T) {
	if err := checkExpectedLeague("Mirage", "Mirage"); err != nil {
		t.Fatalf("matching expected and resolved should pass, got error: %v", err)
	}
}

func TestCheckExpectedLeague_mismatchFails(t *testing.T) {
	err := checkExpectedLeague("Standard", "Mirage")
	if err == nil {
		t.Fatal("mismatch between EXPECTED_LEAGUE and resolved league must return an error")
	}
	// The message must name both leagues so an operator can see what was
	// expected versus what the runtime actually resolved.
	if !strings.Contains(err.Error(), "Standard") || !strings.Contains(err.Error(), "Mirage") {
		t.Fatalf("error should name both the expected and resolved league, got: %v", err)
	}
}

// itemStoreFunc gives the seven poe.ninja item-overview categories a store path
// that differs from the gem/currency/fragment one in exactly one respect: an
// empty 200 is a legitimate answer for an item category, so it must not be
// turned into an error and a MinSleep retry loop.

func TestItemStoreFunc_emptyPayloadStoresNothingWithoutError(t *testing.T) {
	var logged bytes.Buffer
	logger := slog.New(slog.NewTextHandler(&logged, &slog.HandlerOptions{Level: slog.LevelInfo}))

	called := false
	insert := func(ctx context.Context, scope league.Scope, snapTime time.Time, snapshots []collector.ItemSnapshot) (int, error) {
		called = true
		return len(snapshots), nil
	}

	store := itemStoreFunc(insert, league.Historical("Allflame"), "Vial", logger)
	inserted, err := store(context.Background(), time.Now(), &collector.FetchResult{})

	// A fresh league with nothing listed in Vial is not a transient API issue;
	// the tick is normal, so no error and no retry pressure.
	if err != nil {
		t.Fatalf("empty item payload must not be an error, got: %v", err)
	}
	if inserted != 0 {
		t.Errorf("inserted = %d, want 0", inserted)
	}
	if called {
		t.Error("repository insert was called for an empty payload; nothing should be written")
	}
	// The category has to be named, or an operator sees a silent zero-row tick
	// with no way to tell which of the seven served nothing.
	if out := logged.String(); !strings.Contains(out, "Vial") {
		t.Errorf("log output should name the empty category, got: %q", out)
	}
}

func TestItemStoreFunc_populatedPayloadIsInserted(t *testing.T) {
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))

	var got []collector.ItemSnapshot
	insert := func(ctx context.Context, scope league.Scope, snapTime time.Time, snapshots []collector.ItemSnapshot) (int, error) {
		got = snapshots
		return len(snapshots), nil
	}

	store := itemStoreFunc(insert, league.Historical("Allflame"), "Vial", logger)
	inserted, err := store(context.Background(), time.Now(), &collector.FetchResult{
		ItemData: []collector.ItemSnapshot{{Category: "Vial", NinjaID: 39826}},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if inserted != 1 {
		t.Errorf("inserted = %d, want 1", inserted)
	}
	if len(got) != 1 || got[0].NinjaID != 39826 {
		t.Errorf("insert received %+v, want the fetched line 39826", got)
	}
}
