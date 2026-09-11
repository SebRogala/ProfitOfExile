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
leaves the slice unfilled (**the null-slice half is amended by the POE-275
amendment below**). A fallback origin outside the placed tolerance is
logged, noticed, read successfully and remembered through `ssot::remember_anchor`;
a null-slice fallback origin is remembered the same way after a successful read.
The old sweep cadence, coarse candidate pass, geometry cap and session plate
memory are retired.

## Amendment: incursion arms buy no placed-miss fallback (2026-09-09)

The placed-miss fallback is spent under the Manual arm (Re-arm) only. Under
AlvaStart and TempleArea the first tick's miss is the sheet not being open
yet, and the sweep it bought ran 30–34 s per incursion on the debug build with
no tick in between (app.log 2026-09-08/09). The null-slice fallback is
unchanged (**amended by the POE-275 amendment below**). `temple/run.rs`,
`cold_sweep_reason`.

That release buys one retry only; a second withheld sweep keeps the key spent
until the `(temple_epoch, temple_rearm)` key changes.

## Amendment: POE-270 — Merc placed crop and geometry rows (2026-09-07)

POE-270 is implemented. Merc's normal detect now derives its padded OCR crop
from `ssot::placements(&ScreenSlice).merc.panel`; the full-screen path is only
the one-shot fallback. `cellfit::refine` still measures the gold support frame,
settles the scale and writes the screen slice; it does not locate the panel.

The fallback key is `(merc_refit, trigger_generation)`. A missing or unplaced
screen slice, or a placed crop whose Wager/Recruit/button anchors are absent
while the voice gate is active, spends one full-screen locate for that key
(**the placed-crop half is superseded by the POE-278 amendment below**). A
successful merc read has no post-read withheld measurement to repay: the only
`accepted == false` result at this seam is `MercOcr` drift refusal, so it does
not release another locate. A located panel beyond the named half-cell origin
tolerance is read for the current session and remembered through
`ssot::remember_anchor` only after that read succeeds; the contradiction and
the single geometry notice are logged at the merc detect seam.

The live placed layout enumerates row centres from the placed panel, the fitted
row pitch and the fitted cell size (**the centre-source sentence is superseded
by the POE-273 amendment below**): the first is `panel.y + pitch + cell/2`,
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
then spends the keyed full-screen fallback (**the fallback half is superseded
by the POE-278 amendment below**). Chrome-less fallback detects pass
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

## Amendment: POE-273 — OCR-seeded placed rows and vertical-pitch drift (2026-09-08)

The PC replay measured the placed-row defect. At UI scale `0.9009`, the live
fallback used the old `49.3 * scale = 44.42` px pitch and produced centres
`618.2, 662.7, 707.1, 751.5, 795.9, 840.3`; the frame fit found only two
same-column cells and declined with `NoLeverArm { span: 0 }`. The OCR lines in
that replay were centred at `616.5, 659.5, 703.5, 746.5, 790.5, 833.5`, with
43.0–43.7 px gaps. `cellfit::REF_PITCH = 48.67` is the horizontal slot-pitch
constant measured from the support frames; `MercGeometry::row_pitch = 49.3`
is the vertical OCR reference. The committed reference fixture measures a
48.4 vertical row pitch and a 48.67 horizontal slot pitch, so 49.3 is 1.8 %
high; the PC's 47.8–48.6 reference px agree with that fixture, not with 49.3.
POE-270 was the first code to PLACE rows with the known-high constant instead
of only dividing by it, and the drift — about 1 px per row, 44.42 used
against the 43.0–43.7 measured — accumulated beyond the fit's ±3 px vertical
search. The two constants are different axes; correcting
them is POE-216.

The accepted rule is now: geometry determines the row COUNT, including the
footer/button bound and `max_rows`; a pass-1 skill-name line can neither add nor
remove a row. For each geometry row, matched non-anchor name lines whose x is
in the skill-name band and whose centre is within `0.45 * pitch` replace the
geometry centre with their mean. A row with no matched line keeps its geometry
centre and remains an unread row if its name is not read.

The pitch used for both the geometry count and the matching band is a held
`session.fitted.pitch` when the session holds one, else
`MercGeometry::row_pitch * scale` (49.3 by default; an explicit `rowPitch`
override remains the OCR fallback). The held pitch is the frame's horizontal
slot pitch, used as the vertical proxy because it is the only frame-verified
length the session holds: the value it holds is `REF_PITCH · scale` in screen
px, and 48.67 in reference space is closer to the fixture's 48.4 vertical row
pitch than 49.3 is. POE-216 resolves this axis crossing. `cellfit::REF_PITCH`
remains the horizontal frame-fit unit, and `cellfit::refine` remains the only
writer of frame-fit x, cell size and scale, so OCR centres seed the y
registration without replacing the frame verification.

The row-mismatch message remains change-gated: a correct read produces no line,
and an unchanged mismatch is not re-emitted on each detect tick.

## Amendment: the gem tooltip region is a fixed rule, not a scaled literal (2026-09-08)

Owner ruling: the hovered gem's name prints anchored to the top border of the
screen, so the lab gem region is not an anchor (there is nothing on screen to
anchor it to) and not the shipped `[30, 45, 550, 75]` literal (which also took
the tag and level lines). It is `[client.x, client.y, client.w −
INVENTORY_PANEL_W_REF · ui_scale, GEM_NAME_BAND_H_REF · ui_scale]`: from the
screen's left edge to the right-docked inventory's left edge, exactly one name
line tall, never taller. The two reference px (731 and 44) are PROVISIONAL,
measured off one 1920×1080 screenshot at ui_scale 0.90 (inventory frame at
≈1262 px, name band ≈40 px), to be corrected from the OCR Regions preview on
the game. An unmeasured screen gets the same rule at 1080p, `[0, 0, 1262, 40]`.
The POE-233 per-user override deleted by POE-268 is not restored; the rule
replaces it.

