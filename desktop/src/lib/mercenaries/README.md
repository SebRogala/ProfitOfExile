# Mercenary module — behaviour contracts

The Mercenaries module (ADR-014 module id `mercenary`) reads the in-game recruit
window and tells the player whether the mercenary is worth the wager. The Rust
half lives in `desktop/src-tauri/src/mercenary/` (trigger, capture, OCR, icon
matching, trade lookup); this directory holds the frontend half — the verdict
engine, the guide rulesets, and the wording of the page and the overlay strip.
Component-level entries are in `desktop/src/lib/README.md` → "Mercenary Data";
Windows overlay mechanics and smoke checks are in `docs/OVERLAY-GUIDE.md`.

This file records the behaviours that are owner decisions rather than
derivable from the code. Change the code and this file together.

## Overlay strip — when it is on screen

Owner decision, 2026-09-01.

The strip is on screen **for as long as a recruit window is being worked, and
four seconds longer**:

| Module status | Strip | What it shows |
|---|---|---|
| `scanning` | shown, for as long as the burst looks | `scanning for the recruit window…`, prefixed `heard <name> · ` when a voice line named the speaker — a `Scan now` burst has no speaker and shows the bare line |
| `live` | shown, for as long as the read lasts | `reading · N rows · …`, header, verdict line, per-row glyphs |
| `done` | shown, for as long as the window is on screen | `done · N rows · …`, header, verdict line, per-row glyphs |
| `idle` — window gone | shown for **4 s** after the retire, then cleared | `recruit window gone — last read` over the last verdict |
| `idle` — waiting | shown for **4 s** after going idle, then cleared | `waiting for a mercenary · Scan now on the page` |
| `off`, `unavailable` | never shown | — |

"Cleared" means the panel is not rendered at all — the transparent window stays
where Settings placed it, but nothing is drawn, so the overlay disappears.

Rules that follow from the table:

- A burst that fires during the linger (a voice line right after the window
  closed) shows the strip in full; the idle after it gets a fresh four seconds.
- Staying idle does not restart the clock. The route polls the slice every
  three seconds, and a clock restarted on each poll would never run out.
- The clear is driven by the linger, not by the next poll: the route arms one
  timeout for the remainder of the four seconds, plus 50 ms so a timer that
  fires on the boundary reads the clock as expired. The strip therefore goes
  within ~50 ms of the four seconds rather than up to a poll (3 s) later.
- The cost is known and accepted: an armed module with no strip on screen is
  indistinguishable from one that never triggered. The Mercenaries page carries
  the waiting state, and the triggering itself is to be reworked (open, no
  ticket yet).

Where it lives: `overlay-view.ts` — `LINGER_MS`, `OverlayLinger`,
`lingerAdvance`, `lingerRemainingMs`, `overlayShown` (all unit-tested in
`overlay-view.test.ts`); the clock and the single timeout are in
`desktop/src/routes/overlay/mercenary/+page.svelte`, which has no test harness
and owns nothing else about the decision.

History: 2026-08-25 the strip gained the always-present status line so an idle
module was not pixel-identical to an overlay that failed to build; 2026-09-01
that permanence was withdrawn in favour of the four-second linger, because a
panel that stays over the game after the decision is made costs more than it
tells.
