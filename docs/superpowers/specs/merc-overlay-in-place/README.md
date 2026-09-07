# Merc overlay design sources

Status: design sources for the proposed in-place merc overlay — see
[the spec](../2026-09-07-merc-overlay-in-place-design.md). Last verified 2026-09-07.

Artboards of the design canvas, in Claude Design's `.dc.html` format, plus the
canvas layout and the two screenshot crops they reference. The canvas itself is
published at https://claude.ai/code/artifact/d49c7343-03a1-490c-8046-bf7bf13771c1
(owner's account; the editor baked into it is a preview and does not update).

- `Main.dc.html` — the proposal: marks painted under each support icon of the
  recruit panel, and the beside panel (name, verdict, status) anchored to its
  right. `PanelStates.dc.html` — the beside panel in four states.
- `Mirror.dc.html`, `InContext.dc.html` — the strip restyle that SHIPPED on
  2026-09-07 (commit `8a922d9`) as the fallback layout; kept for the record.
- `DirectionB.dc.html` (support names as chips), `DirectionC.dc.html` (marks in
  place, everything else as tags on the panel) — rejected directions.
- `Current.dc.html` + `current-strip.jpg` — the strip before the restyle.
- `game-panel.jpg` — a 1080p crop of the recruit panel the marks are placed on
  (Vaudren, 2026-09-07); its icon centres are at a 44 px pitch, rows at 43.5 px.

To re-seed a canvas from these files, run the `/design` skill and point it at
this directory; edits made in the published canvas do not flow back here, so
export changed artboards before relying on them.
