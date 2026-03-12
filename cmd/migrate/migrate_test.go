package main

import (
	"strings"
	"testing"
)

func TestRun(t *testing.T) {
	tests := []struct {
		name        string
		args        []string
		databaseURL string
		wantErr     string
	}{
		{
			name:        "empty database URL returns error",
			args:        []string{"up"},
			databaseURL: "",
			wantErr:     "database URL is required",
		},
		{
			name:        "no subcommand returns usage error",
			args:        []string{},
			databaseURL: "postgres://user:pass@localhost/db",
			wantErr:     "usage",
		},
		{
			name:        "unknown command returns error",
			args:        []string{"unknown"},
			databaseURL: "postgres://user:pass@localhost/db",
			wantErr:     "unknown command",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := run(tt.args, tt.databaseURL)
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tt.wantErr) {
				t.Errorf("error = %q, want it to contain %q", err.Error(), tt.wantErr)
			}
		})
	}
}
