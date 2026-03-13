package collector

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestSignMercureJWT_producesValidHS256Token(t *testing.T) {
	token, err := signMercureJWT("test-secret")
	if err != nil {
		t.Fatalf("signMercureJWT: %v", err)
	}

	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		t.Fatalf("JWT should have 3 parts, got %d", len(parts))
	}

	// Verify header is HS256.
	headerJSON, err := base64.RawURLEncoding.DecodeString(parts[0])
	if err != nil {
		t.Fatalf("decode header: %v", err)
	}
	var header map[string]string
	if err := json.Unmarshal(headerJSON, &header); err != nil {
		t.Fatalf("unmarshal header: %v", err)
	}
	if header["alg"] != "HS256" {
		t.Errorf("alg = %q, want HS256", header["alg"])
	}

	// Verify payload contains mercure publish claim.
	payloadJSON, err := base64.RawURLEncoding.DecodeString(parts[1])
	if err != nil {
		t.Fatalf("decode payload: %v", err)
	}
	var payload map[string]any
	if err := json.Unmarshal(payloadJSON, &payload); err != nil {
		t.Fatalf("unmarshal payload: %v", err)
	}
	mercure, ok := payload["mercure"].(map[string]any)
	if !ok {
		t.Fatal("payload missing mercure claim")
	}
	publish, ok := mercure["publish"].([]any)
	if !ok || len(publish) == 0 {
		t.Fatal("mercure claim missing publish array")
	}
	if publish[0] != "*" {
		t.Errorf("publish[0] = %v, want *", publish[0])
	}
}

func TestPublishMercureEvent_emptyURLReturnsNil(t *testing.T) {
	// When mercureURL is empty, the function should return nil without making
	// any HTTP call, regardless of other parameters.
	err := PublishMercureEvent(context.Background(), "", "some-secret", "topic", "data")
	if err != nil {
		t.Errorf("expected nil error for empty URL, got: %v", err)
	}
}

func TestPublishMercureEvent_emptySecretReturnsNil(t *testing.T) {
	// When publisherSecret is empty, the function should return nil without
	// making any HTTP call, regardless of other parameters.
	err := PublishMercureEvent(context.Background(), "http://hub/.well-known/mercure", "", "topic", "data")
	if err != nil {
		t.Errorf("expected nil error for empty secret, got: %v", err)
	}
}

func TestPublishMercureEvent_bothEmptyReturnsNil(t *testing.T) {
	err := PublishMercureEvent(context.Background(), "", "", "topic", "data")
	if err != nil {
		t.Errorf("expected nil error when both URL and secret are empty, got: %v", err)
	}
}

func TestPublishMercureEvent_successfulPost(t *testing.T) {
	var (
		receivedContentType string
		receivedAuth        string
		receivedBody        string
	)

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		receivedContentType = r.Header.Get("Content-Type")
		receivedAuth = r.Header.Get("Authorization")
		body, _ := io.ReadAll(r.Body)
		receivedBody = string(body)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	err := PublishMercureEvent(context.Background(), server.URL, "test-signing-secret", "prices-updated", `{"ts":"now"}`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if receivedContentType != "application/x-www-form-urlencoded" {
		t.Errorf("Content-Type = %q, want %q", receivedContentType, "application/x-www-form-urlencoded")
	}

	// The Authorization header should contain a signed JWT (three dot-separated base64 segments),
	// not the raw secret.
	if !strings.HasPrefix(receivedAuth, "Bearer ") {
		t.Fatalf("Authorization header missing Bearer prefix: %q", receivedAuth)
	}
	token := strings.TrimPrefix(receivedAuth, "Bearer ")
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		t.Errorf("JWT should have 3 parts (header.payload.signature), got %d parts", len(parts))
	}
	if token == "test-signing-secret" {
		t.Error("Authorization should be a signed JWT, not the raw secret")
	}

	// Verify form body contains topic and data fields.
	if receivedBody == "" {
		t.Fatal("request body is empty")
	}
	if !strings.Contains(receivedBody, "topic=prices-updated") {
		t.Errorf("body = %q, want it to contain topic=prices-updated", receivedBody)
	}
	if !strings.Contains(receivedBody, "data=") {
		t.Errorf("body = %q, want it to contain data= field", receivedBody)
	}
}

func TestPublishMercureEvent_nonSuccessStatusReturnsError(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer server.Close()

	err := PublishMercureEvent(context.Background(), server.URL, "bad-token", "topic", "data")
	if err == nil {
		t.Fatal("expected error for 401 response, got nil")
	}
	if !strings.Contains(err.Error(), "401") {
		t.Errorf("error = %q, want it to mention status 401", err.Error())
	}
}

func TestPublishMercureEvent_networkErrorReturnsError(t *testing.T) {
	// Use a URL that will fail to connect.
	err := PublishMercureEvent(context.Background(), "http://127.0.0.1:1", "secret", "topic", "data")
	if err == nil {
		t.Fatal("expected error for unreachable URL, got nil")
	}
	if !strings.Contains(err.Error(), "mercure") {
		t.Errorf("error = %q, want it to mention mercure", err.Error())
	}
}
