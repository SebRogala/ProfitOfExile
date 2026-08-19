package exchange

import (
	"sort"
	"time"
)

// Mode names the shape of a play: how many markets it touches.
//
// A direct play is a same-market flip (buy low, sell high, edge = high/low - 1).
// A one-hop play is a three-leg triangle through a second quote currency
// (edge = highXinB * highBinA / lowXinA - 1). Ranking prefers the direct shape
// when everything else ties, because it carries one execution risk instead of
// three.
type Mode string

const (
	// ModeDirect is a same-market flip: buy and sell the same item on the same
	// market within the hour.
	ModeDirect Mode = "direct"
	// ModeOneHop is a three-leg triangle: buy the item against one currency,
	// sell it against another, convert the proceeds back.
	ModeOneHop Mode = "1-hop"
)

// Leg is one executed step of a play on one market.
//
// Price is the optimistic side of that step — the hour's cheapest realized price
// for a buy, the dearest for a sell — expressed in Quote units per one Item. The
// two extremes come from different trades within the hour and did not
// necessarily coexist, so a leg's price is the best case, not a fill.
type Leg struct {
	// Action is "buy" or "sell", from the player's point of view.
	Action string `json:"action"`
	// Item is the raw feed id being traded. POE-175's handler runs it through
	// Humanize; the engine never carries display names.
	Item string `json:"item"`
	// Quote is the raw feed id of the currency Price is denominated in.
	Quote string `json:"quote"`
	// Price is Quote units per one Item, on the optimistic side of the hour.
	Price float64 `json:"price"`
	// Volume is the recency-weighted mean of the units of Item traded per hour
	// on this market across the hours the play was seen.
	Volume float64 `json:"volume"`
	// Stock is the market's highest stock of Item in the newest hour the play
	// was seen.
	Stock int64 `json:"stock"`
}

// Play is one ranked opportunity aggregated over the window's hours.
//
// Edge and every Leg.Volume are recency-weighted means over the hours where the
// recipe passed its depth gates; prices, stocks and LastHour come from the newest
// such hour. The edge is built from realized hourly extremes rather than a live
// order book, so treat it as an upper bound on what was takeable: nothing
// guarantees the cheap trade and the dear trade were available at the same
// moment, and on items that move in one-or-two-divine ticks the gap between them
// is mostly quantization.
type Play struct {
	// Key identifies the recipe across hours: "direct:<marketID>" or
	// "1-hop:<item>|<buyQuote>|<sellQuote>".
	Key string `json:"key"`
	// Mode is the play's shape.
	Mode Mode `json:"mode"`
	// Legs are the steps in execution order: two for direct, three for 1-hop.
	Legs []Leg `json:"legs"`
	// Edge is the weighted mean fractional gain, 0.05 meaning +5%.
	Edge float64 `json:"edge"`
	// Depth is the thinnest leg's Volume — the units per hour the whole recipe
	// can absorb.
	Depth float64 `json:"depth"`
	// HoursSeen counts the window hours where the recipe passed its gates. A
	// low count on a wide window is a ghost: an edge that showed up once.
	HoursSeen int `json:"hoursSeen"`
	// LastHour is the newest feed hour the recipe was seen in, UTC.
	LastHour time.Time `json:"lastHour"`
}

// Result is one BestPlays answer, shaped for JSON.
//
// From is the oldest feed hour used and To the newest hour plus one, so the
// window reads as the half-open interval [From, To). Hours is how many distinct
// hours actually carried data — it can be smaller than Config.WindowHours when
// the feed skipped hours or the league is young. Plays is ranked and never nil.
//
// Every Edge inside is an upper bound derived from realized hourly extremes, not
// a quoted spread.
type Result struct {
	League string    `json:"league"`
	From   time.Time `json:"from"`
	To     time.Time `json:"to"`
	Hours  int       `json:"hours"`
	Plays  []Play    `json:"plays"`
}

// Config tunes the window, the depth floor and the ranking cut.
//
// The two filters answer different failure modes: MinVolumePerHour drops edges
// nobody could have executed, and MinHoursSeen drops ghosts — a single hour's
// print that never repeated. QuotePriority decides which side of a market reads
// as the currency (see orient).
type Config struct {
	// WindowHours is how many of the newest distinct hours to aggregate.
	WindowHours int
	// MinVolumePerHour is the per-leg floor on units traded in an hour.
	MinVolumePerHour float64
	// MinEdge is the smallest fractional edge kept, 0.02 meaning +2%. A zero
	// value means "use the default"; to disable the floor pass a negative
	// value (e.g. -1), which also surfaces the losing direction of a loop.
	MinEdge float64
	// MinHoursSeen is the smallest number of window hours a play must appear
	// in. It is capped at the number of hours actually present, so a
	// single-hour window still returns plays.
	MinHoursSeen int
	// MaxPlays truncates the ranked result.
	MaxPlays int
	// QuotePriority lists currency ids from most to least preferred as the
	// quote side of a market.
	QuotePriority []string
}

