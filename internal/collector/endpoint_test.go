package collector

import (
	"context"
	"strings"
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

func TestParseEndpointOverrides_numericWithoutUnitIgnored(t *testing.T) {
	// time.ParseDuration requires a unit suffix — "1800" alone is invalid.
	t.Setenv("NOUNIT_MAX_AGE", "1800")
	t.Setenv("NOUNIT_FALLBACK_INTERVAL", "900")
	t.Setenv("NOUNIT_MIN_SLEEP", "30")

	cfg := ParseEndpointOverrides("NOUNIT")

	if cfg.MaxAge != 0 {
		t.Errorf("MaxAge = %v, want 0 (numeric without unit suffix should be ignored)", cfg.MaxAge)
	}
	if cfg.FallbackInterval != 0 {
		t.Errorf("FallbackInterval = %v, want 0 (numeric without unit suffix should be ignored)", cfg.FallbackInterval)
	}
	if cfg.MinSleep != 0 {
		t.Errorf("MinSleep = %v, want 0 (numeric without unit suffix should be ignored)", cfg.MinSleep)
	}
}

func TestParseEndpointOverrides_ninjaIntervalAliasNotHandled(t *testing.T) {
	// The NINJA_INTERVAL deprecated alias is handled in cmd/collector/main.go,
	// not in ParseEndpointOverrides. Setting PREFIX_INTERVAL should have no
	// effect on parsed config — the alias wiring is caller responsibility.
	t.Setenv("ALIAS_INTERVAL", "20m")

	cfg := ParseEndpointOverrides("ALIAS")

	// FallbackInterval should be zero because _INTERVAL is not a recognized
	// suffix — only _FALLBACK_INTERVAL is parsed.
	if cfg.FallbackInterval != 0 {
		t.Errorf("FallbackInterval = %v, want 0 (_INTERVAL is not parsed, only _FALLBACK_INTERVAL)", cfg.FallbackInterval)
	}
}

func TestNinjaIntervalAlias_mergePreservesBase(t *testing.T) {
	// When no env var overrides are set, MergeEndpointConfig retains the base
	// FallbackInterval. This verifies the starting condition that the alias
	// pattern in main.go relies on: if overrides.FallbackInterval == 0, the
	// base default (30m) is active after merge, and the alias is free to
	// override it.
	overrides := ParseEndpointOverrides("LEGACY_NOMATCH_XYZ")

	if overrides.FallbackInterval != 0 {
		t.Fatalf("precondition: FallbackInterval = %v, want 0", overrides.FallbackInterval)
	}

	base := DefaultNinjaConfig()
	merged := MergeEndpointConfig(base, overrides)

	// Base default is preserved when no override is set.
	if merged.FallbackInterval != 30*time.Minute {
		t.Errorf("FallbackInterval = %v, want %v (base default should be preserved when override is zero)", merged.FallbackInterval, 30*time.Minute)
	}
}

func TestNinjaIntervalAlias_fallbackIntervalTakesPrecedence(t *testing.T) {
	// When FALLBACK_INTERVAL is explicitly set, the legacy INTERVAL alias
	// should NOT override it. This mirrors the main.go guard:
	// if ninjaOverrides.FallbackInterval == 0 && ninjaIntervalStr != "" { ... }
	t.Setenv("PRIO_FALLBACK_INTERVAL", "25m")

	overrides := ParseEndpointOverrides("PRIO")

	if overrides.FallbackInterval != 25*time.Minute {
		t.Fatalf("precondition: FallbackInterval = %v, want %v", overrides.FallbackInterval, 25*time.Minute)
	}

	base := DefaultNinjaConfig()
	merged := MergeEndpointConfig(base, overrides)

	// Simulate the alias guard: only apply if overrides.FallbackInterval == 0.
	legacyInterval := 20 * time.Minute
	if overrides.FallbackInterval == 0 {
		merged.FallbackInterval = legacyInterval
	}

	// FALLBACK_INTERVAL wins over the legacy alias.
	if merged.FallbackInterval != 25*time.Minute {
		t.Errorf("FallbackInterval = %v, want %v (explicit FALLBACK_INTERVAL should take precedence over alias)", merged.FallbackInterval, 25*time.Minute)
	}
}

func TestMergeEndpointConfig_jitterOverrides(t *testing.T) {
	base := DefaultNinjaConfig()
	overrides := EndpointConfig{
		JitterMin: 5 * time.Second,
		JitterMax: 15 * time.Second,
	}

	merged := MergeEndpointConfig(base, overrides)

	if merged.JitterMin != 5*time.Second {
		t.Errorf("JitterMin = %v, want %v (should be overridden)", merged.JitterMin, 5*time.Second)
	}
	if merged.JitterMax != 15*time.Second {
		t.Errorf("JitterMax = %v, want %v (should be overridden)", merged.JitterMax, 15*time.Second)
	}
	// Other fields should retain base values.
	if merged.MaxAge != 1800*time.Second {
		t.Errorf("MaxAge = %v, want %v (should retain base)", merged.MaxAge, 1800*time.Second)
	}
}

func TestDefaultNinjaConfig_functionalFieldsAreNil(t *testing.T) {
	// DefaultNinjaConfig only sets scalar configuration. Callers must
	// provide FetchFunc, StoreFunc, and StalenessFunc for their endpoint.
	cfg := DefaultNinjaConfig()

	if cfg.FetchFunc != nil {
		t.Error("FetchFunc should be nil — caller must set it")
	}
	if cfg.StoreFunc != nil {
		t.Error("StoreFunc should be nil — caller must set it")
	}
	if cfg.StalenessFunc != nil {
		t.Error("StalenessFunc should be nil — caller must set it")
	}
	if cfg.Name != "" {
		t.Errorf("Name = %q, want empty — caller must set it", cfg.Name)
	}
}

func TestParseEndpointOverrides_zeroMaxRetriesIgnored(t *testing.T) {
	// parsePositiveInt rejects n <= 0, so MAX_RETRIES=0 should be ignored.
	t.Setenv("ZERO_MAX_RETRIES", "0")

	cfg := ParseEndpointOverrides("ZERO")

	if cfg.MaxRetries != 0 {
		t.Errorf("MaxRetries = %d, want 0 (zero should be rejected by parsePositiveInt)", cfg.MaxRetries)
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

func TestFetchResult_Validate(t *testing.T) {
	tests := []struct {
		name    string
		result  FetchResult
		wantErr bool
		errMsg  string
	}{
		{
			name:    "valid gem data only",
			result:  FetchResult{GemData: []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}}},
			wantErr: false,
		},
		{
			name:    "valid currency data only",
			result:  FetchResult{CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}}},
			wantErr: false,
		},
		{
			name:    "valid empty result (no data, not modified false)",
			result:  FetchResult{},
			wantErr: false,
		},
		{
			name: "NotModified with GemData populated returns error",
			result: FetchResult{
				NotModified: true,
				GemData:     []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
			},
			wantErr: true,
			errMsg:  "NotModified=true but data slices are populated",
		},
		{
			name: "NotModified with CurrencyData populated returns error",
			result: FetchResult{
				NotModified:  true,
				CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}},
			},
			wantErr: true,
			errMsg:  "NotModified=true but data slices are populated",
		},
		{
			name: "both GemData and CurrencyData populated returns error",
			result: FetchResult{
				GemData:      []GemSnapshot{{Name: "Arc", Variant: "default", Chaos: 10}},
				CurrencyData: []CurrencySnapshot{{CurrencyID: "divine", Chaos: 210}},
			},
			wantErr: true,
			errMsg:  "both GemData and CurrencyData are populated",
		},
		{
			name:    "NotModified with no data is valid",
			result:  FetchResult{NotModified: true},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.result.Validate()
			if tt.wantErr {
				if err == nil {
					t.Fatal("expected error, got nil")
				}
				if !strings.Contains(err.Error(), tt.errMsg) {
					t.Errorf("error = %q, want it to contain %q", err.Error(), tt.errMsg)
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error: %v", err)
				}
			}
		})
	}
}

