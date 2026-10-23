# Temple advisor feedback investigation — 2026-10-23

Status: historical investigation; section C decided and implemented by owner ruling,
2026-10-23. Historical observations below describe the reporting build.
Last verified: 2026-10-23 by source inspection and supplied screenshots/report;
Rust execution remains pending. Canonical for this investigation's evidence and
owner policy decision. Current rule semantics live in
[ADR-022's final amendment](../adr/022-room-values-are-chaos-denominated-and-market-fed-presets-are-default-and-custom.md).

## Evidence and limits

Observed: Vertolka's seven screenshots and accompanying HTML report; current
`advisor::rank`, `rules::architect_rules`, `Sim::rollout_from`,
`Sim::offered_change`, `RoomRank`, valuation and drop-table code. Original
market payload, settings, seed and standard errors were not supplied.
The image numbers below identify the supplied images, not repository fixtures.
No production identifiers or original captures are copied into this public log.

Inferred: interpreting those explanations through the current `rank` code,
no `ExpectedValue` reason and no veto imply both architects were inside the
noise band. This assumes the reporting build used the current ranking logic.
A printed R4 line alone does not prove R4 decided anything: rule evaluation
collects eligibility before ranking, and ranking may subsequently choose EV.

## Per-item diagnosis

1. **Storage, image4.** Observed: Warehouses E5 chosen at −1486.73 against
   Cultivar's 1.2c; R4, no EV reason. Inferred: byte 4 of the architect key
   selected upgrade `[1,0,0,0,1]` over change `[1,0,0,0,0]` inside noise.
   Verdict: policy gap, not a grade-pricing inversion; non-target means junk
   even when the competing line has a better grade or priced utility.
2. **Cellar, image6.** Observed: Strongbox E4 best mean −1492.43 versus Torment
   D2 −1504.59, gap 12.16; the full list has no EV reason. Observed code:
   both architect keys are `[1,0,0,0,0]`; D=5 and E=2 before live rescaling.
   Inferred: equal-key mean fallthrough inside noise caused the reversal.
   Verdict: missing grade tie-break, fixed without overriding separated EVs.
3. **Armourer's Workshop, image1.** Observed: Armoury 4.3c chosen at −1655.75
   against Reactor Plant 11c; R4, no EV reason. Inferred: byte-4 R4 preference
   inside noise, as in item 1. Verdict: defensible under the present policy;
   whether C versus B still permits pool removal is an owner policy decision.
4. **Tombs, image7.** Observed: Vault E3 no-door −1153.02 chosen over its own
   B0–B1 door option −1148.68. RU spends 4.34 score to protect the adjacent
   Sanctum's picks; this explains the door, not the kill. Both kill keys are
   `[1,0,0,0,0]` (B0–B1 is closed, so RC does not fire). Inferred: the kill
   repeats the fallback tie gap. Sacrifice D3's mean is absent, preventing an
   exact rival-score reconstruction. Verdict: fallback tie defect addressed;
   no claim of exact historical replay.
5. **Pools, image3.** Observed: Sanctum 36c chosen over Forge 41c at −737.07,
   explicitly by EV; R4 was also listed. Observed code: upgrade has a 50%
   double-tier roll, and `rollout_from` spends the current incursion before
   drawing any future one. Inferred: the tier-3 upside can explain the pick;
   with an explicit 0.6 fraction, 36c implies tier-3 60c and mean 48c.
   Verdict: defensible pick; last-incursion R4 eligibility and the contradictory
   explanation were bugs. The gate now follows the model's existing count
   interpretation; the parser alone cannot independently verify game timing.
6. **Vault, image5.** Observed: Treasury chosen at −1804.79; Hall of Lords
   0.75c, vial 1050c, roughly one in 840 runs. Inferred: R4 byte 4 chose within
   noise. Observed formula includes vial EV: `1050 × 0.1 × 20 / 1689 = 1.24334c`
   at tier 3. A 0.6 fraction yields 0.7460c, matching the displayed 0.75c;
   the fraction is inferred, not recovered from settings. Queen Atziri is a
   content label; her separate unique-drop pool is absent from this valuation.
   Verdict: vial EV already counted, separate content valuation gap; no change.
7. **Torment, image2.** Observed: Torture D2 chosen at −1978.67 over Defense
   Workshop's 17c; R4, no EV reason. Inferred: R4 byte 4 inside noise despite
   D versus B. Verdict: the same policy gap as item 1; no policy change made.

## Bounded changes

`StrategyProfile::grade_fallback` carries per-tier provenance from
`TempleProfileSettings::to_profile`. Inside a noise band containing only
fallback-valued offers, equal architect keys compare `RoomRank` before sampled
mean. This uses the order behind `pickOrder`, including within-letter position,
without changing any chaos rung. Measured/partial/instrumental/override offers,
vetoes, differing architect keys and separated EVs retain their precedence.
A grade reason makes that comparison visible.

`architect_rules` requires remaining > 1 for R4 and both carve-out branches.
The test board helper now states a five-incursion budget rather than silently
using `BoardState::empty`'s unread-budget zero. RC remains independent.
The EV sentence states that architect tie-breaks did not decide; eligible
architect rules and door rules can still be listed consistently.

Synthetic estimate tests isolate the ranking defect: D2/E4 with the report's
12.16 mean gap, D3/E3, within-letter E3/E4, outside-band EV, and a tier-specific
Custom override. They use the real valuation bridge and rule evaluator but
controlled estimates; they are not historical-board replays. A synthetic
last-incursion board exercises the 36c/41c double-tier choice. Rule tests cover
remaining 0, 1 and 2, including both carve-out outcomes, rule-key preferences
and vetoes. The tooltip wording has a direct output assertion.

## C. R4 junk cutoff — decided, 2026-10-23

Owner ruling: "That grade letter sounds good, if other rules are kept, and it
handles Vertolka cases, that's good one."

Decided and implemented: retain the non-target condition, and allow direct
R4 only when the change offer's letter is at most one band better than the
upgrade line's letter, using `RoomRank.tier` from `rooms::LINES`. This removes
R4 for item 1 E→C and item 7 D→B, and retains it for item 3 C→B. Positions and
chaos values do not affect the cutoff.

`score_options` is the seam: it knows both resolved offers, whereas
`architect_rules` evaluates one. `rules::limit_r4_by_grade` removes only the R4
preference and explanation before ranking; carve-out, remaining > 1, RT, RC,
reroll and grade-fallback behavior stay intact. Missing/unranked offers and
same-kind pairs preserve existing R4 behavior. The conditional second-door
pass contains one fixed architect, so its architect preference remains constant
across the door candidates.

This is a present-choice gate. `Sim::rollout` still upgrades non-target ordinary
lines under its existing future-turn policy. An EV-separated upgrade can still
win after losing R4; no changed historical winner is promised.

The existing R4/carve-out tests were left unchanged, including factory-B→museum-E
and hall-of-champions-C→museum-E comparisons. New tests cover the three reported
pairs, worse/equal change grades, one offer, each unranked side, same-kind pairs,
and preservation of both carve-out outcomes when E→C would suppress ordinary R4.
A separate regression pins the reroll preference byte and explanation under
the same grade gap.
They exercise the production scoring seam with zero rollouts to isolate the
rule decision from simulated future value. Rust execution and applied mutation
checks remain for the orchestrator; no Docker/cargo gate ran here.

## Incident accounting

**Incident 1 — fallback ties ignored the grade order · severity: medium**

- What: item 2's equal architect keys let a worse fallback grade win on a mean
  difference the band treated as indistinguishable; item 4 is consistent with
  the same mechanism.
- Root cause: Observed, valuation provenance was absent from the ranking
  profile and equal keys fell straight back to sampled mean. Inferred,
  line-specific rejection draws in `offered_change` desynchronise otherwise
  shared random streams and can contribute sampling differences; this was
  not isolated by a running reproduction.
- Caught by: player report, then source inspection; existing tests did not pin
  a better fallback grade against an adverse in-band mean.
- Prevention applied: provenance bridge, grade comparison, reason and synthetic
  regression tests in the current diff. Runtime validation remains pending.

**Incident 2 — final-incursion pool advice · severity: low**

- What: R4 was eligible on the final incursion although no future pool draw
  remained.
- Root cause: Observed, `architect_rules` did not read remaining; rollout did.
- Caught by: player report and comparison with `rollout_from`/`spend_incursion`.
- Prevention applied: shared count interpretation in the rule gate and boundary
  tests, including architect-key assertions rather than reasons alone.

**Incident 3 — contradictory EV explanation · severity: low**

- What: "no rule discriminated" appeared alongside an eligible R4 line.
- Root cause: Observed, reasons collected eligible rules before `rank` added
  its EV explanation, whose text conflated eligibility with deciding the kill.
- Caught by: player report and tracing reason collection through ranking.
- Prevention applied: accurate EV wording and an output regression assertion.

### What worked (the net held)

Observed: the reports retained scores and reasons, enough to distinguish the
architect and door axes. Existing provenance separates fallback from prices,
partial sums, instrumental values and overrides. The noise-band boundary gives
a bounded fix that preserves separated board effects. The brief's policy guard
kept the junk redefinition out of this patch.

### Upstream fixes (not mine)

No upstream code fix identified. The global test-contract paths under
`/root/repos/pipeforge` were unavailable; the explicit brief supplied installed
contracts, which were used instead. Rust runtime validation belongs to the
orchestrator's existing Docker gates, as the brief requires.

### Carry-forward checklist

1. Run desktop Rust tests/checks before accepting these uncommitted changes.
2. Execute mutation checks for the new grade comparison and remaining gate.
3. Keep fallback provenance per tier when bridging any new valuation source.
4. Keep the decided direct-choice grade gate separate from any future-policy change.
5. Retain market/settings/stderr when collecting a future exact-replay report.

Prevention homes: code and regression tests in this diff; normative rule
amendment in ADR-022; evidence, deferred validation and owner proposal in this
history entry. No proposed prevention is presented as an applied runtime gate.
