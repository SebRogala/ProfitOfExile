---
uid: a6350460-c1ae-46bc-81e5-284082d47cb1
---

# ADR-014: Desktop Features Are Modules with a Work Toggle and a View Page

## Status

Accepted. Carries one **proposed** amendment — see
[Amendment note (proposed, POE-246, 2026-09-03)](#amendment-note-proposed-poe-246-2026-09-03)
at the end: a module's arming clock should measure absence of its subject
rather than age of the signal. Nothing in the Decision below has moved.

## Context

New desktop capabilities keep arriving (mercenary triage now; CX flippables,
Temple advisor, LabCompass planned), and each one carries background work —
OCR capture loops, Client.txt watchers, trade refreshes — that costs CPU and
API budget whether or not the user cares about that feature this session.
Before this decision there was no shared lifecycle: the existing background
tasks (log watcher, focus poller, OCR loops) are unconditional, spawned in
`.setup()` with three different ad-hoc cancellation idioms (generation
counters, mpsc stop channels, a watch channel), and no way to switch a
feature's work off short of a code change. Feature UI was equally ad hoc:
overlay windows have per-overlay enabled flags, page-level features have
none.

Separately, commit `058fca3` established a hard lesson: when a feature's data
collection lives inside its UI toggle, turning the UI off silently kills the
collection. UI visibility and background work are different axes and must not
share a switch by accident.

## Decision

A desktop feature is a **module** with two independent surfaces and a shared
state contract:

1. **The work toggle** — a Sidebar "Modules" row driving `set_module_enabled`.
   It governs ONLY the module's Rust background tasks, through the registry in
   `desktop/src-tauri/src/modules.rs`: watch-channel cancellation, a
   grace-then-abort reaper, spawn/stop decided by a pure `reconcile` over the
   persisted enablement map. The registry's module doc is the normative
   add-a-module recipe (lock order, thread-poll rule, logging seam).
2. **The view page** — a Sidebar nav item + route (the Lab Farming pattern).
   Everything viewable lives there: rules/settings/verdicts/overviews. The
   page is browsable while the module is off; reading never requires running
   the work.
3. **Shared state flows only through SSOT slices** (`AppSsotSnapshot`,
   polled per window). The toggle and the page never share component state or
   props; a module's slice stays registered as inert memory while disabled.

Each module declares `disabled_means` explicitly: `NoWork` (the flag stops the
tasks) or `NoWindow` (work is unconditional — reconcile structurally cannot
stop it; the flag only gates windows). This encodes the `058fca3` rule in the
type system instead of in reviewers' memories.

Enablement persists as a delta against registry defaults — an untouched
module follows a future default change; an explicit choice survives it.

## Consequences

- Adding a feature is bounded: spawn fn + `ModuleDef` entry + Sidebar label +
  a page route. No new lifecycle plumbing, no new persistence pattern.
- Unused features cost nothing: no background task runs, and the webview side
  polls only from open windows.
- The four legacy overlay flags (`comparator/compass/pathstrip/timer` +
  `lab_overlays_enabled`) and the unconditional core tasks (log watcher,
  focus poller) are NOT modules and stay as they are; migrating them is a
  deliberate future decision, not a side effect.
- The `modules` slice reports intent, not liveness — a crashed task still
  shows enabled. Health surfacing needs a task-exit signal (future work).
- Module stop-time cleanup is best-effort and never runs on app exit
  (`std::process::exit(0)` on close); no module may rely on its cancel path
  for durability.

First consumer: mercenary triage (POE-165) — Sidebar toggle runs its OCR
capture; the Mercenaries page holds per-source rule panels, verdicts, and
settings.

Currency Exchange (the "CX flippables" of Context above) landed in POE-176 as a
plain view page with no module and no work toggle: all of its work happens
server-side — the ranking is recomputed there and pushed over Mercure — so it
needs surface 2 and has nothing for surface 1 to govern. A feature with no
Rust-side background work does not get a `ModuleDef`.

## Amendment note (proposed, POE-246, 2026-09-03)

Status of this section: **proposed, not accepted.** It records a rule POE-246
shipped (commit `0dde882`) and argues it belongs in the Decision above. Nothing
in the Decision moves until an owner takes it.

### The proposal

**A module's arming clock measures ABSENCE of the thing it works on, not age of
the signal that announced it.**

The Decision above gives a module one work toggle and leaves what it does while
enabled entirely to the module. The temple filled that gap in POE-242 with a
capture gate armed by Client.txt: an Alva voice line arms the loop, and a timer
started at that line stands it back down. POE-246 measured what that produces.

### The incident

Observed 2026-09-03 on the laptop: `capture armed by Re-arm` 14:30:24 →
`layout panel found` 14:36:14 → `capture stood down — waiting for Alva` 14:37:00
**with the panel still open**. Separately, the module toggled on at 17:28:31 with
the panel open stood down in the same second, and the owner saw the overlay
"blink and disappear".

Both are one mistake. The clock ran from the ANNOUNCEMENT (the voice line, the
toggle) rather than from the SUBJECT (the panel on screen), so the module stood
itself down while looking straight at the thing it exists to read. POE-242's
disarm is a pure status write that leaves `layout` and `advice` standing, and
the overlay gate reads status alone — so the visible symptom was the overlay
vanishing with a full, correct board still in the slice.

### The rule as shipped

`temple::trigger::arm_source` takes `ArmSource { Trigger(reason) | PanelOnScreen
| StartupProbe }` and `PANEL_TAIL_MS` (120 s) runs from the tick the panel was
**last seen** (`LoopState.panel_seen_ms`, written only by an anchored tick — a
miss passes `None` and cannot extend the clock). Leaving the zone ends the claim
at once rather than two minutes later: a `You have entered <not the temple>` line
stamps `left_area_ms` on the same clock, and the panel branch requires
`seen > left_area_ms`, because a sighting is a claim about a screen the player
has left. A start-up probe gives a starting loop exactly one tick to notice a
panel that is already there, spent whatever it finds.

### Why it is a candidate for this ADR rather than a temple detail

The temple is the first module whose work is gated on a signal at all, but the
shape is not temple-specific: any module that arms on an event and disarms on a
timer has the same choice of clock, and the merc capture's voice-line gate is
the nearest neighbour. Encoding it here would put it beside `disabled_means`,
which is the other place this ADR turns a `058fca3`-class lesson into a rule
rather than into reviewers' memories.

**What is not settled**, and why this is proposed rather than accepted: whether
"absence of the subject" generalises to modules whose subject is not a
rectangle on screen, and whether a module with no cheap way to observe its
subject every tick is expected to pay for one. The temple gets its answer free
(the anchor is already running); a module that would have to add a probe to
answer it has a cost this note has not weighed.
