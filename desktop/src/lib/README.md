# Desktop App Library (`$lib`)

Component registry for the ProfitOfExile desktop app. Read this first before creating or modifying components.

## Stores

| File | Export | Description |
|------|--------|-------------|
| `stores/status.svelte.ts` | `store`, `initStatusStore()` | Shared app state — event-driven from Rust backend. No polling. Call `initStatusStore()` once from root layout. Read `store.status` and `store.logs` reactively. |

## Components

| File | Props | Description |
|------|-------|-------------|
| `components/TopBar.svelte` | `status` | Custom title bar — logo, status indicators, debug toggle (dev only), settings link, window controls (min/max/close). Draggable. |
| `components/Sidebar.svelte` | `open`, `currentPath`, `onToggle` | Collapsible nav — strategies, tools, overlay quick-toggles. Collapsed state shows thin clickable strip. |

## Overlay Utilities

| File | Exports | Description |
|------|---------|-------------|
| `overlay/manager.ts` | `showOverlay()`, `destroyOverlay()`, `getOverlay()`, `isOverlayActive()`, `readOverlayRegion()` | Spawn/destroy/manage Tauri overlay windows. Tracks active overlays by name. |

## Routes

| Route | Description |
|-------|-------------|
| `(app)/+layout.svelte` | App shell — TopBar + Sidebar + content slot. Initializes status store. |
| `(app)/+page.svelte` | Lab page — scan controls, trade lookup, test tools, logs. Will host migrated dashboard components. |
| `(app)/settings/+page.svelte` | Settings — General, Game Integration (2 OCR regions), Trade, Overlays. |
| `overlay/+page.svelte` | Capture region overlay — transparent, draggable, resizable. |

## Conventions

- **Stores**: `.svelte.ts` extension (Svelte 5 runes). Export objects, mutate properties (NOT reassign).
- **Components**: `.svelte` files in `components/`. Props via `$props()`. Scoped styles.
- **Utilities**: `.ts` files. Pure functions, no reactivity.
- **Styling**: CSS custom properties from `app.css` (`--bg`, `--surface`, `--border`, `--text`, `--text-muted`, `--accent`, `--success`, `--warning`).
- **Tauri commands**: Use `invoke()` from `@tauri-apps/api/core`. Prefer event listeners (`listen()`) over polling.
- **State flow**: Rust emits events → `status.svelte.ts` store updates → components react. Pages never poll.
- **Settings persistence**: `%AppData%/profitofexile/settings.json`. Saved automatically on every mutation.
- **Logging**: `%AppData%/profitofexile/app.log` (persistent) + in-memory buffer (50 entries, UI).
- **DPI**: Overlay constructors take logical pixels. Regions stored as physical. Convert with `devicePixelRatio`.
- **Error handling**: Log errors to `app_log`, never silently discard. Throttle capture loop errors.

## Migration Notes (web → desktop)

When migrating components from `frontend/src/routes/lab/`:
- Replace `fetch('/api/...')` with `invoke('command_name', { args })` for Tauri commands
- Replace Svelte 4 stores (`$store`) with Svelte 5 runes (`$state`, `$derived`, `$props`)
- Replace `export let` props with `let { prop } = $props()`
- Replace `<slot />` with `{@render children()}`
- Replace Tailwind classes with CSS custom properties (`var(--accent)`, etc.)
- Data from Go server API: use `fetch()` against `store.status.server_url` base URL
- Mercure SSE: same pattern as web (`EventSource` to server's Mercure hub)
