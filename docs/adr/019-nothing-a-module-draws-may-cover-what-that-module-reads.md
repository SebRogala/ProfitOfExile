---
uid: e3a56433-a589-49b2-8cf4-b32e6805e777
---

# ADR-019: Nothing a Module Draws May Cover What That Module Reads

## Status

Accepted (POE-244, commit `c175946`, 2026-09-02). Written up in the POE-223
follow-up audit, 2026-09-04, together with the violation it closes.

Amended 2026-09-04 (POE-248) — see [the amendment at the end](#amendment-the-line-exception-is-retired-2026-09-04):
the one carve-out this ADR took for a PLACER's own output, the kill callout's
arrow, is gone with the arrow. The stored-placement carve-out below is
untouched.

Scope: every overlay surface a module draws over a screen the same module OCRs
or samples. Today that is the temple; the merc verdict strip and the lab
overlays are the next candidates, and the rule is written for them rather than
for the one module that needed it first.

## Context

The temple builder draws two surfaces over the game — a kill callout with an
arrow into an architect block, and a room diamond — while its capture loop is
reading that same screen at up to 1 Hz. The capture is `capture_screen`: a real
grab of the whole game monitor, with whatever is on it, including the app's own
overlay.

That makes an overlay pixel indistinguishable from a game pixel. A box twenty
pixels into the side panel's OCR crop does not produce an error, a fallback, or
a lower confidence score. It produces a **confident wrong board**: the panel
reads with one architect instead of two, the advisor ranks the boards it was
given, and the overlay states the answer with the same certainty it states a
correct one. Nothing anywhere reports a failure, and the only surface that could
have reported one is the surface that caused it.

This is not the ordinary "don't cover the UI" concern. The cost of covering the
game's own chrome is that the player cannot see it, which the player notices
immediately. The cost of covering the module's crop is that the app lies to
itself, and neither the player nor the log can tell.

Two facts about the failure decided the shape of the rule:

- **It is silent by construction.** There is no seam at which a bad read can be
  detected as bad, so the rule cannot be "check afterwards"; it has to be a
  placement that cannot happen.
- **An overlay window has no devtools and a `.svelte` file has no unit-test
  harness in this app.** So the arithmetic has to live outside the component,
  in a pure module a test can reach.

## Decision

**Nothing a module draws may cover a rectangle that module reads.**

- **The READER publishes the never-cover set.** `temple::run::read_rois` is the
  one builder over the six functions that own those rectangles —
  `run::panel_rect` (the side panel crop), `run::diamond_rect` (the panel's own
  diamond), `run::remaining_rect` (the budget line), `panel::name_strip` and
  `panel::numeral_box` (unioned into one rect per plate, 13 of them), and
  `Lattice::edge_midpoint` with `lattice::PATCH_HALF` (26 corridor beam
  patches): 42 rects on a full board. It reaches the webview as `layout.rois` on
  the SSOT slice.
- **The DRAWER never recomputes it.** `overlay-geometry.ts` converts capture px
  to CSS px and filters; it derives no rect from `origin` and `scale` of its
  own. A TypeScript copy of any of those six functions' constants would be a
  second answer to "where is the module looking", and the two would drift with
  nothing failing.
- **An EMPTY never-cover set means place nothing yet — never "the screen is
  free".** The set is empty when there is no layout and when the window's scale
  factor has not resolved; in both, an empty obstacle list makes every position
  legal, which is the exact wrong answer for the exact reason it is empty. All
  three placers state this themselves rather than inheriting it from a null
  anchor (`calloutPlacement`, `bannerPlacement`, `doorDefaultPlacement`), and
  the door's caller repeats it so no default is offered to the host at all.
- **`avoidRects` is the arithmetic, and `null` is a real answer.** The nearest
  position clear of every obstacle, or nothing. A box that cannot be placed
  clear is NOT drawn: the game's own panel is on screen either way, and a callout
  that costs the module its read is the worse trade.
- **One bounded exception: a thin LINE may cross a read region; a FILLED shape
  may not.** *(RETIRED 2026-09-04 — see the amendment at the end. Kept as
  written because the reasoning is what the amendment answers.)* A 3 px stroke
  over a glyph is not what breaks an OCR read; a panel sitting on the text is,
  and so is an arrowhead, which is a solid triangle. So the arrow is allowed and
  the head is not: `calloutArrow` stops the line `ARROW_STANDOFF_CSS` (10 px)
  short of the block, and the head is 8 px in USER units
  (`markerUnits="userSpaceOnUse"`; the default is `strokeWidth`, which
  multiplied an 8 px head by the 3 px stroke into a 24 px triangle on the
  block's first glyphs). The point lands about 9 px clear of the text. The
  exception is bounded where it is TAKEN, not where the rule is enforced —
  nothing that goes through `avoidRects` may overlap anything.
### What the rule binds, and the one carve-out

**It binds PLACERS: code that derives a position from the game.** A placer reads
where the module is looking and answers where a surface goes, and the player
never saw the arithmetic. That is the whole class this ADR governs —
`calloutPlacement`, `bannerPlacement`, `doorDefaultPlacement` today.

**A rectangle the USER owns is not a placer's output and is outside the rule.**
Two of those, and they are the same thing at different ages:

- **A stored placement.** Once the user drags the door diamond onto a plate it
  goes where they put it. `placementFor` converts, rebases and clamps that
  rectangle and consults no obstacle set — correctly. Refusing to draw a widget
  where its owner put it would be the app overruling a decision it just asked
  for.
- **The registry's shipped default.** `placementFor`'s no-stored-placement
  branch returns `spec.defaults`, and `doorDefaults` falls back to it when the
  game-derived answer is `null`. That is a fixed rectangle, identical on every
  machine, visible the moment the widget draws, and movable in one drag. It
  cannot be vetted against a board it has never seen, and the honest response to
  "there is no board" is to show the widget somewhere the user can find and move
  it — not to draw nothing.

The line between the two is **who is accountable for the position**. A placer is
accountable and must therefore prove the position is clear; a user-owned
rectangle is the user's, and its failure mode (a widget on a plate) is visible,
attributable and one drag from fixed — unlike a placer's, which is silent.

The cost is real and accepted: a shipped default CAN sit on a read region on a
screen nobody has vetted. The Windows smoke item for the Debug-capture diff says
so explicitly — a difference that appears only after a drag is the user's
placement and not a defect. Narrowing this carve-out (say, clamping a stored
placement off a read region) would be a new decision and needs its own ADR; it
trades a silent failure for a surprising one.

## The violation this ADR was written from

`bannerPlacement` — the leave-the-map banner — went through `avoidRects` like
everything else, and was still wrong. It wants the top centre of the host, which
is a position the HOST alone can supply, so with an empty obstacle list
`avoidRects` found the wanted rect clear and RETURNED it. The banner drew
top-centre; on the one 1920×1080 frame this repository holds a centred ~440 px
banner reaches x 1180 and the panel's crop starts at 1131.

Every other placer escaped the same input by accident: the callout's anchor and
the door's panel rect are `null` at an unresolved scale factor, so they refused
for a reason that was not the rule. The banner had no anchor to be null.

That is why "empty means place nothing" is a Decision bullet in its own right
and is asserted at each placer rather than once at the caller. Going through the
avoidance is not the same as obeying the rule; a surface whose wanted position
does not depend on the board is the one that shows the difference.

## Consequences

- **Every new overlay-over-OCR surface inherits three obligations**: take the
  published never-cover set as an input, refuse on an empty one, and have a
  not-drawn state. A surface with no not-drawn state cannot obey this ADR and
  needs its own decision, not an exception folded in here.
- **Every guard lives in the pure modules, and that is load-bearing rather than
  stylistic.** All three empty-set refusals and the avoidance itself are in
  `desktop/src/lib/temple/overlay-geometry.ts` and
  `desktop/src/lib/overlay/widgets/widget-avoid.ts`, which is the only form a
  test can reach — an overlay window has no devtools. The components
  (`TempleKillCallout.svelte`, the temple overlay route) call them and add no
  arithmetic of their own; a `.svelte` file that computed a position would put
  this rule somewhere nothing can assert it.
- **The check that verifies it is a Debug-capture DIFF, not an eye.** The
  capture is a real grab, so the overlay is in it: dump with the overlay up and
  again with the module's overlay off on the same board, and the room title,
  both architect blocks, the incursion count, `current`, `doors` and
  `unknownRooms` must be identical. The Windows smoke items in
  `docs/OVERLAY-GUIDE.md` carry this; a visual "it looks clear" proves nothing,
  because the failure mode is that it looks clear.
- **Widening the arrow exception re-opens this ADR.** *(Moot since 2026-09-04:
  the exception is retired, and TAKING one again is now the new decision.)* The
  bound was "thin line yes, filled ink no", with a stated standoff.
- **Cost accepted: the module sometimes says nothing.** On a board where nothing
  is free the callout is not drawn and the banner is not drawn. That is the
  intended trade — the information is on the Temple page and in the game's own
  panel, and the read survives.
- **Rects are published, so they can be published wrong.** This ADR moves the
  correctness question upstream to whoever owns the reading rectangles; see
  [ADR-020](020-one-shared-screen-scale-a-module-corroborates-or-withholds.md)'s
  read-region clause, which is the rule that keeps them keyed on the layout
  anchor rather than on the screen edge.

## Amendment: the line exception is retired (2026-09-04)

POE-248, from the owner's first live session on the v2 overlay. The decision is
a product one and the ADR only follows it: **no arrows anywhere** on the temple
overlay. The kill callout keeps its box and its placement — level with the
architect block, immediately outside the panel — and the thing that points once
the panel closes is the cyan kill glyph on the room widget, drawn on the same
spot inside the room where the game draws that architect's own icon.

What changes here is narrower than it looks, and worth stating because the
carve-out was the only soft edge this rule had:

- **No placer output crosses a read region any more.** Everything the module
  derives a position for goes through `avoidRects` and is outside every
  published rect; there is no "allowed to cross" class left, so no bound to
  argue about and no standoff to keep correct. This is the class the rule binds
  — a rectangle the USER placed is still outside it, exactly as the
  stored-placement carve-out above says, and dragging the room widget onto a
  plate still covers that plate.
- **`calloutArrow` and `ARROW_STANDOFF_CSS` are deleted**, not left dead. A pure
  helper nothing calls is an invitation to call it, and the invitation here is
  to re-take an exception the rule no longer has.
- **Taking a line exception again is a new decision**, and it needs this ADR
  amended rather than a comment: the original reasoning (a 3 px stroke does not
  break OCR; a filled shape does) was sound and is still available above, but it
  bought a pointer the overlay no longer needs, and an exception nothing uses is
  strictly worse than none.

Nothing else in this ADR moves. The never-cover set, the empty-set rule, the
`null` answer, the stored-placement carve-out and the Debug-capture diff are
untouched.
