# Desktop Agent

Desktop app implementation principles for the Tauri + SvelteKit desktop client. Extends the general agent. Covers both the Rust backend and the SvelteKit frontend since they're tightly coupled.

## Architecture Overview

The desktop app is a Tauri v2 app with a Rust backend and SvelteKit 5 frontend. The two sides communicate via:
- **Commands**: Frontend calls Rust via `invoke('command_name', { args })` — synchronous request/response.
- **Events**: Rust emits to frontend via `app.emit("event-name", payload)` — async push. Frontend subscribes via `listen("event-name", callback)`.

No polling. All state updates flow through events.

## Project Structure

```
desktop/
  src-tauri/
    src/
      lib.rs              — Tauri commands, AppState, event emitters, app setup, capture loop
      settings.rs          — Persistent settings (JSON to %AppData%/profitofexile/)
      font_parser.rs       — Font panel OCR parser (keyword-based craft option detection)
      trade/               — Trade API client (direct GGG calls)
        client.rs          — TradeApiClient: search → fetch → build result
        query.rs           — GGG search query builder (mirrors Go's buildSearchQuery)
        rate_limiter.rs    — Multi-tier sliding window (mirrors Go's ratelimiter.go)
        signals.rs         — Market signals computation (mirrors Go's ComputeSignals)
        types.rs           — TradeLookupResult, TradeListingDetail, TradeSignals
      capture.rs           — Screen capture (Windows-only, xcap)
      ocr.rs               — OCR engine (Windows.Media.Ocr)
      gem_matcher.rs       — Fuzzy gem name matching
      lab_state.rs         — Lab state machine (Idle → FontReady → PickingGems → Done)
      log_watcher.rs       — Client.txt file watcher (notify crate, filesystem events)
    capabilities/
      default.json         — Tauri permissions (window, webview, shell)
    tauri.conf.json        — App config (window size 1024x768, decorations: false, identifier)
  src/
    lib/
      README.md            — Component registry. READ THIS FIRST before creating/modifying.
      stores/
        status.svelte.ts   — Shared reactive state (event-driven from Rust, no polling)
      components/
        TopBar.svelte      — Custom title bar with window controls, status indicators
        Sidebar.svelte     — Collapsible nav with strategies, tools, overlay toggles
      overlay/
        manager.ts         — Spawn/destroy/track Tauri overlay windows
    routes/
      (app)/               — App shell group (topbar + sidebar + content)
        +layout.svelte     — Root app layout, initializes status store
        +page.svelte       — Lab page (main content)
        settings/+page.svelte — Settings page
      overlay/             — Overlay windows (outside app shell, transparent)
        +layout.svelte     — Transparent layout for all overlays
        +page.svelte       — Capture region overlay (red-bordered, draggable)
        comparator/
          +page.svelte     — Comparator results overlay (game overlay, draggable)
    app.css                — Theme variables and global styles
    app.html               — HTML shell with favicon
```

## Rust Patterns

### AppState
All mutable state lives in `AppState` behind `Mutex<T>`. Access via `app.state::<AppState>()` from `AppHandle`.

### Commands
Tauri commands that mutate state must:
1. Take `app: AppHandle` (not just `state: tauri::State`)
2. Call `persist_settings(&app)` after changing any persisted value
3. Call `emit_status(&app)` so the frontend sees the change

```rust
#[tauri::command]
fn set_something(value: String, app: AppHandle) {
    let state = app.state::<AppState>();
    *state.something.lock().unwrap_or_else(|e| e.into_inner()) = value;
    persist_settings(&app);
    emit_status(&app);
}
```

### Logging
Use `app_log(&app, msg)` — it appends to the in-memory log buffer (50 entries, shown in UI), writes to `%AppData%/profitofexile/app.log` (persistent), AND emits `"logs-changed"` to the frontend. Takes `&AppHandle`. In background tasks that have `app: &AppHandle`, call `app_log(app, msg)` (no extra `&`).

For errors in the capture loop (runs every 500ms), use throttled logging to avoid spam:
```rust
if loop_count % 20 == 1 { // log every ~10s
    app_log(app, format!("Capture failed: {}", e));
}
```

