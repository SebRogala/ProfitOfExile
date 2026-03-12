package handlers

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/go-chi/chi/v5"
)

// mockPinger implements the Pinger interface for testing.
type mockPinger struct {
	PingFn func(ctx context.Context) error
}

func (m *mockPinger) Ping(ctx context.Context) error {
	return m.PingFn(ctx)
}

func TestHealth(t *testing.T) {
	healthyPinger := &mockPinger{
		PingFn: func(ctx context.Context) error { return nil },
	}
	failingPinger := &mockPinger{
		PingFn: func(ctx context.Context) error { return errors.New("connection refused") },
	}

	tests := []struct {
		name            string
		pinger          Pinger
		method          string
		wantStatus      int
		wantBody        *healthResponse
		wantContentType string
	}{
		{
			name:            "GET returns 200 with db ok when ping succeeds",
			pinger:          healthyPinger,
			method:          http.MethodGet,
			wantStatus:      http.StatusOK,
			wantBody:        &healthResponse{Status: "ok", Version: "dev", DB: "ok"},
			wantContentType: "application/json",
		},
		{
			name:            "GET returns 503 with db error when ping fails",
			pinger:          failingPinger,
			method:          http.MethodGet,
			wantStatus:      http.StatusServiceUnavailable,
			wantBody:        &healthResponse{Status: "degraded", Version: "dev", DB: "error"},
			wantContentType: "application/json",
		},
		{
			name: "GET returns 503 with db error when context is cancelled",
			pinger: &mockPinger{
				PingFn: func(ctx context.Context) error { return context.Canceled },
			},
			method:          http.MethodGet,
			wantStatus:      http.StatusServiceUnavailable,
			wantBody:        &healthResponse{Status: "degraded", Version: "dev", DB: "error"},
			wantContentType: "application/json",
		},
		{
			name:       "POST returns 405 Method Not Allowed",
			pinger:     NopPinger{},
			method:     http.MethodPost,
			wantStatus: http.StatusMethodNotAllowed,
		},
		{
			name:       "PUT returns 405 Method Not Allowed",
			pinger:     NopPinger{},
			method:     http.MethodPut,
			wantStatus: http.StatusMethodNotAllowed,
		},
		{
			name:       "DELETE returns 405 Method Not Allowed",
			pinger:     NopPinger{},
			method:     http.MethodDelete,
			wantStatus: http.StatusMethodNotAllowed,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			router := chi.NewRouter()
			router.Get("/api/health", Health(tt.pinger))

			req := httptest.NewRequest(tt.method, "/api/health", nil)
			w := httptest.NewRecorder()

			router.ServeHTTP(w, req)

			if w.Code != tt.wantStatus {
				t.Errorf("status = %d, want %d", w.Code, tt.wantStatus)
			}

			if tt.wantContentType != "" {
				got := w.Header().Get("Content-Type")
				if got != tt.wantContentType {
					t.Errorf("Content-Type = %q, want %q", got, tt.wantContentType)
				}
			}

			if tt.wantBody != nil {
				var got healthResponse
				if err := json.NewDecoder(w.Body).Decode(&got); err != nil {
					t.Fatalf("failed to decode response body: %v", err)
				}
				if got.Status != tt.wantBody.Status {
					t.Errorf("Status = %q, want %q", got.Status, tt.wantBody.Status)
				}
				if got.Version != tt.wantBody.Version {
					t.Errorf("Version = %q, want %q", got.Version, tt.wantBody.Version)
				}
				if got.DB != tt.wantBody.DB {
					t.Errorf("DB = %q, want %q", got.DB, tt.wantBody.DB)
				}
			}
		})
	}
}

func TestHealth_NilPingerPanics(t *testing.T) {
	defer func() {
		if r := recover(); r == nil {
			t.Error("expected panic when pinger is nil, but did not panic")
		}
	}()
	Health(nil)
}

func TestHealthResponseJSONFields(t *testing.T) {
	router := chi.NewRouter()
	router.Get("/api/health", Health(NopPinger{}))

	req := httptest.NewRequest(http.MethodGet, "/api/health", nil)
	w := httptest.NewRecorder()

	router.ServeHTTP(w, req)

	var raw map[string]interface{}
	if err := json.NewDecoder(w.Body).Decode(&raw); err != nil {
		t.Fatalf("failed to decode response body: %v", err)
	}

	requiredFields := []string{"status", "version", "db"}
	for _, field := range requiredFields {
		if _, ok := raw[field]; !ok {
			t.Errorf("response JSON missing required field %q", field)
		}
	}

	if len(raw) != len(requiredFields) {
		t.Errorf("response JSON has %d fields, want %d", len(raw), len(requiredFields))
	}
}
