# ADR-015: Exchange Quality Gates Live Client-side; the Server Serves Everything Sane

## Status

Accepted (POE-191, 2026-08-20), then **amended in part 2026-08-21 (POE-193)**:
the split below stands, but the second Decision bullet is superseded in whole —
the desktop's gate knobs ship OFF and the levels are a documented recommendation.
See the dated note at the bottom.

**Amended again 2026-08-22 (POE-193):** the FIRST Decision bullet is superseded
in whole by
[ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md), which extends
this ADR's principle from the four quality gates to the engine's own floors. The
levels that bullet names as what the server keeps — MinVolumePerHour 10, a
per-horizon MinHoursSeen, MaxPlays 500 — are not the served defaults any more.
ADR-017's own same-day amendment finishes that bullet off: **the positivity
floor is gone too.** MinEdge is now where a play is FLAGGED (`lowLiquidity`), a
losing round trip IS served with its measured negative return, and `hoursSeen`
widened again to count every hour the feed priced the recipe. The split this ADR
decided is unchanged and is what ADR-017 argues from.

## Context

Until POE-191 the Currency Exchange engine applied five absolute quality gates
before serving anything: MinROIChaos 3c per exchange, MinTurnoverChaos
10,000c/h, MaxTick 0.10, MinEdgeTickRatio 5, and MinEdge 2%. Those levels were
tuned for one playstyle at one league phase. Measured against the owner's real
use they hid his actual flips: Sacrifice fragments (~0.2–1c a piece, ~4k c/h
market turnover, one-integer-step prices) failed three gates at once, and the
divine leg's coarse tick kept every 1-hop route at zero. Absolute cutoffs
cannot see league phase or intent; the reader can.

## Decision

The server serves everything sane and the quality judgement is the reader's.

- The server's own gates are reduced to: per-leg liveness
  (MinVolumePerHour 10 with stock standing on both sides), persistence
  (MinHoursSeen per horizon), the positivity floor (MinEdge 0.001 — a play
  that loses money or gains only float noise is never served, under any
  EXCHANGE_MIN_EDGE, because the withDefaults clamps keep the payout gates at
  ≥ 0), suspect flagging, and the MaxPlays 500 payload cap. The four quality
  gates default to off; their env knobs remain as operator-side sanity bounds
  that can only tighten the served set.
- The old levels became the desktop's default-on Gates knobs
  (`gateDefaults` in `desktop/src/lib/exchange/filters.ts`: 3 / 10,000 / 10% /
  5 / 2%): an EMPTY knob runs at the old server level, an explicit 0 turns it
  off. Out of the box the table therefore shows what the old server showed,
  plus the persistence superset below.
- **`hoursSeen` changed meaning**: an hour counts when the play was alive and
  cleared the positivity floor in it — not the full old gate stack — so the
  fraction reads higher than before and the served set is a superset of the
  old one. Tooltips state this at the control.

## Consequences

- Client-side gates run after the server's ranking and the MaxPlays
  truncation, so they can only narrow — a play the cap drops is gone however
  the knobs are set. Measured at adoption: 438 of the 500 cap.
- Future exchange surfaces (scale derivation, watchlists) inherit this split:
  they may narrow or annotate the served set, never re-gate it server-side
  per user.
- The four levels exist in two languages with no mechanical link:
  `gateDefaults` (TypeScript) and the migration-invariant test
  `TestBestPlays_recordedHourUnderTheClientsDefaultLevels_yieldsNoOneHopRoutes`
  (Go) must agree for "untouched knobs show the old answer" to stay true.
  Each side carries a comment naming the other; changing one is changing both.

## Note, 2026-08-21 (POE-193 follow-up)

The desktop defaults moved to off — the levels remain the documented
recommended tightening; visibility is the default. POE-193's expected-ROI
ranking makes the quality judgement on measured expectation and flags what it
cannot stand behind (lowCoverage, suspect, a negative expectation), so an
absolute cutoff in front of it hid measured-real plays: on 2026-08-21 the armed
levels hid 142 of 143 served 1-hop plays and an Apocalypse card flip at 8,532
c/h against the 10,000 line. `gateDefaults` is therefore all zeros — until
POE-196 added a sixth knob, `minItemPrice`, shipping at 0.5c as the one
sanctioned default-on client filter (see
[ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md)'s last two
Consequences bullets); the five this ADR is about remain zero — the third
consequence above no longer holds (the two languages are no longer one claim,
and the Go test — renamed
`TestBestPlays_recordedHourUnderTheOldServerLevels_yieldsNoOneHopRoutes` —
now documents what the levels cut when a reader arms them), and the SECOND
DECISION BULLET IS SUPERSEDED IN WHOLE — the knobs are not default-on, an empty
knob does not run at the old server level, and out of the box the table shows
everything the server serves rather than what the old server showed. What
survives of that bullet is the knobs themselves and the spelling of off: an
explicit `0` still turns a gate off, and now so does leaving it alone. The split
this ADR decided is unchanged — the server serves everything sane, the quality
bar is the reader's — with the bar now empty until the reader sets it.
