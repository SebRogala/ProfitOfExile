package exchange

import (
	"log/slog"
	"math"
	"sort"
	"time"
)

// Mode names the shape of a play: how many markets it touches.
//
// A direct play is a same-market flip (buy low, sell high, roiPct = high/low - 1).
// A one-hop play is a three-leg triangle through a second quote currency
// (roiPct = highXinB * highBinA / lowXinA - 1). Ranking prefers the direct shape
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
// Price is the median across the window's hours of that hour's extreme on the
// leg's side — the cheapest realized price for a buy, the dearest for a sell —
// expressed in Quote units per one Item. It is the executable recipe: a market
// maker posts at the edges, and the median is what that edge was worth in a
// typical hour rather than in the loudest one.
//
// Fair is the median of the hours' volume-weighted average prices, the price the
// traded mass actually cleared at, and it is the anchor Price should be read
// against: a Price far outside Fair is quantization, not opportunity. Tick says
// how far apart two representable prices are on this market, as a fraction.
type Leg struct {
	// Action is "buy" or "sell", from the player's point of view.
	Action string `json:"action"`
	// Item is the raw feed id being traded. POE-175's handler runs it through
	// Humanize; the engine never carries display names.
	Item string `json:"item"`
	// Quote is the raw feed id of the currency Price and Fair are denominated in.
	Quote string `json:"quote"`
	// Price is Quote units per one Item: the median hourly low on a buy leg,
	// the median hourly high on a sell leg.
	Price float64 `json:"price"`
	// Fair is the median hourly volume-weighted average price of one Item in
	// Quote units — where the hour's mass traded, between the two extremes. Only
	// the hours that HAD a volume-weighted price count toward it; it is 0 when
	// no hour in the window did, which reads as "no anchor", not as a free item.
	Fair float64 `json:"fair"`
	// Tick is the median hourly price resolution of this market as a fraction
	// of the price: 0.5 means the next representable price is 50% away. See
	// tickOf for why it is the strongest predictor of a fake spread.
	Tick float64 `json:"tick"`
	// Volume is the median of the units of Item traded per hour on this market
	// across the hours the play was seen.
	Volume float64 `json:"volume"`
	// Stock is the market's highest stock of Item in the newest hour the play
	// was seen. Liveness only — it is an hourly max of total book size and does
	// not describe the extreme (measured corr <= 0.13 against the edge).
	Stock int64 `json:"stock"`
}