### Events
- `emit_status(&app)` — emits full `AppStatus` as `"status-changed"`
- `emit_logs(&app)` — emits log array as `"logs-changed"`
- `app.emit("custom-event", payload)` — for specific events like `"gem-detected"`, `"font-jackpot"`
- Always check emit result: `if let Err(e) = app.emit(...) { log::warn!(...) }` — never `let _ =`

### Settings Persistence
`settings.rs` saves/loads JSON to `%AppData%/profitofexile/settings.json`. Uses `#[serde(default)]` for forward compatibility. Persisted fields: `client_txt_path`, `server_url`, `gem_region`, `font_region`, `window` (position/size/maximized). Call `persist_settings(&app)` after mutating any of these. Window settings are saved separately on close event.

### Mutex Handling
Always use `.unwrap_or_else(|e| e.into_inner())` on mutex locks — recovers from poisoned mutexes instead of panicking.

### Trade Module
Port of the Go `internal/trade/` package. Same two-phase GGG API flow (POST search → GET fetch), same query format, same signal computation. Uses browser-like User-Agent. Rate limiter syncs from `X-Rate-Limit-*` response headers.

## SvelteKit Patterns

### Svelte 5 Runes
- State: `let x = $state(value)` — NOT Svelte 4 stores or `export let`
- Props: `let { prop } = $props()` — NOT `export let prop`
- Derived: `let x = $derived(expression)` — NOT `$:`
- Children: `{@render children()}` — NOT `<slot />`

### Shared Store
`stores/status.svelte.ts` exports `store` (reactive object) and `initStatusStore()`. The store is initialized once from the `(app)/+layout.svelte`. All pages read `store.status` and `store.logs` — never poll.

```ts
// Svelte 5: export an object, mutate properties (NOT reassign)
export const store = $state({
    status: null as any,
    logs: [] as string[],
});
```

### Styling
- CSS custom properties from `app.css` — use `var(--bg)`, `var(--surface)`, `var(--accent)`, etc.
- Scoped `<style>` blocks in components — NOT Tailwind, NOT global CSS classes
- Dark scrollbar styled globally in `app.css`
- No emojis in code unless user requests them

### Component Reuse — MANDATORY
**NEVER create "pure" one-off UI elements. ALWAYS extract reusable components into `$lib/components/` and document them in `$lib/README.md`.**

Before creating any new UI:
1. Check `$lib/README.md` for an existing component that fits
2. If none exists, create one in `$lib/components/` with proper props
3. Add it to the README registry
4. Use it everywhere — pages should compose lib components, not contain raw HTML patterns

Examples of what MUST be components:
- Buttons (action, small, cancel, stop variants)
- Section cards (the `<section>` wrapper with header)
- Setting rows (label + value + action)
- Status indicators (dots, badges)
- Any UI pattern used more than once

### Overlay Windows
Created via `overlay/manager.ts`:
```ts
import { showOverlay, destroyOverlay, readOverlayRegion } from '$lib/overlay/manager';
const win = await showOverlay('region', { url: '/overlay', width: 550, height: 75, x: 30, y: 45 });
const region = await readOverlayRegion('region');
await destroyOverlay('region');
```

Overlays are Tauri WebviewWindows — transparent, always-on-top, no decorations. Route to `/overlay/{name}`.

**CRITICAL — Window Capabilities**: Every new overlay window label MUST be added to `capabilities/default.json` in the `"windows"` array. Tauri v2 scopes permissions by window label — if a window label isn't listed, ALL Tauri APIs (`startDragging`, `startResizeDragging`, `show`, `hide`, `destroy`, etc.) silently fail with no error. This is the #1 gotcha when creating new overlay windows.

```json
// capabilities/default.json — add every window label here
"windows": ["main", "overlay", "comparator", "overlay-comparator-pos"],
```

