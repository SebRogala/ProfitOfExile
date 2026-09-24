package main

import (
	"fmt"
	"log/slog"
	"os"
	"strconv"
	"strings"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/server"
	"profitofexile/internal/trade"
)

// sharedServiceConfig contains the process-wide service settings consumed by
// the router, Mercure publisher/subscriber, and exchange services.
type sharedServiceConfig struct {
	MercureURL           string
	MercureSecret        string
	DevMode              bool
	MercureSubscriberKey string
	MercurePublicURL     string
	AllowedOrigins       []string
	IconCacheDir         string
}

// loadPort reads and validates the HTTP listen port. The caller owns the
// fatal startup response so invalid PORT keeps the existing error timing.
func loadPort() (string, error) {
	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	p, err := strconv.Atoi(port)
	if err != nil || p < 1 || p > 65535 {
		return port, fmt.Errorf("PORT must be a number between 1 and 65535, got %q", port)
	}
	return port, nil
}

func loadSharedServiceConfig() sharedServiceConfig {
	cfg := sharedServiceConfig{
		MercureURL:           os.Getenv("MERCURE_URL"),
		MercureSecret:        os.Getenv("MERCURE_JWT_SECRET"),
		DevMode:              os.Getenv("APP_ENV") == "dev",
		MercureSubscriberKey: os.Getenv("MERCURE_SUBSCRIBER_KEY"),
		MercurePublicURL:     os.Getenv("MERCURE_PUBLIC_URL"),
		AllowedOrigins:       corsOrigins(),
		IconCacheDir:         getEnvDefault("ICON_CACHE_DIR", server.DefaultIconCacheDir),
	}

	if cfg.MercureURL != "" && cfg.MercureSecret == "" {
		slog.Warn("MERCURE_URL is set but MERCURE_JWT_SECRET is empty — publish operations will be skipped")
	}
	return cfg
}

func loadExchangeConfig() exchange.Config {
	// Ranking knobs are overridable per deploy. Unlike the TRADE_* fallbacks,
	// an unusable value here is logged loudly rather than swallowed: a typo in
	// a threshold silently changes which plays users are shown, with no other
	// symptom to notice it by.
	//
	// Since POE-191 the four quality gates below (turnover, tick, edge/tick,
	// chaos payout) ship OFF and the desktop applies them client-side, so
	// setting one here re-arms it for everyone and can only tighten what the
	// server serves — envPositiveFloat rejects a zero or negative value, which
	// is also the only way back to "off" (unset the variable).
	cfg := exchange.DefaultConfig()
	// WindowHours and MinHoursSeen are per horizon (below), not on the base
	// config: the base values would be overwritten by every horizon overlay, so
	// an override here would read as configured and do nothing.
	cfg.MinVolumePerHour = envPositiveFloat("EXCHANGE_MIN_VOLUME_PER_HOUR", cfg.MinVolumePerHour)
	cfg.MaxPlays = envPositiveInt("EXCHANGE_MAX_PLAYS", cfg.MaxPlays)
	cfg.MinTurnoverChaos = envPositiveFloat("EXCHANGE_MIN_TURNOVER_CHAOS", cfg.MinTurnoverChaos)
	cfg.MaxTick = envPositiveFloat("EXCHANGE_MAX_TICK", cfg.MaxTick)
	cfg.MinEdgeTickRatio = envPositiveFloat("EXCHANGE_MIN_EDGE_TICK_RATIO", cfg.MinEdgeTickRatio)
	cfg.MinROIChaos = envPositiveFloat("EXCHANGE_MIN_ROI_CHAOS", cfg.MinROIChaos)
	// The junk bands are fractions of an hour's VWAP, so both are positive and
	// the low one is meant to be under 1 while the high one is over it. Nothing
	// enforces that ordering here: a deploy that wants to widen or narrow either
	// side of "believable" is allowed to, and inverting them would flag every
	// leg rather than none, which is loud enough to notice.
	cfg.SuspectLowBand = envPositiveFloat("EXCHANGE_SUSPECT_LOW_BAND", cfg.SuspectLowBand)
	cfg.SuspectHighBand = envPositiveFloat("EXCHANGE_SUSPECT_HIGH_BAND", cfg.SuspectHighBand)
	// HideSuspect turns the flag into a filter. Default false: a flagged row can
	// be argued with, a missing one cannot.
	cfg.HideSuspect = envBool("EXCHANGE_HIDE_SUSPECT", cfg.HideSuspect)
	// MinEdge is where the engine FLAGS a play as having no spread worth taking
	// (Play.LowLiquidity), not a floor it drops below — since 2026-08-22 raising
	// this marks more rows and hides none, and an operator who wants rows GONE
	// arms EXCHANGE_MIN_EDGE_TICK_RATIO or EXCHANGE_MIN_ROI_CHAOS instead. It is
	// the one knob where a negative value is meaningful (it stops the small gains
	// from being marked), so only an exact 0 is rejected — the engine reads 0 as
	// "unset" and would restore the default behind the log line, making the
	// configured value a lie.
	cfg.MinEdge = envMinEdge("EXCHANGE_MIN_EDGE", cfg.MinEdge)

	// Both served horizons are recomputed from one read; each has its own span
	// and its own persistence demand. EXCHANGE_WINDOW_HOURS and
	// EXCHANGE_MIN_HOURS_SEEN predate the split and still work: they set the
	// RECENT horizon, which is the one an unqualified request gets, and the
	// EXCHANGE_RECENT_* names override them for anyone who wants to say so.
	horizons := append([]exchange.HorizonConfig(nil), cfg.Horizons...)
	for i, horizon := range horizons {
		switch horizon.Horizon {
		case exchange.HorizonRecent:
			horizon.WindowHours = envPositiveInt("EXCHANGE_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_MIN_HOURS_SEEN", horizon.MinHoursSeen)
			horizon.WindowHours = envPositiveInt("EXCHANGE_RECENT_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_RECENT_MIN_HOURS_SEEN", horizon.MinHoursSeen)
		case exchange.HorizonDay:
			horizon.WindowHours = envPositiveInt("EXCHANGE_DAY_WINDOW_HOURS", horizon.WindowHours)
			horizon.MinHoursSeen = envPositiveInt("EXCHANGE_DAY_MIN_HOURS_SEEN", horizon.MinHoursSeen)
		}
		horizons[i] = horizon
	}
	cfg.Horizons = horizons
	return cfg
}

