package collector

import (
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

// Tests for the forwarding wrapper. The core JWT signing and publish logic
// is tested in internal/mercure/publisher_test.go.

func TestPublishMercureEvent_emptyURLReturnsNil(t *testing.T) {
	err := PublishMercureEvent(context.Background(), "", "some-secret", "topic", "data")
	if err != nil {
		t.Errorf("expected nil error for empty URL, got: %v", err)
	}
}

func TestPublishMercureEvent_emptySecretReturnsNil(t *testing.T) {
	err := PublishMercureEvent(context.Background(), "http://hub/.well-known/mercure", "", "topic", "data")
	if err != nil {
		t.Errorf("expected nil error for empty secret, got: %v", err)
	}
}

func TestPublishMercureEvent_forwardsToMercurePackage(t *testing.T) {
	var receivedBody string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		receivedBody = string(body)
		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	err := PublishMercureEvent(context.Background(), server.URL, "test-secret", "test-topic", `{"test":true}`)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !strings.Contains(receivedBody, "topic=test-topic") {
		t.Errorf("body = %q, want it to contain topic=test-topic", receivedBody)
	}
}
