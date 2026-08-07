package server

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
	"time"
)

// TestMercureSubscriber_TopicFallbackToPayload locks in the POE-111 hotfix:
// dunglas/mercure SSE feeds carry only `id:` + `data:` lines (no `event:`),
// so dispatch logic that branches on `ev.Topic` would otherwise silently drop
// every event. The subscriber must fall back to a `topic` field embedded in
// the JSON payload when the SSE event field is absent.
func TestMercureSubscriber_TopicFallbackToPayload(t *testing.T) {
	tests := []struct {
		name       string
		ssePayload string
		wantTopic  string
		wantData   string
	}{
		{
			name:       "topic field in payload populates Topic",
			ssePayload: `id: urn:uuid:111` + "\n" + `data: {"topic":"poe/collector/trade-tick","minAge":"5m"}` + "\n\n",
			wantTopic:  "poe/collector/trade-tick",
			wantData:   `{"topic":"poe/collector/trade-tick","minAge":"5m"}`,
		},
		{
			name:       "no topic field leaves Topic empty",
			ssePayload: `id: urn:uuid:222` + "\n" + `data: {"endpoint":"ninja-gems"}` + "\n\n",
			wantTopic:  "",
			wantData:   `{"endpoint":"ninja-gems"}`,
		},
		{
			name:       "non-JSON data leaves Topic empty without panic",
			ssePayload: `id: urn:uuid:333` + "\n" + `data: not-json` + "\n\n",
			wantTopic:  "",
			wantData:   "not-json",
		},
		{
			name:       "explicit event line wins over payload topic",
			ssePayload: `id: urn:uuid:444` + "\n" + `event: poe/explicit` + "\n" + `data: {"topic":"poe/payload"}` + "\n\n",
			wantTopic:  "poe/explicit",
			wantData:   `{"topic":"poe/payload"}`,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			var (
				mu       sync.Mutex
				once     sync.Once
				received []MercureEvent
			)
			done := make(chan struct{})

			srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				flusher, ok := w.(http.Flusher)
				if !ok {
					t.Errorf("response writer does not support flushing")
					return
				}
				fmt.Fprint(w, tc.ssePayload)
				flusher.Flush()
				<-r.Context().Done()
			}))
			defer srv.Close()

			sub := NewMercureSubscriber(srv.URL, []string{"poe/collector/trade-tick"}, "", func(ev MercureEvent) {
				mu.Lock()
				received = append(received, ev)
				mu.Unlock()
				once.Do(func() { close(done) })
			})

			ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
			defer cancel()

			runDone := make(chan struct{})
			go func() {
				sub.Run(ctx)
				close(runDone)
			}()

			select {
			case <-done:
			case <-ctx.Done():
				t.Fatalf("handler not invoked within 2s; SSE parse may be broken")
			}

			cancel()
			<-runDone

			mu.Lock()
			defer mu.Unlock()
			if len(received) != 1 {
				t.Fatalf("expected exactly 1 event, got %d", len(received))
			}
			if received[0].Topic != tc.wantTopic {
				t.Errorf("Topic: got %q, want %q", received[0].Topic, tc.wantTopic)
			}
			if received[0].Data != tc.wantData {
				t.Errorf("Data: got %q, want %q", received[0].Data, tc.wantData)
			}
		})
	}
}
