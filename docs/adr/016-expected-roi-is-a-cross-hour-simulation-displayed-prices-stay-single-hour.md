# ADR-016: Expected ROI Is a Cross-hour Simulation; Displayed Prices Stay Single-hour

## Status

Accepted (POE-193, 2026-08-21)

Amended 2026-09-01 (POE-220) — see the amendment at the end: the third
Consequence's "both ranked below the plays that measured well" is true of the
SERVED order only, and no longer describes the desktop table.

## Context

The Currency Exchange engine's headline number was `RoiPct`/`Roi`: one hour's
low-to-high round trip at the undercut prices, and until POE-193 it was also
what the ranking sorted on. It is honest about the hour it came from and
dishonest about the future. Both extremes were printed somewhere inside that
hour and nothing says a posted order would have caught either — least of all
both, on two different legs, in sequence.

Measured over 960 top-20 play-hours across 48 hours of Allflame (2026-08-19 to
2026-08-21), the displayed percentage overstates the realized outcome by 4-8x,
and the MEDIAN top play never round-trips at its posted prices at all.

The obvious cheap fix — keep the single-hour arithmetic and discount it — does
not work, and the reason matters more than the measurement. The error is not
that the extremes are too far apart; it is that they usually do not RECUR, for
both legs, inside the window a trade would live in. A static transform scales a
number that should sometimes be absent. Two variants were measured and both
failed:

- The suspect bands, at 0.67/1.5, catch nothing on the served set. The plays
  that are wrong are not the ones the suspect flag catches.
- Clamping the leg extremes toward the VWAP left a large positive bias at every
  band tried. The best clamp found, 0.89/1.11, still ran about 2.5x optimistic.

What separates a play that realizes from one that does not is whether later
hours came back to its prices — a question no transform of one hour's numbers
can answer, and the only one a reader actually has.

## Decision

The headline number and the ranking move to a fill-simulated per-recipe
expectation. The displayed prices do not move at all.

- `Play.ExpectedRoi` / `ExpectedRoiPct` is the mean outcome of POSTING the
  play's undercut orders, simulated per recipe over each of the newest
  `SimWindowHours` (24) data hours in which it produced a gated candidate, and
  reading the fills out of the hours that followed. `SimEntries` is the count
  the mean is over; `LowCoverage` says that count fell under the guard — itself
  capped at the hours that could have been entries — or that nothing was
  measured at all.
- The ranking sorts Suspect asc, then LowCoverage asc, then ExpectedRoi desc,
  then Turnover desc, then direct before 1-hop, then Key asc. Note the
  dimension: the key is the chaos expectation, not the fraction — the ranking
  moved from a percentage to a payout, so a bigger stake outranks a better rate.
- `RoiPct`, `Roi`, `Investment`, the leg prices, Tick and Depth stay exactly
  what they were: one hour's, the hour `LastHour` names. Nothing is discounted,
  clamped, or blended on the wire. The optimistic pair remains recheckable by a
  client from the row it is shown in.

The estimator, normatively (`internal/exchange/sim.go`): CHASE-BUY the entry leg
at the entry hour's `low*(1+tick)`, re-priced to the first later data hour's own
undercut low if that hour's low did not come down to the posted price; POSTED
SELL at the entry hour's `high*(1-tick)`, filled by the first hour strictly
after the buy that printed a high at or above it, inside h+1..h+3 truncated at
the newest hour; MID FIRE-SALE for what never sold, at the lookahead's last data
hour, halfway between that hour's VWAP and its undercut high. Each entry's
realized fraction is valued in chaos at ITS OWN hour's divine rate.

**This is a scoped exception to the single-hour doctrine, not its repeal.** That
doctrine ("no price mixes hours", measured against Mawr Blaidd/Chaos printing
lows of 62.5-81 chaos against a VWAP near 250) lives in code prose — `doc.go` and
`plays.go` — and never had an ADR of its own; this decision is where its one
exception is written down. The simulation is a labelled cross-hour STATISTIC,
in the same category as `HoursSeen`, and never a price anyone is shown or asked
to trade at.

**The parameters are calibration-locked and take no environment override.**
`SimWindowHours` (24), `SimLookaheadHours` (3) and `MinSimEntries` (12) are
`Config` fields with defaults and no env knobs, deliberately: these are the
values the measurement below was taken at, and an operator override would
silently invalidate it while the number kept looking authoritative. Changing one
is a deliberate redeploy plus a re-calibration, not a deploy-time setting.

