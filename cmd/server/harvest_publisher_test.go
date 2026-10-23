package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/harvest"
	"profitofexile/internal/league"
)

type harvestUpdateRows struct {
	hour time.Time
}

func (r *harvestUpdateRows) NewestHour(context.Context, league.Scope) (time.Time, bool, error) {
	return r.hour, true, nil
}

func (r *harvestUpdateRows) LoadRows(context.Context, league.Scope, time.Time, time.Time) ([]exchange.StoredRow, error) {
	return nil, nil
}

func TestHarvestUpdates_recomputeBurstPublishesNewestStoredHour(t *testing.T) {
	ctx := context.Background()
	cache := harvest.NewCache()
	first := time.Date(2026, 10, 23, 10, 0, 0, 0, time.UTC)
	latest := first.Add(time.Hour)
	type update struct {
		Topic          string `json:"topic"`
		PriceHour      string `json:"priceHour"`
		League         string `json:"league"`
		LeagueRevision *int64 `json:"leagueRevision"`
	}
	delivered := make(chan update, 2)
	hub := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			t.Errorf("parse publish form: %v", err)
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		if got := r.Form.Get("topic"); got != "poe/harvest/updated" {
			t.Errorf("published topic = %q, want poe/harvest/updated", got)
		}
		var event update
		if err := json.Unmarshal([]byte(r.Form.Get("data")), &event); err != nil {
			t.Errorf("decode publish payload: %v", err)
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		for _, h := range []exchange.Horizon{exchange.HorizonRecent, exchange.HorizonDay} {
			snapshot, warm := cache.Snapshot(h)
			if !warm || !snapshot.PriceHour.Equal(latest) {
				t.Errorf("%s snapshot at publish = {warm %v, hour %v}, want warm at %v", h, warm, snapshot.PriceHour, latest)
			}
		}
		delivered <- event
		w.WriteHeader(http.StatusOK)
	}))
	defer hub.Close()
	publisher := marketUpdatePublisher{
		scope:         publisherScope,
		mercureURL:    hub.URL,
		mercureSecret: "test-harvest-publisher-secret",
	}
	debounce := newHarvestUpdateDebouncer(ctx, cache, publisher)
	defer debounce.Stop()
	rows := &harvestUpdateRows{hour: first}
	service := harvest.NewService(rows, publisherScope, cache, debounce.Signal, nil)
	if err := service.Recompute(ctx); err != nil {
		t.Fatalf("first Recompute: %v", err)
	}
	rows.hour = latest
	if err := service.Recompute(ctx); err != nil {
		t.Fatalf("latest Recompute: %v", err)
	}

	select {
	case event := <-delivered:
		if event.Topic != "poe/harvest/updated" || event.PriceHour != "2026-10-23T11:00:00Z" || event.League != "Mirage" || event.LeagueRevision == nil || *event.LeagueRevision != 0 {
			t.Errorf("published event = %+v, want Harvest update for Mirage revision 0 at 2026-10-23T11:00:00Z", event)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("no Harvest update published after the recompute burst")
	}
	select {
	case event := <-delivered:
		t.Errorf("extra update for one recompute burst: %+v", event)
	default:
	}
}