func envMinEdge(key string, def float64) float64 {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	f, err := strconv.ParseFloat(v, 64)
	switch {
	case err != nil:
		slog.Warn("ignoring unparseable environment override; keeping the default",
			"var", key, "value", v, "default", def)
	case f == 0:
		slog.Warn("EXCHANGE_MIN_EDGE=0 reads as unset by the engine; keeping the default (pass a small negative value to stop flagging small gains)",
			"default", def)
	default:
		return f
	}
	return def
}

func loadTradeCacheMax() int {
	// The cache cap is read before the DB warm-up so the cache has its capacity
	// when the warm results are loaded. Invalid and non-positive values retain
	// the existing silent fallback.
	cacheMax := 200
	if v := os.Getenv("TRADE_CACHE_MAX"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			cacheMax = n
		}
	}
	return cacheMax
}

func loadTradeConfig(leagueName string, cacheMax int) trade.TradeConfig {
	cfg := trade.TradeConfig{
		LeagueName:      leagueName,
		CacheMaxEntries: cacheMax,
	}
	if os.Getenv("TRADE_ENABLED") != "true" {
		return cfg
	}

	// Trade is opt-in. Once enabled, these defaults and override rules are the
	// existing gate settings: malformed ceiling and duration values are silent
	// fallbacks, while parsed zero/negative durations remain accepted.
	cfg.Enabled = true
	cfg.CeilingFactor = 0.65
	if v := os.Getenv("TRADE_CEILING"); v != "" {
		if f, err := strconv.ParseFloat(v, 64); err == nil && f > 0 && f <= 1 {
			cfg.CeilingFactor = f
		}
	}

	cfg.LatencyPadding = time.Second
	if v := os.Getenv("TRADE_LATENCY_PAD"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.LatencyPadding = d
		}
	}

	cfg.DefaultSearchRate = 1
	cfg.DefaultFetchRate = 1
	cfg.MaxQueueWait = 30 * time.Second
	if v := os.Getenv("TRADE_MAX_WAIT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.MaxQueueWait = d
		}
	}

	cfg.UserAgent = getEnvDefault("TRADE_USER_AGENT", "profitofexile/0.1.0")
	cfg.SyncWaitBudget = 500 * time.Millisecond
	if v := os.Getenv("TRADE_SYNC_WAIT"); v != "" {
		if d, err := time.ParseDuration(v); err == nil {
			cfg.SyncWaitBudget = d
		}
	}
	return cfg
}

// corsOrigins returns allowed CORS origins from the CORS_ORIGINS env var.
// Comma-separated list, e.g. "http://localhost:1420,tauri://localhost".
// Returns nil (no CORS) when unset.
func corsOrigins() []string {
	raw := os.Getenv("CORS_ORIGINS")
	if raw == "" {
		return nil
	}
	var origins []string
	for _, o := range strings.Split(raw, ",") {
		if o = strings.TrimSpace(o); o != "" {
			origins = append(origins, o)
		}
	}
	return origins
}

// envPositiveInt reads key as a positive integer, falling back to def when the
// variable is unset. An unparseable or non-positive value logs a Warn naming the
// variable and keeps def: the engine treats a non-positive count as "unset" and
// would restore the default anyway, so the log is the only way an operator finds
// out their override never took effect.
func envPositiveInt(key string, def int) int {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	n, err := strconv.Atoi(v)
	if err != nil || n <= 0 {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return n
}

// envPositiveFloat is envPositiveInt for a float knob, with the same
// keep-the-default-and-say-so contract.
func envPositiveFloat(key string, def float64) float64 {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	f, err := strconv.ParseFloat(v, 64)
	if err != nil || f <= 0 {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return f
}

// envBool is envPositiveInt for a switch, with the same
// keep-the-default-and-say-so contract. There is no "unset" value to inherit
// from — a bool that parses replaces the default outright — so an empty
// variable and an unparseable one are the only ways to keep it.
func envBool(key string, def bool) bool {
	v := os.Getenv(key)
	if v == "" {
		return def
	}
	b, err := strconv.ParseBool(v)
	if err != nil {
		slog.Warn("ignoring invalid environment override; keeping the default",
			"var", key, "value", v, "default", def)
		return def
	}
	return b
}

func getEnvDefault(key, fallback string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return fallback
}
