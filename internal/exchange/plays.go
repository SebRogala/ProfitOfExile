package exchange

import (
	"log/slog"
	"math"
	"sort"
	"time"
)

// Mode names the shape of a play: how many markets it touches.
//
// A direct play is a same-market flip (buy low, sell high, roiPctRaw =
// high/low - 1). A one-hop play is a three-leg triangle through a second quote
// currency (roiPctRaw = highXinB * highBinA / lowXinA - 1). Both formulas are
// the RAW return on the hour's observed extremes; RoiPct, what the gates and the
// ranking use, is the same round trip after each leg pays its tick. Ranking
// prefers the direct shape when everything else ties, because it carries one
// execution risk instead of three.
type Mode string

const (
	// ModeDirect is a same-market flip: buy and sell the same item on the same
	// market within the hour.
	ModeDirect Mode = "direct"
	// ModeOneHop is a three-leg triangle: buy the item against one currency,
	// sell it against another, convert the proceeds back.
	ModeOneHop Mode = "1-hop"
)

// Leg is one executed step of a play on one market, as ONE feed hour observed
// it — the hour named by Play.LastHour, and no other.
//
// Price is that hour's realized extreme on the leg's side — the cheapest price
// for a buy, the dearest for a sell — expressed in Quote units per one Item, and
// it is RAW: the undercut a real order needs lives in Play.RoiPct and Play.Roi,
// never in this number. Reading it against Fair, the hour's volume-weighted
// average price, is how a quantization artefact is told from an opportunity, and
// Suspect is that comparison already made (see Config.SuspectLowBand). Tick says
// how far apart two representable prices are on this market, as a fraction, so a
// client can rebuild the undercut price: Price*(1+Tick) to buy, Price*(1-Tick)
// to sell.
type Leg struct {
	// Action is "buy" or "sell", from the player's point of view.
	Action string `json:"action"`
	// Item is the raw feed id being traded. POE-175's handler runs it through
	// Humanize; the engine never carries display names.
	Item string `json:"item"`
	// Quote is the raw feed id of the currency Price and Fair are denominated in.
	Quote string `json:"quote"`
	// Price is Quote units per one Item: the hour's low on a buy leg, the
	// hour's high on a sell leg. Raw, not undercut.
	Price float64 `json:"price"`
	// Fair is the hour's volume-weighted average price of one Item in Quote
	// units — where the hour's mass traded, between the two extremes. It is 0
	// when the hour's quote side reported no volume, which is a missing reading
	// rather than a free item; FairOK is what tells the two apart.
	Fair float64 `json:"fair"`
	// FairOK says whether the hour had a volume-weighted price at all. When it
	// is false Fair is 0 and Suspect is false: with no anchor there is nothing
	// to call the extreme suspect against.
	FairOK bool `json:"fairOk"`
	// Tick is this market's price resolution in the hour, as a fraction of the
	// price: 0.5 means the next representable price is 50% away. See tickOf for
	// why it is the strongest predictor of a fake spread.
	Tick float64 `json:"tick"`
	// Volume is the units of Item traded on this market in the hour.
	Volume float64 `json:"volume"`
	// Stock is the market's highest stock of Item in the hour. Liveness only —
	// it is an hourly max of total book size and does not describe the extreme
	// (measured corr <= 0.13 against the edge).
	Stock int64 `json:"stock"`
	// Suspect marks an extreme too far from Fair to trade on: a buy leg whose
	// low is under Fair*Config.SuspectLowBand, a sell leg whose high is over
	// Fair*Config.SuspectHighBand. Measured on 221 liquid chaos markets over 24
	// hours, extremes sit 11-13% off the hour's VWAP at the median hour and 50%
	// at p90, so a leg outside the bands is one trade nobody could repeat rather
	// than a price. The play is still served — the reader judges it — but it
	// ranks after every clean one.
	Suspect bool `json:"suspect"`
}

