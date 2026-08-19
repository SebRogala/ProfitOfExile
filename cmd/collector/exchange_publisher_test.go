package main

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"sync"
	"testing"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
)

// publisherScope is the league every stamp assertion reads back. Historical
// needs no database, and its revision is zero by contract.
var publisherScope = league.Historical("Mirage")

// exchangeEvent is one runner payload, built with the same keys exchange.Runner
// publishes so the adapter is exercised on the real shape.
func exchangeEvent() map[string]any {
	return map[string]any{
		"topic":      exchange.Topic,
		"endpoint":   exchange.EndpointName,
		"hour":       int64(1787119200),
		"nextCursor": int64(1787122800),
		"rows":       42,
		"timestamp":  "2026-08-19T14:00:00Z",
	}
}

// hubRecorder is a Mercure hub stand-in that keeps every posted form. The mutex
// guards the slice against the handler goroutine, which the race detector must
// not have to reason about through the socket.
type hubRecorder struct {
	server *httptest.Server
	mu     sync.Mutex
	forms  []url.Values
}

func (h *hubRecorder) posted() []url.Values {
	h.mu.Lock()
	defer h.mu.Unlock()
	return append([]url.Values(nil), h.forms...)
}

func newHubRecorder(t *testing.T) *hubRecorder {
	t.Helper()
	rec := &hubRecorder{}
	rec.server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, err := io.ReadAll(r.Body)
		if err != nil {
			t.Errorf("read hub request body: %v", err)
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		form, err := url.ParseQuery(string(body))
		if err != nil {
			t.Errorf("parse hub request form: %v", err)
			w.WriteHeader(http.StatusInternalServerError)
			return
		}
		rec.mu.Lock()
		rec.forms = append(rec.forms, form)
		rec.mu.Unlock()
		w.WriteHeader(http.StatusOK)
	}))
	t.Cleanup(rec.server.Close)
	return rec
}

func TestStampedPublisher_hubNotConfigured_succeedsWithoutPublishing(t *testing.T) {
	// The collector starts the ingest walk whether or not a hub is configured, so
	// an unconfigured publisher must report success rather than failing the walk
	// on every stored hour.
	publisher := stampedPublisher{scope: publisherScope, mercureURL: "", mercureSecret: ""}

	if err := publisher.Publish(context.Background(), exchange.Topic, exchangeEvent()); err != nil {
		t.Fatalf("Publish with no hub configured: %v", err)
	}
}

func TestStampedPublisher_emptySecret_doesNotReachTheHub(t *testing.T) {
	// A hub URL with no publishing secret cannot produce a signed request; the
	// adapter must skip the POST instead of sending an unauthorized one.
	hub := newHubRecorder(t)
	publisher := stampedPublisher{scope: publisherScope, mercureURL: hub.server.URL, mercureSecret: ""}

	if err := publisher.Publish(context.Background(), exchange.Topic, exchangeEvent()); err != nil {
		t.Fatalf("Publish with an empty secret: %v", err)
	}

	posted := hub.posted()
	if len(posted) != 0 {
		t.Errorf("hub received %d requests, want 0", len(posted))
	}
}

func TestStampedPublisher_configuredHub_postsTheStampedPayloadOnTheTopic(t *testing.T) {
	hub := newHubRecorder(t)
	publisher := stampedPublisher{
		scope:         publisherScope,
		mercureURL:    hub.server.URL,
		mercureSecret: "test-secret",
	}

	if err := publisher.Publish(context.Background(), exchange.Topic, exchangeEvent()); err != nil {
		t.Fatalf("Publish: %v", err)
	}

	posted := hub.posted()
	if len(posted) != 1 {
		t.Fatalf("hub received %d requests, want 1", len(posted))
	}
	form := posted[0]
	if got := form.Get("topic"); got != exchange.Topic {
		t.Errorf("form topic = %q, want %q", got, exchange.Topic)
	}

	var got map[string]any
	if err := json.Unmarshal([]byte(form.Get("data")), &got); err != nil {
		t.Fatalf("unmarshal the posted data field: %v", err)
	}

	// The stamp the server's league event guard reads back. Without both fields
	// every event is dropped on arrival.
	if want := publisherScope.ID(); got["league"] != want {
		t.Errorf("data league = %v, want %q", got["league"], want)
	}
	revision, ok := got["leagueRevision"]
	if !ok {
		t.Errorf("data carries no leagueRevision field (got: %v)", got)
	} else if revision != float64(publisherScope.Revision()) {
		t.Errorf("data leagueRevision = %v, want %d", revision, publisherScope.Revision())
	}

	// The runner's own fields survive the stamp untouched.
	strings := map[string]string{
		"topic":     exchange.Topic,
		"endpoint":  exchange.EndpointName,
		"timestamp": "2026-08-19T14:00:00Z",
	}
	for key, want := range strings {
		if got[key] != want {
			t.Errorf("data %s = %v, want %q", key, got[key], want)
		}
	}
	numbers := map[string]float64{
		"hour":       1787119200,
		"nextCursor": 1787122800,
		"rows":       42,
	}
	for key, want := range numbers {
		if got[key] != want {
			t.Errorf("data %s = %v, want %v", key, got[key], want)
		}
	}
}
