package exchange

import (
	"sort"
	"time"
)

// The fill simulation: what a play's orders would ACTUALLY have realized, hour
// after hour, instead of what its hour printed.
//
// Play.RoiPct is one hour's low-to-high round trip at the undercut prices. It is
// honest about that hour and dishonest about the future: both extremes were
// realized somewhere inside the same hour and nothing says a posted order would
// have caught either. Measured over 960 top-20 play-hours across 48 hours of
// Allflame, the displayed percentage overstates the outcome by 4-8x and the
// MEDIAN top play never round-trips at its posted prices at all. Against five
// owner-observed real outcomes the simulation below lands at a serving-level MAE
// of 13.1 chaos where the displayed number sits near 122.
//
// So the simulation posts the play's orders in hour h and reads the outcome from
// the hours that followed, per recipe, over the newest Config.SimWindowHours
// hours: chase-buy the entry leg, post the sell, and fire-sale what never sold.
// The mean over those entries is Play.ExpectedRoi, and it is the only number in
// this package built from more than one hour's prices — a labelled cross-hour
// STATISTIC, like HoursSeen, never a price anyone is shown
// (docs/adr/016-expected-roi-is-a-cross-hour-simulation-displayed-prices-stay-single-hour.md).
//
// The calibration record is POE-193, and it was taken on DIRECT plays. One-hop
// routes run the same mechanics with the conversion leg valued at the fill
// hour's VWAP, uncalibrated: nothing in the measurement says a triangle behaves
// like a flip.

// simObs is what ONE hour observed about ONE recipe, reduced to the prices a
// fill simulation needs: where the entry leg could be bought, where the sell leg
// could be sold, and — for a triangle — what the conversion market paid.
//
// It is built from the same gated candidate the ranking scores (recordSim), so
// nothing here re-derives a price: entryLow/sellHigh/sellVwap and the ticks are
// the obs values gatedLeg already read through priceIn, vwapIn and tickOf.
//
// convVwap is 1 with convVwapOK true on a direct flip, because a same-market
// round trip converts its proceeds through the identity. That keeps one
// arithmetic path for both shapes instead of a branch per entry.
//
// entryRate is the chaos value of one unit of the ENTRY leg's quote in this
// hour (chaosPerUnit at the hour's divineRateOf), and valuable says the hour can
// be an entry at all: a quote with no chaos rate this hour cannot value what the
// entry realizes, so that hour is not simulable. Later hours need no rate — they
// contribute fill PRICES, and the entry hour's own rate is what values the
// outcome.
type simObs struct {
	hour time.Time

	entryLow  float64
	entryTick float64

	sellHigh   float64
	sellTick   float64
	sellVwap   float64
	sellVwapOK bool

	convVwap   float64
	convVwapOK bool

	entryRate float64
	valuable  bool
}

// simSeries is one recipe key's observations across the sim window, one per hour
// in which the recipe produced a gated candidate. The hours are the recipe's own
// DATA hours: an hour where a leg failed gatedLeg observed nothing about this
// recipe and is not a fill opportunity either.
type simSeries struct {
	obs []simObs
}

// simOutcome is one recipe's simulated expectation over the sim window: the mean
// realized outcome per entry, the count of entries the mean is over, and whether
// that count is too small to lean on.
type simOutcome struct {
	expectedRoi    float64
	expectedRoiPct float64
	simEntries     int
	lowCoverage    bool
}

// recordSim files one hour's gated candidate under its recipe key.
//
// It is called for EVERY candidate of every sim-window hour, before and
// independently of evaluate: a simulable entry hour is one where the recipe
// produced a gated candidate (all legs passed gatedLeg) and the entry quote can
// be valued in chaos that hour — not one where the play cleared MinEdge and the
// rest. An hour whose percentage was too small to serve is still an hour in
// which the orders could have been posted, and pretending otherwise would
// average only the flattering hours.
//
// The legs are read in execution order and mean the same thing in both shapes:
// legs[0] is the leg the entry buys, legs[1] the leg the sell posts on, and
// legs[2] — a triangle only — the conversion back. A shape with a different
// number of legs records nothing rather than mispricing one, the same guard
// roiPctOf carries.
func recordSim(series map[string]*simSeries, c candidate, hour time.Time, hourRate float64, hourRateOK bool) {
	if c.mode == ModeOneHop && len(c.legs) != 3 {
		return
	}
	if c.mode != ModeOneHop && len(c.legs) != 2 {
		return
	}
	if !hourRateOK {
		hourRate = 0
	}

	entry, sell := c.legs[0].obs, c.legs[1].obs
	o := simObs{
		hour:       hour,
		entryLow:   entry.low.price,
		entryTick:  entry.tick,
		sellHigh:   sell.high.price,
		sellTick:   sell.tick,
		sellVwap:   sell.vwap,
		sellVwapOK: sell.vwapOK,
		convVwap:   1,
		convVwapOK: true,
		entryRate:  chaosPerUnit(c.legs[0].quote, hourRate),
	}
	if c.mode == ModeOneHop {
		o.convVwap, o.convVwapOK = c.legs[2].obs.vwap, c.legs[2].obs.vwapOK
	}
	o.valuable = o.entryRate > 0 && o.entryLow > 0

	s := series[c.key]
	if s == nil {
		s = &simSeries{}
		series[c.key] = s
	}
	s.obs = append(s.obs, o)
}

