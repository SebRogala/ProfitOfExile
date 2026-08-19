package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/exchange"
)

// scarabID is a third feed id, so the fixture's plays are not all quoted in the
// same two currencies. It is also a real id the item asset resolves
// ("Domination Scarab of Evolution"), which its humanized form — "Domination 3"
// — would not have produced.
const scarabID = "Metadata/Items/Scarabs/ScarabDomination3"

// The window the fixture result covers: six hours ending on the 06:00 feed hour,
// so To is 07:00 and the newest feed hour (lastUpdated) is 06:00.
var (
	fixtureFrom     = time.Date(2026, 8, 19, 1, 0, 0, 0, time.UTC)
	fixtureTo       = time.Date(2026, 8, 19, 7, 0, 0, 0, time.UTC)
	fixtureLastHour = time.Date(2026, 8, 19, 6, 0, 0, 0, time.UTC)
)

// directDivinePlay is the fixture's headline flip: buy a divine for 196 chaos,
// sell it for 201.
func directDivinePlay() exchange.Play {
	return exchange.Play{
		Key:  "direct:" + exchange.ChaosID + "|" + exchange.DivineID,
		Mode: exchange.ModeDirect,
		Legs: []exchange.Leg{
			{Action: "buy", Item: exchange.DivineID, Quote: exchange.ChaosID, Price: 196, Volume: 65361, Stock: 8878},
			{Action: "sell", Item: exchange.DivineID, Quote: exchange.ChaosID, Price: 201, Volume: 65361, Stock: 8878},
		},
		Edge:      0.0255,
		Depth:     65361,
		HoursSeen: 6,
		LastHour:  fixtureLastHour,
	}
}

// directScarabPlay is the second direct play, quoted in divine.
func directScarabPlay() exchange.Play {
	return exchange.Play{
		Key:  "direct:" + exchange.DivineID + "|" + scarabID,
		Mode: exchange.ModeDirect,
		Legs: []exchange.Leg{
			{Action: "buy", Item: scarabID, Quote: exchange.DivineID, Price: 0.0625, Volume: 300, Stock: 60},
			{Action: "sell", Item: scarabID, Quote: exchange.DivineID, Price: 0.08, Volume: 300, Stock: 60},
		},
		Edge:      0.28,
		Depth:     300,
		HoursSeen: 4,
		LastHour:  fixtureLastHour,
	}
}

// oneHopPlay is the three-leg triangle: buy the scarab in divine, sell it in
// chaos, convert back.
func oneHopPlay() exchange.Play {
	return exchange.Play{
		Key:  "1-hop:" + scarabID + "|" + exchange.DivineID + "|" + exchange.ChaosID,
		Mode: exchange.ModeOneHop,
		Legs: []exchange.Leg{
			{Action: "buy", Item: scarabID, Quote: exchange.DivineID, Price: 0.0625, Volume: 300, Stock: 60},
			{Action: "sell", Item: scarabID, Quote: exchange.ChaosID, Price: 8, Volume: 250, Stock: 25},
			{Action: "sell", Item: exchange.DivineID, Quote: exchange.ChaosID, Price: 64, Volume: 65361, Stock: 8878},
		},
		Edge:      1,
		Depth:     250,
		HoursSeen: 3,
		LastHour:  fixtureLastHour,
	}
}

// warmExchangeCache holds a computed ranking of two direct plays and one 1-hop.
func warmExchangeCache(t *testing.T) *exchange.Cache {
	t.Helper()
	cache := exchange.NewCache()
	cache.Set(exchange.Result{
		League: "Mirage",
		From:   fixtureFrom,
		To:     fixtureTo,
		Hours:  6,
		Plays:  []exchange.Play{oneHopPlay(), directScarabPlay(), directDivinePlay()},
	})
	return cache
}

// exchangeLegBody mirrors the leg contract the desktop and web clients read:
// every engine field plus the two display names the transport layer adds.
type exchangeLegBody struct {
	Action    string  `json:"action"`
	Item      string  `json:"item"`
	Quote     string  `json:"quote"`
	Price     float64 `json:"price"`
	Volume    float64 `json:"volume"`
	Stock     int64   `json:"stock"`
	ItemName  string  `json:"itemName"`
	QuoteName string  `json:"quoteName"`
}

type exchangePlayBody struct {
	Key       string            `json:"key"`
	Mode      string            `json:"mode"`
	Edge      float64           `json:"edge"`
	Depth     float64           `json:"depth"`
	HoursSeen int               `json:"hoursSeen"`
	LastHour  time.Time         `json:"lastHour"`
	Legs      []exchangeLegBody `json:"legs"`
}

type exchangePlaysBody struct {
	League      string             `json:"league"`
	LastUpdated *string            `json:"lastUpdated"`
	From        *string            `json:"from"`
	To          *string            `json:"to"`
	Hours       int                `json:"hours"`
	Warm        bool               `json:"warm"`
	Mode        string             `json:"mode"`
	Count       int                `json:"count"`
	Plays       []exchangePlayBody `json:"plays"`
}

