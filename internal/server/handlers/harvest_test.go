package handlers

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"profitofexile/internal/exchange"
	"profitofexile/internal/harvest"
)

// harvestFossilID is a weighted type of the fossil family, so a seeded price for
// it reaches the family's tiers in the snapshot.
const harvestFossilID = "Metadata/Items/Currency/CurrencyDelveCraftingLightning"

// harvestHour is the feed hour both seeded snapshots were priced at. Prices and
// tiers must report this same instant.
var harvestHour = time.Date(2026, 10, 12, 18, 0, 0, 0, time.UTC)

// harvestFamilyCount is the number of families in the embedded family data. A
// cold body still lists every one of them.
const harvestFamilyCount = 7

// harvestSnapshot is a warm snapshot for one horizon whose fossil price is chaos
// and whose fossil family has one tier entry.
func harvestSnapshot(h exchange.Horizon, chaos float64) harvest.Snapshot {
	return harvest.Snapshot{
		League:          "Mirage",
		Horizon:         h,
		PriceHour:       harvestHour,
		DivineChaosRate: 200,
		Items: map[string]harvest.ItemPrice{
			harvestFossilID: {Chaos: &chaos},
		},
		Lifeforce: map[string]harvest.LifeforcePrice{},
		Tiers: map[string]harvest.FamilyTiers{
			"fossil": {
				BoundariesChaos: []float64{5},
				ByItem:          map[string]string{harvestFossilID: "HIGH"},
				Names:           []string{"HIGH", "LOW"},
			},
		},
	}
}

// harvestCache returns a cache holding the given horizons, each warm at chaos.
func harvestCache(t *testing.T, chaos map[exchange.Horizon]float64) *harvest.Cache {
	t.Helper()
	cache := harvest.NewCache()
	for h, c := range chaos {
		cache.Set(h, harvestSnapshot(h, c))
	}
	return cache
}

// harvestBody is the GET /api/harvest body as a client reads it. Nullable fields
// are pointers so a JSON null is distinguishable from a zero value.
type harvestBody struct {
	Prices struct {
		League          string  `json:"league"`
		Horizon         string  `json:"horizon"`
		LastUpdated     *string `json:"lastUpdated"`
		PriceHour       *string `json:"priceHour"`
		Warm            bool    `json:"warm"`
		DivineChaosRate float64 `json:"divineChaosRate"`
		Prices          map[string]struct {
			Chaos *float64 `json:"chaos"`
		} `json:"prices"`
	} `json:"prices"`
	Families struct {
		Warm     bool                `json:"warm"`
		Families []harvestFamilyBody `json:"families"`
	} `json:"families"`
}

// harvestFamilyBody is one family as a client reads it, reduced to the fields
// the tests assert on.
type harvestFamilyBody struct {
	ID    string            `json:"id"`
	Tiers *harvestTiersBody `json:"tiers"`
}

// harvestTiersBody is a family's tiers as a client reads them.
type harvestTiersBody struct {
	ByItem    map[string]string `json:"byItem"`
	PriceHour string            `json:"priceHour"`
}

// getHarvest serves one request through a chi router, the way the server mounts
// the handler, and returns the recorder.
func getHarvest(t *testing.T, cache *harvest.Cache, target string) *httptest.ResponseRecorder {
	t.Helper()
	router := chi.NewRouter()
	router.Get("/api/harvest", Harvest(cache))

	w := httptest.NewRecorder()
	router.ServeHTTP(w, httptest.NewRequest(http.MethodGet, target, nil))
	return w
}

// decodeHarvest decodes a 200 body, failing the test on any other status.
func decodeHarvest(t *testing.T, w *httptest.ResponseRecorder) harvestBody {
	t.Helper()
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusOK, w.Body.String())
	}
	var body harvestBody
	if err := json.NewDecoder(w.Body).Decode(&body); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	return body
}

// familyByID returns the response family with id, failing the test when absent.
func familyByID(t *testing.T, body harvestBody, id string) harvestFamilyBody {
	t.Helper()
	for _, f := range body.Families.Families {
		if f.ID == id {
			return f
		}
	}
	t.Fatalf("no family %q in %d families", id, len(body.Families.Families))
	return harvestFamilyBody{}
}

