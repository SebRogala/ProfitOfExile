# ADR-017: No Default Engine Floor May Hide a Live Market

## Status

Accepted (POE-193, 2026-08-22). Extends
[ADR-015](015-exchange-quality-gates-live-client-side-the-server-serves-everything-sane.md)
from the four QUALITY gates to the engine's own remaining floors, and
**supersedes ADR-015's first Decision bullet in whole** — the floor levels that
bullet listed as what the server keeps (MinVolumePerHour 10, a per-horizon
MinHoursSeen, MaxPlays 500) are not those levels any more.

## Context

ADR-015 moved the four absolute quality gates client-side and kept a short list
server-side as "not a matter of taste": per-leg liveness at 10 traded units an
hour, per-horizon persistence at 4-of-6 and 18-of-24 hours seen, the positivity
floor, and a 500-row payload cap. Those looked like facts about a market rather
than judgements about a reader.

Two of the three are not.

**Liveness.** Measured 2026-08-22 over 24 hours of the chaos/Apocalypse-card
market — the full measurement, stated here once and referenced from the code
rather than restated in it: hourly turnover ran **1,017–11,628 chaos** and the
**item side moved 3–43 units per hour**. `MinVolumePerHour = 10` dropped the
market in **11 of those 24 hours**. The market was not thin; the item was
expensive, so real money moved in few units. A unit count is a proxy for size,
and size is exactly the bankroll judgement ADR-015 handed to the reader.

**Persistence.** `BestPlays` already refuses to serve a recipe that did not
clear in the window's NEWEST hour, so the price on every served row is current
whatever `MinHoursSeen` is. Everything the persistence floor added on top was
the removal of rows whose `hoursSeen` would have said "3 of 6" out loud — a
number the reader can weigh, deleted before they could.

**The cap.** On the same date the recent horizon filled the old 500 exactly and
the truncation cut inside the flagged band, so plays the ranking had already
decided to show-and-flag were dropped by a payload bound instead. A guard that
binds is a gate under another name.

## Decision

**No default engine floor may hide a live market.**

- **Liveness = a trade happened.** `DefaultConfig().MinVolumePerHour` is **1**,
  beside `gatedLeg`'s unchanged demand for stock on BOTH sides of the market.
  Anything above 1 is a size preference and belongs to the reader.
- **Persistence and thinness are expressed by RANKING and FLAGS, never by
  default-on drops.** `hoursSeen`, `simEntries`/`lowCoverage` and `suspect` are
  served for every play; `MinHoursSeen` defaults to **1** on the base config and
  on both horizons, which cannot drop a served play at all.
- **The cap is a payload guard sized above the sane set.** `MaxPlays` is
  **2000**. If it is observed binding again, it is raised, not defended.
- **The levels survive as the recommended tightening.** 10 units an hour, 4-of-6
  and 18-of-24 hours seen, and ADR-015's four quality levels keep their measured
  rationale as what a reader or an operator should TYPE — through
  `EXCHANGE_MIN_VOLUME_PER_HOUR`, `EXCHANGE_RECENT_MIN_HOURS_SEEN`,
  `EXCHANGE_DAY_MIN_HOURS_SEEN`, `EXCHANGE_MAX_PLAYS` and the four quality knobs
  — not as what an untouched install applies. Every knob still accepts a
  positive value only, so a deploy can tighten and cannot loosen further.
- **The only default-on drops that remain** are the two that describe something
  that is not an opportunity at all: nothing traded on a leg this hour, and a
  round trip that loses or gains nothing but float noise (`MinEdge` 0.001, with
  `withDefaults` clamping the payout gates at ≥ 0 so a negative `EXCHANGE_MIN_EDGE`
  still cannot surface a losing route).

## Consequences

- The served set grows, at both horizons, and the growth is concentrated in thin
  and expensive markets — the ones the unit floor was hitting.
- **The fill simulation gains entries rather than losing them.** Candidates are
  recorded before the served gates, so an hour a thin recipe was previously
  dropped in now produces a simulable entry. The coverage guard
  (`MinSimEntries` 12) is UNCHANGED and keeps doing the trust labeling: more
  entries do not mean a trustworthy expectation, `lowCoverage` is what says
  whether it was measured on enough of them.
- `Config.MinVolumePerHour` and `Config.MinHoursSeen` now sit at the bottom of
  their expressible range. `withDefaults` reads a non-positive value as unset, so
  1 is both the default and the floor: callers can only tighten, and there is no
  way to ask for "no liveness check at all" — which is intended, since a leg with
  no trade has no price to serve.
- The Go test that documents the old levels
  (`TestBestPlays_recordedHourUnderTheOldServerLevels_yieldsNoOneHopRoutes`)
  now arms `MinVolumePerHour = 10` explicitly rather than inheriting it, since
  the default it used to inherit has moved; so do
  `TestDirectCandidates_itemVolumeExactlyAtAnArmedFloor_keepsTheCandidate` and
  its one-unit-under sibling. Tests that arm a level are the record of what a
  knob is FOR.
- **At these defaults the two horizons serve the same list.** Measured
  2026-08-22 on the local stack: recent and day both returned 664 plays in the
  same key order, differing only in each play's `hoursSeen` (n of 6 against n of
  24). The day horizon is therefore a near-no-op for a reader, and the second
  `BestPlays` pass buys one integer per row. Whether to collapse the two or to
  differentiate them again (a different sim window, a different ranking key) is
  open follow-up work, not settled here.
- **No client-side price floor exists today.** Keeping sub-chaos noise out of a
  view is a reader-side concern by ADR-015's split — it filters what one reader
  is shown and changes nothing about what the server hands the next one — and
  the sanctioned home for it is the desktop's existing filter chain. Building
  one is open work under POE-196; until it lands, a reader who wants that cut
  has the client gate knobs and nothing purpose-built.
