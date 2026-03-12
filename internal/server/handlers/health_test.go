package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/go-chi/chi/v5"
)

func TestHealth(t *testing.T) {
	router := chi.NewRouter()
	router.Get("/api/health", Health(nil))

	tests := []struct {
		name           string
		method         string
		wantStatus     int
		wantBody       *healthResponse
		wantContentType string
	}{
		{
			name:           "GET returns 200 with db unavailable when pool is nil",
			method:         http.MethodGet,
			wantStatus:     http.StatusOK,
			wantBody:       &healthResponse{Status: "ok", Version: "dev", DB: "unavailable"},
			wantContentType: "application/json",
		},
		{
			name:       "POST returns 405 Method Not Allowed",
			method:     http.MethodPost,
			wantStatus: http.StatusMethodNotAllowed,
		},
		{
			name:       "PUT returns 405 Method Not Allowed",
			method:     http.MethodPut,
			wantStatus: http.StatusMethodNotAllowed,
		},
		{
			name:       "DELETE returns 405 Method Not Allowed",
			method:     http.MethodDelete,
			wantStatus: http.StatusMethodNotAllowed,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
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

func TestHealthResponseJSONFields(t *testing.T) {
	router := chi.NewRouter()
	router.Get("/api/health", Health(nil))

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
