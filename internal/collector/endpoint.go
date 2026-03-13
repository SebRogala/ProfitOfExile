package collector

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"strconv"
	"time"
)

// Canonical endpoint name constants. Used to configure endpoints and to
// conditionally run endpoint-specific post-collect logic (e.g., gem color
// upsert).
const (
	EndpointNinjaGems     = "ninja-gems"
	EndpointNinjaCurrency = "ninja-currency"
)

// FetchResult holds the outcome of a single endpoint fetch. Concrete typed
// fields (GemData, CurrencyData) avoid the need for type assertions; the
// StoreFunc closure on EndpointConfig reads the appropriate field.
type FetchResult struct {
	GemData      []GemSnapshot
	CurrencyData []CurrencySnapshot
	ETag         string
	Age          int  // seconds since origin server generated the response
	NotModified  bool // true when source returned 304 Not Modified
}

// Validate checks FetchResult invariants: a NotModified result must have no
// data, and at most one of GemData/CurrencyData may be populated.
func (r *FetchResult) Validate() error {
	if r.NotModified && (len(r.GemData) > 0 || len(r.CurrencyData) > 0) {
		return fmt.Errorf("FetchResult: NotModified=true but data slices are populated")
	}
	if len(r.GemData) > 0 && len(r.CurrencyData) > 0 {
		return fmt.Errorf("FetchResult: both GemData and CurrencyData are populated")
	}
	return nil
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

	// MaxAge is the assumed cache duration of the source data. See
	// DefaultNinjaConfig for poe.ninja defaults. Used with Age to calculate
	// optimal sleep time.
	MaxAge time.Duration

	// FallbackInterval is the polling interval when no cache headers are
	// available. Also used as the maximum sleep cap.
	FallbackInterval time.Duration

	// MaxRetries is the number of consecutive 304 Not Modified responses
	// allowed before falling back to FallbackInterval sleep. The
	// (MaxRetries+1)th consecutive 304 triggers the fallback.
	MaxRetries int

	// MinSleep is the minimum time between fetches, even when cache math
	// suggests a shorter interval. Prevents tight polling loops when Age
	// exceeds MaxAge (stale-while-revalidate).
	MinSleep time.Duration

	// JitterMin is the minimum random startup delay before the first fetch.
	JitterMin time.Duration

	// JitterMax is the maximum random startup delay before the first fetch.
	JitterMax time.Duration
}

// Validate checks EndpointConfig invariants: Name must be non-empty, FetchFunc
// must be non-nil, FallbackInterval must be positive, and Source should be
// non-empty (logged as warning if empty since rate limiting will be skipped).
func (c EndpointConfig) Validate() error {
	if c.Name == "" {
		return fmt.Errorf("empty Name")
	}
	if c.FetchFunc == nil {
		return fmt.Errorf("endpoint %q: nil FetchFunc", c.Name)
	}
	if c.FallbackInterval <= 0 {
		return fmt.Errorf("endpoint %q: non-positive FallbackInterval", c.Name)
	}
	if c.Source == "" {
		slog.Warn("endpoint has empty Source, rate limiting will be skipped",
			"endpoint", c.Name,
		)
	}
	if c.MinSleep > 0 && c.FallbackInterval > 0 && c.MinSleep > c.FallbackInterval {
		return fmt.Errorf("endpoint %q: MinSleep (%s) exceeds FallbackInterval (%s)", c.Name, c.MinSleep, c.FallbackInterval)
	}
	return nil
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
// EndpointConfig with any overrides applied. Fields that are not set, have
// invalid values, or have zero/non-positive values are left at their zero
// value — the caller should merge with defaults via MergeEndpointConfig.
// Zero-valued fields in the returned config mean "not overridden", so it is
// not possible to intentionally override a field to zero.
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
		if d, err := parsePositiveDuration(prefix+"_MAX_AGE", v); err == nil {
			cfg.MaxAge = d
		}
	}

	if v := os.Getenv(prefix + "_FALLBACK_INTERVAL"); v != "" {
		if d, err := parsePositiveDuration(prefix+"_FALLBACK_INTERVAL", v); err == nil {
			cfg.FallbackInterval = d
		}
	}

	if v := os.Getenv(prefix + "_MAX_RETRIES"); v != "" {
		n, err := parsePositiveInt(v)
		if err != nil {
			slog.Warn("invalid env var, ignoring",
				"var", prefix+"_MAX_RETRIES",
				"value", v,
				"error", err,
			)
		} else {
			cfg.MaxRetries = n
		}
	}

	if v := os.Getenv(prefix + "_MIN_SLEEP"); v != "" {
		if d, err := parsePositiveDuration(prefix+"_MIN_SLEEP", v); err == nil {
			cfg.MinSleep = d
		}
	}

	return cfg
}

// MergeEndpointConfig applies non-zero overrides on top of a base config.
// Only timing and numeric fields are merged; identity fields (Name, Source) and
// function fields (FetchFunc, StoreFunc, StalenessFunc) are never overridden.
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

// parsePositiveInt parses a string as a positive integer (> 0).
func parsePositiveInt(s string) (int, error) {
	n, err := strconv.Atoi(s)
	if err != nil || n <= 0 {
		return 0, fmt.Errorf("not a positive integer: %q", s)
	}
	return n, nil
}

// parsePositiveDuration parses a duration string and rejects non-positive values.
// Logs a warning and returns an error for invalid or non-positive durations.
func parsePositiveDuration(varName, value string) (time.Duration, error) {
	d, err := time.ParseDuration(value)
	if err != nil {
		slog.Warn("invalid env var, ignoring",
			"var", varName,
			"value", value,
			"error", err,
		)
		return 0, err
	}
	if d <= 0 {
		slog.Warn("non-positive duration env var, ignoring",
			"var", varName,
			"value", value,
		)
		return 0, fmt.Errorf("non-positive duration: %q", value)
	}
	return d, nil
}
