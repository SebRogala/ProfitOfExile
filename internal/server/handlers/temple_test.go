package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"profitofexile/internal/league"
	"profitofexile/internal/temple"
)

var templeScope = league.Historical("Allflame")

func templeRequest(t *testing.T, cache *temple.Cache) map[string]any {
	t.Helper()

	rec := httptest.NewRecorder()
	TempleMarket(cache, templeScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/temple-market", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	if got := rec.Header().Get("Content-Type"); got != "application/json" {
		t.Errorf("Content-Type = %q, want application/json", got)
	}

	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v (body %s)", err, rec.Body.String())
	}
	return body
}

func warmTempleCache(t *testing.T) *temple.Cache {
	t.Helper()

	cache := temple.NewCache(templeScope)
	cache.Set(temple.Build("Allflame", []temple.Line{
		{Category: temple.RoomCategory, Name: "A (Tier 1)", Chaos: 10, Listings: 700},
		{Category: temple.RoomCategory, Name: "B (Tier 1)", Chaos: 10, Listings: 700},
		{Category: temple.RoomCategory, Name: "Locus of Corruption (Tier 3)", Chaos: 853.6, Listings: 2489},
		{Category: "Vial", Name: "Vial of Fate", Chaos: 1, Divine: 0.002, Listings: 343},
	}, nil))
	return cache
}

func TestTempleMarket_servesTheWarmMarketFromMemory(t *testing.T) {
	body := templeRequest(t, warmTempleCache(t))

	if body["floor"] != float64(10) {
		t.Errorf("floor = %v, want 10", body["floor"])
	}
	rooms, _ := body["rooms"].([]any)
	if len(rooms) != 3 {
		t.Fatalf("rooms = %d, want 3", len(rooms))
	}
	locus := rooms[2].(map[string]any)
	if locus["name"] != "Locus of Corruption (Tier 3)" || locus["saleDelta"] != 843.6 || locus["aboveFloor"] != true {
		t.Errorf("above-floor room = %v, want Locus of Corruption at a saleDelta of 843.6", locus)
	}
}

func TestTempleMarket_coldCacheServesTheStaticAnswerWithNoObservations(t *testing.T) {
	// The server accepts traffic against a cold cache after every deploy. The
	// answer is 200 with the recipe table and a null asOf — the same shape
	// MarketOverview beside it takes — not an error the client has to special-case.
	body := templeRequest(t, temple.NewCache(templeScope))

	if body["asOf"] != nil {
		t.Errorf("asOf = %v on a cold cache, want null — that is how a client tells cold from warm", body["asOf"])
	}
	if rooms, _ := body["rooms"].([]any); len(rooms) != 0 {
		t.Errorf("rooms = %d on a cold cache, want none", len(rooms))
	}
	if recipes, _ := body["recipes"].([]any); len(recipes) != 11 {
		t.Errorf("recipes = %d on a cold cache, want the 11-row table (it needs no data)", len(recipes))
	}
	if body["floorRule"] != temple.FloorRule {
		t.Errorf("floorRule = %v, want %q", body["floorRule"], temple.FloorRule)
	}
}

func TestTempleMarket_coldResponseIsTheServersLeague(t *testing.T) {
	// The endpoint is keyed on the process-active scope, so even the answer that
	// carries no observations names the league it would have been about.
	body := templeRequest(t, temple.NewCache(templeScope))

	if body["league"] != "Allflame" {
		t.Errorf("league = %v, want Allflame", body["league"])
	}
}

func TestTempleMarket_nilCacheServesColdRatherThanPanicking(t *testing.T) {
	// A server started without the temple pillar still registers the route.
	body := templeRequest(t, nil)

	if body["league"] != "Allflame" || body["asOf"] != nil {
		t.Errorf("nil-cache response = %v, want the cold answer for Allflame", body)
	}
}

func TestTempleMarket_emptySlicesMarshalAsArraysNotNull(t *testing.T) {
	// A client that iterates rooms would have to null-check on the cold path
	// alone if these came out as null.
	rec := httptest.NewRecorder()
	TempleMarket(temple.NewCache(templeScope), templeScope)(rec, httptest.NewRequest(http.MethodGet, "/api/analysis/temple-market", nil))

	for _, field := range []string{`"rooms":[]`, `"items":[]`, `"categoriesSeen":[]`} {
		if !strings.Contains(rec.Body.String(), field) {
			t.Errorf("cold body does not contain %s; body = %s", field, rec.Body.String())
		}
	}
}