## Amendment: the font panel region is a fixed rule, centred left of the inventory (2026-09-09)

The Divine Font panel is the second lab region to leave its scaled literal
behind. The shipped `[460, 270, 530, 350]` (and its reference-px derivation
`FONT_PANEL_REF`) started 150 px right of the option icons, began below the
first option's line and stopped 90 px above "Crafts Remaining" on the owner's
1920×1080 screenshot of 2026-09-09. The region is now a rule in the same terms
as the gem rule: the panel opens centred in the space left of the right-docked
inventory (panel, CRAFT button and "Crafts Remaining" box all centre at ≈630 px,
the space's centre at 631) and top-anchored, so `lab.font` is `[client.x +
(space − FONT_PANEL_W_REF · ui_scale) / 2, client.y + FONT_PANEL_TOP_REF ·
ui_scale, FONT_PANEL_W_REF · ui_scale, FONT_PANEL_H_REF · ui_scale]` with
`space = client.w − INVENTORY_PANEL_W_REF · ui_scale`. It is the panel's
text-bearing interior from under the title bar's rule to under the "Crafts
Remaining" box, wheel and CRAFT button included, because `font_parser` reads
both texts from one crop and is keyword-anchored. The three reference px (740,
244, 757) are PROVISIONAL: width and top measured off that one screenshot at
ui_scale 0.90 (body interior ≈300–965 px, top 220 px); the height is that
four-option panel's 587 (bottom 748 px) plus room for the two further options
the panel can list — the chrome's top stays put and the list grows downward
(owner, 2026-09-09: six options at most) — two rows at the measured 57-px pitch
each allowed to wrap; the six-option height itself is unmeasured, and a
six-option panel in the OCR Regions preview corrects it. The centring itself is inferred from a 16:9
screen where it equals a fixed left offset; the 1920×1200 laptop's preview
discriminates (centred x ≈ 224 vs ≈ 331 at ui_scale 1.0). An unmeasured screen
gets the rule at 1080p, `[298, 220, 666, 681]`; no lab region has a shipped
literal any more.

## Amendment: the gem band spans the full client width (2026-09-09)

The gem tooltip band no longer stops at the inventory's left edge: `lab.gem` is
`[client.x, client.y, client.w, GEM_NAME_BAND_H_REF · ui_scale]`. The
2026-09-08 amendment's right bound assumed the name prints left of the
inventory; the tooltip in fact follows the hovered item horizontally, so a gem
hovered in the inventory prints its name across that edge. Evidence (owner's
app.log, 2026-09-09 01:01): the band read `EXPLOSIVE CONCOCTION OF DESTRUCTIC`
and, three times over 30 s, `POISONOUS CONCOCTION O` — the same cut on every
read, so the band's edge and not OCR noise — and the third gem was never
detected. The inventory frame's top 40 px carry ornament only. Owner decision
2026-09-09. `INVENTORY_PANEL_W_REF` stays: the font rule centres the Divine Font
panel in the space left of that edge. An unmeasured screen gets `[0, 0, 1920,
40]`.

## Amendment: POE-278 — merc placed misses buy no locate outside a manual scan (2026-09-11)

The capture contract — one full read, at most two rounds on the unknown only,
then liveness only; a live capture's geometry moved only by a manual scan — is
stated once in
[ADR-025](025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md).
Its clause 4 governs every full-frame locate this ADR describes — Decision 3's
one fallback locate and each amendment's.

For merc, two sentences above become Manual-only (Scan now, Recalibrate):

- the POE-270 amendment's "a placed crop whose Wager/Recruit/button anchors are
  absent while the voice gate is active, spends one full-screen locate for that
  key" — the missing or unplaced screen slice in the same sentence is ADR-025's
  cold start and stands;
- the POE-270 fix round's "rejects a pass-1 name-column median that moves
  beyond the half-cell band, then spends the keyed full-screen fallback" — the
  rejection stands as a placed miss; outside a manual scan it re-locates
  nothing.

It follows that the POE-270 amendment's remembered located panel
(`ssot::remember_anchor` after a successful read) comes only from a manual
scan's locate. Incident and evidence: ADR-025 Context (app.log 2026-09-10,
17:25:58, a tooltip-occluded fallback remembered two row pitches low). Merc's
implementation shipped in POE-278 WI-B (`f78786e`); ADR-025's Status records it.

Temple is unchanged: the 2026-09-09 amendment above already spends the
placed-miss fallback under the Manual arm only, and ADR-025 records it as the
temple half of clause 4. The contract is shared; the code is not.

## Amendment: the temple null-slice sweep has a cadence and runs off the loop (POE-275, 2026-09-11)

The POE-269 amendment's null-slice fallback ("a null or unplaced slice uses the
same key") and the 2026-09-09 amendment's "the null-slice fallback is
unchanged" no longer describe current temple behaviour. Since POE-275 WI-2 a
null or unplaced slice sweeps every 3 consecutive clean misses, up to 10 per
`(temple_epoch, temple_rearm)` key, never on the first miss and never over a
live panel; a second found-but-withheld result ends the key's null sweeps; and
every sweep runs off the loop thread while the placed recheck keeps running.
The placed-miss fallback (Manual arm only, once per key) is unchanged, and so
is merc. See [ADR-025](025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md)'s
2026-09-11 amendment and [Temple Lifecycle](../TEMPLE-LIFECYCLE.md), "Cadences
and budgets".
