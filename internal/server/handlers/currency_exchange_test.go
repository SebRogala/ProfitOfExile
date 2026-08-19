package handlers

import (
	"bytes"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
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
// every engine field plus the display names and icon paths the transport layer
// adds. The icons are pointers because the wire contract distinguishes a path to
// fetch from an explicit null for an item with no artwork.
type exchangeLegBody struct {
	Action    string  `json:"action"`
	Item      string  `json:"item"`
	Quote     string  `json:"quote"`
	Price     float64 `json:"price"`
	Volume    float64 `json:"volume"`
	Stock     int64   `json:"stock"`
	ItemName  string  `json:"itemName"`
	ItemIcon  *string `json:"itemIcon"`
	QuoteName string  `json:"quoteName"`
	QuoteIcon *string `json:"quoteIcon"`
}

// String renders the leg with its icon pointers dereferenced, so a failed
// comparison names the paths instead of two heap addresses.
func (l exchangeLegBody) String() string {
	return fmt.Sprintf("{action:%s item:%s quote:%s price:%v volume:%v stock:%d itemName:%q itemIcon:%s quoteName:%q quoteIcon:%s}",
		l.Action, l.Item, l.Quote, l.Price, l.Volume, l.Stock,
		l.ItemName, quoteOrNull(l.ItemIcon), l.QuoteName, quoteOrNull(l.QuoteIcon))
}

func quoteOrNull(value *string) string {
	if value == nil {
		return "null"
	}
	return `"` + *value + `"`
}

// iconPath is the API-relative path the handler is expected to emit for id.
// Spelled out rather than built with exchange.IconPath: a test that asks the
// production helper what to expect cannot notice the helper changing.
func iconPath(id string) *string {
	path := "/currency-exchange/icon/" + strings.ReplaceAll(id, "/", "%2F")
	return &path
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

func TestCurrencyExchangePlays_legsCarryDisplayNamesAndIconPathsBesideTheRawFeedIDs(t *testing.T) {
	// The engine deliberately carries raw ids; resolving them to in-game names and
	// icon paths is the transport layer's addition, and the raw ids must survive
	// it — the client keys on them. Every leg names both sides of the trade, so
	// the item AND the quote carry a name and an icon.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?mode=1-hop"))

	play := playByKey(t, body, oneHopPlay().Key)
	want := []exchangeLegBody{
		{
			Action: "buy", Item: scarabID, Quote: exchange.DivineID,
			Price: 0.0625, Volume: 300, Stock: 60,
			ItemName: "Domination Scarab of Evolution", ItemIcon: iconPath(scarabID),
			QuoteName: "Divine Orb", QuoteIcon: iconPath(exchange.DivineID),
		},
		{
			Action: "sell", Item: scarabID, Quote: exchange.ChaosID,
			Price: 8, Volume: 250, Stock: 25,
			ItemName: "Domination Scarab of Evolution", ItemIcon: iconPath(scarabID),
			QuoteName: "Chaos Orb", QuoteIcon: iconPath(exchange.ChaosID),
		},
		{
			Action: "sell", Item: exchange.DivineID, Quote: exchange.ChaosID,
			Price: 64, Volume: 65361, Stock: 8878,
			ItemName: "Divine Orb", ItemIcon: iconPath(exchange.DivineID),
			QuoteName: "Chaos Orb", QuoteIcon: iconPath(exchange.ChaosID),
		},
	}
	if len(play.Legs) != len(want) {
		t.Fatalf("got %d legs, want %d", len(play.Legs), len(want))
	}
	for i, wantLeg := range want {
		if !reflect.DeepEqual(play.Legs[i], wantLeg) {
			t.Errorf("leg %d = %v, want %v", i, play.Legs[i], wantLeg)
		}
	}
}

// unknownItemID is shaped like a feed id but is absent from the item asset — the
// item GGG adds mid-league, before the next regeneration.
const unknownItemID = "Metadata/Items/Currency/CurrencyNotInTheAssetYet"

// unknownHumanizedName is what Humanize makes of unknownItemID: the leading
// category word is dropped and the CamelCase tail is split.
const unknownHumanizedName = "Not In The Asset Yet"

// unknownItemCache holds one play whose single leg buys an item the asset does
// not cover, quoted in chaos.
func unknownItemCache(t *testing.T) *exchange.Cache {
	t.Helper()
	cache := exchange.NewCache()
	cache.Set(exchange.Result{
		League: "Mirage",
		From:   fixtureFrom,
		To:     fixtureTo,
		Hours:  6,
		Plays: []exchange.Play{{
			Key:  "direct:" + exchange.ChaosID + "|" + unknownItemID,
			Mode: exchange.ModeDirect,
			Legs: []exchange.Leg{
				{Action: "buy", Item: unknownItemID, Quote: exchange.ChaosID, Price: 3, Volume: 100, Stock: 20},
			},
			LastHour: fixtureLastHour,
		}},
	})
	return cache
}

// firstLegRaw returns the first leg of the first play as raw JSON fields, which
// is the only way to tell an absent key from one whose value is null.
func firstLegRaw(t *testing.T, w *httptest.ResponseRecorder) map[string]json.RawMessage {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusOK, w.Body.String())
	}
	var body struct {
		Plays []struct {
			Legs []map[string]json.RawMessage `json:"legs"`
		} `json:"plays"`
	}
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	if len(body.Plays) != 1 || len(body.Plays[0].Legs) != 1 {
		t.Fatalf("want exactly one play with one leg, got %+v", body)
	}
	return body.Plays[0].Legs[0]
}

