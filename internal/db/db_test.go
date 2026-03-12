package db

import (
	"context"
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
