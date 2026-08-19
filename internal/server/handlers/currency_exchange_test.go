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

// divineChaosRate is the rate the fixture result valued every divine-quoted
// play at: the 198.97 chaos a divine the reference window measured.
const divineChaosRate = 198.97

// directDivinePlay is the fixture's headline flip: buy a divine for 196 chaos,
// sell it for 201 — 197 and 199.97 once each side has paid its tick, which is
// why roiPct is under half of roiPctRaw.
func directDivinePlay() exchange.Play {
	return exchange.Play{
		Key:  "direct:" + exchange.ChaosID + "|" + exchange.DivineID,
		Mode: exchange.ModeDirect,
		Legs: []exchange.Leg{
			{Action: "buy", Item: exchange.DivineID, Quote: exchange.ChaosID, Price: 196, Fair: 198.97, FairOK: true, Tick: 1.0 / 196.0, Volume: 65361, Stock: 8878},
			{Action: "sell", Item: exchange.DivineID, Quote: exchange.ChaosID, Price: 201, Fair: 198.97, FairOK: true, Tick: 1.0 / 196.0, Volume: 65361, Stock: 8878},
		},
		RoiPct:     0.0151,
		Edge:       0.0151,
		RoiPctRaw:  0.0255,
		Roi:        2.9745,
		Investment: 197,
		Turnover:   13001051,
		Tick:       1.0 / 196.0,
		Depth:      65361,
		HoursSeen:  6,
		LastHour:   fixtureLastHour,
	}
}

// directScarabPlay is the second direct play, quoted in divine.
func directScarabPlay() exchange.Play {
	return exchange.Play{
		Key:  "direct:" + exchange.DivineID + "|" + scarabID,
		Mode: exchange.ModeDirect,
		Legs: []exchange.Leg{
			{Action: "buy", Item: scarabID, Quote: exchange.DivineID, Price: 0.0625, Fair: 0.07, FairOK: true, Tick: 0.0625, Volume: 300, Stock: 60},
			{Action: "sell", Item: scarabID, Quote: exchange.DivineID, Price: 0.08, Fair: 0.07, FairOK: true, Tick: 0.0625, Volume: 300, Stock: 60},
		},
		RoiPct:     0.1294,
		Edge:       0.1294,
		RoiPctRaw:  0.28,
		Roi:        1.7099,
		Investment: 13.2131,
		Turnover:   59691,
		Tick:       0.0625,
		Depth:      300,
		HoursSeen:  4,
		LastHour:   fixtureLastHour,
	}
}

// oneHopPlay is the three-leg triangle: buy the scarab in divine, sell it in
// chaos, convert the chaos proceeds back into divine. Its last leg sells the
// SECOND quote back into the first, which is why it is priced in divine per
// chaos — the same chaos/divine market directDivinePlay flips, read the other
// way round, so 1/196 at the dearest against a fair of 1/198.97.
//
// Its middle leg sold at 24.5 chaos against a fair of 15, so the leg AND the
// play carry the suspect flag — the handler is a transport and has to pass both
// through untouched.
//
// The numbers are the ones those legs imply: 24.5 * (1/196) / 0.0625 - 1 is the
// raw 100%, and 0.8353 is the same round trip once each leg has paid its own
// tick, for 11.0369 chaos on an entry of 13.2131.
func oneHopPlay() exchange.Play {
	return exchange.Play{
		Key:  "1-hop:" + scarabID + "|" + exchange.DivineID + "|" + exchange.ChaosID,
		Mode: exchange.ModeOneHop,
		Legs: []exchange.Leg{
			{Action: "buy", Item: scarabID, Quote: exchange.DivineID, Price: 0.0625, Fair: 0.07, FairOK: true, Tick: 0.0625, Volume: 300, Stock: 60},
			{Action: "sell", Item: scarabID, Quote: exchange.ChaosID, Price: 24.5, Fair: 15, FairOK: true, Tick: 0.02, Volume: 250, Stock: 25, Suspect: true},
			{Action: "sell", Item: exchange.ChaosID, Quote: exchange.DivineID, Price: 1.0 / 196.0, Fair: 1.0 / divineChaosRate, FairOK: true, Tick: 1.0 / 196.0, Volume: 13001051, Stock: 4564191},
		},
		RoiPct:     0.8353,
		Edge:       0.8353,
		RoiPctRaw:  1,
		Roi:        11.0369,
		Investment: 13.2131,
		Turnover:   59691,
		Tick:       0.0625,
		Depth:      250,
		Suspect:    true,
		HoursSeen:  3,
		LastHour:   fixtureLastHour,
	}
}

