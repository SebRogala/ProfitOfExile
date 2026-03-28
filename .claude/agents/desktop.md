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
      lib.rs              — Tauri commands, AppState, event emitters, app setup
      settings.rs          — Persistent settings (JSON to %AppData%)
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
    tauri.conf.json        — App config (window size, decorations: false, identifier)
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
        +page.svelte       — Capture region overlay
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
Use `app_log(&app, msg)` — it appends to the in-memory log buffer AND emits `"logs-changed"` to the frontend. Takes `&AppHandle`. In background tasks that have `app: &AppHandle`, call `app_log(app, msg)` (no extra `&`).

### Events
- `emit_status(&app)` — emits full `AppStatus` as `"status-changed"`
- `emit_logs(&app)` — emits log array as `"logs-changed"`
- `app.emit("custom-event", payload)` — for specific events like `"gem-detected"`

### Settings Persistence
`settings.rs` saves/loads JSON to `%AppData%/profitofexile/settings.json`. Uses `#[serde(default)]` for forward compatibility. Persisted fields: `client_txt_path`, `server_url`, `gem_region`. Call `persist_settings(&app)` after mutating any of these.

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

### Overlay Windows
Created via `overlay/manager.ts`:
```ts
import { showOverlay, destroyOverlay, readOverlayRegion } from '$lib/overlay/manager';
const win = await showOverlay('region', { url: '/overlay', width: 550, height: 75, x: 30, y: 45 });
const region = await readOverlayRegion('region');
await destroyOverlay('region');
```

Overlays are Tauri WebviewWindows — transparent, always-on-top, no decorations. Route to `/overlay/{name}`.

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

## Key References
- `desktop/src/lib/README.md` — Component registry (read first)
- `CLAUDE.md` — Project-wide conventions
- `BACKBONE.md` — Full project design document
- `docs/superpowers/specs/2026-03-28-desktop-app-shell-design.md` — App shell spec
