package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"profitofexile/internal/exchange"
	"profitofexile/internal/harvest"
)

// harvestResponse is the GET /api/harvest body: the prices and the families,
// both read from one cache snapshot per horizon so they cannot straddle a
// recompute.
type harvestResponse struct {
	Prices   harvestPricesResponse   `json:"prices"`
	Families harvestFamiliesResponse `json:"families"`
}

// harvestPricesResponse is one horizon's prices. LastUpdated and PriceHour are
// the same feed hour (the newest stored hour), as RFC3339 UTC strings, and are
// pointers so a cold cache renders them as null rather than as the zero time.
// Warm distinguishes COLD (no recompute has found a feed hour) from a warm
// snapshot; a cold body carries League "" and empty maps.
type harvestPricesResponse struct {
	League          string                            `json:"league"`
	Horizon         exchange.Horizon                  `json:"horizon"`
	LastUpdated     *string                           `json:"lastUpdated"`
	PriceHour       *string                           `json:"priceHour"`
	Warm            bool                              `json:"warm"`
	DivineChaosRate float64                           `json:"divineChaosRate"`
	Prices          map[string]harvest.ItemPrice      `json:"prices"`
	Lifeforce       map[string]harvest.LifeforcePrice `json:"lifeforce"`
}

// harvestFamiliesResponse lists every Harvest family in file order. Warm is the
// horizon's warmth; a family's tiers are null until the horizon is warm and the
// snapshot has an entry for that family.
type harvestFamiliesResponse struct {
	Warm     bool                    `json:"warm"`
	Families []harvestFamilyResponse `json:"families"`
}

// harvestFamilyResponse is one family's static data plus its tiers. Every field
// is spelled out here rather than marshalled from harvest.Family, so the wire
// names are fixed by this file: note is the family's _note, and null when absent.
type harvestFamilyResponse struct {
	ID         string                `json:"id"`
	Label      string                `json:"label"`
	Lifeforce  *string               `json:"lifeforce"`
	RerollCost *int                  `json:"rerollCost"`
	Weights    *harvest.Weights      `json:"weights"`
	Tiers      *harvestTiersResponse `json:"tiers"`
	Note       *string               `json:"note"`
}

// harvestTiersResponse is a family's tiers from the snapshot, plus the feed hour
// the prices behind them were read at.
type harvestTiersResponse struct {
	harvest.FamilyTiers
	PriceHour string `json:"priceHour"`
}

// Harvest serves the cached Harvest prices and tier families.
//
// GET /api/harvest?horizon=recent|day — the parameter is optional and defaults
// to day, the Harvest page's default. Anything else is a 400 rather than a
// silent fallback, so a typo cannot quietly serve the other horizon's prices.
//
// The handler never touches the database: harvest.Service recomputes both
// horizons into the cache, and this reads one horizon's snapshot. A nil cache
// and a cold cache both answer 200 with warm:false, no prices, and every family
// present with null tiers, so the client can render the page before the first
// recompute lands.
//
// Responses are Cache-Control: no-store, because the body changes whenever a
// feed hour lands.
func Harvest(cache *harvest.Cache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		horizon := exchange.Horizon(r.URL.Query().Get("horizon"))
		if horizon == "" {
			horizon = exchange.HorizonDay
		}
		switch horizon {
		case exchange.HorizonRecent, exchange.HorizonDay:
		default:
			jsonError(w, http.StatusBadRequest, "horizon must be one of recent, day")
			return
		}

		// A nil *harvest.Cache reads as cold; Snapshot handles the nil receiver.
		snap, warm := cache.Snapshot(horizon)

		body := harvestResponse{
			Prices:   buildHarvestPrices(horizon, snap, warm),
			Families: buildHarvestFamilies(snap, warm),
		}

		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		json.NewEncoder(w).Encode(body)
	}
}

func buildHarvestPrices(horizon exchange.Horizon, snap harvest.Snapshot, warm bool) harvestPricesResponse {
	out := harvestPricesResponse{
		Horizon:   horizon,
		Warm:      warm,
		Prices:    make(map[string]harvest.ItemPrice, len(snap.Items)),
		Lifeforce: make(map[string]harvest.LifeforcePrice, len(snap.Lifeforce)),
	}
	if !warm {
		return out
	}
	hour := snap.PriceHour.UTC().Format(time.RFC3339)
	out.League = snap.League
	out.LastUpdated = &hour
	out.PriceHour = &hour
	out.DivineChaosRate = snap.DivineChaosRate
	for id, price := range snap.Items {
		out.Prices[id] = price
	}
	for colour, price := range snap.Lifeforce {
		out.Lifeforce[colour] = price
	}
	return out
}

func buildHarvestFamilies(snap harvest.Snapshot, warm bool) harvestFamiliesResponse {
	families := harvest.Families()
	out := harvestFamiliesResponse{
		Warm:     warm,
		Families: make([]harvestFamilyResponse, 0, len(families)),
	}
	var hour string
	if warm {
		hour = snap.PriceHour.UTC().Format(time.RFC3339)
	}
	for _, f := range families {
		fam := harvestFamilyResponse{
			ID:         f.ID,
			Label:      f.Label,
			Lifeforce:  f.Lifeforce,
			RerollCost: f.RerollCost,
			Weights:    f.Weights,
			Note:       f.Note,
		}
		if tiers, ok := snap.Tiers[f.ID]; warm && ok {
			fam.Tiers = &harvestTiersResponse{
				FamilyTiers: normalizeFamilyTiers(tiers),
				PriceHour:   hour,
			}
		}
		out.Families = append(out.Families, fam)
	}
	return out
}

// normalizeFamilyTiers replaces a nil slice or map with an empty one, so the
// tiers encode as [] and {} rather than null.
func normalizeFamilyTiers(t harvest.FamilyTiers) harvest.FamilyTiers {
	if t.BoundariesChaos == nil {
		t.BoundariesChaos = []float64{}
	}
	if t.ByItem == nil {
		t.ByItem = map[string]string{}
	}
	if t.Names == nil {
		t.Names = []string{}
	}
	return t
}
