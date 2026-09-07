# Merc Overlay: In-Place Marks and a Beside Panel

> Status: **proposed / unimplemented** (POE-274). Last verified 2026-09-07 against
> commit `8a922d9` (the strip restyle) and ADR-023. Canonical for the TARGET form
> of the merc verdict overlay and its fallback rule; not for the strip that ships
> today, which `desktop/src/routes/overlay/mercenary/+page.svelte` and
> `desktop/src/lib/mercenaries/overlay-view.ts` are canonical for. Superseded by
> nothing. Design sources: [`merc-overlay-in-place/`](merc-overlay-in-place/README.md).

## 1. Problem

The strip drew its read marks as a glyph run in a user-placed window, and the
player had to map "cell 2 of Blood Mortar" onto the recruit panel's icons by
counting (owner, 2026-09-07: "the supports table is right aligned, while the
merc supports UI has them left aligned, and it creates a bit confusion when
there is a need to check the gem"). The 2026-09-07 restyle (`8a922d9`) fixed the
ORDER — cells left-aligned in slot order at the panel's square rhythm — and is
the layout this spec keeps as its fallback. The target removes the mapping:
the mark is drawn on the icon it describes.

## 2. The design

Two surfaces, both display-only and click-through like the strip today.

**In-place marks**, one per support cell, placed from the cell's own screen
rect (`MercSupportRead.rect`, physical px, converted to CSS px the way the
temple's `overlay-geometry.ts` converts capture px — ADR-019's drawer never
recomputes a rect):

- a 3 px bar, 30 px wide, centred under the icon (bar top ≈ cell centre + 18 px
  at the 1080p reference; scale with `capture.scale`), coloured by the cell's
  read tone — green read, amber unsure, red unread (`READ_TONE_COLOUR`);
- on an UNSURE cell, a 15 px amber badge with `?` at the icon's top-right
  (centre + 8, centre − 23 at the reference); on an UNREAD cell the same badge
  in red with `✕`. Read cells carry the bar alone.
- a row with no cells draws nothing in place; the beside panel's row list is
  gone in this form, so "no supports" is simply an unmarked row, as in the game.

**The beside panel**, anchored 12 px right of the recruit panel's right edge
and top-aligned with its first row, 300 px wide:

- the name, bold, wrapping; `class · lvl n` under it (`overlayHeader`);
- the verdict as the restyle draws it: the headline bar (`verdictBlock`), and
  under a WORTH one chip per rung — guide, rung, tier tag deepening mv → gg;
- the status line at the foot with the pulse dot (`statusLine`, `statusPulse`).

The panel's four states — SKIP while reading, WORTH with one guide, WORTH with
three guides under a mangled name, WORTH with a tiered rung — are the
`PanelStates` artboard. The wording, gates and linger are unchanged from the
strip: every line still comes from `overlay-view.ts`.

## 3. What already exists

- Cell rects and the capture scale on the slice; row rects and the panel anchor
  in Rust (`geometry.rs`: `panel_anchor`, `MercLayout`), not yet published.
- The screen slice (`ssot.screen`: origin, width, height, monitor) for the
  capture-px → CSS-px conversion.
- ADR-023: the merc window is excluded from every grab while it is taken, so
  the marks may sit on the cells the templates match. Measured 2026-09-07.
- ADR-021 (proposed): a module draws one window on the game's monitor and
  widgets inside it — the shape the in-place surface wants.

## 4. Rules

1. **Fallback is automatic, never a preference.** Rust publishes the exclusion
   outcome on the merc slice (`captureExclusion: 'held' | 'refused' |
   'unknown'`, written where `capture.rs` logs its once-lines). On `refused`
   the overlay draws the strip layout of `8a922d9` with no in-place marks and
   the status line says why; on `unknown` (no grab yet) it draws the panel and
   no marks until the first grab settles it. Two layouts in one route is the
   accepted cost; a user toggle is not (owner, 2026-09-07).
2. **The drawer converts, it never derives.** Every rect the marks use comes
   from Rust — cell rects today, the panel rect once published. A TypeScript
   copy of `cell_offset_x` or `cell_pitch` is the drift ADR-019 forbids.
3. **The beside panel never covers the recruit panel.** Anchored right of it;
   when the panel's right edge leaves no 312 px of screen, the beside panel
   goes LEFT of the recruit panel instead. It never overlaps the panel and is
   never user-placed. What Settings → Overlay Positions offers for the merc
   row then is the breakdown's decision: it stays for the fallback strip, or
   it goes.
4. **Marks describe the window on screen.** Like `liveRowGlyphs`, marks draw
   only while the capture is on screen (`live`, `done`); a retired capture
   keeps the beside panel with the window-gone line and draws no marks.
5. **The window model is decided at breakdown**, between: (a) one monitor-sized
   click-through window per ADR-021 holding the marks and the panel as
   widgets, or (b) the strip window moved and resized by Rust over the recruit
   panel's bounding box plus the panel's 312 px. (a) is the architecture the
   temple is converging on; (b) is the smaller diff. Either way the window is
   built once and moved (guide guard 4), and its height follows content as
   today only in the fallback.
6. **POE-273 is a dependency of correctness, not of this design.** The marks
   cannot draw a row the capture does not have; a dropped row is a missing set
   of marks, which the panel's status line must count ("6 rows on screen, 5
   read") once POE-273 publishes the icon-column count.

## 5. Acceptance

- At 1080p and at 1440p the bars sit under their icons with no visible drift
  across all six rows (the 2026-09-07 dumps at scale 0.899 are the 1080p board).
- The reader's dump shows no mark and no panel (ADR-023's smoke item); the
  player's screenshot shows both.
- With the exclusion refused (simulate by forcing the `refused` outcome), the
  strip layout draws, marks do not, and the status line names the reason.
- A recruit panel at the screen's right edge gets the beside panel on its left.
- Hover confirmations still work: a `?` badge clears to a bar on confirm.

## 6. Out of scope

Support names on the marks (direction B, rejected: no name for an unread
cell); a per-guide row list in the panel (the page keeps it); any change to
`verdict.ts` or the wording module beyond the `captureExclusion` reason line.