// Play is one ranked opportunity: ONE hour's PRICES, plus two cross-hour
// readings of how the recipe has been behaving.
//
// Every price-shaped number a Play carries comes from a single feed hour, the
// one LastHour names — and for a served play that is the window's NEWEST hour,
// the last snapshot, because BestPlays drops any recipe that did not clear its
// gates there. No price is blended across hours, because a blend is a trade
// nobody could have made: Mawr Blaidd/Chaos printed lows of 62-81 chaos in four
// consecutive hours against a VWAP near 250, which is why a price shown here
// belongs to one hour.
//
// Two fields are deliberately not prices and do read across hours, each labelled
// as what it is. HoursSeen counts the window hours in which the recipe cleared
// the gates on that hour's own prices, so a one-hour ghost and a play that has
// held all day are told apart by a count. ExpectedRoi is the fill simulation's
// mean outcome over the last day of entries (sim.go): what posting these orders
// would have realized, as opposed to what the hour printed. Neither is a price
// and neither replaces one — the legs, RoiPct, Roi and Investment stay the last
// snapshot's, and ADR-016 scopes the exception.
//
// RoiPct, Roi, Investment, Tick and Depth are arithmetic on that one hour's
// legs — a ratio, a product, a min or a max across them — so a client can
// recheck them from what it shows; Turnover is the same hour's quote-side volume
// valued in chaos, which the legs do not carry. The simulation's four —
// ExpectedRoi, ExpectedRoiPct, SimEntries and LowCoverage — are the fields a
// client CANNOT recheck from the row, because their inputs are hours the row
// does not carry.
type Play struct {
	// Key identifies the recipe across hours: "direct:<marketID>" or
	// "1-hop:<item>|<buyQuote>|<sellQuote>".
	Key string `json:"key"`
	// Mode is the play's shape.
	Mode Mode `json:"mode"`
	// Legs are the steps in execution order: two for direct, three for 1-hop.
	Legs []Leg `json:"legs"`
	// RoiPct is the fractional gain of one round trip at the UNDERCUT prices,
	// 0.05 meaning +5%: each leg pays one of its own ticks to be the order that
	// fills (buy at Price*(1+Tick), sell at Price*(1-Tick)). It is what the
	// gates and the ranking use, because it is the return an order that
	// actually gets taken can expect.
	RoiPct float64 `json:"roiPct"`
	// Edge is RoiPct under its old name, kept on the wire for clients written
	// before POE-184.
	//
	// Deprecated: read RoiPct. The two always carry the same value.
	Edge float64 `json:"edge"`
	// RoiPctRaw is the same round trip at the raw observed extremes — what the
	// hour printed, with nothing paid for the fill. It is always at least
	// RoiPct, and the gap between them is what the ticks cost: on a coarse
	// market it eats the whole spread.
	RoiPctRaw float64 `json:"roiPctRaw"`
	// Roi is chaos gained per exchange of ONE unit at the undercut prices:
	// = Investment * RoiPct. Both factors are priced at the undercut entry, so
	// the identity holds by construction and a client can check the payout
	// against the outlay without knowing which tick was paid.
	Roi float64 `json:"roi"`
	// Investment is the chaos per unit at the UNDERCUT entry price: what one
	// unit costs to enter on the first leg after paying that leg's tick to be
	// the order that fills. Leg.Price stays RAW — this is Price*(1+Tick) on a
	// buy entry, valued in chaos at the hour's rate.
	Investment float64 `json:"investment"`
	// Turnover is chaos per hour flowing through the play's thinnest leg — the
	// hour's quote-side volume valued in chaos. It is the liquidity reading;
	// unit volume is not one (corr(ln edge, ln units) = +0.06 against -0.30 for
	// chaos turnover).
	Turnover float64 `json:"turnover"`
	// Tick is the coarsest price resolution among the legs: the worst step the
	// recipe has to live with.
	Tick float64 `json:"tick"`
	// Depth is the thinnest leg's Volume — the units per hour the whole recipe
	// can absorb.
	Depth float64 `json:"depth"`
	// Suspect is true when ANY leg is: one unrepeatable extreme is enough to
	// make the whole percentage a story. Suspect plays rank after every clean
	// one, and Config.HideSuspect drops them instead.
	Suspect bool `json:"suspect"`
	// HoursSeen counts the window hours in which the recipe cleared every gate
	// on THAT hour's own prices. A low count on a wide window is a ghost: an
	// edge that showed up once.
	HoursSeen int `json:"hoursSeen"`
	// ExpectedRoi is the chaos one exchanged unit ACTUALLY returned, averaged
	// over the simulated entries of the last Config.SimWindowHours hours: post
	// this play's undercut orders in each of those hours, chase the buy, wait for
	// the sell, fire-sale what never sold, and value each entry's outcome at its
	// OWN hour's divine rate (sim.go). It is the ranking's headline number and
	// the honest counterpart of Roi, which is one hour's optimistic reading —
	// measured over 960 top-20 play-hours the displayed number overstates the
	// outcome by 4-8x. It can be NEGATIVE, and such plays are still served and
	// ranked rather than hidden, per ADR-015's serve-and-flag principle.
	ExpectedRoi float64 `json:"expectedRoi"`
	// ExpectedRoiPct is the same mean expressed as a fraction, 0.05 meaning +5%:
	// the fractional counterpart of RoiPct. It is a mean of per-entry fractions
	// rather than ExpectedRoi divided by Investment, so the RoiPct/Roi identity
	// does NOT carry over — each entry has its own chased outlay, and the two
	// means are taken separately.
	ExpectedRoiPct float64 `json:"expectedRoiPct"`
	// SimEntries is how many hours the two means are averaged over: the sim
	// window's hours in which this recipe produced a gated candidate that could
	// be valued and had at least one later hour to fill in. It is the sample
	// size, and a mean of one entry is a story rather than an expectation.
	SimEntries int `json:"simEntries"`
	// LowCoverage says SimEntries fell under Config.MinSimEntries (capped at the
	// hours that could have been entries). The play is served either way — it
	// simply ranks after every covered one, because "we could not measure this"
	// and "we measured this and it is bad" are different claims and only the
	// second deserves a place in the order.
	LowCoverage bool `json:"lowCoverage"`
	// LastHour is the hour every price above was read from, UTC. For a served
	// play it is always the window's NEWEST hour: BestPlays drops a recipe that
	// did not clear in the last snapshot, so a stale row can never present
	// itself as a current price. HoursSeen, not this field, is what says how
	// long the recipe has been holding up.
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
	// DivineChaosRate is the NEWEST hour's divine/chaos VWAP, the current chaos
	// value of one divine. Because a served play must have cleared in that hour,
	// it IS the rate every play in the list was valued at; the older hours' rates
	// govern only whether a recipe counted towards HoursSeen. It is 0 when the
	// newest hour had no divine/chaos trade.
	DivineChaosRate float64 `json:"divineChaosRate"`
	Plays           []Play  `json:"plays"`
}

