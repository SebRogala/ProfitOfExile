---
uid: 5b7e5c7a-1e24-4ca4-9573-014ee7bd3452
---

# ADR-021: A Module Draws ONE Window on the Game's Monitor, and Widgets Inside It

## Status

Proposed (POE-223 epic, commits `1847358`…`dbf59de`, amended by
`bb488cf`…`f700ac4`, 2026-09-02, and by `c175946` (POE-244) and `6959e8c`
(POE-245)). Written up in the POE-223 follow-up audit, 2026-09-04.

It stays Proposed until the epic's Windows smoke list passes; three of its items
have never been run on Windows and all three are named in the Consequences below
— the fullscreen WebView2 frame cost (epic smoke item 1), the cross-display
rebuild AS FIXED (epic POE-223, "Windows smoke — follow-up batch additions"
item 10, `docs/OVERLAY-GUIDE.md`'s "Game fullscreen on the secondary monitor")
and `WATCHDOG_PERIOD_MS` (same section, item 9). Everything else in this ADR is
merged and has been exercised in at least one live session (POE-245, POE-248
were both filed FROM one).

Extends [ADR-014](014-desktop-features-are-modules-with-a-work-toggle-and-a-view-page.md):
that ADR gave a module a work toggle and a view page; this one says what a
module's third surface — the thing it draws over the game — is made of.
Bounded above by [ADR-019](019-nothing-a-module-draws-may-cover-what-that-module-reads.md),
which decides what a widget may be drawn ON, and fed by
[ADR-020](020-one-shared-screen-scale-a-module-corroborates-or-withholds.md),
whose slice is what a game-anchored widget is placed from.

Scope, as taken (epic D1): the engine and the temple, its first consumer. The
lab OCR windows and the merc verdict strip keep their own per-surface windows
and migrate in follow-ups.

## Context

Before this batch every overlay surface was its own `WebviewWindow` with its own
persisted rectangle, and click-through was a SINGLETON: `overlay_hook.rs`
tracked one HWND, so registering a second interactive window silently displaced
the first — `install_hook` returned `None` and the earlier window stopped being
interactive — and the `WS_EX_TRANSPARENT` repair, the only thing that undoes
WebView2 stripping the style off its own child HWNDs, reached that one window
alone.

Three things follow from that starting point and together they decided the
shape:

- **Widgets are small and there are going to be many.** A kill callout, a room
  diagram, a door hint, a tier readout. One window each means one persisted
  rect each, one construction each, one click-through registration each, and a
  z-order between them that nothing owns.
- **The hook cannot see z-order.** `mouse_hook_proc` runs inside
  `LowLevelHooksTimeout` (300 ms default, training data) and may not do the
  Win32 enumeration that would answer "which of these two windows is on top".
  So a window-per-widget design owes an answer to "who takes this click" that
  the only component in a position to decide cannot compute.
- **A monitor-sized transparent window is an unmeasured cost.** Epic D9 recorded
  it as unmeasured with a stated plan B (a union-rect window covering only the
  widgets), and it is still unmeasured — see Consequences.

## Decision

**A module draws ONE click-through window, sized to the GAME's monitor, and
everything it draws over the game is a WIDGET inside that window.**

### 1. The window is the game's monitor, and it follows the game there

- `routes/(app)/+layout.svelte` builds it from Rust's `get_game_monitor`, which
  the focus poller answers from the PoE window's own HWND on each transition
  INTO the game (POE-237, `6c5082d`). `overlay/monitor-choice.ts` matches that
  against an `availableMonitors()` entry by POSITION — the two enumerations do
  not share an id space — and falls back to `primaryMonitor()`, which is what
  shipped before POE-237.
- **The same monitor is what `capture::capture_screen` grabs and what
  `ssot.screen` is keyed on.** That is what makes epic D7's unit true:
  window-relative physical px ARE capture px. D7 as written said "primary
  monitor both"; POE-237 replaced the primary with the game's, which is the same
  identity resting on a display the game is actually on rather than on the one
  Windows happens to call first.
- The window has no persisted rect, is not resizable, and is deliberately NOT in
  `RESIZABLE_OVERLAY_LABELS`: `fit_overlay_height` would shrink the canvas every
  widget's persisted coordinate is measured against.
- **A different display is a rebuild, not a move.** Guard 4 in
  `docs/OVERLAY-GUIDE.md` ("move instead of recreate") is about repositioning
  within one display; another display is another canvas — different size,
  different scale factor, different coordinate space for every widget in it. Two
  cases do not rebuild: a notice naming the display the window was already built
  on, and a live widget-config session, which defers to its end because a
  rebuild is a destroy and that window is the surface the user is dragging on.
  A notice arriving while the create is in flight is RECORDED
  (`pendingGameMonitor`) and consumed by `reconcileTempleMonitor` once the
  driver settles; that second path skips the config-session deferral, because
  deferring there would strand the correction for the session
  (`+layout.svelte` ~:838-848).

### 2. Widgets are declared, and their placements are the user's

- The registry is `desktop/src/lib/overlay/widgets/widget-registry.ts`, keyed
  `"<module>.<widget>"`, with shipped defaults in CSS px. Placements persist in
  PHYSICAL, window-relative px in `Settings.widgets`.
- **A stored placement is REBASED, then clamped** (POE-239, `9e553bf`). Every
  placement carries the host size it was made against (`host_width` /
  `host_height`); `rebase()` scales by the two axis ratios before
  `clampToHost()` sees it, so a widget two-thirds across a 3840×2160 monitor is
  two-thirds across a 1920×1080 one. The clamp is last-resort safety only — on
  its own it could pin a widget to an edge and the next Save would write that
  edge back over the user's intent permanently. A row with `0` for either host
  field is UNKNOWN and is never rebased.
- **Two kinds, and the host draws them differently.** A PLACEABLE widget gets a
  positioned box, a persisted rectangle and a config-mode frame. An `anchored`
  one gets none of those — where a callout goes is a function of where the GAME
  drew the thing it points at — so the host contributes the window, the frame
  and the `data-hot` claim, and the module positions its own content. An
  anchored widget carries no persisted position and no Configure placement —
  only a Show row in Settings, because an overlay surface the user cannot switch
  off is the one surface with no control at all.
- **A user-placed rectangle outranks every default**, including a module's
  game-derived one (`defaultsFor`, POE-244). This is the same line ADR-019 draws
  between a PLACER's output and a rectangle the user owns.

### 3. Which window takes a click is decided by SHOWN order, not by z-order

- `set_overlay_clickthrough(label)` registers a window with the shared
  `WH_MOUSE_LL` hook. Registration is not about being interactive — it is what
  lets the hook repair `WS_EX_TRANSPARENT`. Every overlay registers, so every
  overlay is repaired; the singleton is gone (epic D5).
- **Where two windows claim the same click, `hit_test` gives it to the highest
  `shown_seq`** (POE-239, `9e553bf`) — the most recently SHOWN, where shown is
  the window's registration or the false→true EDGE in `set_overlay_has_content`.
  Those are the only two show signals Rust receives. The rule it replaces was
  first-registered-wins, under which the window built FIRST — the one most
  likely to be underneath — took the click.
- **Only the edge counts.** A page re-asserting content it already has is not a
  show, or a window whose sender happens to be chattier would out-rank one the
  user just opened.
- **A config-mode window is SKIPPED, not ranked.** Config mode is exclusive by
  construction: the window is genuinely interactive
  (`set_ignore_cursor_events(false)`) and the webview takes those clicks
  natively, so the hook has nothing to award. The skip must not be inverted into
  "config mode wins" — that would consume the click and re-emit it as an
  `overlay-click`, the one thing the arranging window is not listening for.
- **Hot rects and `has_content` are ONE declaration.** `hit_test` skips a window
  whose `has_content` is false before reading a rect, and the flag starts false,
  so `use-hot-rects.ts` arms it from the rects' own emptiness. A host that sent
  only rects would have every button it draws swallowed by the game, silently.

### 4. Arranging widgets is an in-window session that starts no work

- Config mode is IN-WINDOW (epic D2): the module's own window flips interactive
  and carries Save/Cancel, rather than a `/overlay?sync=` copy. The ordering is
  the contract — raise and show the window, set the Rust flag, THEN emit
  `widget-config` webview-scoped — and `docs/OVERLAY-GUIDE.md`'s
  "Config-mode ordering contract" is normative for it.
- **A session raises the WINDOW and never the module flag** (POE-241,
  `dbf59de`). Arranging widget positions runs no capture loop and no OCR. The
  transient force-enable that an earlier cut of this batch used is deleted, and
  a module-coupled overlay's desired state is
  `(module flag || widgetConfigLive(label)) && feature grant`, with
  `module-lifecycle.ts`'s driver as the one builder either term goes through.
- Because the session never touches the work toggle, **ADR-014 needs no
  exception**: the work/view separation it decides is intact rather than carved
  out.

## Alternatives rejected

- **One window per widget.** It is the obvious reading of "small draggable
  panels", and it fails on the click question above: the arbiter would be the
  mouse hook, and the hook cannot read z-order inside its timeout. It also
  multiplies the two things that have already gone wrong once each here — Tauri
  label cleanup being asynchronous (guard 4's "already exists" failures) and
  persisted-rect restoration (guard 2/3).
- **Z-order from the hook.** Rejected for the timeout, not for taste. The
  substitute — `shown_seq` — is a strictly worse model of what is on top, and it
  is the best one available to a component that must return in under 300 ms.
  Registration order is kept as a tiebreak that one monotonic counter cannot
  actually reach, so the answer is total.
- **First-registered-wins**, the rule POE-239 replaced. It is what a naive
  registry does, and it systematically awards the click to the window least
  likely to be visible.
- **A separate configuration window** (`/overlay?sync=`, the shape the OCR
  region editors use). Rejected because the thing being arranged is the widgets'
  positions IN this window, and a copy would arrange them somewhere else. The
  cost taken instead is a monitor-sized interactive rectangle over the game for
  the length of a session, which is why the layout arms a ten-minute ceiling under
  a session that never ends.
- **A union-rect window** covering only the widgets rather than the monitor.
  Still the plan B recorded in epic D9, not rejected on evidence — it is
  rejected on cost-of-complexity until the frame-cost measurement says it is
  needed, because the union changes every time a widget moves and an anchored
  widget's position is a function of the game.

## Consequences

- **`Settings.widgets` is owned by an `AppState` mutex and must stay OUT of
  `persist_overlay_settings`** — guard 5's stated exception. It travels through
  `from_state`, and carrying the file's copy forward would undo the write
  `set_widget_geometry` just made. `settings.rs` has a test that fails if it is
  ever added there.
- **A placement written from outside the window is announced.**
  `set_widget_geometry` emits `widget-geometry-changed {module}` with
  `emit_to(module)`, and the host re-reads on it outside config mode. Rust does
  not validate widget ids against the frontend registry, so a retired widget's
  stored row is left INERT rather than removed (`temple.board`), and an anchored
  widget's stored rectangle is never consulted again while its `visible` flag
  still is.
- **A webview-scoped `emit_to` needs a webview-scoped listener.** A bare
  `listen()` in a window that is not the target never sees the event; this is
  the failure mode behind both `game-monitor-changed` (`emit_to("main")`) and
  `widget-config`.
- **UNVERIFIED — the fullscreen transparent WebView2 frame cost** (epic D9,
  smoke item 1: frame rate with the temple window up versus the module off). It
  has never been measured and the item is not currently in
  `docs/OVERLAY-GUIDE.md`'s smoke list. If it is material, the union-rect window
  above is the recorded fallback and re-opens this ADR's first Decision bullet.
- **UNVERIFIED — the cross-display rebuild AS FIXED.** The pre-fix path ran on
  Windows: the owner's 2026-09-03 stranded overlay is what POE-245 was filed
  from. The fix (`6959e8c`) has not been run there;
  `docs/OVERLAY-GUIDE.md`'s "Game fullscreen on the secondary monitor" is the
  acceptance, and it is epic POE-223, "Windows smoke — follow-up batch
  additions" item 10.
- **UNVERIFIED — `WATCHDOG_PERIOD_MS = 3000`** (POE-238, `0576894`), the period
  at which the message loop checks whether the `WH_MOUSE_LL` hook is still
  installed. Its own doc comment says it is a guess: the only measured anchor is
  `LowLevelHooksTimeout`'s 300 ms, which bounds how long a proc may take and not
  how often a removal should be looked for. The smoke item that settles it is
  "Hook re-install after a silent removal" (epic POE-223, "Windows smoke —
  follow-up batch additions" item 9) — raise the period if it shows re-installs
  the user never asked for.
- **What a widget may be drawn ON is ADR-019's question, not this one's.** This
  ADR decides the window, the registry, the unit and the click; the never-cover
  rule decides where a placer may put a box, and a stored placement is outside
  both.
- **A third consumer is a migration, not a new engine.** The lab OCR windows and
  the merc strip are the named follow-ups (epic D1). Each arrives as a widget
  registry entry and a host mount, and a surface that cannot express itself that
  way is a new decision rather than an exception folded in here.
