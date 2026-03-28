# Desktop App Library (`$lib`)

Component registry for the ProfitOfExile desktop app. Read this first before creating or modifying components.

## Stores

| File | Export | Description |
|------|--------|-------------|
| `stores/status.svelte.ts` | `appStatus`, `appLogs`, `initStatusStore()` | Shared app state — event-driven from Rust backend. No polling. Call `initStatusStore()` once from root layout. All pages read `appStatus` and `appLogs` reactively. |

## Components

| File | Props | Description |
|------|-------|-------------|
| `components/TopBar.svelte` | `status`, `pairCode`, `onToggleSidebar` | App header — logo, connection/scanning indicators, pair code, debug toggle, settings link |
| `components/Sidebar.svelte` | `open`, `currentPath` | Collapsible nav — strategies, tools, overlay quick-toggles |

## Overlay Utilities

| File | Exports | Description |
|------|---------|-------------|
| `overlay/manager.ts` | `showOverlay()`, `destroyOverlay()`, `getOverlay()`, `isOverlayActive()`, `readOverlayRegion()` | Spawn/destroy/manage Tauri overlay windows. Tracks active overlays by name. |

## Conventions

- **Stores**: `.svelte.ts` extension (Svelte 5 runes). One source of truth per concern.
- **Components**: `.svelte` files in `components/`. Props via `$props()`. Scoped styles.
- **Utilities**: `.ts` files. Pure functions, no reactivity.
- **Styling**: CSS custom properties from `app.css` (`--bg`, `--surface`, `--border`, `--text`, `--text-muted`, `--accent`, `--success`, `--warning`).
- **Tauri commands**: Use `invoke()` from `@tauri-apps/api/core`. Prefer event listeners (`listen()`) over polling.
- **State flow**: Rust emits events → `status.svelte.ts` store updates → components react. Pages never poll.