// warmExchangeCache holds a computed ranking of two direct plays and one 1-hop
// in the default horizon.
func warmExchangeCache(t *testing.T) *exchange.Cache {
	t.Helper()
	cache := exchange.NewCache()
	cache.Set(exchange.DefaultHorizon, exchange.Result{
		League:          "Mirage",
		Horizon:         string(exchange.DefaultHorizon),
		From:            fixtureFrom,
		To:              fixtureTo,
		Hours:           6,
		DivineChaosRate: divineChaosRate,
		Plays:           []exchange.Play{oneHopPlay(), directScarabPlay(), directDivinePlay()},
	})
	return cache
}

// dayOnlyPlay is a flip that only survived the day horizon's twenty-four hour
// window, so a body carrying it cannot have come from the recent one.
func dayOnlyPlay() exchange.Play {
	return exchange.Play{
		Key:  "direct:" + exchange.ChaosID + "|" + scarabID,
		Mode: exchange.ModeDirect,
		Legs: []exchange.Leg{
			{Action: "buy", Item: scarabID, Quote: exchange.ChaosID, Price: 12, Fair: 13, FairOK: true, Tick: 1.0 / 12.0, Volume: 900, Stock: 120},
			{Action: "sell", Item: scarabID, Quote: exchange.ChaosID, Price: 15, Fair: 13, FairOK: true, Tick: 1.0 / 12.0, Volume: 900, Stock: 120},
		},
		RoiPct:     0.0577,
		Edge:       0.0577,
		RoiPctRaw:  0.25,
		Roi:        0.75,
		Investment: 13,
		Turnover:   11700,
		Tick:       1.0 / 12.0,
		Depth:      900,
		HoursSeen:  22,
		LastHour:   fixtureLastHour,
	}
}