**CRITICAL — Overlay Click-Through**: Making overlays click-through on Windows/WebView2 is complex. `WM_NCHITTEST`, `setIgnoreCursorEvents`, `focusable: false`, `WS_EX_NOACTIVATE` alone do NOT work. The proven solution uses `WS_EX_TRANSPARENT` + `WH_MOUSE_LL` global hook that toggles transparency based on cursor position. **Read `docs/OVERLAY-GUIDE.md` before touching any overlay code.** Key points:
- Cross-window JS API calls (`outerPosition`, `destroy`, `setPosition`) return wrong values — only `getCurrentWebviewWindow()` from within the overlay is reliable
- `window.hwnd()` in Rust fails if called immediately after creation — delay 1 second
- `SetWindowSubclass` must run on the window's thread — don't call from spawned threads
- Button columns must be CSS `position: fixed; right: 0` to match the hook's hit zone
- Never use `.catch(() => {})` — always log errors, even on expected-flaky operations
- Game focus detection is via `GetForegroundWindow` polling in Rust — do NOT add JS-side focus listeners
- `onMount` doesn't fire in overlay windows — use `$effect` for initialization

### DPI Awareness
The WebviewWindow constructor takes **logical** pixels. Screen capture regions store **physical** pixels. Convert with `window.devicePixelRatio`:
```ts
const dpr = window.devicePixelRatio || 1;
// Physical → logical for constructor
new WebviewWindow('overlay', { width: Math.round(physW / dpr), ... });
// outerPosition/outerSize return physical — store directly
```

### Navigation
SvelteKit client-side routing within the `(app)` group. Use `<a href="/settings">` for links. Overlay routes are outside the group and don't get the app shell layout.

## File Watcher
Uses the `notify` crate (filesystem events, NOT polling). Watches the parent directory, filters by filename. Supports cancel via `tokio::sync::watch` channel — restarts automatically when the Client.txt path changes in settings.

## Capture Loop (lib.rs: run_capture_loop)
Dual-region OCR that runs when lab state is `PickingGems`:
- **Region 1** (gem tooltip): OCR → fuzzy match gem names → emit `"gem-detected"` events
- **Region 2** (font panel): OCR → parse craft options via `font_parser` → track session rounds
- Sends completed font sessions to `POST /api/desktop/font-session` on the Go server
- Emits `"font-jackpot"` when "non-Transfigured" option detected
- 3 consecutive font panel misses → stop OCR (user walked away or font empty)
- Client.txt `InstanceClientLabyrinthCraftResultOptionsList` restarts OCR

## Font Panel Parser (font_parser.rs)
Keyword-based detection — scans OCR lines for anchor text, extracts numeric values:
- `"random Transfigured Gem"` → standard transform (always present)
- `"non-Transfigured"` → JACKPOT (direct transfigure, ~6% rate)
- `"quality to a Gem"` → extract +X% value
- `"experience to a Gem"` → extract Xm value
- `"Facetor's Lens"` or `"Faction's Lens"` (OCR misread) → extract X% value
- `"Crafts Remaining"` → extract N counter
- Joins all lines for cross-line keyword matching

## Server Endpoints (Go)
- `POST /api/desktop/gems` — gem detection events → Mercure publish
- `POST /api/desktop/font-session` — font session data (transactional insert)
- Font session stores per-round craft options with types and numeric values

## Custom Title Bar
`decorations: false` in tauri.conf.json. TopBar.svelte provides:
- Window drag (`startDragging` on mousedown, excluding buttons/links)
- Minimize/maximize/close buttons (Windows-style, red hover on close)
- Window position/size saved to settings on close, restored on startup

## Key References
- `docs/OVERLAY-GUIDE.md` — **READ FIRST for any overlay work.** Complete guide: click-through, positioning, capabilities, cross-window gotchas
- `desktop/src/lib/README.md` — Component registry (read first for UI work)
- `CLAUDE.md` — Project-wide conventions
- `BACKBONE.md` — Full project design document
- `docs/superpowers/specs/2026-03-28-desktop-app-shell-design.md` — App shell spec
- `frontend/src/routes/lab/` — Web dashboard components (migration source)
- `frontend/src/lib/api.ts` — Web API client (fetch patterns to replicate via Tauri invoke)
