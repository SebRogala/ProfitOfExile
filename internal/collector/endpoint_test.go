package collector

import (
	"testing"
	"time"
)

func TestDefaultNinjaConfig_sensibleDefaults(t *testing.T) {
	cfg := DefaultNinjaConfig()

	if cfg.Source != "ninja" {
		t.Errorf("Source = %q, want %q", cfg.Source, "ninja")
	}
	if cfg.MaxAge != 1800*time.Second {
		t.Errorf("MaxAge = %v, want %v", cfg.MaxAge, 1800*time.Second)
	}
	if cfg.FallbackInterval != 30*time.Minute {
		t.Errorf("FallbackInterval = %v, want %v", cfg.FallbackInterval, 30*time.Minute)
	}
	if cfg.MaxRetries != 5 {
		t.Errorf("MaxRetries = %d, want %d", cfg.MaxRetries, 5)
	}
	if cfg.MinSleep != 30*time.Second {
		t.Errorf("MinSleep = %v, want %v", cfg.MinSleep, 30*time.Second)
	}
	if cfg.JitterMin != 2*time.Second {
		t.Errorf("JitterMin = %v, want %v", cfg.JitterMin, 2*time.Second)
	}
	if cfg.JitterMax != 7*time.Second {
		t.Errorf("JitterMax = %v, want %v", cfg.JitterMax, 7*time.Second)
	}
}

func TestMergeEndpointConfig_overridesNonZeroFields(t *testing.T) {
	base := DefaultNinjaConfig()
	overrides := EndpointConfig{
		MaxAge:     900 * time.Second,
		MaxRetries: 10,
	}

	merged := MergeEndpointConfig(base, overrides)

	if merged.MaxAge != 900*time.Second {
		t.Errorf("MaxAge = %v, want %v (should be overridden)", merged.MaxAge, 900*time.Second)
	}
	if merged.MaxRetries != 10 {
		t.Errorf("MaxRetries = %d, want %d (should be overridden)", merged.MaxRetries, 10)
	}
	// Non-overridden fields should retain base values.
	if merged.FallbackInterval != 30*time.Minute {
		t.Errorf("FallbackInterval = %v, want %v (should retain base)", merged.FallbackInterval, 30*time.Minute)
	}
	if merged.MinSleep != 30*time.Second {
		t.Errorf("MinSleep = %v, want %v (should retain base)", merged.MinSleep, 30*time.Second)
	}
	if merged.Source != "ninja" {
		t.Errorf("Source = %q, want %q (should retain base)", merged.Source, "ninja")
	}
}

func TestMergeEndpointConfig_zeroOverridesPreserveBase(t *testing.T) {
	base := DefaultNinjaConfig()
	overrides := EndpointConfig{} // all zero values

	merged := MergeEndpointConfig(base, overrides)

	// Cannot compare structs with function fields directly; check each field.
	if merged.MaxAge != base.MaxAge {
		t.Errorf("MaxAge = %v, want %v", merged.MaxAge, base.MaxAge)
	}
	if merged.FallbackInterval != base.FallbackInterval {
		t.Errorf("FallbackInterval = %v, want %v", merged.FallbackInterval, base.FallbackInterval)
	}
	if merged.MaxRetries != base.MaxRetries {
		t.Errorf("MaxRetries = %d, want %d", merged.MaxRetries, base.MaxRetries)
	}
	if merged.MinSleep != base.MinSleep {
		t.Errorf("MinSleep = %v, want %v", merged.MinSleep, base.MinSleep)
	}
	if merged.JitterMin != base.JitterMin {
		t.Errorf("JitterMin = %v, want %v", merged.JitterMin, base.JitterMin)
	}
	if merged.JitterMax != base.JitterMax {
		t.Errorf("JitterMax = %v, want %v", merged.JitterMax, base.JitterMax)
	}
}