// getPlays serves one request through a chi router, the way the server mounts
// the handler, and returns the recorder.
func getPlays(t *testing.T, cache *exchange.Cache, target string) *httptest.ResponseRecorder {
	t.Helper()
	router := chi.NewRouter()
	router.Get("/api/currency-exchange/plays", CurrencyExchangePlays(cache))

	w := httptest.NewRecorder()
	router.ServeHTTP(w, httptest.NewRequest(http.MethodGet, target, nil))
	return w
}

// decodePlays decodes a 200 body, failing the test on any other status.
func decodePlays(t *testing.T, w *httptest.ResponseRecorder) exchangePlaysBody {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusOK, w.Body.String())
	}
	var body exchangePlaysBody
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	return body
}

// playByKey returns the single response play carrying key.
func playByKey(t *testing.T, body exchangePlaysBody, key string) exchangePlayBody {
	t.Helper()
	for _, play := range body.Plays {
		if play.Key == key {
			return play
		}
	}
	t.Fatalf("no play keyed %q in %d plays", key, len(body.Plays))
	return exchangePlayBody{}
}

func TestCurrencyExchangePlays_coldCache_answersWithNoPlaysInsteadOfFailing(t *testing.T) {
	// A nil cache (server started without the pillar) and a fresh one (recompute
	// has not finished) are the same answer: the route exists as soon as the
	// server does, and the client learns the answer is not ready yet rather than
	// getting a 503 it would have to special-case.
	tests := []struct {
		name  string
		cache *exchange.Cache
	}{
		{"nil cache", nil},
		{"cache before the first recompute", exchange.NewCache()},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			body := decodePlays(t, getPlays(t, tc.cache, "/api/currency-exchange/plays"))

			if body.Warm {
				t.Error("warm = true, want false — nothing has been computed yet")
			}
			if body.Count != 0 {
				t.Errorf("count = %d, want 0", body.Count)
			}
			if body.Hours != 0 {
				t.Errorf("hours = %d, want 0", body.Hours)
			}
			if body.Plays == nil {
				t.Error("plays = null, want [] — clients iterate the field unconditionally")
			}
			if len(body.Plays) != 0 {
				t.Errorf("got %d plays, want 0", len(body.Plays))
			}
		})
	}
}

func TestCurrencyExchangePlays_coldCache_rendersEveryTimestampAsNull(t *testing.T) {
	// "The feed has no hour yet" and "the feed's newest hour is year zero" must
	// not read the same, so the three timestamps are null rather than absent or
	// the zero time.
	w := getPlays(t, exchange.NewCache(), "/api/currency-exchange/plays")

	var raw map[string]json.RawMessage
	if err := json.NewDecoder(w.Body).Decode(&raw); err != nil {
		t.Fatalf("decode body: %v", err)
	}

	for _, key := range []string{"lastUpdated", "from", "to"} {
		value, present := raw[key]
		if !present {
			t.Errorf("body has no %q key, want it present and null", key)
			continue
		}
		if string(value) != "null" {
			t.Errorf("%s = %s, want null", key, value)
		}
	}
}

func TestCurrencyExchangePlays_warmCache_filtersByMode(t *testing.T) {
	cache := warmExchangeCache(t)

	tests := []struct {
		name      string
		target    string
		wantMode  string
		wantKeys  []string
		wantCount int
	}{
		{
			name:      "mode absent defaults to all",
			target:    "/api/currency-exchange/plays",
			wantMode:  "all",
			wantCount: 3,
		},
		{
			name:      "mode=all keeps every play",
			target:    "/api/currency-exchange/plays?mode=all",
			wantMode:  "all",
			wantCount: 3,
		},
		{
			name:      "mode=direct keeps the two flips",
			target:    "/api/currency-exchange/plays?mode=direct",
			wantMode:  "direct",
			wantKeys:  []string{directScarabPlay().Key, directDivinePlay().Key},
			wantCount: 2,
		},
		{
			name:      "mode=1-hop keeps the triangle",
			target:    "/api/currency-exchange/plays?mode=1-hop",
			wantMode:  "1-hop",
			wantKeys:  []string{oneHopPlay().Key},
			wantCount: 1,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			body := decodePlays(t, getPlays(t, cache, tc.target))

			if body.Mode != tc.wantMode {
				t.Errorf("mode = %q, want %q — the body echoes the applied filter", body.Mode, tc.wantMode)
			}
			if len(body.Plays) != tc.wantCount {
				t.Fatalf("got %d plays, want %d", len(body.Plays), tc.wantCount)
			}
			if body.Count != tc.wantCount {
				t.Errorf("count = %d, want %d — it counts what was returned, not what was cached",
					body.Count, tc.wantCount)
			}
			for _, key := range tc.wantKeys {
				playByKey(t, body, key)
			}
		})
	}
}

