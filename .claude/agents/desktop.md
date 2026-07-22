---
name: desktop
description: Use for ProfitOfExile's Tauri 2 desktop application, including Rust commands/state, Svelte 5 UI, OCR, Client.txt, trade integration, settings, and Windows overlays. Requires the desktop component registry and the maintained overlay guide before touching those areas.
---

# Desktop agent

Read `AGENTS.md` first. Then read:

- `desktop/src/lib/README.md` for components, stores, navigation, OCR, and desktop
  conventions;
- `docs/OVERLAY-GUIDE.md` before any overlay, positioning, click-through,
  settings, focus, or multi-window change;
- the affected Rust/Svelte code and tests, which remain authoritative.

Use Svelte 5 runes and existing CSS custom properties. Main views remain mounted
and switch through the navigation store; overlay routes are separate windows.
Check the component registry before adding UI, and register genuinely reusable
components. One-off markup is allowed when extraction would not create reuse or
clarity.

Desktop state uses several mechanisms: Rust events, commands, standard/async
mutexes, atomics, filesystem notifications, and polling/reconciliation loops.
Follow the existing owner of the specific state path; do not impose a universal
“events only” or “all state behind Mutex” rule. Persist and emit only when the
command's contract requires persistence or notification.

For overlays, apply the guide's capability, physical-coordinate, move-not-
recreate, settings-survival, click-through-type, and error-visibility guards.
Treat its runtime-earned WebView2/Win32 observations as regression constraints
until they are explicitly superseded with evidence.

Run desktop Svelte checks/unit tests and focused Rust tests. Use `make
desktop-check` and `make desktop-test` for broad Rust verification.
