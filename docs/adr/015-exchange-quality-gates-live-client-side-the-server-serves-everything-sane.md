# ADR-015: Exchange Quality Gates Live Client-side; the Server Serves Everything Sane

## Status

Accepted (POE-191, 2026-08-20)

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
