---
uid: bfb3e433-51dd-4452-b65b-7c1ac41f26d4
---

# ADR-024: Module OCR Positions Are Placed From the Screen Slice, Not Located

**The rule in full**: `ssot::placements(&ScreenSlice)` is the one pure home of
every module rectangle — the temple Entrance origin, the merc recruit window,
and the Lab gem and font regions. The result is derived state exposed through
`AppSsotSnapshot`, never a second stored geometry answer (POE-266).

## Status

Accepted (owner decision 2026-09-07, epic POE-265); implementation pending in
POE-267…POE-272. Until those tasks land, the current code still locates or
re-detects per tick through the Temple and Merc paths
(`desktop/src-tauri/src/temple/run.rs:2969-2979`,
`desktop/src-tauri/src/mercenary/run.rs:3035-3045`); see Consequences.

**Superseded by the POE-269 amendment below.**

## Context

ADR-020 §5 defines the scale lifecycle as trusted at start, verified on first
use, and re-measured only on a display or dimension mismatch, a failed
verification, or Recalibrate (`docs/adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md:145-167`).
The repository has no equivalent position contract: `ScreenSlice` carries
capture dimensions, scale, provenance, timestamps, monitor id and origin, but
no module rectangle (`desktop/src-tauri/src/ssot.rs:176-229`). The app snapshot
is the existing derived-state boundary (`desktop/src-tauri/src/ssot.rs:232-239`),
so position belongs beside the scale rather than in module settings (POE-266).

The owner premise is explicit: the temple layout panel and the merc recruit
window do not move in-game (owner, 2026-09-07). The current contrary assumptions
are transitional code, not product behavior: Merc still has a panel-anchor
path that treats a changed column as movement (`desktop/src-tauri/src/mercenary/geometry.rs:749-789`),
and Temple still has a per-board geometry cap (`desktop/src-tauri/src/temple/run.rs:740-753`).

### Finding

The scale rule was written and honoured; position was not specified (POE-266).
The observed current paths are:

| Module | Current path | Evidence and cost |
|---|---|---|
| Merc | Whole-screen OCR finds line clusters and `panel_anchor`; a known-panel crop can fall back to a full look. | The full-screen OCR tick is recorded at 4,504 ms (`desktop/src-tauri/src/mercenary/geometry.rs:1000-1010`); crop selection and the `crop→full` retry are in `desktop/src-tauri/src/mercenary/run.rs:3035-3045` and `:3106-3138`. |
| Temple | `detect_cheap` performs a hinted NCC `recheck`; misses reach `SweepGate` and its cold sweep, while board re-placements are capped. | The cheap/recheck path is `desktop/src-tauri/src/temple/anchor.rs:604-624`; the sweep gate is reached from `desktop/src-tauri/src/temple/run.rs:3061-3107`; the cap is `GEOMETRY_READS_CAP = 8` (`desktop/src-tauri/src/temple/run.rs:550-562`, `:740-753`). The measured PC release stages include 64 ms cheap detect and 96 ms anchor (`docs/TEMPLE-LIFECYCLE.md:248-253`). |
| Lab | Reference rectangles are scaled from `ui_scale`, with a user rectangle taking precedence; a missing scale assumes 1080p. | `GEM_REGION_REF`, `FONT_PANEL_REF` and their provisional status are documented in `desktop/src-tauri/src/lib.rs:33-57`; `effective_region` implements the three-way choice in `desktop/src-tauri/src/lib.rs:83-108`; the settings fields remain in `desktop/src-tauri/src/settings.rs:20-47`. |

The static measurements support arithmetic placement. The temple fixture records
the Entrance origin at `(960, 713)` on the 1920×1080 laptop
(`desktop/src-tauri/src/temple/anchor.rs:1837-1842`), and ADR-020 records the
panel's fixed reference offsets from that origin (`docs/adr/020-one-shared-screen-scale-a-module-corroborates-or-withholds.md:174-182`).
The merc fixtures provide both machines' crop origins and the PC's skill-column
and row positions (`desktop/src-tauri/src/mercenary/cellfit.rs:945-963`,
`:1067-1075`), while the 1920×1200 dump records a panel at
`[698, 615, 555, 477]`, not a vertically centred window
(`desktop/src-tauri/src/mercenary/geometry.rs:1113-1119`). The crop origins are
inputs to fixture coordinates, not panel-position constants (POE-266).

## Decision

1. **One placement function.** `ssot::placements(&ScreenSlice) -> Placements`
   is the single pure source of `temple.entrance_origin`, `merc.panel`,
   `lab.gem` and `lab.font`. It is exposed in `AppSsotSnapshot` as derived
   state, so the preview, Settings card and modules consume the same result;
   it is never persisted as a second settings geometry (POE-266).

