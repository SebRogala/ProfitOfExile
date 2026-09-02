# Tauri v2 Overlay Guide

> **Status: Current implementation guide.** Last verified against desktop code:
> 2026-07-22. Runtime observations that are valuable but not statically provable
> are labelled as such. Earlier debugging detail is preserved in
> [the historical overlay notes](history/overlay-debugging-notes.md).

Read `desktop/src/lib/README.md` for desktop components, stores, navigation, and
OCR lifecycle. The proposed Rust-owned navigation contract is specified in
tracker item `POE-88` (LabCompass fidelity restoration and overlay SSOT).

Switchable background modules are a separate mechanism with its own recipe: see
the module doc of `desktop/src-tauri/src/modules.rs`. The four overlay `enabled`
flags and `lab_overlays_enabled` documented here are NOT modules.

## Non-negotiable regression guards

1. **Capabilities:** every overlay and configuration-window label must appear in
   `desktop/src-tauri/capabilities/default.json`. Missing labels make Tauri APIs
   unavailable to that window.
2. **Physical persistence:** `outerPosition()` and `outerSize()` produce the
   physical coordinates persisted in settings. Restore existing windows with the
   Rust `move_overlay` command, which applies `PhysicalPosition` and
   `PhysicalSize`.
3. **Logical construction:** `WebviewWindow` constructor dimensions are logical.
   Convert persisted physical width/height with Tauri `scaleFactor()` for initial
   construction, then apply exact `PhysicalPosition`/`PhysicalSize` in
   `tauri://created`. Do not use `window.devicePixelRatio` for overlay geometry.
4. **Move instead of recreate:** reposition a live overlay with `move_overlay`.
   Avoid destroy/recreate cycles for position updates; Tauri label cleanup is
   asynchronous and has produced “already exists” failures.
5. **Settings survival:** overlay settings are not rebuilt from `AppState`.
   `persist_overlay_settings` must copy every persisted overlay field, and
   `test_overlay_settings_survive_persist_cycle` must cover additions. WIDGET
   placements are the exception and must stay out of that function:
   `Settings.widgets` IS owned by an `AppState` mutex, so it travels through
   `from_state`, and carrying the file's copy forward would undo the write
   `set_widget_geometry` just made. `settings.rs` has a test that fails if it is
   ever added there.
6. **Error visibility:** log failed invoke, window, and event operations. Do not
   silently swallow failures in overlay paths.

## Overlay types

EVERY overlay calls `set_overlay_clickthrough(label)`, and that call does two
things: it installs `WS_EX_TRANSPARENT`/`WS_EX_NOACTIVATE` behavior on the
window, and it REGISTERS the window with the shared `WH_MOUSE_LL` hook
(`desktop/src-tauri/src/overlay_hook.rs`). Registration is not about being
interactive — it is what lets the hook repair the `WS_EX_TRANSPARENT` WebView2
strips off when it rebuilds child windows. The hook is installed by the first
window to register and torn down when the last one is destroyed.

Which clicks a window CLAIMS is a separate, per-window declaration made by the
window's own page:

- `set_overlay_hot_rects(label, rects)` — window-relative rectangles in
  PHYSICAL pixels (`getBoundingClientRect()` × the window's Tauri
  `scaleFactor()`; the conversion and the change test are in
  `desktop/src/lib/overlay/hot-rects.ts`). A click inside one is consumed and
  re-emitted to that window as `overlay-click {label, x, y}`; everything else
  reaches the game. Declaring nothing — which is what compass, path-strip,
  timer and the merc strip do — makes a window display-only. Listen with
  `getCurrentWebviewWindow().listen('overlay-click', …)`; a bare `listen()` from
  `@tauri-apps/api/event` registers for the `Any` target and a labelled
  `emit_to` does not match it (tauri 2.10.3 `manager/mod.rs:602-628`).
- `set_overlay_has_content(label, has_content)` — a window that is drawing
  nothing claims nothing, whatever it declared.
- `set_overlay_config_mode(label, on)` — while on, the window is genuinely
  interactive (`set_ignore_cursor_events(false)`) and the hook leaves it
  entirely alone: it neither repairs `WS_EX_TRANSPARENT` nor intercepts.

Outside config mode a window's `WS_EX_TRANSPARENT` is never cleared, so a
button appearing and disappearing is a hot rect being declared and withdrawn,
never a transparency flip.

The comparator is the interactive one today: its page declares the button
column (and the trade-queue row while that shows) and maps the emitted
coordinates with `elementFromPoint` + `data-action`. Buttons must be inside a
declared rect and expose that `data-action` metadata; they no longer have to
align with the window's right edge, because the claim is the element's own
rectangle rather than a band measured from the edge.

