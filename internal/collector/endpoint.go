package collector

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"time"
)

// Canonical endpoint name constants. Used by main.go config and scheduler's
// postCollect gate to identify which endpoints have completed.
const (
	EndpointNinjaGems     = "ninja-gems"
	EndpointNinjaCurrency = "ninja-currency"
)

// FetchResult holds the outcome of a single endpoint fetch. We use concrete
// typed fields instead of generics because Go generics cannot be used with
// function-typed struct fields in EndpointConfig.FetchFunc without making
// EndpointConfig itself generic, which would prevent mixing different endpoint
// types in a single []EndpointConfig slice. The StoreFunc closure on
// EndpointConfig reads the appropriate field — no type assertions needed.
type FetchResult struct {
	GemData      []GemSnapshot
	CurrencyData []CurrencySnapshot
	ETag         string
	Age          int  // seconds since origin server generated the response
	NotModified  bool // true when source returned 304 Not Modified
}

// FetchFunc fetches data from an external source. The etag parameter is the
// ETag from the previous successful fetch (empty on first call). Returning
// a FetchResult with NotModified=true signals that the source data hasn't
// changed since the last fetch.
type FetchFunc func(ctx context.Context, league string, etag string) (*FetchResult, error)

// StoreFunc persists snapshot data from a FetchResult. It receives the full
// result and the snapshot timestamp; the closure reads the appropriate typed
// field (GemData or CurrencyData). Returns the number of rows inserted.
type StoreFunc func(ctx context.Context, snapTime time.Time, result *FetchResult) (int, error)

// StalenessFunc returns the time of the last stored snapshot for an endpoint.
// Used at startup to decide whether to fetch immediately or wait. A nil
// StalenessFunc means the endpoint always fetches on startup.
type StalenessFunc func(ctx context.Context) (time.Time, error)

// EndpointConfig defines the configuration and behavior for a single data
// collection endpoint. Each endpoint runs in its own goroutine with
// independent cache-aware sleep calculation.
type EndpointConfig struct {
	// Name is a human-readable identifier for logging and metrics.
	Name string

	// Source is the semaphore key (e.g. "ninja"). Endpoints sharing a source
	// share the same rate-limit semaphore to avoid hammering the API.
	Source string

	// FetchFunc fetches data from the external source.
	FetchFunc FetchFunc

	// StoreFunc persists the fetched data.
	StoreFunc StoreFunc

	// StalenessFunc returns the last snapshot time for startup check.
	// Nil means always fetch on startup.
	StalenessFunc StalenessFunc

	// MaxAge is the expected cache duration of the source data (e.g. 1800s
	// for poe.ninja). Used with Age to calculate optimal sleep time.
	MaxAge time.Duration

	// FallbackInterval is the polling interval when no cache headers are
	// available. Also used as the maximum sleep cap.
	FallbackInterval time.Duration

	// MaxRetries is the maximum number of consecutive 304 Not Modified
	// responses before falling back to FallbackInterval sleep.
	MaxRetries int

	// MinSleep is the minimum time between fetches, even when cache math
	// suggests a shorter interval. Prevents tight polling loops when Age
	// exceeds MaxAge (stale-while-revalidate).
	MinSleep time.Duration

	// JitterMin is the minimum random jitter added to sleep duration.
	JitterMin time.Duration

	// JitterMax is the maximum random jitter added to sleep duration.
	JitterMax time.Duration
}

// DefaultNinjaConfig returns an EndpointConfig with sensible defaults for
// poe.ninja endpoints. Callers must still set Name, FetchFunc, StoreFunc,
// and StalenessFunc for their specific endpoint type.
func DefaultNinjaConfig() EndpointConfig {
	return EndpointConfig{
		Source:           "ninja",
		MaxAge:           1800 * time.Second,
		FallbackInterval: 30 * time.Minute,
		MaxRetries:       5,
		MinSleep:         30 * time.Second,
		JitterMin:        2 * time.Second,
		JitterMax:        7 * time.Second,
	}
}

// ParseEndpointOverrides reads environment variables by prefix and returns an
// EndpointConfig with any overrides applied. Fields that are not set or have
// invalid values are left at their zero value (caller should merge with
// defaults). Invalid values log a warning and are skipped.
//
// Supported env vars (for prefix "NINJA"):
//
//	NINJA_MAX_AGE           — e.g. "1800s", "30m"
//	NINJA_FALLBACK_INTERVAL — e.g. "30m"
//	NINJA_MAX_RETRIES       — e.g. "5"
//	NINJA_MIN_SLEEP         — e.g. "30s"
func ParseEndpointOverrides(prefix string) EndpointConfig {
	var cfg EndpointConfig

	if v := os.Getenv(prefix + "_MAX_AGE"); v != "" {
		d, err := time.ParseDuration(v)
		if err != nil {
			slog.Warn("invalid env var, ignoring",
				"var", prefix+"_MAX_AGE",
				"value", v,
				"error", err,
			)
		} else {
			cfg.MaxAge = d
		}
	}

	if v := os.Getenv(prefix + "_FALLBACK_INTERVAL"); v != "" {
		d, err := time.ParseDuration(v)
		if err != nil {
			slog.Warn("invalid env var, ignoring",
				"var", prefix+"_FALLBACK_INTERVAL",
				"value", v,
				"error", err,
			)
		} else {
			cfg.FallbackInterval = d
		}
	}

	if v := os.Getenv(prefix + "_MAX_RETRIES"); v != "" {
		var n int
		_, err := parsePositiveInt(v)
		if err != nil {
			slog.Warn("invalid env var, ignoring",
				"var", prefix+"_MAX_RETRIES",
				"value", v,
				"error", err,
			)
		} else {
			n, _ = parsePositiveInt(v)
			cfg.MaxRetries = n
		}
	}

	if v := os.Getenv(prefix + "_MIN_SLEEP"); v != "" {
		d, err := time.ParseDuration(v)
		if err != nil {
			slog.Warn("invalid env var, ignoring",
				"var", prefix+"_MIN_SLEEP",
				"value", v,
				"error", err,
			)
		} else {
			cfg.MinSleep = d
		}
	}

	return cfg
}

// MergeEndpointConfig applies non-zero overrides on top of a base config.
// Zero-valued fields in overrides are ignored, preserving the base defaults.
func MergeEndpointConfig(base, overrides EndpointConfig) EndpointConfig {
	if overrides.MaxAge > 0 {
		base.MaxAge = overrides.MaxAge
	}
	if overrides.FallbackInterval > 0 {
		base.FallbackInterval = overrides.FallbackInterval
	}
	if overrides.MaxRetries > 0 {
		base.MaxRetries = overrides.MaxRetries
	}
	if overrides.MinSleep > 0 {
		base.MinSleep = overrides.MinSleep
	}
	if overrides.JitterMin > 0 {
		base.JitterMin = overrides.JitterMin
	}
	if overrides.JitterMax > 0 {
		base.JitterMax = overrides.JitterMax
	}
	return base
}

// parsePositiveInt parses a string as a positive integer.
func parsePositiveInt(s string) (int, error) {
	var n int
	for _, c := range s {
		if c < '0' || c > '9' {
			return 0, fmt.Errorf("not a positive integer: %q", s)
		}
		n = n*10 + int(c-'0')
	}
	if n <= 0 {
		return 0, fmt.Errorf("not a positive integer: %q", s)
	}
	return n, nil
}