2. **Arithmetic from the client rect.** Each rectangle is a measured
   client-rect anchor — an edge or centre, per panel — plus a measured
   reference-pixel offset multiplied by `ui_scale`. The shipped anchor and
   offset constants are the SEED and are measured on both machines; they are
   not a new locate operation (POE-265, owner 2026-09-07). The anchor key is
   monitor id plus the game's client rect, not the screen edge. POE-234 §1
   called the client rect the first step for windowed support; the focus poller
   resolves the game window's HWND on every poll and hands it to
   `capture::remember_game_monitor` on the transition into focus
   (`desktop/src-tauri/src/lib.rs:3421`, `:3474`;
   `desktop/src-tauri/src/capture.rs:277-292`); the handle is not retained today,
   so POE-272 must keep it or re-resolve it. The hypothesis
   `ui_scale = client_height / 1200` is pending measurement in POE-267; it is
   not yet an invariant (POE-266).

3. **Seed → verify → remember.** Placed rectangles are trusted at start. On
   first use, the module verifies them with its existing cheap check: Temple's
   `anchor::recheck` NCC check (`desktop/src-tauri/src/temple/anchor.rs:604-624`)
   and Merc's `cellfit::refine` plus the Wager/Recruit text anchors inside the
   crop (`desktop/src-tauri/src/mercenary/cellfit.rs:249-263`,
   `desktop/src-tauri/src/mercenary/geometry.rs:520-630`). A read miss at the
   placed rectangle permits **one** fallback locate on the existing search
   path. A located anchor corroborated by a successful read is REMEMBERED per
   screen key — monitor id plus client rect — in the slice and is dropped with
   that slice on a dimension, monitor or client-rect change, or
   `geometry_recalibrate` (`desktop/src-tauri/src/ssot.rs:814-823`,
   `:894-909`). Nothing else locates (POE-266; owner decision 2026-09-07).

4. **Contradictions are visible, not blocking.** If a remembered anchor
   disagrees with the shipped seed, log it once and surface the informational
   geometry notice with the placed-vs-located bug capture. The user continues
   reading for the session; the seed is corrected in code, not silently copied
   into settings and not used to block a release (POE-266; owner decision
   2026-09-07).

5. **The fixed-position premise replaces drag tracking.** The temple layout
   panel and merc recruit window do not move in-game (owner, 2026-09-07).
   The current, now-retired `window drags` / moved-panel assumptions in Merc and
   Temple are retired by this ADR; the affected module prose is marked
   transitional in `desktop/src-tauri/src/mercenary/geometry.rs:1-10`,
   `desktop/src-tauri/src/mercenary/cellfit.rs:1-26`,
   `docs/TEMPLE-LIFECYCLE.md:20-23` and `docs/TEMPLE-LIFECYCLE.md:188-192`.
   Their locating paths remain only
   as the one-read fallback until POE-269 and POE-270 land
   (`desktop/src-tauri/src/mercenary/geometry.rs:749-789`,
   `desktop/src-tauri/src/temple/run.rs:550-562`; POE-266).

## Consequences

- Until POE-267…POE-272 land, the code still pays the current Temple NCC/sweep
  and Merc locate/re-detect paths; this ADR is the contract for replacing them,
  not a claim that implementation has shipped (`desktop/src-tauri/src/temple/run.rs:2969-2979`,
  `desktop/src-tauri/src/mercenary/run.rs:3035-3045`, POE-266).

**Superseded by the POE-269 amendment below.**

- A fallback that finds a panel elsewhere is read for the session and raises
  the geometry notice. The remembered anchor is slice-local and is discarded
  on its screen-key change or Recalibrate; it is not a persisted machine
  override (POE-266; owner decision 2026-09-07).
- The client-rect capture path and the `client_height / 1200` measurement are
  follow-up work, so the existing monitor/capture path remains the available
  integration seam until POE-267 and the windowed task land
  (`desktop/src-tauri/src/capture.rs:194-201`, `:277-292`, POE-234 §1).
- Lab's current manual region path is transitional: `effective_region` and
  `Settings.gem_region` / `font_region` still exist while POE-268 is pending;
  that task deletes them (`desktop/src-tauri/src/lib.rs:83-108`,
  `desktop/src-tauri/src/settings.rs:20-47`; POE-268).
- A seed that is wrong for both machines is a code defect. The bug capture is
  the owner-facing correction path; the release user is not made to wait for a
  seed fix (POE-266; owner decision 2026-09-07).

## Amendment: POE-268 landed the placement projection (2026-09-07)

POE-268 is implemented. `ssot::placements(&ScreenSlice)` now exists as the
single derived source for the Lab, Temple and Merc placement values, and the
snapshot exposes it without persisting a second geometry owner.