The hook pairs button-up with button-down: it consumes a release only when it
consumed the matching press, so a drag that started on the game keeps its
release. Two overlapping windows are resolved by registration order, first
match wins — the hook cannot see z-order.

The capture/configuration overlay is deliberately interactive and has different
drag, resize, save, and cancel behavior. Treat it as a third type.

Temple (`temple`) and merc verdict (`mercenary`) are overlays COUPLED TO A
MODULE FLAG rather than to an overlay setting: the module toggle creates and
destroys the window, and `desktop/src/lib/overlay/module-lifecycle.ts` orders
those transitions so a fast off→on→off cannot strand a transparent
always-on-top window. They still appear in the Rust focus poller's game-focus
show/hide list and in `set_debug_mode`'s force-show branch. Persisted geometry
is independent of the coupling: the merc strip has `mercenary_overlay`, and the
temple window has none because it IS the primary monitor (below).

The merc strip's **on-screen lifecycle** (owner decision, 2026-09-01) is: shown
for as long as a recruit window is being worked — `scanning` (the burst after
a voice line or Scan now), `live` (reading), `done` (read paused) — and for
`LINGER_MS` (4 s) after the module goes `idle`, whether that idle is "window
gone" over a retired capture or "waiting" over nothing; then the panel clears
and the whole overlay disappears. The gate is `overlayShown` in
`desktop/src/lib/mercenaries/overlay-view.ts`; the route owns only the clock.
The full contract is in `desktop/src/lib/mercenaries/README.md`.

The merc strip's **height follows content; width and position are persisted**
(owner decision, 2026-08-25). Its route observes its own panel with a
`ResizeObserver` and calls the Rust `fit_overlay_height` command, which converts
the measured CSS height with the WINDOW's own `scale_factor()`, clamps it to the
monitor work area so the strip can never grow over the taskbar, and re-applies
the position along with the size — a resize can disturb position, so the guard
belongs on every call and not just on startup. Width and position are never
touched by that path. `mercenary_overlay.height` is still written (Settings
saves the whole rect) and is then ignored by the real window, which refits on
first paint; the constructor seed in `desktop/src/lib/overlay/overlay-defaults.ts`
only decides what that first frame looks like. A shipped height was wrong twice
over: it clipped the last row, and it was reasoned in CSS pixels while being
applied as physical ones, so it clipped worse the more the display scaled.
Settings → Overlay Positions configures the merc row for **position and width
only**; its config window is given the live overlay's height when one is running
and the seed when the module is off, and that height is LOCKED with
`setSizeConstraints` (width stays draggable — Tauri's `resizable` flag has no
per-axis form, and the guide detail line ellipsises, so width is a real
setting).

`fit_overlay_height` re-asserts click-through after every resize —
`set_ignore_cursor_events(true)` plus `set_noactivate` on Windows, both
idempotent. This is not belt-and-braces: WebView2 strips `WS_EX_TRANSPARENT`
when it creates or updates child windows, and the only thing that repairs it is
the `WH_MOUSE_LL` hook. The hook now repairs every REGISTERED window, the merc
strip included — before the registry it tracked one HWND, the comparator's, and
the strip was simply unprotected. The re-assert stays anyway, because the repair
is driven by mouse events over the window: a resize that rebuilt WebView2's
children would otherwise leave the strip opaque to the mouse until the cursor
happened to cross it. While it is opaque, clicks stop reaching the game, and a
click landing on the strip takes focus, drops `game_in_foreground` and stops the
capture loop.

For the merc overlay, click-through is a correctness requirement, not a
preference. The capture loop reads the screen only while the game is the RAW
foreground window (`AppState.game_in_foreground`), while overlay visibility
follows `game_focused`, which is HELD over our own windows. The two reads are
deliberately never unified — so a click that focused the verdict overlay would
drop the raw flag and stop the loop producing the verdict on screen.

Known exposure (measured, not fixed): `set_overlay_clickthrough` is
fire-and-forget. It spawns a thread that sleeps ~1 s before calling
`set_ignore_cursor_events`, because the WebView2 HWND is not available sooner,
so a newly created overlay is INTERACTIVE for about a second and the caller's
`await` cannot observe a failure in that setup. For display-only overlays this
is a stray click on the panel; for the merc verdict overlay that click also
takes focus and stops the capture loop until the game is in front again.
Closing it means making the command await its own setup and report, which is a
change to the Rust command rather than a second wait in each creation path.

## Widget overlays