func TestTempleMarket_warmButEmptyMarketServesArraysNotNull(t *testing.T) {
	// WARM-AND-EMPTY is reachable: a league whose IncursionTemple feed 404s
	// inside the lookback while the other six collect, or the window right after
	// a rollover when nothing has been stored yet. The cache hands the handler a
	// COPY, and a copy that appended an empty slice onto nil would serve
	// "rooms": null here — which the desktop's non-nullable list deserializer
	// (POE-258) fails on, on exactly the responses nobody tests by hand.
	cache := temple.NewCache(templeScope)
	cache.Set(temple.Build("Allflame", nil, nil))

	body := templeRequest(t, cache)

	for _, field := range []string{"rooms", "categoriesSeen"} {
		if body[field] == nil {
			t.Errorf("%q is null on a warm-and-empty market, want an array", field)
		}
	}
	// The static half of the answer survives an empty observation set: every
	// recipe member is still served, unpriced, and the recipe table is still
	// there for the client to resolve names against.
	if items, _ := body["items"].([]any); len(items) != 31 {
		t.Errorf("items = %d on a warm-and-empty market, want all 31 recipe members served unpriced", len(items))
	}
	if recipes, _ := body["recipes"].([]any); len(recipes) != 11 {
		t.Errorf("recipes = %d on a warm-and-empty market, want the 11-row table (it needs no data)", len(recipes))
	}
}

func TestTempleMarket_aLeagueQueryParameterDoesNotChangeTheLeagueServed(t *testing.T) {
	// The endpoint is keyed on the process-active scope and takes no parameters.
	// A request naming another league gets this server's league — not that
	// league, and not an error the client has to handle.
	rec := httptest.NewRecorder()
	TempleMarket(warmTempleCache(t), templeScope)(rec,
		httptest.NewRequest(http.MethodGet, "/api/analysis/temple-market?league=Mirage", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200", rec.Code)
	}
	var body map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response: %v (body %s)", err, rec.Body.String())
	}
	if body["league"] != "Allflame" {
		t.Errorf("league = %v for a request asking for Mirage, want Allflame — the server's own league", body["league"])
	}
}

// TestTempleMarket_wireShape pins the field names the desktop reads (POE-256,
// POE-258). Renaming one is a silent client breakage: the desktop would read
// undefined and render an unpriced, floor-less temple with no error anywhere.
func TestTempleMarket_wireShape(t *testing.T) {
	body := templeRequest(t, warmTempleCache(t))

	assertTempleKeys(t, "payload", body, []string{
		"league", "asOf", "floor", "floorRule", "categoriesSeen", "rooms", "items", "recipes",
	})

	rooms, _ := body["rooms"].([]any)
	assertTempleKeys(t, "room", rooms[0].(map[string]any), []string{
		"name", "line", "tier", "chaos", "saleDelta", "aboveFloor", "listings", "lowConfidence", "icon",
	})

	items, _ := body["items"].([]any)
	var priced map[string]any
	for _, raw := range items {
		entry := raw.(map[string]any)
		if entry["name"] == "Vial of Fate" {
			priced = entry
		}
	}
	if priced == nil {
		t.Fatalf("no Vial of Fate in items")
	}
	assertTempleKeys(t, "item", priced, []string{"name", "category", "price", "unpriced", "icon"})
	assertTempleKeys(t, "price", priced["price"].(map[string]any), []string{
		"chaos", "divine", "listings", "lowConfidence", "windowPriced", "windowHours", "windowSamples",
	})

	recipes, _ := body["recipes"].([]any)
	assertTempleKeys(t, "recipe", recipes[0].(map[string]any), []string{"vial", "base", "upgraded"})
}

func TestTempleMarket_unpricedItemCarriesANullPrice(t *testing.T) {
	// The wire contract for "nobody is selling this": price null, unpriced true.
	// Zero would read as a free item.
	body := templeRequest(t, warmTempleCache(t))

	items, _ := body["items"].([]any)
	for _, raw := range items {
		entry := raw.(map[string]any)
		if entry["name"] != "Zerphi's Heart" {
			continue
		}
		if entry["unpriced"] != true {
			t.Errorf("Zerphi's Heart unpriced = %v, want true", entry["unpriced"])
		}
		if entry["price"] != nil {
			t.Errorf("Zerphi's Heart price = %v, want null", entry["price"])
		}
		return
	}
	t.Fatalf("Zerphi's Heart is not in items; every recipe member is served whether priced or not")
}

func assertTempleKeys(t *testing.T, what string, got map[string]any, want []string) {
	t.Helper()

	remaining := make(map[string]struct{}, len(got))
	for key := range got {
		remaining[key] = struct{}{}
	}
	for _, key := range want {
		if _, ok := got[key]; !ok {
			t.Errorf("%s is missing field %q", what, key)
		}
		delete(remaining, key)
	}
	for key := range remaining {
		t.Errorf("%s carries unexpected field %q", what, key)
	}
}