The Lab migration is complete: `effective_region` and
`Settings.gem_region` / `Settings.font_region` are gone, along with the old
user-rectangle precedence rule. Lab rows derive from the same projection as
the OCR loop and no longer accept a manually placed rectangle.

The current capture still supplies the full monitor capture as `ScreenSlice.client`.
POE-272 owns the live client-rectangle read and its refresh rule for focused
window moves and resizes; this amendment does not pull that follow-up into
POE-268. The Temple and Merc locating callers remain transitional until
POE-269 and POE-270 land, as the original Status caveat states.

## Amendment: POE-269 placed-origin fallback (2026-09-07)

POE-269 is implemented. The Temple loop now derives its `CheapHint` from the
current `ScreenSlice` placements and runs one windowed NCC at the placed
Entrance origin on each detect tick. A below-floor placed miss gets one pyramid
fallback per `(temple_epoch, temple_rearm)` key for any `ArmSource::Trigger(_)`
arm; a null or unplaced slice uses the same key and releases it when a fallback
leaves the slice unfilled. A fallback origin outside the placed tolerance is
logged, noticed, read successfully and remembered through `ssot::remember_anchor`;
a null-slice fallback origin is remembered the same way after a successful read.
The old sweep cadence, coarse candidate pass, geometry cap and session plate
memory are retired.

That release buys one retry only; a second withheld sweep keeps the key spent
until the `(temple_epoch, temple_rearm)` key changes.

## Amendment: POE-270 — Merc placed crop and geometry rows (2026-09-07)

POE-270 is implemented. Merc's normal detect now derives its padded OCR crop
from `ssot::placements(&ScreenSlice).merc.panel`; the full-screen path is only
the one-shot fallback. `cellfit::refine` still measures the gold support frame,
settles the scale and writes the screen slice; it does not locate the panel.

The fallback key is `(merc_refit, trigger_generation)`. A missing or unplaced
screen slice, or a placed crop whose Wager/Recruit/button anchors are absent
while the voice gate is active, spends one full-screen locate for that key. A
successful merc read has no post-read withheld measurement to repay: the only
`accepted == false` result at this seam is `MercOcr` drift refusal, so it does
not release another locate. A located panel beyond the named half-cell origin
tolerance is read for the current session and remembered through
`ssot::remember_anchor` only after that read succeeds; the contradiction and
the single geometry notice are logged at the merc detect seam.

The live placed layout enumerates row centres from the placed panel, the fitted
row pitch and the fitted cell size: the first is `panel.y + pitch + cell/2`,
and the last is the smaller of `panel.bottom - 3*pitch - cell/2` and
`button_y - pitch`. The interval is rounded to a count and capped by
`MercGeometry::max_rows`; every geometry band is then read by pass 2. A text
that does not resolve to a skill remains an unread row. The left skill-icon
column is counted with the existing occupied-cell stddev gate, and the debug
screen artifact retains the geometry-band diagnostic; together they retain
POE-273's two independent diagnostics.

Supersession pointers:

- The original Merc whole-screen primary detect, crop selection and crop-to-full
  retry in Status and Consequences are superseded by this amendment's placed
  crop plus keyed fallback.
- The original Merc panel-anchor/drag-tracking description in Decision 5 is
  superseded by the placed geometry and `cellfit::refine` scale-verifier roles
  above.
- Amendment POE-268's statement that Merc remained transitional until POE-270
  is superseded; POE-269 remains the Temple pattern this amendment mirrors.

## Amendment: POE-270 fix round (2026-09-08)

The merc seed's x is centre-anchored. Two machine fixtures measure the x
anchor; the y top anchor remains provisional from one machine. The placed path
now rejects a pass-1 name-column median that moves beyond the half-cell band,
then spends the keyed full-screen fallback. Chrome-less fallback detects pass
the known session/placement panel to the anchor rescue.

The icon sensor samples one pitch above and below the enumerated rows, publishes
`rowsOnScreen` and `rowsRead`, and the strip reports the gap only when the
sensor is ahead. Dark trailing skill-icon rows are trimmed before publication
when a placed geometry seed has no button-line bound. A null-slice full detect
is budgeted even while the gate is resting, and a probe hit hands its translated
OCR lines to the detect instead of OCRing the same crop again.

The debug dump replays `placed_layout` when its report carries a matching SSOT
placement and draws the resulting geometry bands. The crop is about 45% of the
screen area, an accepted approximately 2× OCR saving; the 4,504 ms figure is
the full-screen baseline quoted in the README/module documentation, not an
order-of-magnitude crop claim. The crop timing line is emitted once after the
whole crop tick, and row-mismatch logging is change-gated.
