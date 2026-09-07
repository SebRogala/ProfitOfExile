---
uid: 3c7667f3-5a2f-43c5-bd99-d575f49f39ca
---

# ADR-023: A Window Excluded From the Grab May Cover What Its Module Reads

**The rule in full**: an overlay window named in `capture.rs`'s
`EXCLUDED_WHILE_GRABBING` is removed from every screen grab the app takes, for
the duration of that grab and no longer — so it may sit on the rectangles its
module reads, and it stays in the player's own screenshots between grabs.

## Status

Accepted (2026-09-07, owner smoke on the PC; no ticket — it fell out of the
merc overlay redesign, whose canvas is the only artefact so far). Amends
[ADR-019](019-nothing-a-module-draws-may-cover-what-that-module-reads.md):
that rule's premise — the grab contains the app's own overlay — no longer holds
for a window on the list. Today the list is `mercenary`, the merc verdict strip,
and `overlay-preview`, the Settings-owned read-only OCR frame.

## Context

The merc verdict strip is being redesigned to paint its read marks IN PLACE —
under each support icon of the recruit panel, from the cell rects the capture
already carries — with the name, verdict and status in a panel beside the
recruit panel. That puts app pixels inside the exact 44 px cells the icon
templates are matched on, which ADR-019 forbids for the reason it states: the
reader grabs the whole monitor (`capture::capture_screen`, xcap), so an overlay
pixel is a game pixel to it, and the failure is a confident wrong read with no
error anywhere.

ADR-019's remedy is placement: keep the surface off the read set, or draw
nothing. For in-place marks there is no such placement — the mark IS on the
cell — so the choice was between not building the design and removing the
window from the grab itself.

Windows has a primitive for the second: `SetWindowDisplayAffinity` with
`WDA_EXCLUDEFROMCAPTURE`, which Tauri exposes as `contentProtected` /
`set_content_protected`. Two measurements on the PC, 2026-09-07:

1. **Held permanently** (`contentProtected: true` on the strip's constructor):
   the strip vanished from the reader's dump — omitted, not blanked, game
   pixels where it stood — and ALSO from the player's own screenshots. The
   first half is the fix; the second is a cost the owner refused.
2. **Held per grab** (this ADR): dump `merc-debug/1788782245765` shows game
   pixels only at the strip's rectangle (x 1458–1918, y 410–561, from
   `settings.json`); the owner's ordinary screenshots include the strip; no
   flicker seen on that machine at the 2 s live / 10 s paused cadence.

## Decision

- **Exclusion is per grab, and it lives in the grab.** `capture_screen` holds
  an `ExcludedFromCapture` guard around the single `capture_image` call: set
  `WDA_EXCLUDEFROMCAPTURE` on each listed window's HWND before, `WDA_NONE`
  after, with `Drop` doing the clear so an error inside the grab cannot leave a
  window out of screenshots. Every caller — merc, temple, the debug captures,
  `lib.rs` — gets it without asking, because "the reader's grab" is any grab.
- **The list is by label, not by handle.** A window that is not built is
  skipped; one rebuilt later is picked up by the next grab. The HWND lookup is
  Tauri's blocking `hwnd()` round-trip, once per grab, which is microseconds
  against a full-monitor copy.
- **Membership is a decision, not a default.** Adding a window to
  `EXCLUDED_WHILE_GRABBING` is what releases it from ADR-019's never-cover set,
  and it needs a line in this ADR's list. The temple's surfaces stay under
  ADR-019 until someone adds them here on their own evidence.
- **The current excluded-window list is:** `mercenary` for the in-place verdict
  strip, and `overlay-preview` for the Settings-owned OCR frame and label. Both
  are excluded only for the duration of each grab; every other overlay remains
  under ADR-019.
- **Failure is loud, once per process.** The first successful hold of ANY listed label logs (one flag for the whole list, so with two labels only the first exclusion logs)
  `capture: overlay '<label>' is excluded from each grab while it is taken`;
  the first refused call — an HWND that cannot be resolved, or an affinity the
  OS rejects (from training data: `WDA_EXCLUDEFROMCAPTURE` needs Windows 10
  2004 or later, unverified here) — logs
  `capture: overlay '<label>' … — the listed overlay is IN every grab`. One line
  per process each, because the grab runs every few seconds and the fact does
  not change between them.

## Alternatives rejected

- **Permanent `contentProtected`.** Measured first; removes the strip from the
  player's screenshots, which the owner rejected outright.
- **Hold the affinity by module status** (set on `scanning`/`live`, clear on
  `done`/`idle`). Cheaper to reason about, but the paused state still runs a
  full detect every 10 s (`LIVENESS_INTERVAL`), so the marks would be in those
  frames; and it keeps the strip out of screenshots for the whole live phase,
  which is when the player most wants one.
- **Hide the window for each grab** (`hide()`/`show()`). Changes what is on
  screen — a visible blink at the grab cadence — and races the event loop the
  same way the affinity would, without the affinity's property of leaving the
  on-screen pixels alone.
- **Mask the marks out of the frame** (the reader knows where it drew). Couples
  the reader to the drawer's geometry, which is the coupling ADR-019 exists to
  forbid, and still leaves the beside panel to place.
- **Keep ADR-019 as is and drop in-place marks.** The Mirror direction on the
  design canvas is that design; the owner picked in-place because the whole
  point of the redesign was that the strip's cell order did not map onto the
  panel's.

## Consequences

- **The Debug-capture check flips for a listed window.** ADR-019's smoke item
  proves the overlay is IN the grab and harmless; for `mercenary` the item is
  that the strip is ABSENT from the grab and PRESENT in a screenshot —
  `docs/OVERLAY-GUIDE.md`, Windows smoke checks.
- **One machine measured.** No flicker on the owner's PC. A flicker report from
  another machine is new evidence, and the fallback that keeps the design is
  the status-gated hold above, not a return to placement.
- **Ordering is measured, not proven.** Whether the affinity is in force by the
  time xcap's copy runs was shown by the dump, not by documentation; a strip
  that reappears in dumps after a Windows or xcap update is this assumption
  breaking, and the log's armed line is what says the guard still ran.
- **The release is app-wide, not per module.** Every grab in the app excludes
  a listed window, so it may cover what any module reads; the temple reader is
  as protected from the merc strip as the merc reader is.
- **Cost accepted: two Win32 calls and one event-loop round-trip per grab**,
  bounded by the grab cadence, and a screenshot that lands inside a grab — the
  few milliseconds a copy takes, every 2–10 s — shows no strip.