// DefaultConfig is the engine's tuning: a six-hour window, at least ten units
// traded per leg per hour, a two-percent edge floor, seen in at least two hours,
// the top hundred plays, quoted in divine before chaos.
//
// The window is short on purpose. The feed publishes one hour at a time and the
// point of the engine is what is takeable now; six hours is long enough for the
// weighted mean to punish a one-hour ghost and short enough that a league-day's
// price drift does not dominate the average.
func DefaultConfig() Config {
	return Config{
		WindowHours:      6,
		MinVolumePerHour: 10,
		MinEdge:          0.02,
		MinHoursSeen:     2,
		MaxPlays:         100,
		QuotePriority:    []string{DivineID, ChaosID},
	}
}

// withDefaults fills each unset field from DefaultConfig independently, so a
// caller can set one knob and inherit the rest.
//
// A non-positive count or floor (WindowHours, MinVolumePerHour, MinHoursSeen,
// MaxPlays) reads as unset, because none of them has a meaningful negative
// value. MinEdge is different: only an exact 0 reads as unset, so a caller can
// ask for negative edges and see what the engine would otherwise hide. To run
// effectively without a volume floor, pass a tiny positive number rather than 0.
func (c Config) withDefaults() Config {
	d := DefaultConfig()
	if c.WindowHours <= 0 {
		c.WindowHours = d.WindowHours
	}
	if c.MinVolumePerHour <= 0 {
		c.MinVolumePerHour = d.MinVolumePerHour
	}
	if c.MinEdge == 0 {
		c.MinEdge = d.MinEdge
	}
	if c.MinHoursSeen <= 0 {
		c.MinHoursSeen = d.MinHoursSeen
	}
	if c.MaxPlays <= 0 {
		c.MaxPlays = d.MaxPlays
	}
	if len(c.QuotePriority) == 0 {
		c.QuotePriority = d.QuotePriority
	}
	return c
}

// BestPlays ranks the currency-exchange opportunities in a league's recent
// hours. It is the engine's single entry point and is pure: rows in, ranked
// result out, no database and no clock.
//
// Rows are grouped by feed hour and the newest Config.WindowHours distinct hours
// are kept (N of them). Each hour is scored independently — direct flips and
// one-hop triangles, each leg gated on that hour's traded volume and stock — and
// the surviving candidates are merged by key with a recency weight
//
//	w_i = (N - i) / N   for the i-th newest hour, i = 0 newest
//
// so the newest hour counts fully and the oldest counts 1/N. A play's Edge and
// each Leg.Volume are the weighted means over the hours the recipe survived;
// its prices, stocks and LastHour come from the newest surviving hour, its
// HoursSeen counts those hours, and its Depth is the thinnest leg's mean volume.
//
// What comes back is filtered to Edge >= MinEdge and HoursSeen >= MinHoursSeen
// (capped at N), ranked by Edge desc, then Depth desc, then direct before 1-hop,
// then Key ascending, and truncated to MaxPlays. The ranking is a stable sort
// over a key-sorted list, so identical input always produces identical output
// regardless of the order rows arrive in.
//
// The caveat that governs every number here: the feed publishes each hour's
// realized low and high, not a book. An edge is the gap between two trades that
// happened during the same hour, and nothing says both were available at once,
// so every Edge is an upper bound rather than a profit. Persistence and the
// stricter filtering that goes with it are POE-180's job; this returns raw item
// ids, and POE-175's handler is what runs them through Humanize.
func BestPlays(league string, rows []StoredRow, cfg Config) Result {
	cfg = cfg.withDefaults()
	result := Result{League: league, Plays: []Play{}}

	byHour := make(map[int64][]Row)
	hourAt := make(map[int64]time.Time)
	for _, stored := range rows {
		hour := stored.Hour.UTC()
		stamp := hour.Unix()
		if _, known := hourAt[stamp]; !known {
			hourAt[stamp] = hour
		}
		byHour[stamp] = append(byHour[stamp], stored.Row)
	}
	if len(byHour) == 0 {
		return result
	}

	stamps := make([]int64, 0, len(byHour))
	for stamp := range byHour {
		stamps = append(stamps, stamp)
	}
	sort.Slice(stamps, func(i, j int) bool { return stamps[i] > stamps[j] })
	if len(stamps) > cfg.WindowHours {
		stamps = stamps[:cfg.WindowHours]
	}

	n := len(stamps)
	result.Hours = n
	result.From = hourAt[stamps[n-1]]
	result.To = hourAt[stamps[0]].Add(time.Hour)

	merged := make(map[string]*aggregate)
	for i, stamp := range stamps {
		weight := float64(n-i) / float64(n)
		hour, hourRows := hourAt[stamp], byHour[stamp]

		found := directCandidates(hourRows, cfg)
		found = append(found, crossQuoteCandidates(hourRows, cfg)...)

		seen := make(map[string]bool, len(found))
		for _, c := range found {
			if seen[c.key] {
				continue
			}
			seen[c.key] = true

			a, known := merged[c.key]
			if !known {
				a = newAggregate(c, hour)
				merged[c.key] = a
			}
			a.add(c, weight)
		}
	}

	minHoursSeen := cfg.MinHoursSeen
	if minHoursSeen > n {
		minHoursSeen = n
	}
	for _, key := range sortedIDs(merged) {
		if play, ok := merged[key].play(cfg.MinEdge, minHoursSeen); ok {
			result.Plays = append(result.Plays, play)
		}
	}

	sort.SliceStable(result.Plays, func(i, j int) bool {
		a, b := result.Plays[i], result.Plays[j]
		switch {
		case a.Edge != b.Edge:
			return a.Edge > b.Edge
		case a.Depth != b.Depth:
			return a.Depth > b.Depth
		case a.Mode != b.Mode:
			return modeRank(a.Mode) < modeRank(b.Mode)
		default:
			return a.Key < b.Key
		}
	})
	if len(result.Plays) > cfg.MaxPlays {
		result.Plays = result.Plays[:cfg.MaxPlays]
	}
	return result
}