func TestCurrencyExchangePlays_itemTheAssetDoesNotCover_rendersTheHumanizedNameNotTheRawID(t *testing.T) {
	// The client renders itemName verbatim, so an id with no asset entry still has
	// to arrive as words rather than as a metadata path.
	body := decodePlays(t, getPlays(t, unknownItemCache(t), "/api/currency-exchange/plays"))

	leg := body.Plays[0].Legs[0]
	if leg.ItemName != unknownHumanizedName {
		t.Errorf("itemName = %q, want the humanized fallback %q", leg.ItemName, unknownHumanizedName)
	}
	if leg.Item != unknownItemID {
		t.Errorf("item = %q, want the raw feed id %q untouched", leg.Item, unknownItemID)
	}
}

func TestCurrencyExchangePlays_itemWithNoIcon_sendsItemIconAsAnExplicitNull(t *testing.T) {
	// null and absent are different answers to the client: an absent key reads as
	// undefined and a "" would be joined onto the API base into a request for the
	// base itself. The field must be present and null.
	leg := firstLegRaw(t, getPlays(t, unknownItemCache(t), "/api/currency-exchange/plays"))

	value, present := leg["itemIcon"]
	if !present {
		t.Fatalf("leg has no itemIcon key, want it present and null: %v", leg)
	}
	if string(value) != "null" {
		t.Errorf("itemIcon = %s, want null — the asset has no artwork for %q", value, unknownItemID)
	}
}

func TestCurrencyExchangePlays_itemWithAnIcon_sendsQuoteIconAsAPath(t *testing.T) {
	// The quote is chaos, which the asset does cover, so this leg is the positive
	// half of the pair: an id with artwork must arrive as the route path the
	// client appends to its API base.
	leg := firstLegRaw(t, getPlays(t, unknownItemCache(t), "/api/currency-exchange/plays"))

	want := `"/currency-exchange/icon/Metadata%2FItems%2FCurrency%2FCurrencyRerollRare"`
	if got := string(leg["quoteIcon"]); got != want {
		t.Errorf("quoteIcon = %s, want %s", got, want)
	}
}

func TestCurrencyExchangePlays_itemTheAssetDoesNotCover_isWarnedAboutOncePerHandler(t *testing.T) {
	// The id recurs in every leg of every play on every request, so the warning is
	// once per recorder — and the recorder lives in the handler closure, not in a
	// package-level var. Two requests through ONE handler therefore log one line.
	logs := captureLogs(t)
	handler := CurrencyExchangePlays(unknownItemCache(t))

	for i := 0; i < 2; i++ {
		w := httptest.NewRecorder()
		handler(w, httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil))
		if w.Code != http.StatusOK {
			t.Fatalf("request %d status = %d, want %d (body: %s)", i, w.Code, http.StatusOK, w.Body.String())
		}
	}

	records := warnRecords(t, logs)
	if len(records) != 1 {
		t.Fatalf("logged %d WARN records across two requests, want exactly 1 (log: %q)", len(records), logs.String())
	}
	if got := records[0]["id"]; got != unknownItemID {
		t.Errorf("warned id = %v, want %q — the log must name the id the asset is missing", got, unknownItemID)
	}
}

func TestCurrencyExchangePlays_itemTheAssetDoesNotCover_isWarnedAboutAgainByASecondHandlerOverTheSameCache(t *testing.T) {
	// "Once" is scoped to the handler closure, not to the process: two handlers
	// built over the same cache each carry their own recorder and each warn. A
	// package-level recorder would swallow the second line, and with it every
	// test's ability to see the warning at all once an earlier test had silenced
	// it.
	logs := captureLogs(t)
	cache := unknownItemCache(t)
	handlers := []http.HandlerFunc{CurrencyExchangePlays(cache), CurrencyExchangePlays(cache)}

	for i, handler := range handlers {
		w := httptest.NewRecorder()
		handler(w, httptest.NewRequest(http.MethodGet, "/api/currency-exchange/plays", nil))
		if w.Code != http.StatusOK {
			t.Fatalf("handler %d status = %d, want %d (body: %s)", i, w.Code, http.StatusOK, w.Body.String())
		}
	}

	records := warnRecords(t, logs)
	if len(records) != 2 {
		t.Fatalf("logged %d WARN records across two handlers, want exactly 2 — one per handler (log: %q)", len(records), logs.String())
	}
	for i, rec := range records {
		if got := rec["id"]; got != unknownItemID {
			t.Errorf("record %d warned id = %v, want %q", i, got, unknownItemID)
		}
	}
}

// captureLogs points the default logger at a buffer for one test. The default
// logger is the seam because exchange.UnknownItems warns through package-level
// slog.
func captureLogs(t *testing.T) *bytes.Buffer {
	t.Helper()
	buf := &bytes.Buffer{}
	prev := slog.Default()
	slog.SetDefault(slog.New(slog.NewJSONHandler(buf, nil)))
	t.Cleanup(func() { slog.SetDefault(prev) })
	return buf
}

// warnRecords drains buf as newline-delimited slog JSON and returns the records
// logged at WARN.
func warnRecords(t *testing.T, buf *bytes.Buffer) []map[string]any {
	t.Helper()
	var out []map[string]any
	for _, line := range strings.Split(strings.TrimSpace(buf.String()), "\n") {
		if line == "" {
			continue
		}
		var rec map[string]any
		if err := json.Unmarshal([]byte(line), &rec); err != nil {
			t.Fatalf("log line is not slog JSON: %v (line: %q)", err, line)
		}
		if rec["level"] == "WARN" {
			out = append(out, rec)
		}
	}
	return out
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