func TestCurrencyExchangePlays_warmCache_preservesTheRankedOrder(t *testing.T) {
	// The engine ranked the plays; the handler filters and must not reorder.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays"))

	want := []string{oneHopPlay().Key, directScarabPlay().Key, directDivinePlay().Key}
	if len(body.Plays) != len(want) {
		t.Fatalf("got %d plays, want %d", len(body.Plays), len(want))
	}
	for i, key := range want {
		if body.Plays[i].Key != key {
			t.Errorf("play %d = %q, want %q", i, body.Plays[i].Key, key)
		}
	}
}

func TestCurrencyExchangePlays_warmCache_reportsTheWindowItCovers(t *testing.T) {
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays"))

	if !body.Warm {
		t.Error("warm = false, want true")
	}
	if body.League != "Mirage" {
		t.Errorf("league = %q, want %q", body.League, "Mirage")
	}
	if body.Hours != 6 {
		t.Errorf("hours = %d, want 6", body.Hours)
	}
	timestamps := []struct {
		name string
		got  *string
		want string
	}{
		// lastUpdated is the newest feed hour — To minus one hour — not To and
		// not the recompute's wall clock.
		{"lastUpdated", body.LastUpdated, fixtureLastHour.Format(time.RFC3339)},
		{"from", body.From, fixtureFrom.Format(time.RFC3339)},
		{"to", body.To, fixtureTo.Format(time.RFC3339)},
	}
	for _, ts := range timestamps {
		if ts.got == nil {
			t.Errorf("%s = null, want %q", ts.name, ts.want)
			continue
		}
		if *ts.got != ts.want {
			t.Errorf("%s = %q, want %q", ts.name, *ts.got, ts.want)
		}
	}
}

func TestCurrencyExchangePlays_legsCarryDisplayNamesBesideTheRawFeedIDs(t *testing.T) {
	// The engine deliberately carries raw ids; resolving them to in-game names is
	// the transport layer's addition, and the raw ids must survive it — the
	// client keys on them.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?mode=1-hop"))

	play := playByKey(t, body, oneHopPlay().Key)
	want := []exchangeLegBody{
		{
			Action: "buy", Item: scarabID, Quote: exchange.DivineID,
			Price: 0.0625, Volume: 300, Stock: 60,
			ItemName: "Domination Scarab of Evolution", QuoteName: "Divine Orb",
		},
		{
			Action: "sell", Item: scarabID, Quote: exchange.ChaosID,
			Price: 8, Volume: 250, Stock: 25,
			ItemName: "Domination Scarab of Evolution", QuoteName: "Chaos Orb",
		},
		{
			Action: "sell", Item: exchange.DivineID, Quote: exchange.ChaosID,
			Price: 64, Volume: 65361, Stock: 8878,
			ItemName: "Divine Orb", QuoteName: "Chaos Orb",
		},
	}
	if len(play.Legs) != len(want) {
		t.Fatalf("got %d legs, want %d", len(play.Legs), len(want))
	}
	for i, wantLeg := range want {
		if play.Legs[i] != wantLeg {
			t.Errorf("leg %d = %+v, want %+v", i, play.Legs[i], wantLeg)
		}
	}
}

func TestCurrencyExchangePlays_playKeepsEveryEngineField(t *testing.T) {
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?mode=direct"))

	want := directDivinePlay()
	got := playByKey(t, body, want.Key)

	if got.Mode != string(want.Mode) {
		t.Errorf("mode = %q, want %q", got.Mode, want.Mode)
	}
	if got.Edge != want.Edge {
		t.Errorf("edge = %v, want %v", got.Edge, want.Edge)
	}
	if got.Depth != want.Depth {
		t.Errorf("depth = %v, want %v", got.Depth, want.Depth)
	}
	if got.HoursSeen != want.HoursSeen {
		t.Errorf("hoursSeen = %d, want %d", got.HoursSeen, want.HoursSeen)
	}
	if !got.LastHour.Equal(want.LastHour) {
		t.Errorf("lastHour = %s, want %s", got.LastHour, want.LastHour)
	}
	if len(got.Legs) != len(want.Legs) {
		t.Errorf("got %d legs, want %d", len(got.Legs), len(want.Legs))
	}
}

func TestCurrencyExchangePlays_unknownMode_isRejectedInsteadOfSilentlyReturningEverything(t *testing.T) {
	// A typo that quietly returned every play would look like a working filter.
	w := getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?mode=2-hop")

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusBadRequest, w.Body.String())
	}

	var body map[string]any
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode body: %v", err)
	}
	if want := "mode must be one of all, direct, 1-hop"; body["error"] != want {
		t.Errorf("error = %v, want %q", body["error"], want)
	}
	if _, leaked := body["plays"]; leaked {
		t.Errorf("rejected request still carried plays: %v", body["plays"])
	}
}

func TestCurrencyExchangePlays_isNeverCached(t *testing.T) {
	// The body changes whenever a feed hour lands, and clients are told about
	// that over the update topic rather than by expiry.
	w := getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays")

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", w.Code, http.StatusOK)
	}
	if got := w.Header().Get("Cache-Control"); got != "no-store" {
		t.Errorf("Cache-Control = %q, want %q", got, "no-store")
	}
	if got := w.Header().Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q, want %q", got, "application/json")
	}
}
