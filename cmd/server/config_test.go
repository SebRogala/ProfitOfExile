package main

import (
	"bytes"
	"log/slog"
	"reflect"
	"strings"
	"testing"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/server"
	"profitofexile/internal/trade"
)

var serverConfigEnv = []string{
	"PORT",
	"MERCURE_URL", "MERCURE_JWT_SECRET", "APP_ENV", "MERCURE_SUBSCRIBER_KEY", "MERCURE_PUBLIC_URL",
	"CORS_ORIGINS", "ICON_CACHE_DIR",
	"EXCHANGE_MIN_VOLUME_PER_HOUR", "EXCHANGE_MAX_PLAYS", "EXCHANGE_MIN_TURNOVER_CHAOS",
	"EXCHANGE_MAX_TICK", "EXCHANGE_MIN_EDGE_TICK_RATIO", "EXCHANGE_MIN_ROI_CHAOS",
	"EXCHANGE_SUSPECT_LOW_BAND", "EXCHANGE_SUSPECT_HIGH_BAND", "EXCHANGE_HIDE_SUSPECT",
	"EXCHANGE_MIN_EDGE", "EXCHANGE_WINDOW_HOURS", "EXCHANGE_MIN_HOURS_SEEN",
	"EXCHANGE_RECENT_WINDOW_HOURS", "EXCHANGE_RECENT_MIN_HOURS_SEEN",
	"EXCHANGE_DAY_WINDOW_HOURS", "EXCHANGE_DAY_MIN_HOURS_SEEN",
	"TRADE_ENABLED", "TRADE_CACHE_MAX", "TRADE_CEILING", "TRADE_LATENCY_PAD",
	"TRADE_MAX_WAIT", "TRADE_SYNC_WAIT", "TRADE_USER_AGENT",
}

func clearServerConfigEnv(t *testing.T) {
	t.Helper()
	for _, key := range serverConfigEnv {
		t.Setenv(key, "")
	}
}

func captureServerConfigLogs(t *testing.T) *bytes.Buffer {
	t.Helper()
	var output bytes.Buffer
	previous := slog.Default()
	slog.SetDefault(slog.New(slog.NewTextHandler(&output, nil)))
	t.Cleanup(func() { slog.SetDefault(previous) })
	return &output
}

func TestLoadPort_unsetUses8080(t *testing.T) {
	clearServerConfigEnv(t)

	got, err := loadPort()
	if err != nil {
		t.Fatalf("loadPort() error = %v, want nil", err)
	}
	if got != "8080" {
		t.Fatalf("loadPort() = %q, want %q", got, "8080")
	}
}

func TestLoadPort_acceptsBoundaryPorts(t *testing.T) {
	clearServerConfigEnv(t)
	for _, port := range []string{"1", "65535"} {
		t.Setenv("PORT", port)

		got, err := loadPort()
		if err != nil {
			t.Errorf("loadPort(%q) error = %v, want nil", port, err)
		}
		if got != port {
			t.Errorf("loadPort(%q) = %q, want %q", port, got, port)
		}
	}
}

func TestLoadPort_rejectsInvalidValues(t *testing.T) {
	clearServerConfigEnv(t)
	for _, port := range []string{"0", "-1", "65536", "not-a-port"} {
		t.Setenv("PORT", port)

		got, err := loadPort()
		if err == nil {
			t.Errorf("loadPort(%q) error = nil, want rejection", port)
		}
		if got != port {
			t.Errorf("loadPort(%q) returned port %q, want original value", port, got)
		}
	}
}

func TestLoadSharedServiceConfig_unsetUsesDefaults(t *testing.T) {
	clearServerConfigEnv(t)

	got := loadSharedServiceConfig()
	want := sharedServiceConfig{IconCacheDir: server.DefaultIconCacheDir}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("loadSharedServiceConfig() = %+v, want %+v", got, want)
	}
}