// simulate runs the fill simulation over every recorded recipe and returns one
// expectation per key.
//
// entryHoursPresent is how many of the sim window's hours COULD have been an
// entry — the window's distinct hours minus the newest one, which has no later
// hour to fill in. It caps the coverage guard the same way MinHoursSeen is
// capped at the hours present, so a young league whose whole feed is six hours
// does not flag every play as low-coverage for failing to produce twelve entries
// that could not exist.
//
// The observations are sorted here rather than trusted from the caller, so the
// answer cannot depend on the order BestPlays happened to walk its hours in.
func simulate(series map[string]*simSeries, entryHoursPresent int, cfg Config) map[string]simOutcome {
	guard := cfg.MinSimEntries
	if guard > entryHoursPresent {
		guard = entryHoursPresent
	}

	outcomes := make(map[string]simOutcome, len(series))
	for key, s := range series {
		sort.Slice(s.obs, func(i, j int) bool { return s.obs[i].hour.Before(s.obs[j].hour) })

		sumPct, sumChaos, entries := 0.0, 0.0, 0
		for i := range s.obs {
			pct, chaos, ok := simulateEntry(s.obs, i, cfg)
			if !ok {
				continue
			}
			sumPct += pct
			sumChaos += chaos
			entries++
		}

		// Zero entries is its own clause because the cap can reach 0 and
		// `0 < 0` is false: unlike the MinHoursSeen idiom, whose window always
		// holds at least the one hour a served play cleared in, simEntries has
		// no floor — a feed of a single distinct hour offers no later hour to
		// fill in, so nothing was measured at all. Flagging that is the whole
		// point of the guard; letting it pass would rank an unmeasured recipe
		// above a measured one.
		outcome := simOutcome{simEntries: entries, lowCoverage: entries < guard || entries == 0}
		if entries > 0 {
			outcome.expectedRoiPct = sumPct / float64(entries)
			outcome.expectedRoi = sumChaos / float64(entries)
		}
		outcomes[key] = outcome
	}
	return outcomes
}

// simulateEntry posts the recipe's orders in obs[i] and reads what the following
// hours did with them. It returns the realized fraction, the chaos that fraction
// is worth on one unit, and ok == false when the hour cannot be an entry at all.
//
// The four steps, which are the calibrated estimator and not a place to
// improvise:
//
//  1. CHASE-BUY. The buy is posted at the entry hour's undercut price,
//     low*(1+tick). The first later data hour h' decides it: if that hour's low
//     came down to the posted price the buy filled there, and otherwise the
//     order is re-priced to h's own undercut low — a buyer who wants the unit
//     chases the market up rather than resting forever. The chased price is what
//     the rest of the trip is measured against, which is where most of the
//     displayed number goes.
//
//  2. POSTED SELL. The sell rests at the entry hour's high*(1-tick) and fills
//     the first later hour that printed a high at or above it, scanned STRICTLY
//     after h' — the same hour that filled the buy does not also fill the sell.
//     The window closes at Config.SimLookaheadHours past the entry, truncated at
//     whatever the newest recorded hour is.
//
//  3. FIRE-SALE EXIT. An unsold unit is not a hold, it is a loss taken later:
//     the exit prices at the lookahead's LAST data hour, halfway between that
//     hour's VWAP and its undercut high — a seller who has stopped waiting meets
//     the market between fair and the top. The realized fraction is allowed to
//     go negative, which is the point: across the calibration set 7.9% of PLAYS
//     came out negative in aggregate.
//
//  4. VALUATION. The fraction is turned into chaos at the ENTRY hour's own
//     divine/chaos rate, per this package's per-hour discipline: the outlay was
//     made in that hour, at that hour's rate, and repricing it at the newest
//     hour's rate would be the same mistake as showing another hour's low.
//
// An entry is skipped rather than guessed when the lookahead holds no later data
// hour for this recipe (so the newest hour is never an entry), when an unsold
// exit hour has no VWAP to meet the market halfway against, or when a triangle's
// conversion market did not price the fill hour. Each of those is a missing
// reading, and averaging a fabricated one in would move the headline number.
//
// The missing-VWAP skip is not symmetric, and that is worth saying out loud: it
// can only fire on the unsold branch, so it drops losing outcomes and errs
// optimistic. It stays rare because a leg that passed gatedLeg already traded
// volume that hour, which is what a VWAP is computed from.
func simulateEntry(obs []simObs, i int, cfg Config) (pct, chaos float64, ok bool) {
	e := obs[i]
	if !e.valuable {
		return 0, 0, false
	}

	// The lookahead is a half-open span of CLOCK hours, so a feed gap shortens
	// the trip instead of reaching further back for an hour to sell into.
	deadline := e.hour.Add(time.Duration(cfg.SimLookaheadHours) * time.Hour)
	last := i
	for j := i + 1; j < len(obs) && !obs[j].hour.After(deadline); j++ {
		last = j
	}
	if last == i {
		return 0, 0, false
	}

	fill := e.entryLow * (1 + e.entryTick)
	if chased := obs[i+1]; chased.entryLow > fill {
		fill = chased.entryLow * (1 + chased.entryTick)
	}
	if fill <= 0 {
		return 0, 0, false
	}

	posted := e.sellHigh * (1 - e.sellTick)
	exit, converted, sold := 0.0, obs[last], false
	for j := i + 2; j <= last; j++ {
		if obs[j].sellHigh >= posted {
			exit, converted, sold = posted, obs[j], true
			break
		}
	}
	if !sold {
		if !obs[last].sellVwapOK {
			return 0, 0, false
		}
		exit = (obs[last].sellVwap + obs[last].sellHigh*(1-obs[last].sellTick)) / 2
	}
	if !converted.convVwapOK {
		return 0, 0, false
	}

	// convVwap is 1 on a direct flip, so this is the same line for both shapes:
	// what one unit came back as, over what it cost to get.
	pct = exit*converted.convVwap/fill - 1
	return pct, fill * e.entryRate * pct, true
}
