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