## Consequences

- **Calibration record.** Against five owner-observed real outcomes the
  simulation lands at a serving-level MAE of 13.1 chaos, where the displayed
  number sits near 122. Across the 960-play-hour set the aggregate
  realized/displayed ratio is 40.7% and 7.9% of plays realize NEGATIVE. POE-193
  carries the full record — the ground truths, the rejected transforms, and the
  parameter sweep. This ADR is a summary of it, not a substitute for it.
- **The calibration covers DIRECT plays.** A 1-hop route runs the same mechanics
  with the conversion leg valued at the fill hour's VWAP of the A/B market, and
  is uncalibrated: nothing in the measurement says a triangle behaves like a
  flip. Stated as such in `sim.go`.
- **This is not a quality gate, and ADR-015 stands unchanged.** Nothing new is
  hidden. A low-coverage play and a negative-expectation play are both SERVED,
  both flagged, and both ranked below the plays that measured well **in the
  SERVED order** (amended 2026-09-01, POE-220 — the ranking clause of this
  bullet and of the Decision's second bullet describes the wire order and no
  client table; see the amendment at the end and
  [ADR-018](018-flags-mark-they-never-order.md)) — "we could
  not measure this" and "we measured this and it loses" are different claims and
  the reader gets to see both, which is exactly [ADR-015](015-exchange-quality-gates-live-client-side-the-server-serves-everything-sane.md)'s
  serve-and-flag principle. The server's gate set is untouched, the client's
  Gates knobs still judge the optimistic `roiPct`/`roi` they were calibrated
  against, and the quality judgement stays the reader's.
- **A known one-sided optimism, accepted.** An unsold entry whose fire-sale hour
  carries no VWAP is skipped rather than guessed. That skip can only fire on the
  unsold branch, so it drops losing outcomes and errs optimistic. It stays rare
  because a leg that passed `gatedLeg` already traded volume in that hour, which
  is what a VWAP is computed from. Documented at the skip in `sim.go`; the
  alternative — fabricating an exit price — would move the headline number.
- **The expectation is a property of the feed, not of a horizon.** The sim
  window is independent of the ranking window and is not part of a horizon
  overlay, so both horizons carry the same `ExpectedRoi` for the same recipe.
  `Service.widestWindow` counts `SimWindowHours`, so the one hypertable read
  still covers everything.
- **Four fields a client cannot recheck.** `ExpectedRoi`, `ExpectedRoiPct`,
  `SimEntries` and `LowCoverage` are built from hours the row does not carry.
  `RoiPct`, `Roi`, `Investment`, `Tick` and `Depth` remain arithmetic on the
  legs the row ships; `Turnover` and `HoursSeen` never were, and the `Play`
  type's doc block says so.

## Evidence

- `internal/exchange/sim.go` — the estimator, its four steps, and the skip
  conditions.
- `internal/exchange/plays.go` — the two windows, the ranking comparator, and
  the Play doc block that separates prices from cross-hour readings.
- `internal/exchange/doc.go` — the engine narrative and the gate/env registry.
- `internal/exchange/sim_test.go` — fill, chase, fire-sale, negative-expectation,
  per-hour valuation and coverage-guard capping tests.
- `internal/exchange/plays_test.go` — the ranking bands and the chaos-key
  tie-breaks.
- POE-193 — the calibration record: five ground truths, the n=960 sweep, and the
  rejected static transforms.
- [ADR-015](015-exchange-quality-gates-live-client-side-the-server-serves-everything-sane.md)
  — the serve-and-flag principle this decision extends rather than amends.

## Amended 2026-09-01 (POE-220)

Status of this section: current behaviour. Nothing in the Decision moves; one
Consequence is scoped.

**"Ranked below the plays that measured well" was always a statement about the
SERVED order, and a reader would have read it as a statement about the table.**
The Decision's ranking bullet (Suspect asc, LowCoverage asc, ExpectedRoi desc, …)
still describes exactly what `internal/exchange` emits on the wire. It never
described what the desktop draws, and since POE-220 it demonstrably does not:
every desktop Currency Exchange sort orders by the figure its own column prints
and by nothing else, so a suspect or low-coverage play sits wherever its number
puts it, marked and unmoved. The served order survives only as a tie-break.

The rule that replaces the client half is
[ADR-018](018-flags-mark-they-never-order.md); the incident that forced it is
recorded there.