// bothHorizonsCache holds a different ranking under each horizon, which is what
// makes "the body came from the horizon that was asked for" observable.
func bothHorizonsCache(t *testing.T) *exchange.Cache {
	t.Helper()
	cache := warmExchangeCache(t)
	cache.Set(exchange.HorizonDay, exchange.Result{
		League:          "Mirage",
		Horizon:         string(exchange.HorizonDay),
		From:            fixtureTo.Add(-24 * time.Hour),
		To:              fixtureTo,
		Hours:           24,
		DivineChaosRate: divineChaosRate,
		Plays:           []exchange.Play{dayOnlyPlay()},
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
	Fair      float64 `json:"fair"`
	FairOK    bool    `json:"fairOk"`
	Tick      float64 `json:"tick"`
	Volume    float64 `json:"volume"`
	Stock     int64   `json:"stock"`
	Suspect   bool    `json:"suspect"`
	ItemName  string  `json:"itemName"`
	ItemIcon  *string `json:"itemIcon"`
	QuoteName string  `json:"quoteName"`
	QuoteIcon *string `json:"quoteIcon"`
}

// String renders the leg with its icon pointers dereferenced, so a failed
// comparison names the paths instead of two heap addresses.
func (l exchangeLegBody) String() string {
	return fmt.Sprintf("{action:%s item:%s quote:%s price:%v fair:%v fairOk:%t tick:%v volume:%v stock:%d suspect:%t itemName:%q itemIcon:%s quoteName:%q quoteIcon:%s}",
		l.Action, l.Item, l.Quote, l.Price, l.Fair, l.FairOK, l.Tick, l.Volume, l.Stock, l.Suspect,
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
	Key        string            `json:"key"`
	Mode       string            `json:"mode"`
	RoiPct     float64           `json:"roiPct"`
	Edge       float64           `json:"edge"`
	RoiPctRaw  float64           `json:"roiPctRaw"`
	Roi        float64           `json:"roi"`
	Investment float64           `json:"investment"`
	Turnover   float64           `json:"turnover"`
	Tick       float64           `json:"tick"`
	Depth      float64           `json:"depth"`
	Suspect    bool              `json:"suspect"`
	HoursSeen  int               `json:"hoursSeen"`
	LastHour   time.Time         `json:"lastHour"`
	Legs       []exchangeLegBody `json:"legs"`
}

type exchangePlaysBody struct {
	League          string             `json:"league"`
	LastUpdated     *string            `json:"lastUpdated"`
	From            *string            `json:"from"`
	To              *string            `json:"to"`
	Hours           int                `json:"hours"`
	Warm            bool               `json:"warm"`
	Mode            string             `json:"mode"`
	Horizon         string             `json:"horizon"`
	DivineChaosRate float64            `json:"divineChaosRate"`
	Count           int                `json:"count"`
	Plays           []exchangePlayBody `json:"plays"`
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
			Price: 0.0625, Fair: 0.07, FairOK: true, Tick: 0.0625, Volume: 300, Stock: 60,
			ItemName: "Domination Scarab of Evolution", ItemIcon: iconPath(scarabID),
			QuoteName: "Divine Orb", QuoteIcon: iconPath(exchange.DivineID),
		},
		{
			Action: "sell", Item: scarabID, Quote: exchange.ChaosID,
			Price: 24.5, Fair: 15, FairOK: true, Tick: 0.02, Volume: 250, Stock: 25, Suspect: true,
			ItemName: "Domination Scarab of Evolution", ItemIcon: iconPath(scarabID),
			QuoteName: "Chaos Orb", QuoteIcon: iconPath(exchange.ChaosID),
		},
		{
			Action: "sell", Item: exchange.ChaosID, Quote: exchange.DivineID,
			Price: 1.0 / 196.0, Fair: 1.0 / divineChaosRate, FairOK: true, Tick: 1.0 / 196.0, Volume: 13001051, Stock: 4564191,
			ItemName: "Chaos Orb", ItemIcon: iconPath(exchange.ChaosID),
			QuoteName: "Divine Orb", QuoteIcon: iconPath(exchange.DivineID),
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
	cache.Set(exchange.DefaultHorizon, exchange.Result{
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
	// Every number the engine ranked on has to reach the client unchanged: the
	// DTO embeds exchange.Play rather than copying it precisely so that a field
	// added there cannot go missing here.
	numbers := []struct {
		name      string
		got, want float64
	}{
		{"roiPct", got.RoiPct, want.RoiPct},
		{"edge", got.Edge, want.Edge},
		{"roiPctRaw", got.RoiPctRaw, want.RoiPctRaw},
		{"roi", got.Roi, want.Roi},
		{"investment", got.Investment, want.Investment},
		{"turnover", got.Turnover, want.Turnover},
		{"tick", got.Tick, want.Tick},
		{"depth", got.Depth, want.Depth},
	}
	for _, n := range numbers {
		if n.got != n.want {
			t.Errorf("%s = %v, want %v", n.name, n.got, n.want)
		}
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

func TestCurrencyExchangePlays_flaggedPlay_publishesTheSuspectFlagBesideTheRawPrice(t *testing.T) {
	// The engine serves a play built on an unrepeatable extreme rather than
	// hiding it, so the play-level flag is the only thing telling the reader why
	// a 24.5-chaos sell against a 15-chaos average is not a 100% opportunity. A
	// body that dropped it would render the row as clean. The per-leg fields it
	// was raised from are pinned by the leg-body test above.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?mode=1-hop"))

	play := playByKey(t, body, oneHopPlay().Key)

	if !play.Suspect {
		t.Error("suspect = false, want true — the engine flagged this play")
	}
}

func TestCurrencyExchangePlays_warmCache_publishesTheRateEveryPlayWasValuedIn(t *testing.T) {
	// roi, investment and turnover are all denominated in chaos through this one
	// number, so a client that wants to render a divine-quoted play in divine
	// needs it beside them.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays"))

	if body.DivineChaosRate != divineChaosRate {
		t.Errorf("divineChaosRate = %v, want %v", body.DivineChaosRate, divineChaosRate)
	}
}

func TestCurrencyExchangePlays_horizon_servesTheWindowThatWasAskedFor(t *testing.T) {
	// Both horizons are computed by the same recompute and both live in the
	// cache, so picking one is a lookup — and the body echoes which one, so a
	// client cannot mistake a cached response for the other window's.
	cache := bothHorizonsCache(t)

	tests := []struct {
		name        string
		target      string
		wantHorizon string
		wantHours   int
		wantKeys    []string
	}{
		{
			name:        "horizon absent falls back to recent",
			target:      "/api/currency-exchange/plays",
			wantHorizon: "recent",
			wantHours:   6,
			wantKeys:    []string{oneHopPlay().Key, directScarabPlay().Key, directDivinePlay().Key},
		},
		{
			name:        "horizon=recent is the same answer, asked for explicitly",
			target:      "/api/currency-exchange/plays?horizon=recent",
			wantHorizon: "recent",
			wantHours:   6,
			wantKeys:    []string{oneHopPlay().Key, directScarabPlay().Key, directDivinePlay().Key},
		},
		{
			name:        "horizon=day serves the day ranking",
			target:      "/api/currency-exchange/plays?horizon=day",
			wantHorizon: "day",
			wantHours:   24,
			wantKeys:    []string{dayOnlyPlay().Key},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			body := decodePlays(t, getPlays(t, cache, tc.target))

			if body.Horizon != tc.wantHorizon {
				t.Errorf("horizon = %q, want %q", body.Horizon, tc.wantHorizon)
			}
			if body.Hours != tc.wantHours {
				t.Errorf("hours = %d, want %d", body.Hours, tc.wantHours)
			}
			gotKeys := make([]string, 0, len(body.Plays))
			for _, play := range body.Plays {
				gotKeys = append(gotKeys, play.Key)
			}
			if !reflect.DeepEqual(gotKeys, tc.wantKeys) {
				t.Errorf("keys = %v, want %v", gotKeys, tc.wantKeys)
			}
		})
	}
}

func TestCurrencyExchangePlays_horizonThatIsNotServed_isRejectedInsteadOfFallingBackToTheDefault(t *testing.T) {
	// A typo that quietly returned the other window's ranking would look like a
	// working toggle — and the two windows disagree by design.
	w := getPlays(t, bothHorizonsCache(t), "/api/currency-exchange/plays?horizon=week")

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusBadRequest, w.Body.String())
	}

	var body map[string]any
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode body: %v", err)
	}
	if want := "horizon must be one of recent, day"; body["error"] != want {
		t.Errorf("error = %v, want %q", body["error"], want)
	}
	if _, leaked := body["plays"]; leaked {
		t.Errorf("rejected request still carried plays: %v", body["plays"])
	}
}

func TestCurrencyExchangePlays_horizonWithNoCacheEntry_answersColdRatherThanTheOtherWindow(t *testing.T) {
	// Each horizon warms on its own. Between the first recompute writing one and
	// the next, a request for the other must read as "not ready", never as the
	// window that happens to be warm.
	body := decodePlays(t, getPlays(t, warmExchangeCache(t), "/api/currency-exchange/plays?horizon=day"))

	if body.Warm {
		t.Error("warm = true, want false — nothing has been computed for the day horizon")
	}
	if len(body.Plays) != 0 {
		t.Errorf("got %d plays, want none (%+v)", len(body.Plays), body.Plays)
	}
	if body.Horizon != "day" {
		t.Errorf("horizon = %q, want %q", body.Horizon, "day")
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
