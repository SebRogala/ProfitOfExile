# Mercenaries lifecycle — Inspect trigger, capture and triage

**Status:** current implemented behavior.

**Last verified:** 2026-11-02 at `842d6a18`.

**Canonical for:** Mercenary trigger-to-verdict flow and module lifecycle.

**Supersedes:** duplicated Inspect-trigger lifecycle notes in `desktop/src/lib/README.md`.

The shared capture requirements remain in
[ADR-025](adr/025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md).
Placement remains in [ADR-024](adr/024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md),
and the work/page boundary remains in
[ADR-014](adr/014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md).

## Arming and capture

| Event | Current behavior | Shipped |
| --- | --- | --- |
| Known Inspect text | A normalized multi-word Inspect text arms regardless of speaker shape; a one-word Inspect grunt requires that speaker's previous known multi-word mercenary text. | `c4723819` (POE-280) |
| Unknown, approach or fight text | Does not arm; the embedded English corpus is the positive trigger vocabulary. Scan now remains the manual fallback. | `c4723819` |
| Armed gate | Probe the placed crop after 500 ms, retry once at 1.5 s from the line (or the probe gap after a delayed first probe), then stand down on another miss. | `c4723819` preserves the two-probe gate |
| Chrome found | Hand the probe frame to detection in the same iteration; an automatic placed miss buys no full-frame locate. | `c5e71acc`, preserved in `c4723819` |
| Scan now / Recalibrate | Manual detection may locate after a placed miss; only the manual path may replace standing geometry. | `c5e71acc`, closure `f2f93cf9` |
| Capture read | Apply ADR-025's full/unknown-only read budget, then switch to liveness checks; hover correction remains independent. | `2f53ee61` |
| New identity or manual scan | Refill the read budget; template-generation changes also refill the merc budget. | `2f53ee61`, `f2f93cf9` |

The automatic probes, retries and hover re-reads in this document break [Screen reading (GGG policy)](OVERLAY-GUIDE.md#screen-reading-ggg-policy); POE-390 replaces the merc capture loop and POE-397 the gem icon reads.

A qualifying Inspect line over a held capture is ignored. Otherwise a new
qualifying line can re-arm two probes, bounded by the stale-line window from
the first line in its chain. Text and asset normalization share one trigger
implementation. Local chat without a chat sigil can reproduce a known Inspect
text and cause probes; this is an accepted limitation, not panel confirmation.
Shipped: `c4723819`.

With no placed slice, the probe can hand its frame to the keyed cold-start
locate. A Recalibrate still pending is treated as a manual request. With a
placed crop, Wager, Recruit-verdict or button anchors establish chrome, and
`placed_layout` checks row bounds before publication. Shipped: `c5e71acc`,
`c4723819`; stages extracted in `a9d6612a`.

Both probes require raw `game_in_foreground`. Captured/Refreshed detection
disarms the gate, and live ticks also disarm probes to close the watcher/publish
race. Scan now bypasses the voice probe and resumes a held read off cadence.
Shipped: `c4723819` retains these gate behaviors.

The trigger uses the log line timestamp only inside its bounded backdate
window; otherwise it uses delivery time. Rearms retain a monotonic look count
and a ten-second stale-chain bound. `StandDown::Probed` means both looks missed;
`Stale` means the chain expired before all looks; `Manual` means the manual
foreground grace expired (60 seconds). Armed waiting uses a 100 ms loop quantum.
One log line records each arm and each stand-down. Shipped: `c4723819` retains
the timing and stand-down model.

## Read and liveness ownership

`mercenary::run::LoopState` chooses read plans and cadence;
`mercenary::read` builds and merges the capture. While a live capture still
needs reads, detection runs at the two-second cadence. A complete or
budget-spent capture uses the ten-second liveness cadence without pass-2 row
or skill/support-cell re-reading. Pass 1 still reads the crop's chrome/header
and can fill unresolved header fields; resolved header fields remain kept.
Hover checks run on their own 400 ms cadence. Shipped:
`2f53ee61`; geometry/read separation retained in `a9d6612a`.

Two consecutive counted misses retire a live capture and reset its read budget;
the first miss holds it. Occlusion handling determines whether a failed detect
counts as a miss. Shipped: retirement retained in `2f53ee61`.

The read-budget limits, confident-cell preservation, layout-mismatch exception
and fresh-identity rules are specified once in ADR-025. This guide maps those
rules to the module rather than defining a second capture contract.

## Triage and presentation

`evaluateCapture` in `desktop/src/lib/mercenaries/verdict.ts` evaluates each
enabled source independently. An unconfident read remains unknown rather than
proving absence. The result tree carries the positions and bonuses used to
explain a verdict. Shipped: `1230f903`.

The main page derives its verdict from the capture, rulesets, enabled sources
and league. A first-look partial capture has no verdict until the icon pass
finishes. Shipped: `1bd8c508`, partial-read gate `ec6cabeb`.

Rust owns the excluded-guide setting and echoes `sourcesOff` through the
mercenary SSOT slice, so the page and overlay consume one guide selection.
Shipped: `2c51c662` (POE-199). Native lookup and unresolved-league link behavior
follow the [trade lifecycle](TRADE-LIFECYCLE.md); window and click mechanics
follow the [Overlay Guide](OVERLAY-GUIDE.md).

## Code homes

- `desktop/src-tauri/src/mercenary/trigger.rs`: text matching, speaker history and probe gate.
- `desktop/src-tauri/src/mercenary/run.rs` and `run/detect.rs`: loop, read budget, hover and publication.
- `desktop/src-tauri/src/mercenary/run/session_open.rs`: session open, including the ordered template-store open (purge, reset, load, re-key, seeds, corpus merge).
- `desktop/src-tauri/src/mercenary/read.rs`: read plans, confidence and capture merge.
- `desktop/src/lib/mercenaries/verdict.ts`: pure triage result tree.
- `desktop/src/lib/pages/MercenariesPage.svelte`: main page projection.
- `desktop/src-tauri/src/mercenary/sources.rs`: persisted guide selection.

Server-side `merc_icon_templates` pool rows lose their device link after the period in [Device data retention](DATA-RETENTION.md).
