---
uid: bbe8db0a-8fb2-4424-9c12-d8f800f1555d
---

# ADR-016: Expected ROI Is a Cross-hour Simulation; Displayed Prices Stay Single-hour

## Status

Accepted (POE-193, 2026-08-21), **amended twice**.

Amended 2026-09-01 (POE-220) — see
[Amended 2026-09-01](#amended-2026-09-01-poe-220): the third Consequence's "both
ranked below the plays that measured well" is true of the SERVED order only, and
no longer describes the desktop table.

Amended 2026-09-04 (POE-252) — see
[Amendment: thin pairs price from a trailing window](#amendment-thin-pairs-price-from-a-trailing-window-2026-09-04):
the single-hour doctrine this decision owns now has a SECOND exception, and the
Decision's third bullet below is scoped by it.

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
  what they were: one hour's, the hour `LastHour` names **on every play whose own
  scored hour priced it** (amended 2026-09-04, POE-252 — a pair whose scored hour
  traded under `ThinHourVolume` units prices its legs from a trailing clock window
  instead, and is MARKED `windowPriced` with the span it read; see the second
  amendment at the end). Nothing is discounted, clamped, or blended on the wire,
  and that stays literally true of a window-priced row: each of its prices is ONE
  hour's whole realized print carrying that row's own posted quantity pair, never
  an average, a median or a clamp over the window. "Not blended" is the property
  the Mawr Blaidd measurement was about, and it is the property the window path
  keeps. The optimistic pair remains recheckable by a client from the row it is
  shown in.

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

## Amendment: thin pairs price from a trailing window (2026-09-04)

Status of this section: **Accepted** (POE-252, owner decision 2026-09-04).
Current behaviour. The Decision above is not repealed; its third bullet is scoped
by this section, and the doctrine this ADR owns now has TWO exceptions rather
than one.

The decision above closed on "this decision is where its one exception is written
down". The exception it meant is the simulation: a labelled cross-hour STATISTIC
that is never a price. This one is different in kind — it IS a price, and what
moves is which hour it belongs to.

### The incident

Chaos ↔ Apocalypse (`Metadata/Items/DivinationCards/DivinationCardApocalypse`),
read out of prod on 2026-09-04. The in-game book showed roughly 550 buy / 1150
sell all day. What the feed published, per hour, as the card's price in chaos:

| hour (UTC) | cards | low | high |
| --- | --- | --- | --- |
| 09-04 17:00 | 1 | 552 | 552 |
| 09-04 16:00 | 2 | 511 | 1148 |
| 09-04 15:00 | 3 | 500 | 1148 |
| 09-04 14:00 | 1 | 500 | 500 |
| 09-04 13:00 | 3 | 487 | 1140 |
| 09-04 12:00 | 1 | 486 | 486 |
| 09-04 10:00 | 3 | 487 | 1196 |
| 09-04 09:00 | 1 | 1000 | 1000 |
| 09-04 08:00 | 0 | – | – |
| 09-04 07:00 | 2 | 1196 | 1196 |
| 09-04 06:00 | 8 | 485 | 1196 |
| 09-04 05:00 | 2 | 485 | 486 |
| 09-04 02:00 | 5 | 486 | 1200 |
| 09-04 00:00 | 31 | 485 | 1100 |
| 09-03 21:00 | 4 | 748 | 899 |
| 09-03 19:00 | 12 | 471 | 899 |
| 09-03 16:00 | 6 | 502 | 940 |

Seventeen published hours across twenty-six clock hours; the nine missing rows are
hours the feed published nothing for this market, and 08:00Z is a published hour
that traded nothing.

**At 17:00Z the served row read buy 552 / sell 552, ROI −0%.** That hour's only
trade was the owner's own purchase of one card. One trade cannot print a spread:
the low and the high collapse onto the same number, and the row reads as a dead
market while two sides are standing in the game's book. An hour earlier the same
pair served roughly +125%. And a market with no row in the scored hour was not
enumerated at all, so in the nine hours the feed published nothing for it the pair
was not merely mispriced — it was absent from the table entirely. A play that
flickers with the hour's trade count.

`ExpectedRoi` was already honest here (+8% for the flip): the fill simulation
reads hours the row does not carry, which is this ADR's whole subject. The
`RoiPct`/`Roi` columns and the wire ranking were the ones lying, and they lie
precisely where the feed is thinnest.

The feed publishes REALIZED hourly trades, not a book. That is why an hour with
one trade is not an hour with a narrow spread — it is an hour with no spread
measured, which is a different claim and the one the row was making wrong.

### Decision

**A pair whose scored hour is too thin to have printed a spread prices its legs
from a trailing clock window, marked with the span it read.**

- **Three `Config` fields, defaults 2 / 6 / 2, and no environment override.**
  `ThinHourVolume` (2) is the ITEM-side unit volume under which the scored hour is
  too thin to price; `WindowPriceHours` (6) is the span; `MinWindowVolume` (2) is
  the item-side volume the window must have traded, summed over the rows that
  priced. They take no env knob for a reason adjacent to the sim knobs' above:
  these decide what a served PRICE is, so a per-deployment value would mean two
  installations disagreeing about what the market printed.
- **The window is a CLOSED CLOCK span** `[h − (WindowPriceHours−1)h, h]`, not "the
  last six hours that traded". The span is six hours WIDE and closed at both ends,
  so the oldest print a row can carry is `WindowPriceHours−1` — five hours at the
  default, which is what §6.6 of the row invariant and `doc.go` both state — and
  that is the staleness it bounds
  whatever a market's gaps look like: a pair that traded six times in thirty hours
  is not priced off a thirty-hour-old print. A row contributes only when it cleared
  `MinVolumePerHour` and `priceIn` can price it, so a republished untraded hour
  cannot lend its stale ratios to the extremes.
- **Low is the window's lowest realized print and high its highest, each taken
  WHOLE off the single row that printed it**, with that row's own posted quantity
  pair. Nothing is averaged, medianed or clamped across the window — see the
  Decision's third bullet above, corrected in place.
- **The keying predicate is the scored hour's item-side unit volume**, not
  `LowLiquidity`. The task text named `lowLiquidity` as "the existing thin-hour
  seam"; in code that field is `roiPct < MinEdge`, a SPREAD predicate computed
  after pricing, and keying on it would fire the window path on every
  spreadless-but-thick market. The two remain independent readings on the wire, and
  the mechanism is what keeps them apart rather than a frequency: a row is
  window-priced BECAUSE its scored hour showed no spread, and `lowLiquidity` is
  then judged on the WINDOW's spread — which clears `MinEdge` unless the window is
  flat too. The feed also has
  no trade COUNT — `VolumeTraded` is units — so "trades" in the task text reads as
  units here.
- **The row carries the mark and its span**: `windowPriced`, `windowHours`,
  `windowVolume`, on the play and on each leg, rendered beside `low liquidity`.
- **`tick`, `LastHour` and the result's `DivineChaosRate` stay the SCORED hour's**
  on every leg that had a live hour of its own. `LastHour` is the hour the recipe
  was SCORED at and remains the window's newest hour for every served play; on a
  window-priced play it is NOT the hour the prices printed in, and the age of those
  prices is carried by `windowHours` and nowhere else.
- **Liveness and enumeration read the same window** (the owner's acceptance line:
  served in every hour with at least one trade in the window). A market with no row
  in the scored hour, or one that traded under `MinVolumePerHour`, is still
  enumerated and still served, priced window-only — window-RESCUED. Without this
  the measured pair still vanished in roughly half its hours and the incident was
  only half answered. `directCandidates` walks the scored hour's live rows first,
  exactly as before, then that hour's rescued present rows, then the remaining
  market ids the window index carries, in sorted order; the order is what keeps a
  live row winning its key and keeps the output deterministic. This reach is
  DIRECT plays only: a one-hop triangle is still enumerated from the scored
  hour's rows and needs all three markets present there (`crossquote.go` states
  the asymmetry); a present triangle leg that traded nothing can be rescued,
  an absent one cannot.
- **The stock DEMAND is unchanged** — ADR-017's second amendment stands, and the
  side a leg executes against must still be non-empty. What changes on a rescued
  hour is the stock READING: it comes from the newest CONTRIBUTING window row, so
  it is as old as the price it accompanies and no older, bounded by
  `WindowPriceHours`. `tick`, traded volume, quote volume and the prices come from
  that same row (`direct.go:319-323`). The leg's item/quote ORIENTATION does not:
  `orient` reads `span[0].Row` for `ItemA`/`ItemB` alone, which is constant across
  every row a market id carries, so any row of the span answers identically
  (`direct.go:241-249`). One row family, one hour, one story.
- **`ThinHourVolume: 0` disarms the whole feature** — pricing and rescue together,
  since both gate on the same `< ThinHourVolume` test. It is the one field a
  reviewer flips to prove the feature off.

### Consequences

- **The doctrine now has TWO exceptions, and this ADR says so.** The simulation is
  a cross-hour statistic that is never a price; the window is a price from a
  different hour than the row was scored in, bounded at `WindowPriceHours` and
  disclosed on the row. Both are recorded here, so a reader of this ADR is never
  left believing there is still exactly one.
- **The calibration is untouched, by construction.** `obs` carries the hour
  channels (`hourLow`, `hourHigh`, `hourVwap`, `hourVwapOK`) beside the priced
  ones, and `recordSim` reads those four plus `obs.tick` and the entry leg's quote
  id (`sim.go:114`) — nothing else off `obs`. `tick` carries no window reading on
  the hours that reach it: `recordSim` is called for HOUR-LIVE hours only
  (`plays.go:1061`), where the tick is the scored row's own. A window extreme
  therefore cannot reach `ExpectedRoi`, and
  `TestBestPlays_windowPricingDisarmed_leavesEverySimulationFieldBitIdentical`
  pins it. Sim entries are recorded for HOUR-LIVE hours only, window-priced or not,
  from those hours' own channels; a window-RESCUED hour records none, because it
  has no reading of its own to record.
- **No pre-existing served value moved.** A rescued hour also never counts toward
  `HoursSeen`, which keeps the meaning it always had: the hours the feed priced
  this recipe ON THAT HOUR'S OWN PRICES. The liveness relaxation reaches every
  market in the feed, not only the thin ones, so without those two exclusions
  every quiet hour in the corpus would have moved an `expectedRoi` and a
  `hoursSeen`. The wire goldens are byte-identical apart from the new keys and the
  new market's own plays.
- **The `minHoursSeen` floor is what yields for a window-priced row**, not
  `HoursSeen`. A market rescued in every ranking hour has `hoursCleared == 0` and
  would be dropped by a floor that defends PERSISTENCE against a one-off ghost. A
  window-priced row is not that ghost: its liveness is the window's, bounded at
  `WindowPriceHours`, floored at `MinWindowVolume` and disclosed on the row, so the
  reader is told how thin the evidence is instead of the engine guessing for them.
  The cut exempts a row window-priced in the scored hour. At default config the
  exemption is INERT — `WindowPriceHours` (6) equals the recent horizon's
  `WindowHours` (6) and both count by the same `MinVolumePerHour` predicate — and it
  is live only where the price window reaches back PAST the ranking window. It is
  defence for that configuration, not dead code, and a test builds it.
- **A window-priced row's ROI orders at face value.** The mark is in neither
  comparator — not this package's ranking and not the desktop's `sortPlays`
  (ADR-018) — and no default gate reads it (ADR-017, ADR-015). It marks; it does
  not order and it does not hide.
- **Such a row can ALSO carry `suspect`, and that is the band working.**
  `Leg.Fair` on a window-priced leg becomes the window's POOLED volume-weighted
  price, so the bands judge a window spread against that window's own mass. Expect
  it more often than on an hour-priced row, and for a structural reason rather than
  a defect: six hours of extremes bracket more than one hour's do, so the 0.67/1.5
  bands are crossed more readily by the very width that made the window worth
  showing. On the
  measured 17:00Z case the pooled fair is near 737 chaos against a 486/1148 spread,
  which puts both legs outside 0.67/1.5 — the same reading an hour-priced row
  showing that spread against that fair would get. A 2.4× spread genuinely is wide,
  and ADR-018 keeps the flagged row readable and orderable at face value.
- **`tick = 1/max(priceItemQty, priceQuoteQty)` is not a wire invariant, and was
  not one before this.** `tickOf` takes the COARSER of the source row's two
  quantity pairs while the pair beside a price is the pair THAT price was posted
  at. This amendment widens the ways the two can differ — on a rescued leg the step
  and the price can come from different window rows — without opening the gap.
- **Freshness is bounded and disclosed, never hidden.** The reader can be shown a
  spread that closed five hours ago. `windowHours` is the span behind the price and
  `windowVolume` the mass its extremes were drawn from, both on the row, and the
  desktop renders the span beside the mark. The alternative measured badly in the
  other direction: a row reading −0% on one trade is not more honest than a
  six-hour window that says it is six hours wide.
- **Where the row invariant records it:**
  `docs/CURRENCY-EXCHANGE-ROW-INVARIANT.md` §6.6, with the symbol-level
  consequences in §2 and the `fair`/`suspect` note at the end of §4. The equations
  of §3 are unchanged and close on such a row.
- **The Go tests carrying this**, in `internal/exchange`:
  `TestCorpus_apocalypseThinNewestHour_isPricedFromTheWindow` (the incident above,
  as a corpus scenario derived from the measured hours),
  `TestCorpus_apocalypseThickNewestHour_isPricedFromTheHourAndUnmarked`,
  `TestCorpus_thickMarkets_areNeverWindowPriced` (the POE-220 backtest — no
  pre-existing key window-prices),
  `TestCorpus_apocalypseWindow_isServedInAllSeventeenShifts` (the no-flicker
  acceptance),
  `TestCorpus_deadWindow_isNotServedInEitherHorizon` (the window is not a
  resurrection machine),
  `TestCorpus_windowRescuedHours_recordNoSimulationEntry`,
  `TestCorpus_livenessRelaxation_movesNoPreExistingValue`,
  `TestCorpus_marketRescuedPastTheRankingWindow_isStillServed` (the exemption's
  live configuration),
  `TestBestPlays_windowPricingDisarmed_leavesEverySimulationFieldBitIdentical` and
  `TestBestPlays_windowPricedMark_doesNotOrderTheServedList`. The desktop
  cross-layer tier carries the incident as
  `incident — Apocalypse card, thin newest hour priced from the window (POE-252)`
  in `desktop/src/lib/exchange/corpus.test.ts`.
