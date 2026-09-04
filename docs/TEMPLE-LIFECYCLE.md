# Temple module lifecycle — arming, detection, OCR, and what the overlay shows when

**Status:** normative design, owner-ordered 2026-09-04 (POE-249). Each line is tagged
**shipped** (on `main`, commit named) or **planned** (POE-249 / POE-248). Read this before
touching `desktop/src-tauri/src/temple/{trigger,run,slice}.rs` or the temple overlay widgets.
Related: [Overlay Guide](OVERLAY-GUIDE.md) (windows, click-through, smoke items),
[ADR-014](adr/014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md)
(the module contract; the POE-246 amendment note), [ADR-019](adr/019-nothing-a-module-draws-may-cover-what-that-module-reads.md),
[ADR-020](adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md).

## The one sentence

The capture runs only while something says an incursion is in scope; the sheet-bound overlays
live with the temple **sheet** on screen; the room overlay lives with the **incursion**; a full
OCR read happens once per board, with bounded retries, and then stops.

## States

`idle → waiting → reading → read → playing → idle`

| # | Event | Capture / OCR | Overlay | Status |
|---|---|---|---|---|
| 1 | **Alva voice line** (either) while `idle` | arm the loop (`trigger.rs`, POE-242: speaker match, 120 s tail from the line's stamp); cheap presence tick every **650 ms** (screen grab + one hinted correlation at the remembered screen scale, ADR-020) | centre **"waiting for the temple panel"** info overlay, toggleable in config | arm **shipped** (0dde882 / eb760c2); 650 ms and the info overlay **planned** (today 1000 ms, no info overlay) |
| 2 | **Sheet detected** (cheap tick anchors) | full read: anchor + 13 plates + panel + budget line + door markers, all regions keyed on the Entrance anchor (POE-230, 71df527). Regions that did not read cleanly (unknown plate, unresolved offer, marker count mismatch, unread budget) are re-captured on following ticks **at most 2 more times**; then **all OCR stops** — only the cheap presence tick continues. No periodic panel re-OCR. | hide the info overlay; show the sheet-bound overlays (offer boxes with the cyan frame on the advisor's pick, POE-249) and the room overlay (POE-244/248) | full read + signature gate **shipped** (07cf80c, 71df527); bounded retries and "no OCR after a clean read" **planned** (today: panel text re-OCR every 4 s while the sheet is up, `PANEL_RECHECK_INTERVAL`) |
| 3 | **Sheet gone** (first missed cheap tick) | keep the cheap tick (armed by the panel tail, POE-246: 120 s from the last sighting) so a reopened sheet is noticed; reopening re-shows the same data, no new OCR unless the board signature changed | hide every sheet-bound overlay **on the first miss**; the **room overlay stays** (the player is inside the room, which is exactly when it is needed) | room-overlay persistence **planned** (POE-248 item 1; today advice is dropped at stand-down); first-miss hide **planned** (today `RETIRE_AFTER = 2` ticks) |
| 4 | **Alva voice line again, or zone change** | stand down: stop capturing, clear all read state (`left_area_ms` stamp on zone change, POE-246; the Alva clear is POE-248) | clear and hide every overlay — the incursion is finished or the player died, which is finished either way | zone change **shipped** (0dde882); Alva-line clear **planned** (POE-248) |

Consequences that follow from the order, not from extra rules:

- The waiting overlay appears only on a voice line that **starts** a cycle (from `idle`). The end
  line clears; it does not show "waiting" again. The end line's arm still lets a reopened sheet be
  read (the loop is armed), silently.
- A death is a zone change (row 4). A sheet opened from the hideout with Alva silent is not in
  scope — **Re-arm** is the manual override (`temple_rearm`, 60 s), unchanged since POE-242.
- The keys setting (`temple_keys`) is orthogonal; POE-248 item 9 (the faint second-stone door)
  is what makes it unnecessary.

## Cadences and budgets (measured numbers, where they exist)

- Cheap presence tick: 650 ms **planned** (today `DETECT_INTERVAL` 1000 ms, `DETECT_INTERVAL_SLOW`
  backoff once a tick measured slow). Cost: one monitor grab (225 ms on the laptop's debug build,
  `temple-debug/1788516327712`) plus one windowed correlation.
- Full read: anchor+doors 1.5 s, panel OCR 1.9 s, plate OCR 0.7 s on the laptop's DEBUG build
  (same dump); release is faster.
- Cold start (no remembered scale): the pyramid sweep, 5.3 s in the release container (POE-234,
  29ac1b9), never the 348 s exhaustive sweep.
- Tails: `ALVA_TAIL_MS` = `PANEL_TAIL_MS` = 120 s; `MANUAL_ARM_GRACE_MS` 60 s (`trigger.rs`).

## Where each rule lives

| Rule | Home |
|---|---|
| what arms / disarms, the three clocks | `temple/trigger.rs` (`arm_source`, `ArmState`) |
| tick order: prune → hint → cheap detect → sweep gate → promote → full read → publish | `temple/run.rs` (`tick`, `wants_full_read`, `full_read`, `SweepGate`) |
| the read gate (signature-based "did the board change") | `temple/slice.rs` (`layout_signature`, `panel_signature`, `ReadGate`) |
| which overlay shows on which status / context | `desktop/src/lib/temple/view.ts` (`overlayShowsBoard`, `overlayShowsDoors`) |
| never-cover set and placement | `temple/run.rs::read_rois` → `layout.rois`; `desktop/src/lib/temple/overlay-geometry.ts` (ADR-019) |
| smoke items per rule | `OVERLAY-GUIDE.md` "Windows smoke checks" |

## Owner decisions this encodes (2026-09-04)

"Sheet presence should be every 1 s, as sometimes 4 s is the time the user already finished the
temple — and if that presence test is cheap, I'd even go every 650 ms." "After the Alva voice
line (doesn't matter which one) or zone change, we clear the overlays and hide them, as the user
has finished the incursion or died." "When the user starts playing, the cheap sheet detect notices
the sheet is gone; we hide the info overlay, and only the room overlay stays."
