package server

import (
	"context"
	"profitofexile/internal/league"
	"testing"
)

func TestHandleTradeTick_RejectsInvalidCadence(t *testing.T) {
	for _, raw := range []string{
		"not json", `{"league":"Other"}`, `{}`,
		`{"league":"Test","minTier":"NOPE"}`,
		`{"league":"Test","minAge":"bad"}`,
		`{"league":"Test","minAge":"-1m"}`,
		`{"league":"Test","leagueRevision":2}`,
	} {
		t.Run(raw, func(t *testing.T) {
			selected := ""
			HandleTradeTick(context.Background(), league.Historical("Test"), []byte(raw), func(context.Context) { selected = "lookup" })
			if selected != "" {
				t.Fatalf("invalid cadence selected %q", selected)
			}
		})
	}
}

func TestHandleTradeTick_LegacyHintsDelegateWithoutFiltering(t *testing.T) {
	for _, raw := range []string{
		`{"league":"Test"}`,
		`{"league":"Test","variant":"20/20","minTier":"MID","minAge":"1m"}`,
		`{"league":"Test","variant":"1/0","minTier":"CHAOTIC"}`,
	} {
		t.Run(raw, func(t *testing.T) {
			selected := ""
			HandleTradeTick(context.Background(), league.Historical("Test"), []byte(raw), func(context.Context) { selected = "snapshot:1" })
			if selected != "snapshot:1" {
				t.Fatalf("cadence did not delegate: %q", selected)
			}
		})
	}
}

func TestHandleTradeTick_CancelledClaimsNothing(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	selected := ""
	HandleTradeTick(ctx, league.Historical("Test"), []byte(`{"league":"Test"}`), func(context.Context) { selected = "lookup" })
	if selected != "" {
		t.Fatalf("cancelled tick selected %q", selected)
	}
}

func TestHandleTradeTick_Disabled(t *testing.T) {
	HandleTradeTick(context.Background(), league.Historical("Test"), []byte(`{"league":"Test"}`), nil)
}
