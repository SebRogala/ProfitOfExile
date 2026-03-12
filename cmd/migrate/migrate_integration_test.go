//go:build integration

package main

import (
	"os"
	"strings"
	"testing"
)

func testDatabaseURL(t *testing.T) string {
	t.Helper()
	dbURL := os.Getenv("DATABASE_URL")
	if dbURL == "" {
		t.Skip("DATABASE_URL not set, skipping integration test")
	}
	// golang-migrate uses lib/pq which requires explicit sslmode.
	if !strings.Contains(dbURL, "sslmode=") {
		if strings.Contains(dbURL, "?") {
			dbURL += "&sslmode=disable"
		} else {
			dbURL += "?sslmode=disable"
		}
	}
	return dbURL
}

func TestRunDownValidation(t *testing.T) {
	dbURL := testDatabaseURL(t)

	tests := []struct {
		name    string
		args    []string
		wantErr string
	}{
		{
			name:    "non-numeric step count returns error",
			args:    []string{"down", "abc"},
			wantErr: "invalid step count",
		},
		{
			name:    "zero step count returns error",
			args:    []string{"down", "0"},
			wantErr: "step count must be a positive integer",
		},
		{
			name:    "negative step count returns error",
			args:    []string{"down", "-5"},
			wantErr: "step count must be a positive integer",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := run(tt.args, dbURL)
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Errorf("error = %q, want it to contain %q", err.Error(), tt.wantErr)
			}
		})
	}
}

func TestRunForceValidation(t *testing.T) {
	dbURL := testDatabaseURL(t)

	tests := []struct {
		name    string
		args    []string
		wantErr string
	}{
		{
			name:    "missing version argument returns usage error",
			args:    []string{"force"},
			wantErr: "usage: migrate force VERSION",
		},
		{
			name:    "non-numeric version returns error",
			args:    []string{"force", "notanumber"},
			wantErr: "invalid version",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := run(tt.args, dbURL)
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Errorf("error = %q, want it to contain %q", err.Error(), tt.wantErr)
			}
		})
	}
}

func TestRunUpDownVersion(t *testing.T) {
	dbURL := testDatabaseURL(t)

	// run() uses "file://db/migrations" relative to CWD; chdir to project root.
	origDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("get working directory: %v", err)
	}
	if err := os.Chdir("../../"); err != nil {
		t.Fatalf("chdir to project root: %v", err)
	}
	t.Cleanup(func() { _ = os.Chdir(origDir) })

	// Apply all migrations.
	if err := run([]string{"up"}, dbURL); err != nil {
		t.Fatalf("migrate up: %v", err)
	}

	// Check version reports without error.
	if err := run([]string{"version"}, dbURL); err != nil {
		t.Fatalf("migrate version after up: %v", err)
	}

	// Roll back one migration.
	if err := run([]string{"down", "1"}, dbURL); err != nil {
		t.Fatalf("migrate down 1: %v", err)
	}

	// Version should still work (either reports a version or "no migrations applied").
	if err := run([]string{"version"}, dbURL); err != nil {
		t.Fatalf("migrate version after down: %v", err)
	}

	// Re-apply to leave DB in a clean state.
	if err := run([]string{"up"}, dbURL); err != nil {
		t.Fatalf("migrate up (cleanup): %v", err)
	}
}
