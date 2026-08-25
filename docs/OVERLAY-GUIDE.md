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
   `test_overlay_settings_survive_persist_cycle` must cover additions.
6. **Error visibility:** log failed invoke, window, and event operations. Do not
   silently swallow failures in overlay paths.

## Overlay types

The comparator is interactive. Rust installs `WS_EX_TRANSPARENT`/
`WS_EX_NOACTIVATE` behavior and a `WH_MOUSE_LL` hook through
`set_overlay_clickthrough`. The right-edge interactive width starts at 48
physical pixels and may widen for displayed controls. `HAS_CONTENT` prevents an
empty comparator from intercepting clicks. Buttons must align with the right
edge and expose the `data-action` metadata consumed by the coordinate handler.

Compass, path-strip, and timer overlays are display-only. They still call
`set_overlay_clickthrough`, but with `interactiveWidth: 0`. Do not copy
comparator button/hook assumptions into these overlays.

The capture/configuration overlay is deliberately interactive and has different
drag, resize, save, and cancel behavior. Treat it as a third type.

Temple (`temple`) and merc verdict (`mercenary`) are display-only overlays
COUPLED TO A MODULE FLAG rather than to an overlay setting: the module toggle
creates and destroys the window, and `desktop/src/lib/overlay/module-lifecycle.ts`
orders those transitions so a fast off→on→off cannot strand a transparent
always-on-top window. They still appear in the Rust focus poller's game-focus
show/hide list and in `force_show_overlays`. Persisted geometry is independent
of the coupling: temple has none, the merc strip has `mercenary_overlay`.

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
the `WH_MOUSE_LL` hook, which repairs exactly one window — `OVERLAY_HWND`, the
comparator's. The merc strip is not tracked by it, so a resize that made
WebView2 rebuild its children would leave the strip opaque to the mouse: clicks
stop reaching the game, and a click landing on the strip takes focus, drops
`game_in_foreground` and stops the capture loop.

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

## Current data and lifecycle behavior

- Shared main-window status is event-driven through `status.svelte.ts`.
- The comparator overlay polls Rust-held comparator data every 500 ms.
- Compass/path-strip settings and layout paths include polling/reconciliation.
- Game focus is determined in Rust by a `GetForegroundWindow` poller; Client.txt
  focus events are not the authority.
- Client.txt uses filesystem notifications with a five-second polling fallback.
- Font panel scanning begins from the `LabFinished` navigation event. The third
  Aspirant's Trial is logged but is not the scan trigger.

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

- **Merc strip, after a row-count change** (the content-driven resize path): let
  the strip redraw at a different height — open a recruit window with a
  different number of rows, or let a live capture retire — then sweep the mouse
  across the strip and left-click on it. The click must reach the game and the
  module status must stay `live`/`scanning`. A click that selects something in
  our window instead, or a status that drops to `idle`, means the resize lost
  `WS_EX_TRANSPARENT` and the re-assert in `fit_overlay_height` is not working.
- **Merc strip, first paint**: starting the module must not flash a large empty
  panel. The constructor seed is one line tall and the content replaces it.

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
- `set_overlay_clickthrough` with the correct interactive width.

Verify desktop Svelte checks/unit tests and Rust tests after overlay changes.
