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
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/league"
)

// publisherScope is the league every stamp assertion reads back. Historical
// needs no database, and its revision is zero by contract.
var publisherScope = league.Historical("Mirage")

// updateEventNow is the wall clock the payload is built on, fixed so the
// timestamp field is an exact expectation.
var updateEventNow = time.Date(2026, 8, 19, 14, 37, 5, 0, time.UTC)

// updateResult is a recomputed ranking covering six hours ending on the 06:00
// feed hour, so lastUpdated is 06:00 (To − 1h).
func updateResult() exchange.Result {
	return exchange.Result{
		League: publisherScope.ID(),
		From:   time.Date(2026, 8, 19, 1, 0, 0, 0, time.UTC),
		To:     time.Date(2026, 8, 19, 7, 0, 0, 0, time.UTC),
		Hours:  6,
		Plays:  []exchange.Play{{Key: "direct:a"}, {Key: "direct:b"}, {Key: "1-hop:c"}},
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

func TestExchangeUpdatePublisher_hubNotConfigured_succeedsWithoutPublishing(t *testing.T) {
	// The recompute runs whether or not a hub is configured, and its debounced
	// notify has nowhere to report a failure: an unconfigured publisher must
	// report success rather than logging a warning after every recompute burst.
	publisher := exchangeUpdatePublisher{scope: publisherScope}

	err := publisher.Publish(context.Background(), exchange.UpdatedTopic,
		exchange.UpdatePayload(updateResult(), updateEventNow))

	if err != nil {
		t.Fatalf("Publish with no hub configured: %v", err)
	}
}

func TestExchangeUpdatePublisher_emptySecret_doesNotReachTheHub(t *testing.T) {
	// A hub URL with no publishing secret cannot produce a signed request; the
	// adapter must skip the POST instead of sending an unauthorized one.
	hub := newHubRecorder(t)
	publisher := exchangeUpdatePublisher{scope: publisherScope, mercureURL: hub.server.URL}

	if err := publisher.Publish(context.Background(), exchange.UpdatedTopic,
		exchange.UpdatePayload(updateResult(), updateEventNow)); err != nil {
		t.Fatalf("Publish with an empty secret: %v", err)
	}

	if posted := hub.posted(); len(posted) != 0 {
		t.Errorf("hub received %d requests, want 0", len(posted))
	}
}

func TestExchangeUpdatePublisher_configuredHub_postsOnTheTopicItWasGiven(t *testing.T) {
	hub := newHubRecorder(t)
	publisher := exchangeUpdatePublisher{
		scope:         publisherScope,
		mercureURL:    hub.server.URL,
		mercureSecret: "test-secret",
	}

	if err := publisher.Publish(context.Background(), exchange.UpdatedTopic,
		exchange.UpdatePayload(updateResult(), updateEventNow)); err != nil {
		t.Fatalf("Publish: %v", err)
	}

	posted := hub.posted()
	if len(posted) != 1 {
		t.Fatalf("hub received %d requests, want 1", len(posted))
	}
	// The topic is the argument, not a literal: the ingest topic and the
	// "answer changed" topic go through this same adapter.
	if got := posted[0].Get("topic"); got != exchange.UpdatedTopic {
		t.Errorf("form topic = %q, want %q", got, exchange.UpdatedTopic)
	}
}

func TestExchangeUpdatePublisher_configuredHub_stampsTheLeagueIdentity(t *testing.T) {
	// The server's own LeagueEventGuard drops any event lacking league and
	// leagueRevision, so an unstamped update would be invisible to every
	// subscriber that checks.
	hub := newHubRecorder(t)
	publisher := exchangeUpdatePublisher{
		scope:         publisherScope,
		mercureURL:    hub.server.URL,
		mercureSecret: "test-secret",
	}

	if err := publisher.Publish(context.Background(), exchange.UpdatedTopic,
		exchange.UpdatePayload(updateResult(), updateEventNow)); err != nil {
		t.Fatalf("Publish: %v", err)
	}

	got := postedData(t, hub)
	if want := publisherScope.ID(); got["league"] != want {
		t.Errorf("data league = %v, want %q", got["league"], want)
	}
	revision, ok := got["leagueRevision"]
	if !ok {
		t.Fatalf("data carries no leagueRevision field (got: %v)", got)
	}
	if revision != float64(publisherScope.Revision()) {
		t.Errorf("data leagueRevision = %v, want %d", revision, publisherScope.Revision())
	}
}

func TestExchangeUpdatePublisher_configuredHub_carriesTheUpdatePayloadUntouched(t *testing.T) {
	hub := newHubRecorder(t)
	publisher := exchangeUpdatePublisher{
		scope:         publisherScope,
		mercureURL:    hub.server.URL,
		mercureSecret: "test-secret",
	}

	if err := publisher.Publish(context.Background(), exchange.UpdatedTopic,
		exchange.UpdatePayload(updateResult(), updateEventNow)); err != nil {
		t.Fatalf("Publish: %v", err)
	}

	got := postedData(t, hub)
	strings := map[string]string{
		"topic":       exchange.UpdatedTopic,
		"league":      publisherScope.ID(),
		"lastUpdated": "2026-08-19T06:00:00Z",
		"timestamp":   "2026-08-19T14:37:05Z",
	}
	for key, want := range strings {
		if got[key] != want {
			t.Errorf("data %s = %v, want %q", key, got[key], want)
		}
	}
	numbers := map[string]float64{
		"hours": 6,
		"plays": 3, // the count, not the plays: the event is a notification
	}
	for key, want := range numbers {
		if got[key] != want {
			t.Errorf("data %s = %v, want %v", key, got[key], want)
		}
	}
}

// postedData returns the decoded `data` field of the single posted form.
func postedData(t *testing.T, hub *hubRecorder) map[string]any {
	t.Helper()
	posted := hub.posted()
	if len(posted) != 1 {
		t.Fatalf("hub received %d requests, want 1", len(posted))
	}
	var data map[string]any
	if err := json.Unmarshal([]byte(posted[0].Get("data")), &data); err != nil {
		t.Fatalf("unmarshal the posted data field: %v", err)
	}
	return data
}