A module may instead open ONE fullscreen, click-through window over the primary
monitor and place small panels — WIDGETS — inside it. The temple is the first
(POE-225); the lab windows and the merc strip are not migrated.

- The window is the monitor. `routes/(app)/+layout.svelte` reads
  `primaryMonitor()` (falling back to `currentMonitor()`), constructs at the
  monitor's logical size and applies the exact `PhysicalPosition`/`PhysicalSize`
  in `tauri://created` — guard 3, with the monitor's own scale factor. It has no
  persisted rect, is not resizable, and is NOT in `RESIZABLE_OVERLAY_LABELS`:
  `fit_overlay_height` would shrink the canvas every widget's persisted
  coordinate is measured against.
- The widgets are declared in
  `desktop/src/lib/overlay/widgets/widget-registry.ts`, keyed
  `"<module>.<widget>"`, with shipped defaults in CSS pixels; their placements
  are persisted in PHYSICAL, window-relative pixels in `Settings.widgets`. That
  unit is also capture pixels, because the window and every capture are the same
  monitor — so a user-placed widget and a future game-anchored one need no
  conversion between them.
- `WidgetHost.svelte` owns placement, hot rects and click routing. Any element a
  widget draws with `data-hot` is claimed; one that also carries `data-action`
  is dispatched through `elementFromPoint`. The window declares nothing of its
  own, so a widget overlay with no buttons on screen is display-only.
- **Hot rects and `has_content` are one declaration, not two.** `hit_test` skips
  a window whose `has_content` is false before it reads a single rect, and the
  flag starts false, so `use-hot-rects.ts` sets it from the rects' emptiness —
  armed when the first rect appears, cleared when the last goes away and on
  teardown. A host that only sent rects would have every button it draws
  swallowed by the game, silently. The five older overlays each set the flag
  from their own content rule; a widget window has no separate rule, because
  "drawing something clickable" and "claiming a rectangle" are the same
  statement out here.
- The host is mounted UNCONDITIONALLY by the module's route; the module's own
  content rule is applied inside the `content` snippet, which receives
  `(spec, configMode)`. A host behind an `{#if}` has no `widget-config`
  listener while the module is drawing nothing, so a window flipped into config
  mode then would be genuinely interactive with no Save and no Cancel on it. In
  config mode a widget with no content draws a placeholder carrying its name, so
  an empty frame is still identifiable and still draggable.
- A widget is CONTENT-SIZED until the user drags an edge: Save persists
  `width`/`height` of `0` unless that widget was resized in this config session
  or already had a non-zero stored size, and `placementFor` reads a zero size
  back as "let the content decide" while applying the registry's shipped width
  as a `max-width`. Persisting the measured size on every Save would pin every
  widget in the module the first time any one of them was moved. A stored
  placement is also clamped to the CURRENT window on load, so a rectangle saved
  on a larger monitor cannot render entirely off-screen.

### Config-mode ordering contract

Config mode is IN-WINDOW, not a `/overlay?sync=` copy, and the order the three
steps happen in is what makes it recoverable. Settings (WI-C) does, in this
order:

1. Ensure the module's window EXISTS and is SHOWN — creating it force-shown if
   the module is off or the game is not focused, the `set_debug_mode` path for
   one label. A window that is not on screen cannot be arranged, and one that
   does not exist has nothing to receive the event.
2. `set_overlay_config_mode(label, true)` — the Rust flag first, so the mouse
   hook is already leaving the window alone before it becomes interactive.
3. Emit `widget-config {module, on: true}`, **webview-scoped** to that label.

The host ALSO queries `get_overlay_config_mode(label)` once on mount, chained
onto its `widget-config` listener so the listener is registered FIRST — `listen`
is itself async, and a query that ran beside it could answer false, then miss
the event that arrived before the listener existed. That query is the catch-up
path for step 1 creating the window: such a window has no listener when step 3
fires, and without the query it would sit interactive with no Save and no
Cancel. Because the flag is set before the emit, the query cannot miss it. The
two paths may therefore both fire, which is harmless: entering config mode is
idempotent, so a second Configure press — the user's way out of a window that
somehow missed both — re-emits safely and never reseeds a drag in progress.

The host owns the way out. Save writes every widget of the module through
`set_widget_geometry` — committing only the ids whose invoke RESOLVED, and
staying in config mode with the failure named in the Save/Cancel bar if any
rejected — and Cancel re-reads the persisted map rather than restoring a
snapshot taken on the way in (config mode can begin before the first read has
answered, and restoring an empty snapshot would wipe every placement). Both then
call `set_overlay_config_mode(label, false)` and emit
`widget-config-end {module}`, which is what Settings restores the window's
previous shown/hidden state on. The five per-window config flows above are
untouched.

