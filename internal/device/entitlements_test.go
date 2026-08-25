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
			name:         "editor gets beta channel and the merc feature",
			role:         "editor",
			wantChannel:  "beta",
			wantFeatures: []string{"merc"},
		},
		{
			name:         "admin gets beta channel and the merc feature",
			role:         "admin",
			wantChannel:  "beta",
			wantFeatures: []string{"merc"},
		},
		{
			name:         "user gets stable channel and no features",
			role:         "user",
			wantChannel:  "stable",
			wantFeatures: []string{},
		},
		{
			name:         "empty role gets stable channel and no features",
			role:         "",
			wantChannel:  "stable",
			wantFeatures: []string{},
		},
		{
			name:         "unknown role gets stable channel and no features",
			role:         "beta-tester",
			wantChannel:  "stable",
			wantFeatures: []string{},
		},
		{
			name:         "role match is case-sensitive so Editor is not entitled",
			role:         "Editor",
			wantChannel:  "stable",
			wantFeatures: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			channel, features := Entitlements(tt.role)

			if channel != tt.wantChannel {
				t.Errorf("channel = %q, want %q", channel, tt.wantChannel)
			}
			// DeepEqual against []string{} also fails on a nil slice, which is
			// what the wire format depends on: the handler encodes this list
			// directly, and a nil would become `null` instead of `[]`.
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
