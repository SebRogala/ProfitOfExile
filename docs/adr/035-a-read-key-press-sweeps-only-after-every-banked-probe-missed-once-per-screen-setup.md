---
uid: 1b801afb-7064-4023-99d7-02ed54e38f07
---

# ADR-035: A Read-Key Press Sweeps Only After Every Banked Probe Missed, Once per Screen Setup

## Status

Accepted 2026-11-20 (POE-440; Operator Desk 20.11 18:26, `poe440-unbanked-sweep-1120`).

Amends [ADR-025](025-a-capture-reads-once-re-reads-only-the-unknown-then-stops-only-a-manual-scan-moves-its-geometry.md)
temple clause 4 (see its 2026-11-20 POE-440 amendment). The bank-on-find rule
for the Entrance is
[ADR-024](024-module-ocr-positions-are-placed-from-the-screen-slice-not-located.md)'s
POE-440 amendment.

## Enforcement

Tier: required

Scope: the read-key press path in the desktop app (`read_hotkey::dispatch_press`
and every `PendingRead` that implements `needs_calibration` / `calibrate`).

Summary: Banked probes run first. The whole-frame sweep runs only when every
banked probe missed, at most once per screen setup, and the user sees a status
line while it runs.

## Context

On dev one read-key press took about a minute to resolve to "nothing detected":
the temple reader ran an unbounded whole-frame anchor sweep on every press with
no anchored Entrance origin, on the serial dispatcher thread, so every other
reader waited behind it (POE-440). The Operator's design is calibration first,
once, then banked regions only. Audit A2 of the first POE-440 build found that
calibrating before the probes still swept on presses where another reader would
have matched. The Operator ruled sweep-last-once and asked that the user be told
what is going on.

## Decision

1. **Banked probes first.** Each eligible reader probes only its banked region
   of the one press capture. A miss moves on to the next reader.
2. **Sweep last.** The whole-frame sweep (recalibration) runs only when every
   banked probe missed and a missed reader reports an uncalibrated region
   (`PendingRead::needs_calibration`). After calibrating, that reader probes the
   same frame again.
3. **Once per screen setup.** The budget key is the screen setup — capture
   size, display origin, client size — plus the Recalibrate generation
   (`temple::press::SweepKey`). A sweep that ran and found nothing is spent: no
   later press on that setup sweeps again. A sweep cancelled before it ran is
   not spent.
4. **Re-open triggers.** Recalibrate (a new generation) or a resolution,
   display or client-size change allows one more sweep.
5. **The user is told.** While the sweep runs, the read prompt shows "No
   mechanics found - running a recalibration sweep now." and `app.log` gets
   `Read key: no mechanics found — running a recalibration sweep now`. The
   press's timing line counts the sweep under `calibration`. Text only on the
   existing read-prompt surface; a new visual element needs a Designer seat.

### Where it lives

- Order, sweep gate and log lines: `read_hotkey::dispatch_press`,
  `PendingRead::needs_calibration`, `PendingRead::calibrate`.
- Temple sweep and budget: `temple::press::TemplePending` (`calibrate`),
  `temple::press::SweepKey`, `TempleState::sweep_spent`.
- Status: `read_prompt::begin_sweep`, `desktop/src/lib/read-prompt/view.ts`
  (`RECALIBRATING_TEXT`).

## Consequences

- A press on a calibrated screen, or one where any banked probe matches, runs
  no sweep and stays in milliseconds.
- A screen setup that never shows the sheet costs one sweep, not one per press.
- A wrong placement stays wrong until Recalibrate or a setup change; that is
  the manual fix (ADR-025 clause 4).
