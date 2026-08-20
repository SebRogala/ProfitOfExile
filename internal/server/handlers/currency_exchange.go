package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"profitofexile/internal/exchange"
)

// legResponse is one execution step, the engine's Leg plus display data.
//
// exchange.Leg is EMBEDDED rather than copied field by field: the engine owns
// the leg's shape and its JSON tags, so a field added there appears here without
// an edit, and a field renamed there cannot silently keep serializing under its
// old name. The names, icons, and categories are the only additions the
// transport layer makes — the engine deliberately carries raw feed ids.
//
// The icons are pointers so an item with no artwork serializes as null rather
// than as "": a client must be able to tell "render no icon" from a path it
// should fetch, and an empty string joined onto an API base is a request for the
// base itself. They are API-relative paths into this server's icon route, not
// upstream poewiki URLs — production cannot fetch poewiki (ADR-012).
//
// The categories are plain strings, not pointers, because "" is a usable answer
// rather than an ambiguous one: a client filtering by category treats an
// uncategorized leg as unfiltered, so it needs no way to tell an absent category
// from an empty one. Both sides of the trade carry one, since a filter applies
// to whichever side the reader is shopping for.
type legResponse struct {
	exchange.Leg
	ItemName      string  `json:"itemName"`
	ItemIcon      *string `json:"itemIcon"`
	ItemCategory  string  `json:"itemCategory"`
	QuoteName     string  `json:"quoteName"`
	QuoteIcon     *string `json:"quoteIcon"`
	QuoteCategory string  `json:"quoteCategory"`
}

// playResponse is one ranked play with decorated legs.
//
// Legs shadows the embedded exchange.Play.Legs: encoding/json resolves a tag
// collision in favour of the shallower field, so "legs" serializes from this
// []legResponse and the embedded []exchange.Leg is dropped. Every other Play
// field (see exchange.Play) comes through the embedding untouched.
type playResponse struct {
	exchange.Play
	Legs []legResponse `json:"legs"`
}

// playsResponse is the GET /api/currency-exchange/plays body.
//
// LastUpdated, From and To are pointers so a result covering no hours renders
// them as null rather than as the zero time — "the feed has no hour yet" and
// "the feed's newest hour is year zero" must not read the same. Warm distinguishes COLD (nothing
// computed yet) from WARM-AND-EMPTY (computed, and the honest answer is no
// plays); see the cache-state contract in internal/exchange/service.go.
type playsResponse struct {
	League      string     `json:"league"`
	LastUpdated *time.Time `json:"lastUpdated"`
	From        *time.Time `json:"from"`
	To          *time.Time `json:"to"`
	Hours       int        `json:"hours"`
	Warm        bool       `json:"warm"`
	Mode        string     `json:"mode"`
	// Horizon echoes which window these plays were ranked in, so a client that
	// requested one cannot mistake a cached body for another.
	Horizon string `json:"horizon"`
	// DivineChaosRate is the chaos value of one divine the engine valued every
	// play with, or 0 when the divine/chaos market did not trade in the newest
	// hour (in which case no divine-quoted play is in the list).
	DivineChaosRate float64        `json:"divineChaosRate"`
	Count           int            `json:"count"`
	Plays           []playResponse `json:"plays"`
	// Categories is the in-game Currency Exchange sidebar, in sidebar order, so
	// a client renders its category filter from the response instead of holding
	// its own copy of the taxonomy. It is the whole list, independent of the
	// plays in this body: a filter whose rows appeared and vanished with the
	// ranking would be unusable.
	Categories []string `json:"categories"`
}

// modeAll is the query value (and the default) meaning "do not filter".
const modeAll = "all"