func TestHarvest_coldCache_answersWarmFalseWithEveryFamilyAndNullTiers(t *testing.T) {
	// A nil cache (server started without the pillar) and a fresh one (no recompute
	// has found a feed hour yet) are the same answer: 200, not ready, and every
	// family still listed so the page can render its static data.
	w := getHarvest(t, nil, "/api/harvest?horizon=day")
	body := decodeHarvest(t, w)

	if body.Prices.Warm {
		t.Error("prices.warm = true, want false")
	}
	if body.Prices.LastUpdated != nil || body.Prices.PriceHour != nil {
		t.Errorf("lastUpdated = %v, priceHour = %v; want both null", body.Prices.LastUpdated, body.Prices.PriceHour)
	}
	if body.Families.Warm {
		t.Error("families.warm = true, want false")
	}
	if got := len(body.Families.Families); got != harvestFamilyCount {
		t.Fatalf("got %d families, want %d", got, harvestFamilyCount)
	}
	for _, f := range body.Families.Families {
		if f.Tiers != nil {
			t.Errorf("family %q tiers = %+v, want null while cold", f.ID, f.Tiers)
		}
	}
}

func TestHarvest_coldCache_encodesEmptyPricesAsAnObjectNotNull(t *testing.T) {
	// A client reads prices.prices as a map; null would read as "no map at all".
	w := getHarvest(t, nil, "/api/harvest")

	var raw struct {
		Prices struct {
			Prices    json.RawMessage `json:"prices"`
			Lifeforce json.RawMessage `json:"lifeforce"`
		} `json:"prices"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &raw); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	if got := string(raw.Prices.Prices); got != "{}" {
		t.Errorf("prices.prices = %s, want {}", got)
	}
	if got := string(raw.Prices.Lifeforce); got != "{}" {
		t.Errorf("prices.lifeforce = %s, want {}", got)
	}
}

func TestHarvest_horizonOutsideRecentAndDay_isRejectedEvenWhenWarm(t *testing.T) {
	// The cache is warm for both horizons, so a fallback to either would answer
	// 200. The 400 proves the value was refused, not defaulted.
	cache := harvestCache(t, map[exchange.Horizon]float64{
		exchange.HorizonRecent: 10,
		exchange.HorizonDay:    20,
	})
	w := getHarvest(t, cache, "/api/harvest?horizon=week")

	if w.Code != http.StatusBadRequest {
		t.Fatalf("status = %d, want %d (body: %s)", w.Code, http.StatusBadRequest, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "horizon must be one of recent, day") {
		t.Errorf("body = %s, want the error naming the valid horizons", w.Body.String())
	}
}

func TestHarvest_noHorizonParam_servesTheDayHorizon(t *testing.T) {
	// Only day is warm. A default of recent would answer cold, so this fails if
	// the empty parameter resolves to the wrong horizon.
	cache := harvestCache(t, map[exchange.Horizon]float64{exchange.HorizonDay: 20})
	body := decodeHarvest(t, getHarvest(t, cache, "/api/harvest"))

	if body.Prices.Horizon != "day" {
		t.Errorf("prices.horizon = %q, want %q", body.Prices.Horizon, "day")
	}
	if !body.Prices.Warm {
		t.Fatal("prices.warm = false, want true: the day horizon is warm")
	}
	if got := body.Prices.Prices[harvestFossilID].Chaos; got == nil || *got != 20 {
		t.Errorf("fossil chaos = %v, want 20 from the day snapshot", got)
	}
}

func TestHarvest_horizonParam_servesThatHorizonsSnapshot(t *testing.T) {
	cache := harvestCache(t, map[exchange.Horizon]float64{
		exchange.HorizonRecent: 10,
		exchange.HorizonDay:    20,
	})

	for _, tc := range []struct {
		horizon string
		want    float64
	}{
		{horizon: "recent", want: 10},
		{horizon: "day", want: 20},
	} {
		body := decodeHarvest(t, getHarvest(t, cache, "/api/harvest?horizon="+tc.horizon))
		if body.Prices.Horizon != tc.horizon {
			t.Errorf("horizon %s: prices.horizon = %q", tc.horizon, body.Prices.Horizon)
		}
		got := body.Prices.Prices[harvestFossilID].Chaos
		if got == nil || *got != tc.want {
			t.Errorf("horizon %s: fossil chaos = %v, want %v", tc.horizon, got, tc.want)
		}
	}
}

func TestHarvest_tiersCarryThePricesHour(t *testing.T) {
	cache := harvestCache(t, map[exchange.Horizon]float64{exchange.HorizonDay: 20})
	body := decodeHarvest(t, getHarvest(t, cache, "/api/harvest?horizon=day"))

	want := "2026-10-12T18:00:00Z"
	if body.Prices.PriceHour == nil || *body.Prices.PriceHour != want {
		t.Fatalf("prices.priceHour = %v, want %q", body.Prices.PriceHour, want)
	}
	fossil := familyByID(t, body, "fossil")
	if fossil.Tiers == nil {
		t.Fatal("fossil tiers = null, want the seeded tiers")
	}
	if fossil.Tiers.PriceHour != want {
		t.Errorf("fossil tiers.priceHour = %q, want %q (the prices' hour)", fossil.Tiers.PriceHour, want)
	}
}

func TestHarvest_responseIsNeverCached(t *testing.T) {
	cache := harvestCache(t, map[exchange.Horizon]float64{exchange.HorizonDay: 20})
	w := getHarvest(t, cache, "/api/harvest?horizon=day")

	if got := w.Header().Get("Cache-Control"); got != "no-store" {
		t.Errorf("Cache-Control = %q, want %q", got, "no-store")
	}
}

func TestHarvest_familyWireShape_carriesStaticDataAndTiersByFamily(t *testing.T) {
	cache := harvestCache(t, map[exchange.Horizon]float64{exchange.HorizonDay: 20})
	w := getHarvest(t, cache, "/api/harvest?horizon=day")

	var raw struct {
		Families struct {
			Families []map[string]any `json:"families"`
		} `json:"families"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &raw); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	byID := make(map[string]map[string]any, len(raw.Families.Families))
	for _, f := range raw.Families.Families {
		byID[f["id"].(string)] = f
	}

	fossil := byID["fossil"]
	weights, ok := fossil["weights"].(map[string]any)
	if !ok {
		t.Fatalf("fossil weights = %v, want an object", fossil["weights"])
	}
	sample, ok := weights["sample"].(map[string]any)
	if !ok || sample["rolls"] != float64(10895) {
		t.Errorf("fossil weights.sample = %v, want rolls 10895", weights["sample"])
	}
	if _, ok := fossil["note"].(string); !ok {
		t.Errorf("fossil note = %v, want the family's _note string", fossil["note"])
	}
	tiers, ok := fossil["tiers"].(map[string]any)
	if !ok {
		t.Fatalf("fossil tiers = %v, want an object", fossil["tiers"])
	}
	if _, ok := tiers["byItem"].(map[string]any); !ok {
		t.Errorf("fossil tiers.byItem = %v, want an object", tiers["byItem"])
	}

	delirium, ok := byID["delirium"]
	if !ok {
		t.Fatal("no delirium family in the response")
	}
	note, present := delirium["note"]
	if !present || note != nil {
		t.Errorf("delirium note = %v (present %t), want an explicit null", note, present)
	}
	if delirium["tiers"] != nil {
		t.Errorf("delirium tiers = %v, want null: the snapshot has no delirium entry", delirium["tiers"])
	}
}