## Current data and lifecycle behavior

- Shared main-window status is event-driven through `status.svelte.ts`.
- The comparator overlay polls Rust-held comparator data every 500 ms.
- Compass/path-strip settings and layout paths include polling/reconciliation.
- Game focus is determined in Rust by a `GetForegroundWindow` poller; Client.txt
  focus events are not the authority.
- Client.txt uses filesystem notifications with a five-second polling fallback.
- Font panel scanning begins from the `LabFinished` navigation event. The third
  Aspirant's Trial is logged but is not the scan trigger.
- The merc verdict overlay does not render `mercenary.trade` yet (POE-202).
  The search state reaches every window on the slice regardless, and both
  pieces the Mercenaries page draws it with are already cross-window for that
  reason: `$lib/components/TradeListings.svelte` rather than a component
  beside the Comparator, and `$lib/mercenaries/trade-view.ts` rather than
  wording inside the page.

Do not summarize the desktop architecture as “no polling.” Use the mechanism
owned by the specific state path.

## Runtime-earned observations to preserve

The following were recorded during Windows/WebView2 debugging and remain
relevant constraints even though static code inspection cannot prove every
runtime failure mode:

- Cross-window JavaScript window operations and events have returned stale data
  or failed silently; prefer work performed in the owning window, Rust commands,
  or the established Rust-backed polling path.
- WebView2 child HWNDs defeat parent-only `WM_NCHITTEST` handling.
- HWND access immediately after creation can fail; the click-through command
  delays setup while WebView2 initializes.
- `focusable: false`, `WS_EX_NOACTIVATE` alone, and Tauri cursor-ignore APIs did
  not provide selective interaction reliably.
- `onMount` was observed not to run reliably in overlay windows; established
  overlay initialization uses rune effects/listeners.
- Destroying a transparent always-on-top WebView2 window from its own click path
  has left Win32 mouse capture stuck. Configuration Save/Cancel is coordinated
  by the owning main-window flow.
- The WebView2 transparency resize workaround can disturb position, so exact
  physical position/size is applied after creation.

Do not delete these observations merely because a future code path appears
simpler; reproduce the Windows behavior first or supersede them with a dated
regression test/decision.

## Windows smoke checks

Static gates cannot reach these; run them on a Windows build after touching the
named path.

- **Widget overlay, click-through and the hot rect** (POE-225): with the temple
  module on and the game focused, click the game through an empty part of the
  fullscreen window AND through a widget — both must reach the game. Then start
  the app in debug mode and toggle the module off and on, so the window is built
  with `?debug` and the temple advice widget draws its `data-hot` probe: a click
  on the probe must log `hot-rect probe clicked`, and a click one pixel outside
  it must reach the game. A probe that does nothing means the declaration or the
  `overlay-click` listener is wrong; a click beside it that does NOT reach the
  game means the withdrawn/declared rect is too big.
- **Merc strip, after a row-count change** (the content-driven resize path): let
  the strip redraw at a different height — open a recruit window with a
  different number of rows — then, with the read still running, sweep the mouse
  across the strip and left-click on it. The click must reach the game and the
  module status must stay `live`/`scanning`. A click that selects something in
  our window instead, or a status that drops to `idle`, means the resize lost
  `WS_EX_TRANSPARENT` and the re-assert in `fit_overlay_height` is not working.
  A retire no longer serves as the redraw here: it leaves the strip on screen
  for only the 4 s linger, and `idle` throughout it.
- **Merc strip, first paint**: starting the module must not flash a large empty
  panel. The constructor seed is one line tall and the content replaces it. The
  waiting line shows for 4 s after the start and then the strip clears; a strip
  still showing "waiting for a mercenary" a minute later means the idle linger
  (`overlayShown` / `lingerAdvance`) is not being advanced by the route.