// CurrencyExchangePlays serves the cached currency-exchange ranking.
//
// GET /api/currency-exchange/plays?mode=all|direct|1-hop&horizon=recent|day —
// both parameters are optional and default to all and recent; anything else is a
// 400 rather than a silent fallback, because a typo that quietly returned every
// play, or the other horizon's ranking, would look like a working filter.
//
// The two horizons are computed by the same recompute and both live in the
// cache, so picking one is a map lookup rather than work.
//
// The handler never touches the database. The read side of this pillar is
// exchange.Service's recompute, which stores whole answers in the cache; a query
// here would only repeat work the service already did and would put an unbounded
// hypertable scan behind an unauthenticated route. A nil cache and a cold cache
// therefore both answer 200 with warm:false and no plays — the route exists as
// soon as the server does, and the client learns the answer is not ready yet
// instead of getting a 503 it would have to special-case.
//
// Responses are Cache-Control: no-store: the body changes whenever a feed hour
// lands, and clients are told about that over poe/currency-exchange/updated.
func CurrencyExchangePlays(cache *exchange.Cache) http.HandlerFunc {
	// One recorder per handler, captured in the closure rather than declared at
	// package level. "Warn once per id" then means once per router — which is
	// once per process in cmd/server, the only place a router is built for real
	// — while a test that asserts the log gets a recorder nothing else has
	// already silenced. A package-level var would make that assertion depend on
	// which tests ran before it.
	unknown := &exchange.UnknownItems{}

	return func(w http.ResponseWriter, r *http.Request) {
		mode := r.URL.Query().Get("mode")
		if mode == "" {
			mode = modeAll
		}
		switch mode {
		case modeAll, string(exchange.ModeDirect), string(exchange.ModeOneHop):
		default:
			jsonError(w, http.StatusBadRequest, "mode must be one of all, direct, 1-hop")
			return
		}

		horizon := exchange.Horizon(r.URL.Query().Get("horizon"))
		if horizon == "" {
			horizon = exchange.DefaultHorizon
		}
		switch horizon {
		case exchange.HorizonRecent, exchange.HorizonDay:
		default:
			jsonError(w, http.StatusBadRequest, "horizon must be one of recent, day")
			return
		}

		// A nil *exchange.Cache reads as cold; Snapshot handles the nil receiver.
		result, warm := cache.Snapshot(horizon)

		plays := make([]playResponse, 0, len(result.Plays))
		for _, play := range result.Plays {
			if mode != modeAll && string(play.Mode) != mode {
				continue
			}
			plays = append(plays, playResponse{Play: play, Legs: decorateLegs(play.Legs, unknown)})
		}

		body := playsResponse{
			League:          result.League,
			Hours:           result.Hours,
			Warm:            warm,
			Mode:            mode,
			Horizon:         string(horizon),
			DivineChaosRate: result.DivineChaosRate,
			Count:           len(plays),
			Plays:           plays,
			Categories:      exchange.Categories(),
		}
		if last, ok := exchange.LastUpdated(result); ok {
			last = last.UTC()
			body.LastUpdated = &last
			from, to := result.From.UTC(), result.To.UTC()
			body.From, body.To = &from, &to
		}

		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		json.NewEncoder(w).Encode(body)
	}
}

// decorateLegs attaches display names, icon paths and categories to a play's
// legs, noting every id the item asset does not cover on unknown, which must not
// be nil.
//
// The result is always a non-nil slice so "legs" is [] rather than null on a
// play with no legs.
func decorateLegs(legs []exchange.Leg, unknown *exchange.UnknownItems) []legResponse {
	out := make([]legResponse, 0, len(legs))
	for _, leg := range legs {
		out = append(out, legResponse{
			Leg:           leg,
			ItemName:      itemName(leg.Item, unknown),
			ItemIcon:      itemIcon(leg.Item),
			ItemCategory:  itemCategory(leg.Item),
			QuoteName:     itemName(leg.Quote, unknown),
			QuoteIcon:     itemIcon(leg.Quote),
			QuoteCategory: itemCategory(leg.Quote),
		})
	}
	return out
}

// itemName resolves one feed id to its display name, recording an id the asset
// does not name.
//
// The lookup is what separates "the asset has no name for this" from "the asset
// has one" — exchange.DisplayName alone cannot say which of the two it
// returned, because its Humanize fallback always produces something readable.
// An unnamed id is the symptom that the asset needs regenerating (a new
// league's items), so it has to reach the log rather than quietly render as a
// plausible-looking wrong name. A present-but-blank name falls in the same
// bucket: it renders through the same fallback, so it is worth the same log
// line.
//
// This repeats DisplayName's rule (asset name when non-empty, else Humanize)
// rather than calling it, so that one lookup answers both questions. The two
// must stay in step.
func itemName(id string, unknown *exchange.UnknownItems) string {
	item, ok := exchange.LookupItem(id)
	if !ok || item.Name == "" {
		unknown.Note(id)
		return exchange.Humanize(id)
	}
	return item.Name
}

// itemIcon returns the API-relative icon path for a feed id, or nil when the
// item has no artwork. A known item without an icon is not an anomaly — the
// asset carries a null icon for roughly one id in seven — so it is not noted.
func itemIcon(id string) *string {
	path, ok := exchange.IconPath(id)
	if !ok {
		return nil
	}
	return &path
}

// itemCategory returns the sidebar category an id belongs to, or "" when the
// asset does not cover it.
//
// The miss is not noted on UnknownItems: itemName looked the same id up on the
// same leg and already warned about it, and a second note per leg would only
// double the work behind a log line that is emitted once per id anyway.
func itemCategory(id string) string {
	item, _ := exchange.LookupItem(id)
	return item.Category
}