func harvestF(v float64) *float64 { return &v }

func TestHarvest_warmPricesWireValues(t *testing.T) {
	// Every served value is distinct, so a transport that swapped, dropped or
	// rounded one of them would name the wrong field here.
	const itemX = "Metadata/Items/Currency/ItemX"
	const itemY = "Metadata/Items/Currency/ItemY"
	cache := harvest.NewCache()
	cache.Set(exchange.HorizonDay, harvest.Snapshot{
		League:          "Mirage",
		Horizon:         exchange.HorizonDay,
		PriceHour:       harvestHour,
		DivineChaosRate: 360.07,
		Items: map[string]harvest.ItemPrice{
			itemX: {Chaos: harvestF(12.5), Divine: harvestF(0.04)},
			itemY: {LastSeenChaos: harvestF(300)},
		},
		Lifeforce: map[string]harvest.LifeforcePrice{
			"Wild": {ItemID: harvest.LifeforceWild, Colour: "purple", Chaos: 0.031, PerDivine: 9905},
		},
		Tiers: map[string]harvest.FamilyTiers{
			"fossil": {
				BoundariesChaos:  []float64{192.39, 29.96},
				ByItem:           map[string]string{itemX: "LOW"},
				Names:            []string{"HIGH", "MID"},
				TopBoundaryChaos: harvestF(401.63),
			},
		},
	})

	w := getHarvest(t, cache, "/api/harvest?horizon=day")
	if w.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 (body: %s)", w.Code, w.Body.String())
	}
	var top map[string]any
	if err := json.Unmarshal(w.Body.Bytes(), &top); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	prices := top["prices"].(map[string]any)
	families := top["families"].(map[string]any)

	if got := prices["divineChaosRate"]; got != 360.07 {
		t.Errorf("prices.divineChaosRate = %v, want 360.07", got)
	}
	items := prices["prices"].(map[string]any)
	x := items[itemX].(map[string]any)
	if x["chaos"] != 12.5 || x["divine"] != 0.04 {
		t.Errorf("X chaos/divine = %v/%v, want 12.5/0.04", x["chaos"], x["divine"])
	}
	if x["lastSeenChaos"] != nil {
		t.Errorf("X lastSeenChaos = %v, want null", x["lastSeenChaos"])
	}
	y := items[itemY].(map[string]any)
	if y["chaos"] != nil || y["divine"] != nil {
		t.Errorf("Y chaos/divine = %v/%v, want null/null", y["chaos"], y["divine"])
	}
	if y["lastSeenChaos"] != 300.0 {
		t.Errorf("Y lastSeenChaos = %v, want 300", y["lastSeenChaos"])
	}
	lifeforce, _ := prices["lifeforce"].(map[string]any)
	wild, ok := lifeforce["Wild"].(map[string]any)
	if !ok {
		t.Fatalf("prices.lifeforce = %v, want a Wild entry", prices["lifeforce"])
	}
	if wild["itemId"] != harvest.LifeforceWild || wild["colour"] != "purple" ||
		wild["chaos"] != 0.031 || wild["perDivine"] != 9905.0 {
		t.Errorf("lifeforce Wild = %v, want itemId %q colour purple chaos 0.031 perDivine 9905", wild, harvest.LifeforceWild)
	}

	var fossil map[string]any
	for _, f := range families["families"].([]any) {
		if fm := f.(map[string]any); fm["id"] == "fossil" {
			fossil = fm
		}
	}
	tiers, ok := fossil["tiers"].(map[string]any)
	if !ok {
		t.Fatalf("fossil tiers = %v, want an object", fossil["tiers"])
	}
	if got := tiers["topBoundaryChaos"]; got != 401.63 {
		t.Errorf("fossil tiers.topBoundaryChaos = %v, want 401.63", got)
	}
	if got := tiers["byItem"].(map[string]any)[itemX]; got != "LOW" {
		t.Errorf("fossil tiers.byItem[X] = %v, want LOW", got)
	}
	if got := tiers["boundariesChaos"].([]any); len(got) != 2 || got[0] != 192.39 || got[1] != 29.96 {
		t.Errorf("fossil tiers.boundariesChaos = %v, want [192.39 29.96]", got)
	}
}

