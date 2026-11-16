package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"profitofexile/internal/league"
	"testing"
	"time"
)

func TestRunTradeRefresher_PublishesScopedCadence(t *testing.T) {
	payloads := make(chan string, 2)
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Error(err)
		}
		if got := r.Form.Get("topic"); got != "poe/collector/trade-tick" {
			t.Errorf("topic %q", got)
		}
		payloads <- r.Form.Get("data")
	}))
	defer upstream.Close()
	ticks := make(chan time.Time)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() {
		defer close(done)
		runTradeRefresherTicks(ctx, league.Historical("Test"), upstream.URL, "test-secret", ticks)
	}()
	defer func() { cancel(); <-done }()
	for range 2 {
		ticks <- time.Now()
		select {
		case raw := <-payloads:
			var p map[string]any
			if err := json.Unmarshal([]byte(raw), &p); err != nil {
				t.Fatal(err)
			}
			if len(p) != 3 || p["league"] != "Test" || p["leagueRevision"] != float64(0) || p["topic"] != "poe/collector/trade-tick" {
				t.Fatalf("cadence payload: %s", raw)
			}
		case <-time.After(time.Second):
			t.Fatal("no published cadence")
		}
	}
}
