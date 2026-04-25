package db

import (
	"bytes"
	"context"
	"log/slog"
	"os"
	"strings"
	"testing"
	"time"
)

func TestNewPool(t *testing.T) {
	tests := []struct {
		name        string
		databaseURL string
		wantErr     string
	}{
		{
			name:        "empty URL returns descriptive error",
			databaseURL: "",
			wantErr:     "db:",
		},
		{
			name:        "invalid URL format returns descriptive error",
			databaseURL: "not-a-valid-url://???",
			wantErr:     "db: parse config",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()

			pool, err := NewPool(ctx, tt.databaseURL)
			if err == nil {
				pool.Close()
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Errorf("error = %q, want it to contain %q", err.Error(), tt.wantErr)
			}
		})
	}
}

func TestResolveMaxConns(t *testing.T) {
	tests := []struct {
		name     string
		env      *string // nil = unset; pointer so we can distinguish "" from "unset"
		want     int
		wantWarn bool
	}{
		{"unset returns default", nil, 50, false},
		{"valid override", strPtr("10"), 10, false},
		{"non-numeric falls back", strPtr("invalid"), 50, true},
		{"zero falls back", strPtr("0"), 50, true},
		{"negative falls back", strPtr("-5"), 50, true},
		{"empty string falls back", strPtr(""), 50, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.env != nil {
				// Set path: t.Setenv records the prior value and restores
				// automatically at test end.
				t.Setenv("POE_DB_MAX_CONNS", *tt.env)
			} else {
				// Unset path: do NOT use t.Setenv here — mixing t.Setenv
				// with os.Unsetenv conflicts with t.Setenv's auto-cleanup.
				// Use os.Unsetenv + a manual t.Cleanup that re-asserts unset
				// (cheap and idempotent — no other test in this package
				// uses POE_DB_MAX_CONNS, so leakage risk is low).
				os.Unsetenv("POE_DB_MAX_CONNS")
				t.Cleanup(func() { os.Unsetenv("POE_DB_MAX_CONNS") })
			}

			// Capture slog output to assert WARN emissions on rejected values.
			var buf bytes.Buffer
			prev := slog.Default()
			slog.SetDefault(slog.New(slog.NewJSONHandler(&buf, &slog.HandlerOptions{Level: slog.LevelWarn})))
			t.Cleanup(func() { slog.SetDefault(prev) })

			got := resolveMaxConns()
			if got != tt.want {
				t.Errorf("resolveMaxConns() = %d, want %d", got, tt.want)
			}

			gotWarn := strings.Contains(buf.String(), `"level":"WARN"`)
			if gotWarn != tt.wantWarn {
				t.Errorf("WARN log emitted = %v, want %v (logs: %s)", gotWarn, tt.wantWarn, buf.String())
			}
		})
	}
}

func strPtr(s string) *string { return &s }