func TestLoadSharedServiceConfig_readsSharedOverrides(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("MERCURE_URL", "http://mercure.test/.well-known/mercure")
	t.Setenv("MERCURE_JWT_SECRET", "configured")
	t.Setenv("APP_ENV", "dev")
	t.Setenv("MERCURE_SUBSCRIBER_KEY", "subscriber-key")
	t.Setenv("MERCURE_PUBLIC_URL", "http://public.test/.well-known/mercure")
	t.Setenv("CORS_ORIGINS", " http://localhost:1420, , tauri://localhost ")
	t.Setenv("ICON_CACHE_DIR", "/tmp/icons")

	got := loadSharedServiceConfig()
	if got.MercureURL != "http://mercure.test/.well-known/mercure" || got.MercureSecret != "configured" {
		t.Fatal("Mercure settings did not preserve configured values")
	}
	if !got.DevMode || got.MercureSubscriberKey != "subscriber-key" || got.MercurePublicURL != "http://public.test/.well-known/mercure" {
		t.Fatalf("shared service settings = %+v, want configured values", got)
	}
	if want := []string{"http://localhost:1420", "tauri://localhost"}; !reflect.DeepEqual(got.AllowedOrigins, want) {
		t.Fatalf("AllowedOrigins = %#v, want %#v", got.AllowedOrigins, want)
	}
	if got.IconCacheDir != "/tmp/icons" {
		t.Fatalf("IconCacheDir = %q, want %q", got.IconCacheDir, "/tmp/icons")
	}
}

func TestLoadExchangeConfig_unsetPreservesDefaults(t *testing.T) {
	clearServerConfigEnv(t)

	got := loadExchangeConfig()
	want := exchange.DefaultConfig()
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("loadExchangeConfig() = %+v, want DefaultConfig() %+v", got, want)
	}
}

func TestLoadExchangeConfig_invalidPositiveOverridesWarnAndKeepDefaults(t *testing.T) {
	clearServerConfigEnv(t)
	logs := captureServerConfigLogs(t)
	t.Setenv("EXCHANGE_MIN_VOLUME_PER_HOUR", "0")
	t.Setenv("EXCHANGE_MAX_PLAYS", "-1")
	t.Setenv("EXCHANGE_MAX_TICK", "not-a-number")

	got := loadExchangeConfig()
	want := exchange.DefaultConfig()
	if got.MinVolumePerHour != want.MinVolumePerHour || got.MaxPlays != want.MaxPlays || got.MaxTick != want.MaxTick {
		t.Fatalf("invalid overrides changed config: got min volume %v, max plays %d, max tick %v; want defaults %v, %d, %v",
			got.MinVolumePerHour, got.MaxPlays, got.MaxTick, want.MinVolumePerHour, want.MaxPlays, want.MaxTick)
	}
	for _, key := range []string{"EXCHANGE_MIN_VOLUME_PER_HOUR", "EXCHANGE_MAX_PLAYS", "EXCHANGE_MAX_TICK"} {
		if !strings.Contains(logs.String(), key) {
			t.Errorf("warning log = %q, want it to name %s", logs.String(), key)
		}
	}
}

func TestLoadExchangeConfig_validOverridesPopulateTheirFields(t *testing.T) {
	tests := []struct {
		name   string
		key    string
		value  string
		assert func(t *testing.T, got exchange.Config)
	}{
		{
			name:  "minimum volume",
			key:   "EXCHANGE_MIN_VOLUME_PER_HOUR",
			value: "3.25",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MinVolumePerHour != 3.25 {
					t.Errorf("MinVolumePerHour = %v, want 3.25", got.MinVolumePerHour)
				}
			},
		},
		{
			name:  "maximum plays",
			key:   "EXCHANGE_MAX_PLAYS",
			value: "17",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MaxPlays != 17 {
					t.Errorf("MaxPlays = %d, want 17", got.MaxPlays)
				}
			},
		},
		{
			name:  "minimum turnover",
			key:   "EXCHANGE_MIN_TURNOVER_CHAOS",
			value: "123.5",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MinTurnoverChaos != 123.5 {
					t.Errorf("MinTurnoverChaos = %v, want 123.5", got.MinTurnoverChaos)
				}
			},
		},
		{
			name:  "maximum tick",
			key:   "EXCHANGE_MAX_TICK",
			value: "0.25",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MaxTick != 0.25 {
					t.Errorf("MaxTick = %v, want 0.25", got.MaxTick)
				}
			},
		},
		{
			name:  "minimum edge tick ratio",
			key:   "EXCHANGE_MIN_EDGE_TICK_RATIO",
			value: "2.5",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MinEdgeTickRatio != 2.5 {
					t.Errorf("MinEdgeTickRatio = %v, want 2.5", got.MinEdgeTickRatio)
				}
			},
		},
		{
			name:  "minimum roi",
			key:   "EXCHANGE_MIN_ROI_CHAOS",
			value: "7.75",
			assert: func(t *testing.T, got exchange.Config) {
				if got.MinROIChaos != 7.75 {
					t.Errorf("MinROIChaos = %v, want 7.75", got.MinROIChaos)
				}
			},
		},
		{
			name:  "suspect low band",
			key:   "EXCHANGE_SUSPECT_LOW_BAND",
			value: "0.55",
			assert: func(t *testing.T, got exchange.Config) {
				if got.SuspectLowBand != 0.55 {
					t.Errorf("SuspectLowBand = %v, want 0.55", got.SuspectLowBand)
				}
			},
		},
		{
			name:  "suspect high band",
			key:   "EXCHANGE_SUSPECT_HIGH_BAND",
			value: "2.25",
			assert: func(t *testing.T, got exchange.Config) {
				if got.SuspectHighBand != 2.25 {
					t.Errorf("SuspectHighBand = %v, want 2.25", got.SuspectHighBand)
				}
			},
		},
		{
			name:  "hide suspect",
			key:   "EXCHANGE_HIDE_SUSPECT",
			value: "true",
			assert: func(t *testing.T, got exchange.Config) {
				if !got.HideSuspect {
					t.Error("HideSuspect = false, want true")
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			clearServerConfigEnv(t)
			t.Setenv(tt.key, tt.value)

			tt.assert(t, loadExchangeConfig())
		})
	}
}