- **Merc header, across re-detects** (added after the 2026-08-25 smoke, where
  the header blinked between the mercenary's name and its class every two
  seconds): keep one recruit window open and watch the header line for at least
  three re-detects. Fields may only be FILLED IN, never blanked or swapped for a
  shorter or glyph-prefixed reading, and the name must never equal the class. A
  header that changes back and forth means the sticky merge (`read.rs`'s
  `merge_header`, applied in `run.rs`'s detect tick) is not being applied to the
  published capture.
- **Merc strip, the done state**: with a recruit window open, hover every cell
  the strip marks `?` or `✕` until the status line reads `done · N rows · all
  icons read`. From then on the log says `capture complete — OCR paused` once,
  the strip must stay on screen with its verdict, and closing the window must
  still retire it (up to ~20 s later, two liveness checks): the strip then says
  `recruit window gone — last read` for 4 s and clears entirely. A strip that
  blanks at `done`, or a status that never reaches it on a fully-read window,
  means the on-screen status set (`live` + `done`) or `capture_complete`
  disagrees with what the reader produced; a strip still up more than a few
  seconds after the log says `window gone` means the linger is not running out.
  **Hover still corrects a wrong read while `done`**: park the cursor on a cell
  the module matched WRONG and confirm the tooltip replaces it — the detect is
  what paused, not the hover. The re-read of an already-matched cell is capped
  per cell (`HoverBudget`, 3 per capture), so the correction has to land within
  the first few ticks of the hover; moving off the cell and back does NOT refill
  it, and neither does a retire the module restored from — the budget rides
  along with the confirmations in the retained slot, so a spent cell stays spent
  across a retire and re-detect of the same panel. Only a genuinely new window
  refills it.
- **Merc hover, the occluded panel** (added after the 2026-08-25 smoke, where
  hovering a cell retired the capture two ticks later and the cell flipped back
  to `✕`): with a recruit window open, park the cursor on a support cell for at
  least 5 s so the game tooltip covers the panel, then move off it. The log must
  say `panel occluded (cursor over it) — holding the capture` ONCE per hover and
  must NOT say `window gone`, and the cell must still read `✓` after the tooltip
  closes. A `window gone` under the cursor means the panel rect
  (`geometry.rs`'s `panel_bounds`, stored on the session at detect) is wrong or
  is not being consulted by `miss_kind`. The hold is CONTINUOUS occlusion, not a
  budget that resets per miss: park the cursor on TAKE ITEM and close the window
  with it. Retire lands at the 15 s cap rounded UP to the next detect tick, plus
  one more tick for the second miss — **so check which cadence you are in
  first**, because it dominates the number. A window still being read
  re-detects every 2 s, so it retires ≈20 s after the close; a window the strip
  already calls `done` is on the 10 s liveness cadence, so it retires ≈40 s
  after the close (up to 10 s to notice, misses at +20 s and +30 s). Time it
  from the close and compare against the cadence you are actually in — a `done`
  window still on screen at 60 s, or a live one past ~25 s, means
  `OcclusionRun` is clearing its run on a counted miss and each cap is
  restarting the clock. If the capture does retire — the window really
  closed, or the cap fired — the next detect of the SAME panel must log
  `confirmation(s) and its header restored` and bring back both the `✓` and the
  mercenary's name and level; a `confirmations were dropped` line there means
  `same_panel_positive` found no positive evidence, which a tick that read
  neither a level nor two skill names is expected to produce.
- **Merc rematch / fast swap**: with a capture on screen, press REMATCH (or
  close the window and open a different mercenary within ~20 s, before the
  liveness check retires the first). The header must switch to the NEW
  mercenary's name, class and level in one step, and the log must say `recruit
  window replaced`. A header that keeps the previous mercenary's name — or an
  old level under a new name — means the panel-identity gate (`panel_replaced` /
  `fold_header`) is not being consulted before the sticky merge. The gate wants
  POSITIVE evidence (two levels that disagree, or two disjoint skill sets), so
  the inverse check matters too: a tick that merely read the panel badly must
  NOT log `recruit window replaced`, because that log line means the session's
  hover confirmations were just thrown away.

## Adding an overlay

Start from the closest existing overlay type, then check applicable integration
points rather than cloning every step blindly:

- window and configuration labels in capabilities;
- settings field/default, getter/setter commands, persistence copy, and survival
  regression test when the overlay is configurable;
- creation, exact physical sizing, startup restoration, toggle, and position
  saving in the app layout;
- settings-page maps and sidebar/category controls when user-configurable;
- Rust focus/lab catch-up show-hide lists only for overlays that follow those
  lifecycles;
- an `/overlay/...` route and state catch-up/reconciliation appropriate to that
  overlay type;
- `set_overlay_clickthrough(label)` — for every overlay, so the hook can repair
  its `WS_EX_TRANSPARENT`;
- `set_overlay_hot_rects` from the window's own page ONLY if it has controls the
  player must be able to click; a display-only overlay declares nothing — and
  its `overlay-click` listener must be `getCurrentWebviewWindow().listen(…)`,
  never a bare `listen()` (see "Overlay types").

Verify desktop Svelte checks/unit tests and Rust tests after overlay changes.