func TestHarvest_nilTierCollections_encodeAsEmptyObjectAndArrays(t *testing.T) {
	// A family whose tiers were built without ByItem, BoundariesChaos or Names
	// must still serialize them as {} and [] — never null — for the client.
	cache := harvest.NewCache()
	cache.Set(exchange.HorizonDay, harvest.Snapshot{
		League:    "Mirage",
		Horizon:   exchange.HorizonDay,
		PriceHour: harvestHour,
		Items:     map[string]harvest.ItemPrice{},
		Lifeforce: map[string]harvest.LifeforcePrice{},
		Tiers:     map[string]harvest.FamilyTiers{"fossil": {}},
	})

	w := getHarvest(t, cache, "/api/harvest?horizon=day")
	var top map[string]any
	if err := json.Unmarshal(w.Body.Bytes(), &top); err != nil {
		t.Fatalf("decode body %q: %v", w.Body.String(), err)
	}
	for _, f := range top["families"].(map[string]any)["families"].([]any) {
		fm := f.(map[string]any)
		if fm["id"] != "fossil" {
			continue
		}
		tiers := fm["tiers"].(map[string]any)
		if got, ok := tiers["byItem"].(map[string]any); !ok || len(got) != 0 {
			t.Errorf("tiers.byItem = %v, want {}", tiers["byItem"])
		}
		if got, ok := tiers["boundariesChaos"].([]any); !ok || len(got) != 0 {
			t.Errorf("tiers.boundariesChaos = %v, want []", tiers["boundariesChaos"])
		}
		if got, ok := tiers["names"].([]any); !ok || len(got) != 0 {
			t.Errorf("tiers.names = %v, want []", tiers["names"])
		}
		return
	}
	t.Fatal("no fossil family in the response")
}
