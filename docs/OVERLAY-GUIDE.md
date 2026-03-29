# Tauri v2 Overlay Windows — Implementation Guide

This document captures everything learned from ~10 hours of debugging overlay windows in Tauri v2 with WebView2 on Windows. Follow this guide to avoid repeating these mistakes.

## Architecture Overview

The overlay system uses transparent, always-on-top Tauri WebviewWindows that display over the game. The key challenge: making them **click-through** (so the game receives clicks) while keeping **specific buttons interactive**.

### Window Stack
```
[Game / Other Apps]          ← receives clicks through overlay
[Overlay Window]             ← transparent, always-on-top, WS_EX_TRANSPARENT
  ├─ Content area            ← visual only, click-through
  └─ Button column (right)   ← interactive when cursor hovers
```

## Critical Gotchas (read before touching overlay code)

### 1. Window Labels Must Be in Capabilities

Every new overlay window label **MUST** be added to `capabilities/default.json` in the `"windows"` array. Tauri v2 scopes permissions by window label. If a label isn't listed, **ALL Tauri APIs silently fail** — `startDragging`, `destroy`, `show`, `hide`, `outerPosition`, etc. No error, just nothing happens.

```json
// capabilities/default.json
"windows": ["main", "overlay", "comparator", "overlay-comparator-pos"],
```

**Symptom**: Dragging doesn't work, window operations silently fail.
**Fix**: Add the window label to the array.

### 2. Cross-Window JS API Calls Return Wrong Values

In Tauri v2, calling `outerPosition()`, `outerSize()`, `destroy()`, `close()`, `setPosition()` on a WebviewWindow **from a different window context** returns wrong/stale values or silently fails.

- `outerPosition()` from the parent window returns the **initial** position, not the current one
- `destroy()` / `close()` from the parent window may not actually destroy the window

**What works**: `getCurrentWebviewWindow()` from **within** the overlay's own JavaScript context always returns accurate values.

**Workaround for position saving**: The overlay page saves its own position using `getCurrentWebviewWindow().outerPosition()` (via a periodic interval or event).

**Workaround for destroying**: Use `WebviewWindow.getByLabel()` with a retry loop:
```typescript
const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
for (let i = 0; i < 5; i++) {
    const existing = await WebviewWindow.getByLabel('comparator');
    if (!existing) break;
    try { await existing.close(); } catch (_) {}
    try { await existing.destroy(); } catch (_) {}
    await new Promise(r => setTimeout(r, 100));
}
```

### 3. HWND Not Available Immediately After Window Creation

`window.hwnd()` in Rust returns `Err("the underlying handle is not available")` if called immediately after the window is created. WebView2 needs time to initialize.

**Fix**: Delay HWND access by ~1 second:
```rust
let app2 = app.clone();
std::thread::spawn(move || {
    std::thread::sleep(std::time::Duration::from_millis(1000));
    if let Some(window) = app2.get_webview_window(&label) {
        if let Ok(hwnd) = window.hwnd() {
            // Now safe to use
        }
    }
});
```

### 4. WM_NCHITTEST Does NOT Work with WebView2

WebView2 creates its own child HWNDs (`Chrome_WidgetWin_0`, `Chrome_WidgetWin_1`, `Intermediate D3D Window`) that handle hit-testing **independently**. Subclassing the parent HWND with `WM_NCHITTEST -> HTTRANSPARENT` has **no effect** because WebView2's child windows intercept mouse input before the parent window procedure sees it.

