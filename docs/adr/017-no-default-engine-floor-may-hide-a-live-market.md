# ADR-017: No Default Engine Floor May Hide a Live Market

## Status

Accepted (POE-193, 2026-08-22), **amended the same day** — see
[Amendment: MinEdge is a flag](#amendment-minedge-is-a-flag-2026-08-22), which
completes the rule by demoting the last default-on drop and therefore replaces
the final Decision bullet below. Extends
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
  **Superseded hours later by the amendment below: only the first of the two
  remains, and `MinEdge` is now a flag.**

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
- **A client-side price floor now exists, and it is the one sanctioned default
  filter** (POE-196, 2026-08-22). Keeping sub-chaos noise out of a view is a
  reader-side concern by ADR-015's split — it filters what one reader is shown
  and changes nothing about what the server hands the next one — and the
  sanctioned home for it is the desktop's existing filter chain, where it landed
  as a sixth Gates knob, `minItemPrice` in
  `desktop/src/lib/exchange/filters.ts`. It ships at **0.5 chaos** and is the
  ONLY default-on filter on either side of the wire: it drops a play whose
  `investment` — the per-unit chaos cost of entering one exchange — is under
  that, which removes the bottom of the sub-chaos tier from the out-of-the-box
  table. The rule above is not weakened by it. What the rule bars is a default
  that hides a market the reader could ACT ON, and the claim here is about
  absolute size, not about the market being fake: the predicate never reads
  `tick`, a sub-chaos market quotes as finely as any other, and an entry under
  half a chaos is simply a payout per flip too small to be worth the repeats it
  would take. The exception is bounded by being visible and undoable, which is
  the price of it: the knob sits on the Gates row with the other five, its
  placeholder shows the shipped level where theirs show `off`, the counter
  attributes the rows it takes, the empty-table message names the floor by
  number when the floor is what emptied it, and typing 0 turns it off. Blanking
  the box restores the shipped level — `''` means "whatever this build ships"
  for every gate knob, and this is the knob that makes that property
  load-bearing.
- **The level is 0.5 and not 1, deliberately** (owner call, 2026-08-22).
  ADR-015's own motivating example was Sacrifice fragments at ~0.2–1c — the
  owner's real flips, which that ADR moved the gates client-side to stop hiding.
  A 1c floor would have hidden them again by default. At 0.5 the fragment and
  oil tier stays on the table from half a chaos up, and only its bottom goes;
  the reader who wants that bottom back types 0.

## Amendment: MinEdge is a flag (2026-08-22)

The decision above kept `MinEdge` as one of two surviving default-on drops, on
the reading that a round trip which loses or gains nothing but float noise "is
not an opportunity at all". Measured against a live market the same day, that
reading was wrong, and the rule this ADR is named for was the thing it broke.

### The incident

The Apocalypse card recipe
(`Metadata/Items/DivinationCards/DivinationCardApocalypse` against
`CurrencyRerollRare`) vanished from `/api/currency-exchange/plays` while the
owner was flipping it by hand. In the 07:00 hour the market traded **2 cards at a
single 223:1 print**. One price for the whole hour means the round trip pays two
ticks against no spread, so the newest hour's undercut return was **−0.89%**;
`case roiPct < cfg.MinEdge` dropped that hour's candidate, and the newest-hour
rule — which refuses to serve a recipe that did not clear in the last snapshot —
then deleted the whole recipe. **Five of the window's other six hours had shown
70–92%.** The row a reader would have judged in a second was gone, with no
symptom to notice it by.

This is the same failure class as the `MinVolumePerHour` and `MinHoursSeen`
findings above: an absolute floor answering a question about a whole market with
one hour's number, in a market that is expensive enough to be quiet.

### Decision

**`MinEdge` is demoted from a drop to a flag, and it was the last default that
could hide a market.**

- A play whose newest hour PRICED the recipe and printed no exploitable spread
  is **served**, carrying its **measured** `roiPct` — which may be negative — and
  `Play.LowLiquidity` / `lowLiquidity: true` on the wire. The threshold is
  unchanged at 0.001 and `EXCHANGE_MIN_EDGE` still sets it; what changed is that
  raising it now MARKS more rows and hides none.
- **No replacement drop was added.** A recipe that never clears `MinEdge` in any
  window hour is served too. `ExpectedRoi` is what ranks it, and a recipe whose
  quiet hour is the exception keeps its place because the simulation reads the
  hours the flag does not.
- **The two payout gates apply only when armed above 0.** `MinEdgeTickRatio` and
  `MinROIChaos` default to 0, where the comparisons spell `RoiPct >= 0` and
  `Roi >= 0` — the positivity floor `MinEdge` was holding, under a second name.
  Left unguarded they would have gone on dropping exactly the −0.89% hour this
  amendment exists to show, and the demotion would have demoted nothing. Armed,
  they mean what they always meant, and a reader who types one is asking for a
  spread of a stated width or a payout of a stated size.
- **What still drops is only what cannot be PRICED**: nothing traded on a leg
  this hour, no stock on one side of the market (`gatedLeg`, `MinVolumePerHour`
  1), or an entry currency with no chaos rate that hour. `HideSuspect` remains
  opt-in and off.

### Consequences

- **A served row can now carry a negative `roiPct`, `roi` and `investment`-relative
  loss.** Clients must render a minus rather than treat it as an error state,
  which is the same contract `expectedRoi` already had under ADR-016.
- **POE-184's measured noise markets are served flagged rather than cut, and
  that is the accepted cost.** Divine Vessel (109 chaos/hour) and Delirium
  Scarab both print a 100% tick, so their undercut sell price is `Price*(1-1) = 0`
  and the round trip reads −100%. That arithmetic used to fail the positivity
  floor and was the standing answer to "what stops the relaxed defaults from
  serving POE-184's noise"; the answer is now the ranking, plus the two payout
  gates for a reader who wants the class gone.
- **`HoursSeen` widened.** It counts the window hours in which the recipe
  produced a servable play, and an hour with no spread now counts among them, so
  it reads "hours the feed priced this recipe" rather than "hours it was worth
  acting on". The narrower count is no longer reported by any field. It stays on
  one counter deliberately: `MinHoursSeen` judges the number the row displays, and
  a knob whose subject differed from the displayed count would contradict itself
  in front of the reader. Reporting the edge-hour count beside it is open
  follow-up work.
- **`EXCHANGE_MIN_EDGE` changed kind, not just level.** A deploy that had set it
  to tighten the served list now gets flags instead of removals. The removal knobs
  are `EXCHANGE_MIN_EDGE_TICK_RATIO` and `EXCHANGE_MIN_ROI_CHAOS`, and
  `cmd/server/main.go` says so at the override.
- **The Go tests that documented the old floor now document the flag**, in
  `internal/exchange/plays_test.go`: `..._minEdge_flagsTheReturnsUnderItWithoutDroppingThem`,
  `..._recipeWhoseNewestHourLostItsSpread_isStillServedFlagged` (the incident
  above, in miniature), `..._undercutReturnBelowZero_isServedFlaggedWithItsMeasuredLoss`,
  `..._losingRoundTrip_isRemovedOnlyByAnArmedPayoutGate` and
  `..._measuredNoiseMarkets_areServedFlaggedRatherThanCut`. The drop that remains
  keeps its own test, `..._recipeThatTradedNothingInTheNewestHour_isNotServed`.
- **ADR-016 is unaffected.** The simulation already recorded candidates BEFORE
  the served gates, so the quiet hours were always in the expectation; what
  changes is that the recipe they belong to now reaches the reader.
