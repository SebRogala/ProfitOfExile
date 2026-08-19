package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"profitofexile/internal/exchange"
)

// legResponse is one execution step, the engine's Leg plus display names.
//
// exchange.Leg is EMBEDDED rather than copied field by field: the engine owns
// the leg's shape and its JSON tags, so a field added there appears here without
// an edit, and a field renamed there cannot silently keep serializing under its
// old name. ItemName and QuoteName are the only additions the transport layer
// makes — the engine deliberately carries raw feed ids (see exchange.Humanize).
type legResponse struct {
	exchange.Leg
	ItemName  string `json:"itemName"`
	QuoteName string `json:"quoteName"`
}

// playResponse is one ranked play with humanized legs.
//
// Legs shadows the embedded exchange.Play.Legs: encoding/json resolves a tag
// collision in favour of the shallower field, so "legs" serializes from this
// []legResponse and the embedded []exchange.Leg is dropped. Every other Play
// field (key, mode, edge, depth, hoursSeen, lastHour) comes through the
// embedding untouched.
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
	League      string         `json:"league"`
	LastUpdated *time.Time     `json:"lastUpdated"`
	From        *time.Time     `json:"from"`
	To          *time.Time     `json:"to"`
	Hours       int            `json:"hours"`
	Warm        bool           `json:"warm"`
	Mode        string         `json:"mode"`
	Count       int            `json:"count"`
	Plays       []playResponse `json:"plays"`
}

// modeAll is the query value (and the default) meaning "do not filter".
const modeAll = "all"

// CurrencyExchangePlays serves the cached currency-exchange ranking.
//
// GET /api/currency-exchange/plays?mode=all|direct|1-hop — mode is optional and
// defaults to all; anything else is a 400 rather than a silent fallback, because
// a typo that quietly returned every play would look like a working filter.
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

		// A nil *exchange.Cache reads as cold; Snapshot handles the nil receiver.
		result, warm := cache.Snapshot()

		plays := make([]playResponse, 0, len(result.Plays))
		for _, play := range result.Plays {
			if mode != modeAll && string(play.Mode) != mode {
				continue
			}
			plays = append(plays, playResponse{Play: play, Legs: humanizeLegs(play.Legs)})
		}

		body := playsResponse{
			League: result.League,
			Hours:  result.Hours,
			Warm:   warm,
			Mode:   mode,
			Count:  len(plays),
			Plays:  plays,
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

// humanizeLegs attaches display names to a play's legs. The result is always a
// non-nil slice so "legs" is [] rather than null on a play with no legs.
func humanizeLegs(legs []exchange.Leg) []legResponse {
	out := make([]legResponse, 0, len(legs))
	for _, leg := range legs {
		out = append(out, legResponse{
			Leg:       leg,
			ItemName:  exchange.Humanize(leg.Item),
			QuoteName: exchange.Humanize(leg.Quote),
		})
	}
	return out
}
