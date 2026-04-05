package device

import "testing"

func TestDevice_IsIdentified(t *testing.T) {
	tests := []struct {
		name  string
		alias *string
		want  bool
	}{
		{
			name:  "nil alias is not identified",
			alias: nil,
			want:  false,
		},
		{
			name:  "empty alias is not identified",
			alias: strPtr(""),
			want:  false,
		},
		{
			name:  "non-empty alias is identified",
			alias: strPtr("Seb's PC"),
			want:  true,
		},
		{
			name:  "whitespace-only alias is identified",
			alias: strPtr("  "),
			want:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			d := &Device{Alias: tt.alias}
			if got := d.IsIdentified(); got != tt.want {
				t.Errorf("IsIdentified() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestNilIfEmpty(t *testing.T) {
	tests := []struct {
		name    string
		input   string
		wantNil bool
	}{
		{"empty string returns nil", "", true},
		{"non-empty string returns pointer", "hello", false},
		{"single char returns pointer", "x", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := nilIfEmpty(tt.input)
			if tt.wantNil {
				if got != nil {
					t.Errorf("nilIfEmpty(%q) = %v, want nil", tt.input, *got)
				}
			} else {
				if got == nil {
					t.Fatalf("nilIfEmpty(%q) = nil, want %q", tt.input, tt.input)
				}
				if *got != tt.input {
					t.Errorf("nilIfEmpty(%q) = %q, want %q", tt.input, *got, tt.input)
				}
			}
		})
	}
}

func TestErrAmbiguousPrefix_ErrorMessage(t *testing.T) {
	if ErrAmbiguousPrefix.Error() == "" {
		t.Error("ErrAmbiguousPrefix should have a non-empty error message")
	}
}

func TestErrNotFound_ErrorMessage(t *testing.T) {
	if ErrNotFound.Error() == "" {
		t.Error("ErrNotFound should have a non-empty error message")
	}
}

func strPtr(s string) *string {
	return &s
}