func TestParseEndpointOverrides_validEnvVars(t *testing.T) {
	t.Setenv("TEST_MAX_AGE", "900s")
	t.Setenv("TEST_FALLBACK_INTERVAL", "20m")
	t.Setenv("TEST_MAX_RETRIES", "3")
	t.Setenv("TEST_MIN_SLEEP", "15s")

	cfg := ParseEndpointOverrides("TEST")

	if cfg.MaxAge != 900*time.Second {
		t.Errorf("MaxAge = %v, want %v", cfg.MaxAge, 900*time.Second)
	}
	if cfg.FallbackInterval != 20*time.Minute {
		t.Errorf("FallbackInterval = %v, want %v", cfg.FallbackInterval, 20*time.Minute)
	}
	if cfg.MaxRetries != 3 {
		t.Errorf("MaxRetries = %d, want %d", cfg.MaxRetries, 3)
	}
	if cfg.MinSleep != 15*time.Second {
		t.Errorf("MinSleep = %v, want %v", cfg.MinSleep, 15*time.Second)
	}
}

func TestParseEndpointOverrides_missingEnvVarsReturnZero(t *testing.T) {
	// No env vars set — all fields should be zero.
	cfg := ParseEndpointOverrides("NONEXISTENT_PREFIX")

	if cfg.MaxAge != 0 {
		t.Errorf("MaxAge = %v, want 0", cfg.MaxAge)
	}
	if cfg.FallbackInterval != 0 {
		t.Errorf("FallbackInterval = %v, want 0", cfg.FallbackInterval)
	}
	if cfg.MaxRetries != 0 {
		t.Errorf("MaxRetries = %d, want 0", cfg.MaxRetries)
	}
	if cfg.MinSleep != 0 {
		t.Errorf("MinSleep = %v, want 0", cfg.MinSleep)
	}
}

func TestParseEndpointOverrides_invalidValuesIgnored(t *testing.T) {
	t.Setenv("BAD_MAX_AGE", "notaduration")
	t.Setenv("BAD_MAX_RETRIES", "abc")
	t.Setenv("BAD_MIN_SLEEP", "5x")

	cfg := ParseEndpointOverrides("BAD")

	if cfg.MaxAge != 0 {
		t.Errorf("MaxAge = %v, want 0 (invalid value should be ignored)", cfg.MaxAge)
	}
	if cfg.MaxRetries != 0 {
		t.Errorf("MaxRetries = %d, want 0 (invalid value should be ignored)", cfg.MaxRetries)
	}
	if cfg.MinSleep != 0 {
		t.Errorf("MinSleep = %v, want 0 (invalid value should be ignored)", cfg.MinSleep)
	}
}

func TestParseEndpointOverrides_partialEnvVars(t *testing.T) {
	// Only some env vars set — only those fields should be non-zero.
	t.Setenv("PARTIAL_MAX_AGE", "1200s")

	cfg := ParseEndpointOverrides("PARTIAL")

	if cfg.MaxAge != 1200*time.Second {
		t.Errorf("MaxAge = %v, want %v", cfg.MaxAge, 1200*time.Second)
	}
	if cfg.FallbackInterval != 0 {
		t.Errorf("FallbackInterval = %v, want 0 (not set)", cfg.FallbackInterval)
	}
}

func TestEndpointNameConstants(t *testing.T) {
	// Verify constants exist and have expected values.
	if EndpointNinjaGems != "ninja-gems" {
		t.Errorf("EndpointNinjaGems = %q, want %q", EndpointNinjaGems, "ninja-gems")
	}
	if EndpointNinjaCurrency != "ninja-currency" {
		t.Errorf("EndpointNinjaCurrency = %q, want %q", EndpointNinjaCurrency, "ninja-currency")
	}
}

func TestFetchResult_zeroValueIsNotModified(t *testing.T) {
	// A zero-value FetchResult should have NotModified=false.
	var r FetchResult
	if r.NotModified {
		t.Error("zero-value FetchResult should have NotModified=false")
	}
	if r.ETag != "" {
		t.Error("zero-value FetchResult should have empty ETag")
	}
	if r.Age != 0 {
		t.Error("zero-value FetchResult should have Age=0")
	}
}
