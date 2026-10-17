package device

import (
	"reflect"
	"testing"
)

func TestEntitlements(t *testing.T) {
	tests := []struct {
		name         string
		role         string
		wantChannel  string
		wantFeatures []string
	}{
		{
			name:         "beta gets beta channel and every hidden feature",
			role:         "beta",
			wantChannel:  "beta",
			wantFeatures: []string{"merc", "exchange", "temple", "beta"},
		},
		{
			name:         "editor gets beta channel and every hidden feature",
			role:         "editor",
			wantChannel:  "beta",
			wantFeatures: []string{"merc", "exchange", "temple", "beta"},
		},
		{
			name:         "admin gets beta channel and every hidden feature",
			role:         "admin",
			wantChannel:  "beta",
			wantFeatures: []string{"merc", "exchange", "temple", "beta"},
		},
		{
			name:         "user gets stable channel and only the temple feature",
			role:         "user",
			wantChannel:  "stable",
			wantFeatures: []string{"temple"},
		},
		{
			name:         "empty role gets stable channel and only the temple feature",
			role:         "",
			wantChannel:  "stable",
			wantFeatures: []string{"temple"},
		},
		{
			name:         "unknown role gets stable channel and only the temple feature",
			role:         "beta-tester",
			wantChannel:  "stable",
			wantFeatures: []string{"temple"},
		},
		{
			name:         "case-mismatched Editor gets stable channel and only the temple feature",
			role:         "Editor",
			wantChannel:  "stable",
			wantFeatures: []string{"temple"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			channel, features := Entitlements(tt.role)

			if channel != tt.wantChannel {
				t.Errorf("channel = %q, want %q", channel, tt.wantChannel)
			}
			// Exact lists also reject nil and any unexpected feature.
			if !reflect.DeepEqual(features, tt.wantFeatures) {
				t.Errorf("features = %#v, want %#v", features, tt.wantFeatures)
			}
		})
	}
}

// An entitled caller must not be able to mutate the list a later caller gets.
func TestEntitlements_CallerCannotMutateSharedFeatureList(t *testing.T) {
	_, first := Entitlements(RoleEditor)
	first[0] = "tampered"

	_, second := Entitlements(RoleEditor)

	if second[0] != FeatureMerc {
		t.Errorf("second call features[0] = %q, want %q", second[0], FeatureMerc)
	}
}

func TestEntitlements_StableCallerCannotMutateSharedFeatureList(t *testing.T) {
	_, first := Entitlements(RoleUser)
	first[0] = "tampered"

	_, second := Entitlements(RoleUser)

	if !reflect.DeepEqual(second, []string{FeatureTemple}) {
		t.Errorf("second call features = %#v, want %#v", second, []string{FeatureTemple})
	}
}
