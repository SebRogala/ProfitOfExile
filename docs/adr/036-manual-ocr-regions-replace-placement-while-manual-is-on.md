---
uid: d0be519f-626b-4f36-9f31-2e8edfd326d6
---

# ADR-036: Manual OCR regions replace placement while Manual is on

## Status

Accepted (2026-11-24) (POE-411), per Operator acceptance 2026-11-24.

This ADR is the normative home for Manual OCR geometry. This document does not
claim implementation or delivery.

## Enforcement

Tier: required

Scope: Manual OCR geometry, region readers, the region editor and Recalibrate.

Summary: Proposed Manual-only exception to the accepted placement contract: readers use the stored manual set, only the region editor moves its rectangles, Recalibrate is refused, and screen mismatches are informational.

The tier identifies the proposed system contract; it does not accept the
proposal. The decisions below describe proposed behavior.

## Context

[ADR-024](024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md)
Decision 1 says geometry is derived and “never persisted as a second settings
geometry.” Its gem-tooltip amendment of 2026-09-08 says “The POE-233 per-user
override deleted by POE-268 is not restored.” Manual requires an explicit,
mode-scoped exception to both clauses.

POE-411 records a controller layout on a 3440×1440 screen at UI scale 1.2
whose Font panel sits centre-right, outside the Auto seed crop. The approved
brief replaces the original tracker approach's controller profile and
client-relative, drop-on-resolution-change fallback with exclusive Auto and
Manual modes and capture-relative storage.

## Proposed decision

### Geometry source and coordinate space

Precedence is **Manual set > remembered anchors > seed**. While Manual is on,
the single region resolver takes the stored manual rectangles and ignores
remembered anchors and the slice scale, including when the slice is absent
or mismatched. Readers consume that resolved geometry. The two ADR-024
prohibitions quoted above do not hold in Manual; Auto retains ADR-024's
accepted placement contract and its amendments.

Manual rectangles are capture-relative physical pixels, expressed as
`[x, y, width, height]` from the capture's top-left corner. They stay fixed
when the game client moves and are not rescaled from the Auto slice. Geometry
derived from a manual rectangle uses that rectangle's dimensions rather than
the slice scale.

For the controller example on 3440×1440, stored
`lab_font = [1820,293,888,908]` supplies the Font reader's rectangle,
instead of the Auto seed `[837,293,888,908]`. Remembered anchors or a present
slice scale do not change that answer in Manual.

### Recalibrate and geometry movement

Manual refuses Recalibrate in the backend and disables it in the UI.
[ADR-025](025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md)'s
“only a manual scan moves its geometry” reads, in Manual, as **only the region
editor moves it**. A read, runtime locate or remembered-anchor update cannot
replace the manual geometry. Auto keeps the accepted geometry lifecycle.

### Screen mismatch

The manual set records the monitor size and virtual-desktop origin on which
it was saved. A difference in the live monitor's size or origin shows an
informational warning, following ADR-024 Decision 4's “visible, not blocking”
principle. The warning neither discards the set nor blocks reads; readers
continue resolving the stored set. Client size and movement do not trigger
this monitor comparison.

Capture-edge handling remains a separate read-safety constraint: free crops
are clipped to the capture; wholly outside rectangles and scale-bearing
rectangles crossing its edge are not read. A clipped crop never replaces the
stored rectangle or supplies a smaller derived scale. The mismatch warning
itself is not a read gate.

### Region editor

Under Operator decision 8 (Desk `poe411-plan-1124`, RULED 2026-11-24), the
player first selects one region in the region picker. The editor draws only
that selected region and its Auto reference when available, and moves or
resizes that region alone. There is no all-regions view. When no Auto
reference is available, no Auto rectangle is drawn.

Save calls `set_manual_ocr_regions` once with the whole set, in which only
the selected region's rectangle changed. All other rectangles stay unchanged,
including the gem-slot strip when the Font panel is selected. This ruling
controls over the contextual plan's older all-region wording and Font/slot
move coupling.

## Consequences

Manual provides a geometry fallback for layouts the Auto rules do not cover.
The player owns placement and must edit rectangles when a screen change makes
them unsuitable. Auto arithmetic, seeds and remembered-anchor behavior remain
under the accepted ADRs. Implementation and in-game acceptance are subsequent
work gated on Operator acceptance of this proposal.

## References

- POE-411 — Controller UI layout and manual OCR region fallback; WI-1 drafts this ADR only.
- Operator Desk `poe411-plan-1124`, 2026-11-24 — approved draft scope and decision 8's single-region editor ruling.