This is a documented WebView2 limitation:
- [WebView2Feedback#446](https://github.com/MicrosoftEdge/WebView2Feedback/issues/446)
- [WebView2Feedback#1004](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1004)

### 5. SetWindowSubclass Must Run on the Window's Thread

`SetWindowSubclass()` must be called from the same thread that created the window. Calling it from a background thread (e.g., inside a `std::thread::spawn`) silently fails — the subclass is never installed.

**Symptom**: `PostMessageW(hwnd, WM_APP+1, ...)` is sent but the subclass proc never receives it.

### 6. focusable: false Does Not Work

The `focusable: false` option in the WebviewWindow constructor and `setFocusable(false)` after creation have no effect on preventing focus stealing in Tauri v2 on Windows.

### 7. WS_EX_NOACTIVATE Alone Is Not Enough

Setting `WS_EX_NOACTIVATE` on the parent window and all child windows does NOT prevent WebView2 from stealing focus. WebView2's internal focus management overrides it.

### 8. setIgnoreCursorEvents Blocks Everything

`window.setIgnoreCursorEvents(true)` (Tauri JS API) or `window.set_ignore_cursor_events(true)` (Rust) makes the **entire** window click-through. No mouse events reach the webview at all — not even `pointerenter`/`pointerleave`. So you can't toggle it back from within the webview.

### 9. JS-to-JS Cross-Window Events Are Unreliable

`emit()` from `@tauri-apps/api/event` in one window may not reach `listen()` in another window. Use Rust `app.emit()` for reliable cross-window communication — it uses the proven Rust-to-all-windows event pipeline.

### 10. onMount Does NOT Fire in Overlay Windows

`onMount` from Svelte never fires in overlay WebviewWindows. The component renders (HTML appears), but `onMount` callbacks are never called. Use `$effect` for initialization instead — it runs during component initialization and works reliably.

### 11. Cross-Window Data: Use Rust Polling, Not Events

JS-to-JS events (`emit`/`listen` from `@tauri-apps/api/event`) are unreliable between windows. Rust `app.emit()` also doesn't reliably reach overlay windows. The proven pattern:
1. Main window pushes data to Rust via `invoke('set_comparator_data', { payload })`
2. Overlay polls via `invoke('get_comparator_data')` every 500ms using `$effect` + `setInterval`
3. Rust stores data in `AppState` behind a `Mutex<serde_json::Value>`

### 12. Transparency Workaround Can Reset Position

The WebView2 transparency workaround (`setSize +1/-1 pixels`) in the overlay page's `onMount` can reset window position. If you set position via the constructor and then run the workaround, the position may revert to default.

## The Click-Through Solution

### How It Works (Electron-inspired approach)

Same technique as Electron's `setIgnoreMouseEvents({ forward: true })`:

1. **`set_ignore_cursor_events(true)`** — Tauri's API sets `WS_EX_TRANSPARENT | WS_EX_LAYERED` on the window, making the **entire** window invisible to OS-level hit testing. All clicks pass through. This works because it operates at the OS window-manager level, bypassing WebView2's internal child window hierarchy.

2. **`WH_MOUSE_LL` global hook** — A low-level mouse hook tracks cursor position in screen coordinates. When the cursor enters the interactive button column (rightmost N pixels), it **directly removes `WS_EX_TRANSPARENT`** from the window style via `SetWindowLongW`. When the cursor leaves, it restores `WS_EX_TRANSPARENT`.

3. **Direct style manipulation in hook callback** — The hook callback modifies `WS_EX_TRANSPARENT` directly using `GetWindowLongW` / `SetWindowLongW`. Do NOT use `PostMessageW` + subclass (the subclass fails due to thread affinity issues). The hook runs synchronously before the mouse event is dispatched, so the style change takes effect before the click reaches the window.

4. **`WS_EX_NOACTIVATE`** on parent + all child HWNDs — Prevents clicking the buttons from stealing focus from the game. Applied to all children via `EnumChildWindows` because WebView2 creates multiple child HWNDs. Re-applied after a 500ms delay to catch asynchronously-created children.

### Architecture
```
┌─────────────────────────────────────┐
│ Hook Thread                         │
│  WH_MOUSE_LL hook callback          │
│  ├─ Track cursor position           │
│  ├─ Compare against cached win rect │
│  ├─ In button column?              │
│  │   YES → SetWindowLongW: remove   │
│  │         WS_EX_TRANSPARENT        │
│  │   NO  → SetWindowLongW: add      │
│  │         WS_EX_TRANSPARENT        │
│  └─ Message pump (PeekMessage)      │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ Overlay Window (Tauri WebviewWindow)│
│  WS_EX_TRANSPARENT (default)        │
│  WS_EX_LAYERED                      │
│  WS_EX_NOACTIVATE                   │
│  Always-on-top                      │
│                                     │
│  [Content - click through]  [Btns]  │
│  pointer-events: none       │ ✓  │  │
│                             │ ↻  │  │
│                             │clear│  │
└─────────────────────────────────────┘
```

### Key Implementation Details

**Hook thread message pump**: `WH_MOUSE_LL` requires a message pump on the thread that installs the hook. Uses `PeekMessageW` + 1ms sleep (not blocking `GetMessageW`) so the stop signal can be checked.

**Window rect caching**: The hook fires for every mouse move system-wide. The overlay window rect is cached and only refreshed when `RECT_DIRTY` flag is set (on window move/resize or initial setup).

**Button column detection**: The interactive zone is the rightmost `BUTTON_COLUMN_PX` (48 pixels) of the window. Buttons must be CSS-positioned at `position: fixed; right: 0; top: 0` to align with this zone.

**Cleanup**: The hook stop signal is stored in `AppState.overlay_hook_stop`. On window destroy, the stop signal is sent, which causes the hook thread to `UnhookWindowsHookEx` and exit.

### Rust Code Structure

```
lib.rs
├── overlay_clickthrough module (cfg(windows))
│   ├── HOOK_HANDLE: StdMutex<Option<SendHook>>     — the hook handle
│   ├── OVERLAY_HWND: AtomicIsize                    — tracked window
│   ├── WIN_RECT: StdMutex<(i32,i32,i32,i32)>       — cached rect
│   ├── IS_IGNORED: AtomicBool                       — click-through state
│   ├── RECT_DIRTY: AtomicBool                       — rect needs refresh
│   ├── INTERACTIVE_WIDTH: AtomicI32                 — right-edge zone width (px)
│   ├── mouse_hook_proc()                            — WH_MOUSE_LL callback, toggles WS_EX_TRANSPARENT directly
│   ├── install_hook() -> Sender<()>                 — spawns hook thread with message pump
│   ├── uninstall_hook()                             — cleanup
│   ├── set_overlay_hwnd(HWND)                       — register window for tracking
│   ├── set_interactive_width(i32)                   — configure zone width per overlay
│   ├── invalidate_rect()                            — mark rect dirty (call on move/resize)
│   └── set_noactivate(HWND)                         — WS_EX_NOACTIVATE on parent + all children
│
├── set_overlay_clickthrough command (label, interactive_width)
│   ├── Delays 1s for HWND availability
│   ├── Calls set_ignore_cursor_events(true)
│   ├── Sets WS_EX_NOACTIVATE on parent + children
│   ├── Configures interactive width
│   ├── Registers HWND for tracking
│   ├── Installs mouse hook (idempotent)
│   └── Re-applies WS_EX_NOACTIVATE after 500ms
│
└── AppState.overlay_hook_stop                       — stop signal for cleanup
```

### Frontend Requirements

1. **Button column must be at the right edge of the window**:
```css
.side {
    position: fixed;
    right: 0;
    top: 0;
}
```

2. **Content area must be pointer-events: none**:
```css
.table {
    pointer-events: none;
}
.side {
    pointer-events: auto;
}
```

3. **Text must be unselectable**:
```css
:global(html), :global(body) {
    user-select: none;
    -webkit-user-select: none;
}
```

4. **Call `set_overlay_clickthrough` after window creation**:
```typescript
win.once('tauri://created', async () => {
    await invoke('set_overlay_clickthrough', { label: 'comparator', interactiveWidth: 48 });
});
```

## Overlay Position Management

### Red Frame Positioning
The overlay position is configured via Settings > Overlays > Configure, which opens a red-bordered draggable overlay (`/overlay` route with `?sync=comparator` param). The `move_overlay` Rust command can sync the content overlay position in real-time.

### Position Persistence
- Position saved to `settings.json` via `set_comparator_overlay_settings` Rust command
- Settings page saves on "Save" button click
- Auto-restore on app start if `enabled: true`
- Only position (x, y) is persisted — window size is fixed (900x400)

### Window Sizing
The overlay window is 900x400 logical pixels — oversized because the content is `display: inline-block` and sizes to fit. The transparent area is click-through (WS_EX_TRANSPARENT), so the extra space doesn't interfere with the game.

## Overlay Data Flow

### Comparator Results
```
Main Comparator → emit('comparator-results', { results, tradeData })
                → Overlay listens → updates display
```

### Trade Refresh
```
Overlay button click → invoke('trade_lookup', { gem, variant })
                     → Result stored in overlay's tradeData (spread for reactivity)
```

### Pick Gem
```
Overlay pick button → getCurrentWebviewWindow().emit('overlay-pick', { name, variant, roi })
                    → Main Comparator listens → handleNext()
```

## Creating a New Overlay (Step-by-Step)

Use this checklist when adding a new overlay (e.g., lab compass). Every step is required.

### 1. Add window label to capabilities

```json
// capabilities/default.json — "windows" array
"windows": ["main", "overlay", "comparator", "my-new-overlay"],
```

Without this, ALL Tauri APIs silently fail for this window.

### 2. Create the overlay route

```
desktop/src/routes/overlay/my-new-overlay/+page.svelte
```

The route inherits the transparent layout from `/overlay/+layout.svelte`.

### 3. Essential CSS in the overlay page

```css
:global(html), :global(body) {
    margin: 0;
    padding: 0;
    background: transparent !important;
    overflow: hidden;
    user-select: none;
    -webkit-user-select: none;
}

/* Content area — visible but click-through */
.content {
    pointer-events: none;
    display: inline-block;       /* sizes to fit content */
    background: rgba(15, 17, 23, 0.92);
    backdrop-filter: blur(8px);
}

/* Interactive buttons — MUST be at the right edge of the window */
.buttons {
    pointer-events: auto;
    position: fixed;
    right: 0;
    top: 0;
}
```

### 4. Create the window (JS)

```typescript
const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
const dpr = window.devicePixelRatio || 1;

const win = new WebviewWindow('my-new-overlay', {
    url: '/overlay/my-new-overlay',
    transparent: true,
    decorations: false,
    alwaysOnTop: true,
    resizable: false,
    shadow: false,
    skipTaskbar: true,
    width: 900,      // oversized — transparent area is click-through
    height: 400,
    x: Math.round(savedX / dpr),
    y: Math.round(savedY / dpr),
});

win.once('tauri://created', async () => {
    // Enable click-through + interactive button zone
    await invoke('set_overlay_clickthrough', {
        label: 'my-new-overlay',
        interactiveWidth: 48,    // physical pixels from right edge
    });
});
```

### 5. How `set_overlay_clickthrough` works

The Rust command (called once after window creation):

1. **Delays 1 second** — WebView2 HWND not available immediately
2. **`set_ignore_cursor_events(true)`** — sets `WS_EX_TRANSPARENT | WS_EX_LAYERED`, entire window becomes click-through
3. **`set_noactivate(hwnd)`** — sets `WS_EX_NOACTIVATE` on parent + all WebView2 child HWNDs
4. **Registers HWND** for the mouse hook to track
5. **Installs `WH_MOUSE_LL` hook** (once, shared across all overlays) — tracks cursor globally
6. **Re-applies `WS_EX_NOACTIVATE`** after 500ms to catch late WebView2 children

The hook toggles `WS_EX_TRANSPARENT` directly via `SetWindowLongW`:
- Cursor enters interactive zone (rightmost `interactiveWidth` px) → removes `WS_EX_TRANSPARENT` → buttons clickable
- Cursor leaves → restores `WS_EX_TRANSPARENT` → click-through

### 6. Destroying the overlay

Use the retry loop — single `destroy()` calls are unreliable:

```typescript
const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
for (let i = 0; i < 5; i++) {
    const existing = await WebviewWindow.getByLabel('my-new-overlay');
    if (!existing) break;
    try { await existing.close(); } catch (_) {}
    try { await existing.destroy(); } catch (_) {}
    await new Promise(r => setTimeout(r, 100));
}
```

Also clean up the hook in `on_window_event` when the window is destroyed (see `lib.rs` for the `Destroyed` event handler).

### 7. Position persistence

- Save position via `invoke('set_comparator_overlay_settings', { x, y, w, h, enabled })` (or create a similar command for your overlay)
- Position values from `outerPosition()` are **physical pixels**
- Constructor `x`/`y` are **logical pixels** — divide by `devicePixelRatio`
- Cross-window `outerPosition()` returns **wrong values** — save from the overlay's own context only

### 8. Data flow

For overlay ↔ main window communication:
- **Main → Overlay**: use Tauri `emit()` from `@tauri-apps/api/event` (JS→JS works main→child)
- **Overlay → Main**: use `invoke()` to a Rust command that calls `app.emit()` (reliable Rust→all-windows path)
- **Do NOT** rely on JS `emit()` from overlay reaching the main window

### Current limitations

- **One interactive zone per hook**: the hook tracks one HWND. For multiple overlays with buttons, the module would need to track multiple HWNDs and rects.
- **Interactive zone is right-edge only**: buttons must be positioned at the right edge of the window. A different zone shape would need changes to the hook's hit-test logic.

## References

- [WebView2Feedback#446 — WM_NCHITTEST limitation](https://github.com/MicrosoftEdge/WebView2Feedback/issues/446)
- [WebView2Feedback#1004 — Transparent pages and clickthrough](https://github.com/MicrosoftEdge/WebView2Feedback/issues/1004)
- [Electron PR #10183 — Mouse forward on Windows](https://github.com/electron/electron/pull/10183)
- [Tauri Issue #13070 — Transparent click-through feature request](https://github.com/tauri-apps/tauri/issues/13070)
- [Tauri Issue #6164 — Forward option for setIgnoreCursorEvents](https://github.com/tauri-apps/tauri/issues/6164)
- [Tao commit 4fa8761 — set_ignore_cursor_events implementation](https://github.com/tauri-apps/tao/commit/4fa8761776d546ee3b1b0bb1a02a31d72eedfa80)