func TestLoadExchangeConfig_unparseableHideSuspectWarnsAndKeepsDefault(t *testing.T) {
	clearServerConfigEnv(t)
	logs := captureServerConfigLogs(t)
	t.Setenv("EXCHANGE_HIDE_SUSPECT", "not-a-bool")

	got := loadExchangeConfig()
	if got.HideSuspect {
		t.Fatal("HideSuspect = true for an unparseable value, want false")
	}
	if !strings.Contains(logs.String(), "EXCHANGE_HIDE_SUSPECT") {
		t.Fatalf("warning log = %q, want it to name EXCHANGE_HIDE_SUSPECT", logs.String())
	}
}

func TestLoadExchangeConfig_recentOverridesOutrankLegacyNames(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("EXCHANGE_WINDOW_HOURS", "4")
	t.Setenv("EXCHANGE_MIN_HOURS_SEEN", "2")
	t.Setenv("EXCHANGE_RECENT_WINDOW_HOURS", "9")
	t.Setenv("EXCHANGE_RECENT_MIN_HOURS_SEEN", "7")
	t.Setenv("EXCHANGE_DAY_WINDOW_HOURS", "11")
	t.Setenv("EXCHANGE_DAY_MIN_HOURS_SEEN", "13")

	got := loadExchangeConfig()
	var recent, day exchange.HorizonConfig
	for _, horizon := range got.Horizons {
		switch horizon.Horizon {
		case exchange.HorizonRecent:
			recent = horizon
		case exchange.HorizonDay:
			day = horizon
		}
	}
	if recent.WindowHours != 9 || recent.MinHoursSeen != 7 {
		t.Fatalf("recent horizon = %+v, want window 9 and min hours 7", recent)
	}
	if day.WindowHours != 11 || day.MinHoursSeen != 13 {
		t.Fatalf("day horizon = %+v, want window 11 and min hours 13", day)
	}
}

func TestLoadExchangeConfig_minEdgePreservesNegativeAndRejectsZero(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("EXCHANGE_MIN_EDGE", "-0.25")
	if got := loadExchangeConfig().MinEdge; got != -0.25 {
		t.Fatalf("negative MinEdge = %v, want -0.25", got)
	}

	t.Setenv("EXCHANGE_MIN_EDGE", "0")
	if got, want := loadExchangeConfig().MinEdge, exchange.DefaultConfig().MinEdge; got != want {
		t.Fatalf("zero MinEdge = %v, want default %v", got, want)
	}
}

func TestLoadTradeConfig_disabledKeepsTradeOff(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("TRADE_ENABLED", "false")
	t.Setenv("TRADE_CACHE_MAX", "0")

	got := loadTradeConfig("Mirage", loadTradeCacheMax())
	if got.Enabled {
		t.Fatal("disabled trade config has Enabled=true")
	}
	if got.CacheMaxEntries != 200 {
		t.Fatalf("disabled trade CacheMaxEntries = %d, want default 200", got.CacheMaxEntries)
	}
	if got.LeagueName != "Mirage" {
		t.Fatalf("disabled trade LeagueName = %q, want Mirage", got.LeagueName)
	}
}

func TestLoadTradeConfig_enabledUsesExistingDefaults(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("TRADE_ENABLED", "true")

	got := loadTradeConfig("Mirage", loadTradeCacheMax())
	if !got.Enabled || got.LeagueName != "Mirage" {
		t.Fatalf("trade config identity = enabled %v, league %q; want true, Mirage", got.Enabled, got.LeagueName)
	}
	if got.CeilingFactor != 0.65 || got.LatencyPadding != time.Second || got.DefaultSearchRate != 1 || got.DefaultFetchRate != 1 || got.MaxQueueWait != 30*time.Second || got.CacheMaxEntries != 200 || got.UserAgent != "profitofexile/0.1.0" || got.SyncWaitBudget != 500*time.Millisecond {
		t.Fatalf("trade defaults = %+v, want existing startup defaults", got)
	}
}