// aggregate accumulates one key's candidates across the window's hours.
//
// legs, stocks and lastHour are captured from the FIRST candidate merged in,
// which is the newest hour because BestPlays walks hours newest first. Every
// candidate sharing a key has the same number of legs: the key spells out the
// mode and, for a triangle, the route.
type aggregate struct {
	key       string
	mode      Mode
	legs      []Leg
	lastHour  time.Time
	weight    float64
	edge      float64
	volume    []float64
	hoursSeen int
}

// newAggregate starts an aggregate from the newest hour a key was seen in,
// keeping that hour's legs as the template for prices and stocks.
func newAggregate(c candidate, hour time.Time) *aggregate {
	return &aggregate{
		key:      c.key,
		mode:     c.mode,
		legs:     append([]Leg(nil), c.legs...),
		lastHour: hour,
		volume:   make([]float64, len(c.legs)),
	}
}

// add folds one hour's observation in at that hour's recency weight.
func (a *aggregate) add(c candidate, weight float64) {
	a.weight += weight
	a.edge += weight * c.edge
	for i, leg := range c.legs {
		a.volume[i] += weight * leg.Volume
	}
	a.hoursSeen++
}

// play turns the accumulated hours into a Play, or reports ok == false when the
// weighted edge or the hour count falls short. Dividing by a.weight is safe:
// an aggregate only exists once at least one positive-weight hour was added.
func (a *aggregate) play(minEdge float64, minHoursSeen int) (Play, bool) {
	edge := a.edge / a.weight
	if edge < minEdge || a.hoursSeen < minHoursSeen {
		return Play{}, false
	}

	legs := make([]Leg, len(a.legs))
	depth := 0.0
	for i, leg := range a.legs {
		leg.Volume = a.volume[i] / a.weight
		legs[i] = leg
		if i == 0 || leg.Volume < depth {
			depth = leg.Volume
		}
	}

	return Play{
		Key:       a.key,
		Mode:      a.mode,
		Legs:      legs,
		Edge:      edge,
		Depth:     depth,
		HoursSeen: a.hoursSeen,
		LastHour:  a.lastHour,
	}, true
}

// modeRank orders modes for the ranking tie-break: the direct flip wins a tie
// because it carries one execution risk instead of three.
func modeRank(m Mode) int {
	switch m {
	case ModeDirect:
		return 0
	case ModeOneHop:
		return 1
	default:
		return 2
	}
}