func TestEndpointConfig_Validate_minSleepExceedsFallbackInterval(t *testing.T) {
	ep := EndpointConfig{
		Name:             "test",
		FetchFunc:        func(ctx context.Context, league string, etag string) (*FetchResult, error) { return nil, nil },
		FallbackInterval: 30 * time.Second,
		MinSleep:         60 * time.Second,
	}

	err := ep.Validate()
	if err == nil {
		t.Fatal("expected error when MinSleep exceeds FallbackInterval, got nil")
	}
	if !strings.Contains(err.Error(), "MinSleep") {
		t.Errorf("error = %q, want it to contain %q", err.Error(), "MinSleep")
	}
	if !strings.Contains(err.Error(), "exceeds FallbackInterval") {
		t.Errorf("error = %q, want it to contain %q", err.Error(), "exceeds FallbackInterval")
	}
}

func TestParseEndpointOverrides_negativeDurationIgnored(t *testing.T) {
	// parsePositiveDuration explicitly rejects d <= 0, so valid-but-negative
	// durations like "-5s" should be ignored, leaving the field at zero.
	t.Setenv("NEG_MAX_AGE", "-5s")
	t.Setenv("NEG_FALLBACK_INTERVAL", "-30m")
	t.Setenv("NEG_MIN_SLEEP", "-1s")

	cfg := ParseEndpointOverrides("NEG")

	if cfg.MaxAge != 0 {
		t.Errorf("MaxAge = %v, want 0 (negative duration should be ignored)", cfg.MaxAge)
	}
	if cfg.FallbackInterval != 0 {
		t.Errorf("FallbackInterval = %v, want 0 (negative duration should be ignored)", cfg.FallbackInterval)
	}
	if cfg.MinSleep != 0 {
		t.Errorf("MinSleep = %v, want 0 (negative duration should be ignored)", cfg.MinSleep)
	}
}