// Play is one ranked opportunity aggregated over the window's hours.
//
// The window's hours enter as cross-hour MEDIANS, with no recency weighting:
// one hour of nonsense moves a median by nothing, and 38% of markets print a
// single-hour edge above 5x their own median. Each leg's Price, Fair, Tick and
// Volume is such a median; RoiPct, Roi, Investment, Turnover, Tick and Depth are
// arithmetic ON those medians — a ratio, a product, a min or a max across the
// legs — not medians of themselves. Leg.Stock, RoiPctNewestHour, HoursSeen and
// LastHour are not medians at all: the first two read the newest hour alone, the
// last two count and name the hours. RoiPct is recomputed FROM the emitted leg
// prices, so what the play claims is reproducible from what it shows.
//
// The medians are synthesized across hours in BOTH shapes: even a direct play's
// two Prices are independent medians — one of the hourly lows, one of the hourly
// highs — so its RoiPct is a combination no single hour necessarily offered. A
// 1-hop play compounds that over three such medians AND three separate markets:
// a typical shape of the route rather than a trade someone made.
type Play struct {
	// Key identifies the recipe across hours: "direct:<marketID>" or
	// "1-hop:<item>|<buyQuote>|<sellQuote>".
	Key string `json:"key"`
	// Mode is the play's shape.
	Mode Mode `json:"mode"`
	// Legs are the steps in execution order: two for direct, three for 1-hop.
	Legs []Leg `json:"legs"`
	// RoiPct is the fractional gain of one round trip at the prices the Legs
	// show, 0.05 meaning +5%.
	RoiPct float64 `json:"roiPct"`
	// Edge is RoiPct under its old name, kept on the wire for clients written
	// before POE-184.
	//
	// Deprecated: read RoiPct. The two always carry the same value.
	Edge float64 `json:"edge"`
	// RoiPctNewestHour is the extreme-to-extreme edge of LastHour, the newest
	// hour this recipe was seen in — the single-hour number the engine used to
	// rank on. It is the NOW reading, not a bound: it sits above RoiPct when
	// that hour was louder than the window's typical one and BELOW RoiPct when
	// the newest hour was quieter. That is the "is it moving right now" marker
	// the desktop draws.
	RoiPctNewestHour float64 `json:"roiPctNewestHour"`
	// Roi is chaos gained per exchange of ONE unit: Investment * RoiPct.
	Roi float64 `json:"roi"`
	// Investment is the chaos one unit costs to enter, from the first leg.
	Investment float64 `json:"investment"`
	// Turnover is chaos per hour flowing through the play's thinnest leg — the
	// median quote-side volume valued in chaos. It is the liquidity reading;
	// unit volume is not one (corr(ln edge, ln units) = +0.06 against −0.30 for
	// chaos turnover).
	Turnover float64 `json:"turnover"`
	// Tick is the coarsest median price resolution among the legs: the worst
	// step the recipe has to live with.
	Tick float64 `json:"tick"`
	// Depth is the thinnest leg's median Volume — the units per hour the whole
	// recipe can absorb.
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
type Result struct {
	League string `json:"league"`
	// Horizon names which of the Service's windows produced this result. It is
	// empty when BestPlays was called directly rather than through a horizon.
	Horizon string    `json:"horizon"`
	From    time.Time `json:"from"`
	To      time.Time `json:"to"`
	Hours   int       `json:"hours"`
	// DivineChaosRate is the chaos every play was valued in: the median hourly
	// VWAP of the divine/chaos market over the window. It is 0 when that market
	// did not trade in the window, in which case no divine-quoted play is
	// returned at all.
	DivineChaosRate float64 `json:"divineChaosRate"`
	Plays           []Play  `json:"plays"`
}

// Config tunes the window, the gates and the ranking cut.
//
// The gates answer different failure modes, all of them measured over 26 hours
// of Allflame: MinVolumePerHour drops legs nobody traded, MinHoursSeen drops
// ghosts, MinTurnoverChaos drops markets too small for the edge to be real
// (p50 robust edge is 242% under 100 chaos/hour and 18% over 100k),
// MaxTick and MinEdgeTickRatio drop the spreads that are one integer price step
// wide, and MinROIChaos drops plays whose percentage is fine and whose payout is
// a rounding error. QuotePriority decides which side of a market reads as the
// currency (see orient).
type Config struct {
	// WindowHours is how many of the newest distinct hours to aggregate. It
	// binds only a direct BestPlays call: the served path overlays it per
	// horizon (see DefaultHorizons and horizonConfig in service.go).
	WindowHours int
	// MinVolumePerHour is the per-leg liveness floor on units traded in an
	// hour. It is not the liquidity gate — MinTurnoverChaos is.
	MinVolumePerHour float64
	// MinEdge is the smallest RoiPct kept, 0.02 meaning +2%. A zero value means
	// "use the default"; a negative value lowers the percentage floor and
	// surfaces the small positive returns the default hides. It cannot surface a
	// losing route: MinEdgeTickRatio * Tick and MinROIChaos are always positive,
	// so a negative RoiPct fails those two gates whatever MinEdge says.
	MinEdge float64
	// MinTurnoverChaos is the smallest Play.Turnover kept, in chaos per hour.
	MinTurnoverChaos float64
	// MaxTick is the coarsest Play.Tick kept, as a fraction of the price.
	MaxTick float64
	// MinEdgeTickRatio is how many price steps wide a spread must be to count:
	// RoiPct >= MinEdgeTickRatio * Tick.
	MinEdgeTickRatio float64
	// MinROIChaos is the smallest Play.Roi kept, in chaos per exchanged unit.
	MinROIChaos float64
	// MinHoursSeen is the smallest number of window hours a play must appear
	// in. It is capped at the number of hours actually present, so a
	// single-hour window still returns plays. Like WindowHours it binds only a
	// direct BestPlays call; the served path overlays it per horizon (see
	// DefaultHorizons and horizonConfig in service.go).
	MinHoursSeen int
	// MaxPlays truncates the ranked result.
	MaxPlays int
	// QuotePriority lists currency ids from most to least preferred as the
	// quote side of a market. Only ChaosID and DivineID can be VALUED today (see
	// chaosPerUnit): any other id still orients markets and builds candidates,
	// but every play it quotes is dropped for want of a chaos rate. withDefaults
	// warns when the list carries such an id.
	QuotePriority []string
	// Horizons are the windows the Service recomputes on top of this base
	// config. BestPlays ignores the field: it ranks ONE window, the one
	// WindowHours and MinHoursSeen describe.
	Horizons []HorizonConfig
}

// DefaultConfig is the engine's tuning: a six-hour window, at least ten units
// traded per leg per hour, a two-percent ROI floor, ten thousand chaos an hour
// of turnover, a price resolution no coarser than 10% with an ROI at least five
// steps wide, three chaos per exchanged unit, seen in at least two hours, the
// top hundred plays, quoted in divine before chaos.
//
// The gate LEVELS are derived from the 26-hour Allflame measurement in POE-184,
// whose robust edge was the MEDIAN OF PER-HOUR EDGES: that gate set takes 908
// markets to 135, and the survivors' top 25 are all full-window with 29–71%
// robust edge at 0.1–8.7% ticks. The engine does not gate on that statistic. It
// gates on the ratio of the medians (median high / median low − 1), the related
// statistic chosen so the legs shown and the percentage claimed are one
// reproducible arithmetic statement — which is also why the SQL's 908→135 is a
// calibration of the levels, not a prediction of how many plays come back. The
// served ranking is confirmed by the live check against the running stack, not
// by those counts. Loosening any one level lets a measured noise case back in —
// Divine Vessel returns at 109 chaos/hour of turnover, Delirium Scarab at a 100%
// tick.
//
// WindowHours is the base window, which the Service overrides per horizon (see
// Horizons); the ranking a client sees comes from one of those, not from this.
func DefaultConfig() Config {
	return Config{
		WindowHours:      6,
		MinVolumePerHour: 10,
		MinEdge:          0.02,
		MinTurnoverChaos: 10000,
		MaxTick:          0.10,
		MinEdgeTickRatio: 5,
		MinROIChaos:      3,
		MinHoursSeen:     2,
		MaxPlays:         100,
		QuotePriority:    []string{DivineID, ChaosID},
		Horizons:         DefaultHorizons(),
	}
}

// withDefaults fills each unset field from DefaultConfig independently, so a
// caller can set one knob and inherit the rest.
//
// A non-positive count, floor or cap (WindowHours, MinVolumePerHour,
// MinTurnoverChaos, MaxTick, MinEdgeTickRatio, MinROIChaos, MinHoursSeen,
// MaxPlays) reads as unset, because none of them has a meaningful negative
// value. MinEdge is different: only an exact 0 reads as unset, so a caller can
// push the percentage floor below zero and see the small positive returns the
// default hides (the other gates still bar a losing route). To run
// effectively without one of the other gates, pass a value that cannot bind (a
// tiny positive floor, a huge cap) rather than 0.
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
	if c.MinTurnoverChaos <= 0 {
		c.MinTurnoverChaos = d.MinTurnoverChaos
	}
	if c.MaxTick <= 0 {
		c.MaxTick = d.MaxTick
	}
	if c.MinEdgeTickRatio <= 0 {
		c.MinEdgeTickRatio = d.MinEdgeTickRatio
	}
	if c.MinROIChaos <= 0 {
		c.MinROIChaos = d.MinROIChaos
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
	if len(c.Horizons) == 0 {
		c.Horizons = d.Horizons
	}
	for _, quote := range c.QuotePriority {
		if chaosPerUnit(quote, 1) <= 0 {
			slog.Warn("currency-exchange: quote currency cannot be valued in chaos; its plays are built and then dropped",
				"quote", quote)
		}
	}
	return c
}

// BestPlays ranks the currency-exchange opportunities in a league's recent
// hours. It is the engine's single entry point: rows in, ranked result out, no
// database and no clock.
//
// Rows are grouped by feed hour and the newest Config.WindowHours distinct hours
// are kept. Each hour is scored independently — direct flips and one-hop
// triangles, each leg gated on that hour's traded volume and stock — and the
// surviving candidates are merged by key. The merge takes MEDIANS across the
// hours a recipe survived, not a weighted mean: the failure mode being defended
// against is one hour's blowup, and a mean carries it while a median does not.
//
// Everything a play claims is then rebuilt from those medians. Prices are the
// median hourly extreme on each leg's side, so RoiPct is arithmetic on the legs
// the client is shown rather than a separately averaged number, and Roi,
// Investment and Turnover value them in chaos through DivineChaosRate — the
// median hourly VWAP of the divine/chaos market in this same window. A play
// whose quote is neither chaos nor divine — scarab/scarab, card/scarab and every
// other cross-item market — is dropped outright, because gates denominated in
// chaos (MinTurnoverChaos, MinROIChaos) cannot be evaluated against a payout
// measured in scarabs. When the divine/chaos market itself did not trade in the
// window the rate is 0 and every divine-quoted play is dropped rather than
// valued at a guess; that warning is logged once per HORIZON, so twice per
// Service recompute.
//
// What comes back has passed, in order: HoursSeen >= MinHoursSeen (capped at the
// hours present), RoiPct >= MinEdge, Turnover >= MinTurnoverChaos,
// Tick <= MaxTick, RoiPct >= MinEdgeTickRatio * Tick, and Roi >= MinROIChaos. It
// is ranked by RoiPct desc, then Turnover desc, then direct before 1-hop, then
// Key ascending, and truncated to MaxPlays. The ranking is a stable sort over a
// key-sorted list, so identical input always produces identical output
// regardless of the order rows arrive in.
//
// The caveat that governs the percentages: the feed publishes each hour's
// realized low and high, not a book. Even a median edge is the typical gap
// between two trades of the same hour, and nothing says both were takeable at
// once — that is what the tick gates bound, and what RoiPctNewestHour shows the
// current hour's version of. This returns raw item ids; POE-175's handler runs
// them through Humanize.
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
	// Newest hour first: the merge below reads the newest hour's recipe, stock
	// and edge off the first candidate it sees (aggregate.add re-checks the
	// stamp, so a reordering here costs nothing but the reading).
	sort.Slice(stamps, func(i, j int) bool { return stamps[i] > stamps[j] })
	if len(stamps) > cfg.WindowHours {
		stamps = stamps[:cfg.WindowHours]
	}

	n := len(stamps)
	result.Hours = n
	result.From = hourAt[stamps[n-1]]
	result.To = hourAt[stamps[0]].Add(time.Hour)

	divineChaos, found := divineChaosRate(stamps, byHour)
	if !found {
		slog.Warn("currency-exchange: no divine/chaos market in the window; divine-quoted plays are dropped",
			"league", league, "hours", n)
	}
	result.DivineChaosRate = divineChaos

	merged := make(map[string]*aggregate)
	for _, stamp := range stamps {
		hour, hourRows := hourAt[stamp], byHour[stamp]

		candidates := directCandidates(hourRows, cfg)
		candidates = append(candidates, crossQuoteCandidates(hourRows, cfg)...)

		seen := make(map[string]bool, len(candidates))
		for _, c := range candidates {
			if seen[c.key] {
				continue
			}
			seen[c.key] = true

			a, known := merged[c.key]
			if !known {
				a = newAggregate(c, hour)
				merged[c.key] = a
			}
			a.add(c, hour)
		}
	}

	minHoursSeen := cfg.MinHoursSeen
	if minHoursSeen > n {
		minHoursSeen = n
	}
	for _, key := range sortedIDs(merged) {
		if play, ok := merged[key].play(cfg, minHoursSeen, divineChaos); ok {
			result.Plays = append(result.Plays, play)
		}
	}

	sort.SliceStable(result.Plays, func(i, j int) bool {
		a, b := result.Plays[i], result.Plays[j]
		switch {
		case a.RoiPct != b.RoiPct:
			return a.RoiPct > b.RoiPct
		case a.Turnover != b.Turnover:
			return a.Turnover > b.Turnover
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

// divineChaosRate returns the chaos value of one divine over the window: the
// median across the window's hours of the divine/chaos market's VWAP.
//
// The rate is a window-level scalar rather than a per-play lookup so that two
// plays quoted in divine are valued identically, and it is measured from the
// same table everything else is (198.97 c/div over the reference window) rather
// than hard-coded — the number moves with the league.
//
// found is false when the market did not trade in any of the window's hours; the
// caller drops the divine-quoted plays rather than pricing them at a guess.
func divineChaosRate(stamps []int64, byHour map[int64][]Row) (rate float64, found bool) {
	rates := make([]float64, 0, len(stamps))
	for _, stamp := range stamps {
		for _, r := range byHour[stamp] {
			if pairKey(r.ItemA, r.ItemB) != pairKey(DivineID, ChaosID) {
				continue
			}
			if hourly, ok := vwapIn(r, DivineID, ChaosID); ok {
				rates = append(rates, hourly)
			}
			break
		}
	}
	if len(rates) == 0 {
		return 0, false
	}
	return median(rates), true
}

// chaosPerUnit values one unit of a quote currency in chaos.
//
// Chaos is 1 by definition and divine is the window's measured rate. Any other
// id — a market where neither side is a listed currency, or divine when the rate
// is unknown — returns 0, meaning "this play cannot be valued in chaos", and the
// caller drops it: a ranking denominated in chaos cannot honestly hold a row
// whose payout is in scarabs.
func chaosPerUnit(quote string, divineChaos float64) float64 {
	switch quote {
	case ChaosID:
		return 1
	case DivineID:
		return divineChaos
	default:
		return 0
	}
}

// aggregate accumulates one key's candidates across the window's hours.
//
// legs carry the recipe and the NEWEST hour's observation; samples[i] holds
// every hour's observation of leg i in that same order. BestPlays walks hours
// newest first, so the first candidate merged in is normally the newest — but
// lastHour records which hour the newest fields came from and add only replaces
// them for a strictly newer one, so the aggregate does not depend on that order.
// Every candidate sharing a key has the same number of legs: the key spells out
// the mode and, for a triangle, the route.
type aggregate struct {
	key        string
	mode       Mode
	legs       []candidateLeg
	samples    [][]obs
	newestEdge float64
	lastHour   time.Time
	hoursSeen  int
}

// newAggregate starts an aggregate from the first hour a key was seen in,
// keeping that hour's recipe, stock and edge until add sees a newer one.
func newAggregate(c candidate, hour time.Time) *aggregate {
	return &aggregate{
		key:        c.key,
		mode:       c.mode,
		legs:       append([]candidateLeg(nil), c.legs...),
		samples:    make([][]obs, len(c.legs)),
		newestEdge: c.edge,
		lastHour:   hour,
	}
}

// add folds one hour's observation in. Hours are equal citizens for the medians:
// the aggregation carries no recency weight. The newest-hour fields — the recipe
// legs with their stock, newestEdge and lastHour — are the exception, and they
// move only when hour is strictly newer than the one already recorded, so they
// hold the newest hour's reading whatever order the hours arrive in.
func (a *aggregate) add(c candidate, hour time.Time) {
	if hour.After(a.lastHour) {
		a.legs = append(a.legs[:0:0], c.legs...)
		a.newestEdge = c.edge
		a.lastHour = hour
	}
	for i, leg := range c.legs {
		a.samples[i] = append(a.samples[i], leg.obs)
	}
	a.hoursSeen++
}

// play turns the accumulated hours into a Play, or reports ok == false when any
// gate rejects it.
//
// Order matters only for reading: a play must clear every gate. The prices come
// out first because everything else — RoiPct, Roi, Investment — is arithmetic on
// them, which is the property that makes the shown legs and the claimed return
// the same statement.
func (a *aggregate) play(cfg Config, minHoursSeen int, divineChaos float64) (Play, bool) {
	if a.hoursSeen < minHoursSeen {
		return Play{}, false
	}

	legs := make([]Leg, len(a.legs))
	depth, turnover, tick, entryRate := 0.0, 0.0, 0.0, 0.0
	for i, recipe := range a.legs {
		samples := a.samples[i]

		side := func(o obs) float64 { return o.low }
		if recipe.action == "sell" {
			side = func(o obs) float64 { return o.high }
		}

		legs[i] = Leg{
			Action: recipe.action,
			Item:   recipe.item,
			Quote:  recipe.quote,
			Price:  medianOf(samples, side),
			Fair:   medianWhere(samples, func(o obs) (float64, bool) { return o.vwap, o.vwapOK }),
			Tick:   medianOf(samples, func(o obs) float64 { return o.tick }),
			Volume: medianOf(samples, func(o obs) float64 { return o.volume }),
			Stock:  recipe.obs.stock,
		}

		rate := chaosPerUnit(recipe.quote, divineChaos)
		if rate <= 0 {
			return Play{}, false
		}
		legTurnover := medianOf(samples, func(o obs) float64 { return o.quoteVolume }) * rate

		if i == 0 {
			depth, turnover, tick, entryRate = legs[i].Volume, legTurnover, legs[i].Tick, rate
			continue
		}
		depth = math.Min(depth, legs[i].Volume)
		turnover = math.Min(turnover, legTurnover)
		tick = math.Max(tick, legs[i].Tick)
	}

	roiPct, ok := roiPctOf(a.mode, legs)
	if !ok {
		return Play{}, false
	}
	investment := legs[0].Price * entryRate
	roi := investment * roiPct

	switch {
	case roiPct < cfg.MinEdge,
		turnover < cfg.MinTurnoverChaos,
		tick > cfg.MaxTick,
		roiPct < cfg.MinEdgeTickRatio*tick,
		roi < cfg.MinROIChaos:
		return Play{}, false
	}

	return Play{
		Key:              a.key,
		Mode:             a.mode,
		Legs:             legs,
		RoiPct:           roiPct,
		Edge:             roiPct,
		RoiPctNewestHour: a.newestEdge,
		Roi:              roi,
		Investment:       investment,
		Turnover:         turnover,
		Tick:             tick,
		Depth:            depth,
		HoursSeen:        a.hoursSeen,
		LastHour:         a.lastHour,
	}, true
}

// roiPctOf recomputes a play's return from the prices its legs show, so the
// percentage is reproducible by anyone reading the response:
//
//	direct: sell / buy - 1
//	1-hop:  (sellXinB * sellBinA) / buyXinA - 1
//
// ok is false for a shape with the wrong number of legs or a zero entry price,
// neither of which the candidate builders can produce — the check is there so a
// future third shape cannot silently divide by zero.
func roiPctOf(mode Mode, legs []Leg) (float64, bool) {
	switch {
	case mode == ModeDirect && len(legs) == 2 && legs[0].Price > 0:
		return legs[1].Price/legs[0].Price - 1, true
	case mode == ModeOneHop && len(legs) == 3 && legs[0].Price > 0:
		return legs[1].Price*legs[2].Price/legs[0].Price - 1, true
	default:
		return 0, false
	}
}

// medianWhere is medianOf over the observations that HAVE the field, skipping
// the rest. Leg.Fair uses it because an hour whose quote side reported no volume
// has no VWAP at all: feeding its 0 into the median would drag the fair anchor
// toward zero and make a live market look like a free one. When no hour in the
// window carried the field the result is 0 — the same "no anchor" reading a leg
// gets when nothing ever cleared.
func medianWhere(samples []obs, field func(obs) (float64, bool)) float64 {
	values := make([]float64, 0, len(samples))
	for _, s := range samples {
		if v, ok := field(s); ok {
			values = append(values, v)
		}
	}
	return median(values)
}

// medianOf returns the median of one field across a leg's hourly observations.
func medianOf(samples []obs, field func(obs) float64) float64 {
	values := make([]float64, 0, len(samples))
	for _, s := range samples {
		values = append(values, field(s))
	}
	return median(values)
}

// median returns the middle value of values, or the mean of the two middle ones
// when the count is even, matching the percentile_cont(0.5) the measurements
// were made with. It sorts a copy, so the caller's slice keeps its order. An
// empty input is 0.
func median(values []float64) float64 {
	if len(values) == 0 {
		return 0
	}
	sorted := append([]float64(nil), values...)
	sort.Float64s(sorted)

	mid := len(sorted) / 2
	if len(sorted)%2 == 1 {
		return sorted[mid]
	}
	return (sorted[mid-1] + sorted[mid]) / 2
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