// Config tunes the window, the gates, the junk bands and the ranking cut.
//
// Every gate below still exists and still binds when it is set; what changed in
// POE-191 is which of them DefaultConfig arms. The server's job is to serve
// everything sane and flag the rest, so its defaults keep four things:
// MinVolumePerHour drops legs nobody traded (liveness), MinHoursSeen drops
// ghosts (persistence), MinEdge drops the round trips that lose or break even
// (the sanity floor), and MaxPlays bounds the payload. The four QUALITY gates —
// MinTurnoverChaos, MaxTick, MinEdgeTickRatio, MinROIChaos — default to off and
// are the client's call, because "too small to bother with" is a judgement about
// the reader's bankroll and not a fact about the market. Each of them still
// answers a real failure mode measured over 26 hours of Allflame, and the desktop
// ships those measured levels as its own defaults: MinTurnoverChaos drops markets
// too small for the edge to be real (p50 robust edge is 242% under 100 chaos/hour
// and 18% over 100k), MaxTick and MinEdgeTickRatio drop the spreads that are one
// integer price step wide, and MinROIChaos drops plays whose percentage is fine
// and whose payout is a rounding error. The suspect bands do not gate at all by
// default: they flag an extreme that sits too far from its hour's VWAP to be
// repeatable, and only HideSuspect turns the flag into a filter. QuotePriority
// decides which side of a market reads as the currency (see orient).
type Config struct {
	// WindowHours is how many of the newest distinct hours to aggregate. It
	// binds only a direct BestPlays call: the served path overlays it per
	// horizon (see DefaultHorizons and horizonConfig in service.go).
	WindowHours int
	// MinVolumePerHour is the per-leg liveness floor on units traded in an
	// hour. It is not the liquidity gate — MinTurnoverChaos is.
	MinVolumePerHour float64
	// MinEdge is the smallest RoiPct kept, 0.001 meaning +0.1%, judged on the
	// UNDERCUT return. It is the sanity floor rather than a quality gate: it
	// drops the round trips that lose or barely break even, and how much gain
	// is worth acting on is the client's call. A zero value means "use the
	// default"; a negative value lowers the floor further. It still cannot
	// surface a losing route, because withDefaults clamps MinEdgeTickRatio and
	// MinROIChaos to at least 0 and Investment is positive, so a negative RoiPct
	// makes Roi negative and fails Roi >= MinROIChaos whatever MinEdge says.
	MinEdge float64
	// MinTurnoverChaos is the smallest Play.Turnover kept, in chaos per hour.
	// Default 0, which is off: no turnover can fall under it.
	MinTurnoverChaos float64
	// MaxTick is the coarsest Play.Tick kept, as a fraction of the price.
	// Default 1, which is off: tickOf is 1/max of two positive integer
	// quantities and so can never exceed 1.
	MaxTick float64
	// MinEdgeTickRatio is how many price steps wide a spread must be to count:
	// RoiPct >= MinEdgeTickRatio * Tick. Default 0, which is off — at a ratio of
	// 0 the comparison degenerates to RoiPct >= 0.
	MinEdgeTickRatio float64
	// MinROIChaos is the smallest Play.Roi kept, in chaos per exchanged unit.
	// Default 0, which is off for every profitable play and is also the
	// structural bar on a losing one: Roi >= 0 is Roi's sign, and Roi and RoiPct
	// share it because Investment is positive.
	MinROIChaos float64
	// SuspectLowBand is how far under an hour's VWAP a buy leg's low may sit
	// before it reads as junk: 0.67 flags a low below two thirds of fair.
	SuspectLowBand float64
	// SuspectHighBand is the same on the sell side: 1.5 flags a high above one
	// and a half times fair.
	SuspectHighBand float64
	// HideSuspect drops a play in every hour where any leg's extreme is
	// suspect, instead of serving it flagged and ranked last. Default false:
	// the owner would rather see a flagged row than a missing one, and a hidden
	// play cannot be argued with.
	HideSuspect bool
	// MinHoursSeen is the smallest number of window hours a play must have
	// cleared its gates in. It is capped at the number of hours actually
	// present, so a single-hour window still returns plays. Like WindowHours it
	// binds only a direct BestPlays call; the served path overlays it per
	// horizon (see DefaultHorizons and horizonConfig in service.go).
	MinHoursSeen int
	// SimWindowHours is how many of the newest distinct hours the fill
	// simulation draws its entry hours from. It is INDEPENDENT of WindowHours:
	// the ranking window says how long a recipe must have been clearing, the sim
	// window says how much history the expectation is averaged over, and the
	// horizons deliberately share the second one so both serve the same
	// ExpectedRoi for the same recipe.
	SimWindowHours int
	// SimLookaheadHours is how far past an entry hour the simulation looks for
	// the fills, h+1..h+SimLookaheadHours truncated at the newest hour. An entry
	// needs at least one later data hour inside it, which is why the newest hour
	// is never an entry.
	SimLookaheadHours int
	// MinSimEntries is how many simulated entries a play needs before its
	// expectation is treated as covered; below it Play.LowCoverage is true and
	// the play ranks after the covered ones. It is capped at the number of hours
	// that could have been entries, the same way MinHoursSeen is capped at the
	// hours present, so a young league does not flag everything.
	MinSimEntries int
	// MaxPlays truncates the ranked result. It is the payload guard, not a
	// gate: with the quality gates off the served list is the whole sane set,
	// and this only stops a feed anomaly from publishing an unbounded one.
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
// traded per leg per hour, a 0.1% ROI floor, no turnover demand, no tick cap, no
// edge-to-tick demand, no chaos-per-unit demand, junk bands at two thirds and
// one and a half times fair with the flag shown rather than hidden, cleared in
// at least two hours, the top five hundred plays, quoted in divine before chaos.
//
// The four quality levels this used to carry — 10,000 chaos/hour of turnover, a
// tick no coarser than 10%, an ROI at least five steps wide, three chaos per
// exchanged unit — moved to the client in POE-191, which ships them unchanged as
// the desktop's own default knobs. The out-of-the-box view is therefore the same
// view; what changed is that a user can now lower one of them and see what it
// was hiding, without a redeploy. The measured reason to want that: on 2026-08-20
// the newest hour carried 1368 markets, 881 of them clearing both-side liveness,
// and the four levels served 79 plays. Sacrifice fragments failed three of them
// at once (~0.2-1 chaos payout, ~4k chaos/hour of turnover, one-step prices) and
// the divine leg's tick kept the 1-hop count at zero — all of it invisible to
// the reader, because a dropped play cannot be argued with.
//
// What the server still refuses to serve is what no reader would want under any
// bankroll: a leg nobody traded (MinVolumePerHour), a recipe that has not held
// up (MinHoursSeen), a round trip that loses or breaks even (MinEdge, the
// positivity floor — 0.1% rather than 0 so that a play whose entire "gain" is
// float noise is not a row), and more rows than a payload should carry
// (MaxPlays). Suspect extremes are flagged and ranked last rather than dropped.
//
// The levels the client inherited are derived from the 26-hour Allflame
// measurement in POE-184, whose robust edge was the MEDIAN OF PER-HOUR EDGES:
// that gate set takes 908 markets to 135, and the survivors' top 25 are all
// full-window with 29-71% robust edge at 0.1-8.7% ticks. The engine does not
// gate on that statistic. It gates on ONE hour's prices, hour by hour, and
// counts the hours that cleared — which is why the SQL's 908->135 is a
// calibration of the levels, not a prediction of how many plays come back.
//
// Turning those levels off did NOT bring the measured noise cases back. Divine
// Vessel (109 chaos/hour) and Delirium Scarab both print a 100% tick, so their
// undercut sell price is Price*(1-1) = 0 and the round trip returns -100%: junk
// that coarse fails the positivity floor on its own arithmetic, which is the
// answer to "what stops the relaxed defaults from serving POE-184's noise".
//
// The band levels come from the same feed read the other way round: across 221
// liquid chaos markets over 24 hours the hour's extremes sit 11-13% off its VWAP
// at the median hour and 50% at p90, so 0.67 and 1.5 sit just outside the p90 of
// ordinary noise and catch the 62.5-chaos low under a 245-chaos VWAP that
// started POE-188.
//
// WindowHours is the base window, which the Service overrides per horizon (see
// Horizons); the ranking a client sees comes from one of those, not from this.
//
// The three sim knobs are the odd ones out: a day of entry hours, a three-hour
// lookahead and a twelve-entry coverage guard are CALIBRATION-LOCKED, not
// tuning. They are the parameters ExpectedRoi was validated at against five real
// owner outcomes (POE-193), so changing one invalidates the measurement rather
// than adjusting a preference — which is why, unlike every knob above, they take
// no environment override and moving them is a deliberate redeploy.
func DefaultConfig() Config {
	return Config{
		WindowHours:      6,
		MinVolumePerHour: 10,
		MinEdge:          0.001,
		MinTurnoverChaos: 0,
		MaxTick:          1,
		MinEdgeTickRatio: 0,
		MinROIChaos:      0,
		SuspectLowBand:   0.67,
		SuspectHighBand:  1.5,
		HideSuspect:      false,
		MinHoursSeen:     2,
		MaxPlays:         500,
		QuotePriority:    []string{DivineID, ChaosID},
		Horizons:         DefaultHorizons(),

		SimWindowHours:    24,
		SimLookaheadHours: 3,
		MinSimEntries:     12,
	}
}

// withDefaults fills each unset field from DefaultConfig independently, so a
// caller can set one knob and inherit the rest.
//
// A non-positive count, floor or cap (WindowHours, MinVolumePerHour,
// MinTurnoverChaos, MaxTick, MinEdgeTickRatio, MinROIChaos, SuspectLowBand,
// SuspectHighBand, MinHoursSeen, SimWindowHours, SimLookaheadHours,
// MinSimEntries, MaxPlays) reads as unset, because none of them has a meaningful
// negative value. MinEdge is different: only an exact 0 reads
// as unset, so a caller can push the percentage floor below zero and see returns
// the floor hides (the sign bar below still applies). HideSuspect is a bool and
// has no unset state: false is a choice, and it is also the default.
//
// Three of those branches no longer restore anything, and that is the point.
// MinTurnoverChaos, MinEdgeTickRatio and MinROIChaos default to 0 since POE-191,
// so for them "unset" and "off" are the same number and passing 0 cannot
// resurrect an old level; MaxTick's default of 1 is likewise off, since a tick
// can never exceed 1. What those three branches still do is CLAMP: a caller who
// passes a negative gate value gets 0, which is what keeps MinEdgeTickRatio * Tick and
// MinROIChaos at or above zero and so keeps a losing route unservable however
// MinEdge is set. Do not drop them for looking like no-ops.
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
	if c.SuspectLowBand <= 0 {
		c.SuspectLowBand = d.SuspectLowBand
	}
	if c.SuspectHighBand <= 0 {
		c.SuspectHighBand = d.SuspectHighBand
	}
	if c.MinHoursSeen <= 0 {
		c.MinHoursSeen = d.MinHoursSeen
	}
	if c.SimWindowHours <= 0 {
		c.SimWindowHours = d.SimWindowHours
	}
	if c.SimLookaheadHours <= 0 {
		c.SimLookaheadHours = d.SimLookaheadHours
	}
	if c.MinSimEntries <= 0 {
		c.MinSimEntries = d.MinSimEntries
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
// are kept. Each hour is then scored ALONE: direct flips and one-hop triangles
// are built from that hour's rows, each leg gated on that hour's traded volume
// and stock, and evaluate turns each surviving candidate into a whole Play — leg
// prices, undercut return, chaos payout, gates and all — out of that one hour.
// No PRICE mixes hours. The merge by recipe key keeps two things and nothing
// else: the NEWEST hour's Play, which is what gets served, and a COUNT of the
// hours that cleared, which becomes HoursSeen and is what MinHoursSeen judges.
//
// One reading is deliberately taken across hours, and it is not a price. Over a
// SECOND window — the newest Config.SimWindowHours hours, independent of the
// ranking one — sim.go posts each recipe's undercut orders hour by hour and
// reads the fills out of the hours that followed; the mean outcome becomes
// ExpectedRoi/ExpectedRoiPct over SimEntries entries, and it is what the ranking
// sorts on. The two windows are cut from the same newest-first list and share
// nothing else, so both horizons serve the same expectation for the same recipe
// (ADR-016).
//
// A play is then served only if it cleared in the window's newest hour — the
// last snapshot. The readings are deliberately separate: the PRICES are the last
// snapshot's and nothing older, while PERSISTENCE is a count over the whole
// window and the EXPECTATION is a simulation over the sim window. A recipe that
// cleared four hours ago and has not cleared since is not a current price and
// does not appear, however many hours it once held; one that cleared in the
// last snapshot appears with HoursSeen saying how long it has been holding up.
// Because the newest-hour check compares against the window's newest hour
// rather than against whatever arrived last, the result does not depend on the
// order rows come in.
//
// The measured reason for that split: four consecutive hours on Mawr
// Blaidd/Chaos printed lows of 62.5-81 chaos against a VWAP near 250, which is
// why a price shown here belongs to one hour, the one LastHour names.
//
// Chaos valuation is per hour too: divineRateOf reads each hour's own
// divine/chaos VWAP, so a play quoted in divine is valued at the rate of the
// hour it was observed in, and an hour without that market simply does not clear
// its divine-quoted candidates. A play whose quote is neither chaos nor divine —
// scarab/scarab, card/scarab and every other cross-item market — is dropped
// outright, because gates denominated in chaos (MinTurnoverChaos, MinROIChaos)
// cannot be evaluated against a payout measured in scarabs. Result.DivineChaosRate
// is that newest hour's rate, which — because every served play cleared in that
// hour — is the rate every one of them was valued at, and the warning fires only
// when NO hour in the window carried that market — once per HORIZON, so twice
// per Service recompute.
//
// What comes back has passed, per hour: RoiPct >= MinEdge (on the UNDERCUT
// return, so a coarse tick that eats the spread fails here), Turnover >=
// MinTurnoverChaos, Tick <= MaxTick, RoiPct >= MinEdgeTickRatio * Tick, and
// Roi >= MinROIChaos; and then, across the window, HoursSeen >= MinHoursSeen
// (capped at the hours present). At DefaultConfig only the first of the five
// binds — the other four are off, and the quality judgement they used to make is
// the client's since POE-191 — so what the engine serves is every recipe that is
// alive, has held up, and gains something in its hour. It is ranked clean before
// suspect, then covered before low-coverage, then ExpectedRoi desc, then
// Turnover desc, then direct before 1-hop, then Key ascending, and truncated to
// MaxPlays. Neither LowCoverage nor a negative ExpectedRoi hides a play: both
// sort it down and leave it readable, which is ADR-015's serve-and-flag rule
// applied to the simulation. The ranking is a stable sort over a key-sorted
// list, so identical input always produces identical output regardless of the
// order rows arrive in.
//
// The caveat that governs the percentages: the feed publishes each hour's
// realized low and high, not a book. Both prices are trades that happened inside
// the same hour, and nothing says both were takeable at once — that is what the
// undercut charges for, what Suspect flags when an extreme is too far from the
// hour's VWAP to be repeatable, and what the client's tick knobs bound at their
// defaults (the server's own tick gates are off since POE-191). This returns raw
// item ids; POE-175's handler runs them through Humanize.
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
	// Newest hour first, so stamps[0] names the hour Result.DivineChaosRate is
	// read from and both cuts below count back from the last snapshot.
	sort.Slice(stamps, func(i, j int) bool { return stamps[i] > stamps[j] })

	// Two windows are cut from that one list, and they are read INDEPENDENTLY.
	// The RANKING window is the newest WindowHours hours: which recipes clear,
	// how many hours they cleared in, and whose prices are served. The SIM window
	// is the newest SimWindowHours hours and feeds nothing but the fill
	// simulation, which is why a six-hour horizon and a day horizon carry the
	// SAME ExpectedRoi for a recipe — the expectation is a property of the feed,
	// not of how long the reader wants a play to have been holding up. span is
	// whichever window reaches further, so the hours are walked once.
	rankHours := min(len(stamps), cfg.WindowHours)
	simHours := min(len(stamps), cfg.SimWindowHours)
	span := stamps[:max(rankHours, simHours)]

	result.Hours = rankHours
	result.From = hourAt[stamps[rankHours-1]]
	result.To = hourAt[stamps[0]].Add(time.Hour)

	anyRate := false
	merged := make(map[string]*aggregate)
	series := make(map[string]*simSeries)
	for i, stamp := range span {
		hour, hourRows := hourAt[stamp], byHour[stamp]
		// An hour past the ranking window is here for the simulation alone: it
		// contributes fill prices and nothing to HoursSeen, to the served prices
		// or to the rate warning below, whose subject is the ranked window.
		ranking := i < rankHours

		hourRate, hourRateOK := divineRateOf(hourRows)
		if ranking && hourRateOK {
			anyRate = true
		}
		if i == 0 {
			result.DivineChaosRate = hourRate
		}

		candidates := directCandidates(hourRows, cfg)
		candidates = append(candidates, crossQuoteCandidates(hourRows, cfg)...)

		seen := make(map[string]bool, len(candidates))
		for _, c := range candidates {
			if seen[c.key] {
				continue
			}
			seen[c.key] = true

			// The simulation records the candidate BEFORE the gates: an hour
			// whose percentage was too small to serve is still an hour the orders
			// could have been posted in, and skipping it would average only the
			// flattering hours. It records only the sim window's own hours: span
			// is the WIDER of the two cuts, so when the ranking window is the
			// longer one the hours past simHours are here for the ranking alone
			// and feeding them to the simulation would widen the sim window with
			// the horizon — the one thing the shared expectation depends on not
			// happening.
			if i < simHours {
				recordSim(series, c, hour, hourRate, hourRateOK)
			}
			if !ranking {
				continue
			}

			play, ok := evaluate(c, hour, hourRate, hourRateOK, cfg)
			if !ok {
				continue
			}
			a, known := merged[c.key]
			if !known {
				a = &aggregate{}
				merged[c.key] = a
			}
			a.add(play)
		}
	}
	if !anyRate {
		slog.Warn("currency-exchange: no divine/chaos market in the window; divine-quoted plays are dropped",
			"league", league, "hours", rankHours)
	}

	// An entry needs a later hour to fill in, so the sim window's newest hour can
	// never be one: the hours that COULD have been entries are one fewer than the
	// hours present, and that is what caps the coverage guard.
	sims := simulate(series, simHours-1, cfg)

	minHoursSeen := cfg.MinHoursSeen
	if minHoursSeen > rankHours {
		minHoursSeen = rankHours
	}
	// stamps[0] is the window's newest hour, fixed before any aggregation, so
	// this cut is order-independent.
	newestHour := hourAt[stamps[0]]
	for _, key := range sortedIDs(merged) {
		a := merged[key]
		if a.hoursCleared < minHoursSeen {
			continue
		}
		// The prices a served play shows must be the last snapshot's. A recipe
		// that cleared only in older hours is persistence without a current
		// price, and is dropped rather than served stale.
		if !a.newest.LastHour.Equal(newestHour) {
			continue
		}
		play := a.newest
		play.HoursSeen = a.hoursCleared
		// A served play cleared evaluate in the newest hour, so it produced a
		// gated, valuable candidate there and simulate holds an outcome under its
		// key. That outcome may report no entries, which is a measured fact about
		// the recipe (LowCoverage) rather than a missing lookup.
		outcome := sims[key]
		play.ExpectedRoi = outcome.expectedRoi
		play.ExpectedRoiPct = outcome.expectedRoiPct
		play.SimEntries = outcome.simEntries
		play.LowCoverage = outcome.lowCoverage
		result.Plays = append(result.Plays, play)
	}

	// Clean before suspect, measured before unmeasured, then the best SIMULATED
	// payout — not the hour's optimistic one, which is what the ranking used
	// before POE-193 and which the measurement puts 4-8x too high.
	sort.SliceStable(result.Plays, func(i, j int) bool {
		a, b := result.Plays[i], result.Plays[j]
		switch {
		case a.Suspect != b.Suspect:
			return !a.Suspect
		case a.LowCoverage != b.LowCoverage:
			return !a.LowCoverage
		case a.ExpectedRoi != b.ExpectedRoi:
			return a.ExpectedRoi > b.ExpectedRoi
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

// divineRateOf returns the chaos value of one divine in ONE hour: that hour's
// divine/chaos market VWAP.
//
// It is called twice for different reasons. Per hour, it is the rate every
// divine-quoted candidate of that hour is valued at, so an hour is priced with
// its own exchange rate rather than with a window-wide one — the rate moves with
// the league, and pricing an old hour at today's rate is the same class of
// mistake as pricing a leg at another hour's low. Once more on the window's
// newest hour, it fills Result.DivineChaosRate, the current rate the client
// displays.
//
// found is false when that hour had no divine/chaos trade; the caller drops the
// hour's divine-quoted plays rather than pricing them at a guess, and reports
// the rate as 0.
func divineRateOf(hourRows []Row) (rate float64, found bool) {
	for _, r := range hourRows {
		if pairKey(r.ItemA, r.ItemB) != pairKey(DivineID, ChaosID) {
			continue
		}
		return vwapIn(r, DivineID, ChaosID)
	}
	return 0, false
}

// chaosPerUnit values one unit of a quote currency in chaos.
//
// Chaos is 1 by definition and divine is the hour's measured rate. Any other
// id — a market where neither side is a listed currency, or divine when the hour
// had no rate — returns 0, meaning "this play cannot be valued in chaos", and
// the caller drops it: a ranking denominated in chaos cannot honestly hold a row
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

// evaluate turns ONE hour's candidate into a finished Play, or reports
// ok == false when any gate rejects it in that hour.
//
// It is pure: everything it emits comes from c's per-leg observations, the
// hour's divine rate and cfg. Nothing about other hours reaches it, which is
// what makes "this play's prices are one hour's prices" a property of the code
// rather than a convention.
//
// Two price systems live here at once. The legs carry the RAW extreme, because
// that is what the feed observed and what the reader must be able to check
// against the game. The percentages are computed at the UNDERCUT prices — a buy
// posted one tick above the hour's low, a sell one tick under its high, each leg
// on its OWN tick — because an order at exactly the last realized price is
// behind every order already queued there. The gap between the two is
// RoiPctRaw - RoiPct, and on a coarse market it is the whole spread.
//
// hourRateOK false means the hour had no divine/chaos market; it is passed
// separately from hourRate so a zero rate cannot be mistaken for a real one, and
// it drops exactly the divine-quoted candidates (chaosPerUnit returns 0 for
// them) while leaving chaos-quoted ones alone.
func evaluate(c candidate, hour time.Time, hourRate float64, hourRateOK bool, cfg Config) (Play, bool) {
	if !hourRateOK {
		hourRate = 0
	}

	legs := make([]Leg, len(c.legs))
	raw := make([]float64, len(c.legs))
	undercut := make([]float64, len(c.legs))
	depth, turnover, tick, entryRate := 0.0, 0.0, 0.0, 0.0
	suspect := false

	for i, recipe := range c.legs {
		o := recipe.obs

		price, legSuspect, filled := o.low, false, o.low*(1+o.tick)
		if recipe.action == "sell" {
			price, filled = o.high, o.high*(1-o.tick)
		}
		if o.vwapOK {
			if recipe.action == "sell" {
				legSuspect = o.high > o.vwap*cfg.SuspectHighBand
			} else {
				legSuspect = o.low < o.vwap*cfg.SuspectLowBand
			}
		}

		legs[i] = Leg{
			Action:  recipe.action,
			Item:    recipe.item,
			Quote:   recipe.quote,
			Price:   price,
			Fair:    o.vwap,
			FairOK:  o.vwapOK,
			Tick:    o.tick,
			Volume:  o.volume,
			Stock:   o.stock,
			Suspect: legSuspect,
		}
		raw[i], undercut[i] = price, filled
		suspect = suspect || legSuspect

		rate := chaosPerUnit(recipe.quote, hourRate)
		if rate <= 0 {
			return Play{}, false
		}
		legTurnover := o.quoteVolume * rate

		if i == 0 {
			depth, turnover, tick, entryRate = o.volume, legTurnover, o.tick, rate
			continue
		}
		depth = math.Min(depth, o.volume)
		turnover = math.Min(turnover, legTurnover)
		tick = math.Max(tick, o.tick)
	}

	if suspect && cfg.HideSuspect {
		return Play{}, false
	}

	roiPct, ok := roiPctOf(c.mode, undercut)
	if !ok {
		return Play{}, false
	}
	roiPctRaw, ok := roiPctOf(c.mode, raw)
	if !ok {
		return Play{}, false
	}

	investment := undercut[0] * entryRate
	roi := investment * roiPct

	// The five configurable gates, all judged on this hour alone. At
	// DefaultConfig only MinEdge binds (POE-191 moved the quality levels to the
	// client); the other four sit at values nothing can fail, which also makes
	// the last two read as "Roi and RoiPct are non-negative" — the floor under
	// MinEdge that keeps a losing round trip out even when MinEdge is set below
	// zero.
	switch {
	case roiPct < cfg.MinEdge,
		turnover < cfg.MinTurnoverChaos,
		tick > cfg.MaxTick,
		roiPct < cfg.MinEdgeTickRatio*tick,
		roi < cfg.MinROIChaos:
		return Play{}, false
	}

	return Play{
		Key:        c.key,
		Mode:       c.mode,
		Legs:       legs,
		RoiPct:     roiPct,
		Edge:       roiPct,
		RoiPctRaw:  roiPctRaw,
		Roi:        roi,
		Investment: investment,
		Turnover:   turnover,
		Tick:       tick,
		Depth:      depth,
		Suspect:    suspect,
		LastHour:   hour,
	}, true
}

// roiPctOf computes a round trip's return from one price per leg, in execution
// order:
//
//	direct: sell / buy - 1
//	1-hop:  (sellXinB * sellBinA) / buyXinA - 1
//
// evaluate calls it twice on the same legs — once at the undercut prices for
// RoiPct, once at the raw extremes for RoiPctRaw — so the two percentages are
// the same statement about two price systems rather than two formulas.
//
// ok is false for a shape with the wrong number of legs or a non-positive entry
// price, neither of which the candidate builders can produce — the check is
// there so a future third shape cannot silently divide by zero.
func roiPctOf(mode Mode, prices []float64) (float64, bool) {
	switch {
	case mode == ModeDirect && len(prices) == 2 && prices[0] > 0:
		return prices[1]/prices[0] - 1, true
	case mode == ModeOneHop && len(prices) == 3 && prices[0] > 0:
		return prices[1]*prices[2]/prices[0] - 1, true
	default:
		return 0, false
	}
}

// aggregate is what one recipe key carries across the window's hours: the
// newest hour's finished Play and a count of the hours that produced one.
//
// It holds no prices of its own. Every hour is already a complete answer by the
// time it gets here (evaluate ran the whole arithmetic and every gate on that
// hour alone), so there is nothing to blend — only to pick the newest and to
// count. The one thing that does read across hours, the fill simulation, runs
// BESIDE this rather than through it, on its own window and on candidates that
// never had to clear a gate (sim.go): keeping it out of the aggregate is what
// stops a simulated outcome from leaking into a served price.
type aggregate struct {
	newest       Play
	hoursCleared int
}

// add folds in one hour that cleared. The kept Play is replaced only by a
// strictly newer hour, so which hour is served does not depend on the order the
// hours arrive in; every cleared hour raises the count.
//
// The zero aggregate has no Play, and a Play's LastHour is never the zero time
// (evaluate stamps it with the hour it scored), so the first add always takes.
func (a *aggregate) add(play Play) {
	if a.hoursCleared == 0 || play.LastHour.After(a.newest.LastHour) {
		a.newest = play
	}
	a.hoursCleared++
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
