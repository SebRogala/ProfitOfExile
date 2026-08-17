# ADR-014: Desktop Features Are Modules with a Work Toggle and a View Page

## Status

Accepted

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