func TestLoadTradeConfig_validOverridesPopulateTheirFields(t *testing.T) {
	tests := []struct {
		name   string
		key    string
		value  string
		assert func(t *testing.T, got trade.TradeConfig)
	}{
		{
			name:  "cache maximum",
			key:   "TRADE_CACHE_MAX",
			value: "37",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.CacheMaxEntries != 37 {
					t.Errorf("CacheMaxEntries = %d, want 37", got.CacheMaxEntries)
				}
			},
		},
		{
			name:  "ceiling",
			key:   "TRADE_CEILING",
			value: "0.73",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.CeilingFactor != 0.73 {
					t.Errorf("CeilingFactor = %v, want 0.73", got.CeilingFactor)
				}
			},
		},
		{
			name:  "latency padding",
			key:   "TRADE_LATENCY_PAD",
			value: "250ms",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.LatencyPadding != 250*time.Millisecond {
					t.Errorf("LatencyPadding = %v, want 250ms", got.LatencyPadding)
				}
			},
		},
		{
			name:  "maximum queue wait",
			key:   "TRADE_MAX_WAIT",
			value: "7s",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.MaxQueueWait != 7*time.Second {
					t.Errorf("MaxQueueWait = %v, want 7s", got.MaxQueueWait)
				}
			},
		},
		{
			name:  "sync wait",
			key:   "TRADE_SYNC_WAIT",
			value: "125ms",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.SyncWaitBudget != 125*time.Millisecond {
					t.Errorf("SyncWaitBudget = %v, want 125ms", got.SyncWaitBudget)
				}
			},
		},
		{
			name:  "user agent",
			key:   "TRADE_USER_AGENT",
			value: "fixture-user-agent",
			assert: func(t *testing.T, got trade.TradeConfig) {
				if got.UserAgent != "fixture-user-agent" {
					t.Errorf("UserAgent = %q, want fixture-user-agent", got.UserAgent)
				}
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			clearServerConfigEnv(t)
			t.Setenv("TRADE_ENABLED", "true")
			t.Setenv(tt.key, tt.value)

			tt.assert(t, loadTradeConfig("Mirage", loadTradeCacheMax()))
		})
	}
}

func TestLoadTradeConfig_ceilingOutsideUnitIntervalKeepsDefault(t *testing.T) {
	for _, value := range []string{"0", "1.01", "-0.1"} {
		t.Run(value, func(t *testing.T) {
			clearServerConfigEnv(t)
			t.Setenv("TRADE_ENABLED", "true")
			t.Setenv("TRADE_CEILING", value)

			got := loadTradeConfig("Mirage", loadTradeCacheMax())
			if got.CeilingFactor != 0.65 {
				t.Fatalf("CeilingFactor = %v for %q, want default 0.65", got.CeilingFactor, value)
			}
		})
	}
}

func TestLoadTradeConfig_optionSpecificZeroNegativeHandling(t *testing.T) {
	clearServerConfigEnv(t)
	t.Setenv("TRADE_ENABLED", "true")
	t.Setenv("TRADE_CEILING", "0")
	t.Setenv("TRADE_LATENCY_PAD", "0s")
	t.Setenv("TRADE_MAX_WAIT", "-1s")
	t.Setenv("TRADE_SYNC_WAIT", "invalid")
	t.Setenv("TRADE_CACHE_MAX", "-1")

	got := loadTradeConfig("Mirage", loadTradeCacheMax())
	if got.CeilingFactor != 0.65 {
		t.Fatalf("zero ceiling = %v, want default 0.65", got.CeilingFactor)
	}
	if got.LatencyPadding != 0 {
		t.Fatalf("zero latency padding = %v, want accepted zero", got.LatencyPadding)
	}
	if got.MaxQueueWait != -time.Second {
		t.Fatalf("negative max wait = %v, want accepted -1s", got.MaxQueueWait)
	}
	if got.SyncWaitBudget != 500*time.Millisecond {
		t.Fatalf("invalid sync wait = %v, want default 500ms", got.SyncWaitBudget)
	}
	if got.CacheMaxEntries != 200 {
		t.Fatalf("negative cache max = %d, want default 200", got.CacheMaxEntries)
	}
}
