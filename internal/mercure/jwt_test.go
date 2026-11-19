package mercure

import (
	"testing"
	"time"
)

// Golden tokens were produced by the pre-merge signer bodies (cd505b1d) with
// time.Now() pinned to the fixed instant below.
func TestKeepCritical_SignJWTMatchesBaseTokenBytes(t *testing.T) {
	now := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)

	cases := []struct {
		name  string
		claim map[string]any
		ttl   time.Duration
		want  string
	}{
		{
			name:  "publisher",
			claim: map[string]any{"publish": []string{"*"}},
			ttl:   60 * time.Second,
			want:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3NjcyMjU2NjAsImlhdCI6MTc2NzIyNTYwMCwibWVyY3VyZSI6eyJwdWJsaXNoIjpbIioiXX19.VQ67YH2Z9Aj7KfzhvhQ-sQYfosr2vV6nEkSF0yX_k6I",
		},
		{
			name:  "server subscriber",
			claim: map[string]any{"subscribe": []string{"poe/collector/gems", "poe/admin/recompute"}},
			ttl:   24 * time.Hour,
			want:  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3NjczMTIwMDAsImlhdCI6MTc2NzIyNTYwMCwibWVyY3VyZSI6eyJzdWJzY3JpYmUiOlsicG9lL2NvbGxlY3Rvci9nZW1zIiwicG9lL2FkbWluL3JlY29tcHV0ZSJdfX0.LFnFvGYepdg-xbKUnXHSnaUuho1yF29tjO2WO875kO0",
		},
		{
			name: "browser subscriber",
			claim: map[string]any{"subscribe": []string{
				"poe/league/active",
				"poe/analysis/updated",
				"poe/trade/results",
				"poe/desktop/{pair}",
				"poe/lab/layout",
				"poe/currency-exchange/updated",
				"poe/harvest/updated",
			}},
			ttl:  30 * time.Minute,
			want: "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjE3NjcyMjc0MDAsImlhdCI6MTc2NzIyNTYwMCwibWVyY3VyZSI6eyJzdWJzY3JpYmUiOlsicG9lL2xlYWd1ZS9hY3RpdmUiLCJwb2UvYW5hbHlzaXMvdXBkYXRlZCIsInBvZS90cmFkZS9yZXN1bHRzIiwicG9lL2Rlc2t0b3Ave3BhaXJ9IiwicG9lL2xhYi9sYXlvdXQiLCJwb2UvY3VycmVuY3ktZXhjaGFuZ2UvdXBkYXRlZCIsInBvZS9oYXJ2ZXN0L3VwZGF0ZWQiXX19.Tzu5RSwbTGDOaytHp7ZWctN9thYyG8CcWZWzwc1HDUM",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got, err := signJWTAt("test-secret", tc.claim, tc.ttl, now)
			if err != nil {
				t.Fatalf("signJWTAt: %v", err)
			}
			if got != tc.want {
				t.Errorf("token drifted from base bytes\n got: %s\nwant: %s", got, tc.want)
			}
		})
	}
}
