# ADR-018: Flags Mark; They Never Order

## Status

Accepted (POE-220, 2026-09-01). Completes the client half of
[ADR-015](015-exchange-quality-gates-live-client-side-the-server-serves-everything-sane.md)'s
serve-and-flag principle, and scopes
[ADR-016](016-expected-roi-is-a-cross-hour-simulation-displayed-prices-stay-single-hour.md)'s
ranking clause to the served order (see that ADR's 2026-09-01 amendment).

## Context

The Apocalypse card recipe stood in the book at buy 401c / sell 1200c for over
twenty hours, ROI 795c. The 0.67 × hour-VWAP band called its low junk and set
`suspect`, and every desktop sort partitioned on that flag before reading its own
key: under "ROI" the row ranked **638 of 954**. Nothing was hidden. The reader
simply never got to the row, which is the same outcome by a different mechanism.

[ADR-017](017-no-default-engine-floor-may-hide-a-live-market.md) bars the hiding
half — no default engine floor may drop a live market — and its two amendments
each closed one more place a floor was living. This is the same class in the one
place that ADR left open: the owner filed it, 2026-09-01, as the fifth time a
quality bar turned up hidden inside something that was not called a quality bar.
A partition is a bar with a soft edge; on a 954-row table there is no useful
difference.

## Decision

**On any client table over served plays, a sort orders by the figure its column
prints and nothing else. Every quality signal renders as a flag on the row. No
flag partitions, demotes, hides, or breaks a tie.**

- `suspect`, `lowCoverage`, `lowLiquidity` and `depletedSide` are marks. They
  change how a row is DRAWN and never where it sits.
- A sort key is the emitted figure of the column the reader picked — the same
  number that column prints, at the same posting size — and no second key drawn
  from a flag.
- A row whose key is missing or non-finite sorts last. That is arithmetic about
  the key, not a judgement about the row.

## Consequences

- **The server's ranking is advisory.** `internal/exchange`'s comparator (clean
  before suspect, covered before low-coverage, then `ExpectedRoi` desc) is
  unchanged and still governs the wire order. On the desktop it survives only
  where two rows tie on the picked column, which is what array-stable sorting
  leaves of it.
- **`sortPlays` in `desktop/src/lib/exchange/view.ts` is the enforcement point**,
  and it reads no flag. Its tests pin the rule per sort:
  `desktop/src/lib/exchange/view.test.ts` places a suspect play by its own column
  on all three sorts, and asserts the served order is overridden for Exp. ROI;
  `desktop/src/lib/exchange/corpus.test.ts` pins the neighbours over the engine's
  own golden wire bytes, so a Go-side ranking change cannot quietly re-partition
  the table.
- **A future flag is added to the render path only.** A new wire flag needs a
  badge, a tooltip and a legend entry; adding it to a comparator re-opens this
  ADR and needs one of its own.
- **Quality is still expressible, and still opt-in.** `HideSuspect` and the
  Gates knobs remain the reader's, default off, per ADR-015 and ADR-017. A reader
  who wants the flagged rows gone types it; the table does not decide for them.
