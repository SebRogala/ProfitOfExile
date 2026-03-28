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
| `components/Select.svelte` | `value` (bindable), `options`, `onchange` | Custom dropdown select — styled dark theme, chevron indicator. Used by dashboard components. |

## Overlay Utilities

| File | Exports | Description |
|------|---------|-------------|
| `overlay/manager.ts` | `showOverlay()`, `destroyOverlay()`, `getOverlay()`, `isOverlayActive()`, `readOverlayRegion()` | Spawn/destroy/manage Tauri overlay windows. Tracks active overlays by name. |

## Routes

| Route | Description |
|-------|-------------|
| `(app)/+layout.svelte` | App shell — TopBar + Sidebar + content slot. Initializes status store. |
| `(app)/+page.svelte` | Lab farming dashboard — scan controls, comparator, best plays, font EV, market overview, session queue, logs. |
| `(app)/dev/+page.svelte` | Dev tools — trade lookup test, pipeline test, OCR test. |
| `(app)/settings/+page.svelte` | Settings — General, Game Integration (2 OCR regions), Trade, Overlays. |
| `overlay/+page.svelte` | Capture region overlay — transparent, draggable, resizable. |

## Dashboard Components (Lab)

Located in `routes/(app)/components/`. Lab farming dashboard components migrated from the web frontend.

| File | Description |
|------|-------------|
| `Header.svelte` | Dashboard header — lab selector, divine rate, update timer, connection status |
| `Comparator.svelte` | Gem comparator — search, compare up to 3 gems, trade data, session queue integration. Uses Tauri `listen('gem-detected')` for OCR events. |
| `SessionQueue.svelte` | Session queue — picked gems with snapshot/current prices and delta tracking |
| `BestPlays.svelte` | Sortable gem table — price, ROI, signals, sparklines, expandable rows |
| `ByVariant.svelte` | Variant tabs — filters BestPlays by variant (1/0, 1/20, 20/0, 20/20) and color |
| `FontEVCompare.svelte` | Font EV comparison table — all variants x colors with tier breakdowns |
| `FontEV.svelte` | Single-variant font EV cards — color cards with safe/premium/jackpot tiers |
| `MarketOverview.svelte` | Market stats — prices, volatility, confidence spread, offering timing charts |
| `Legend.svelte` | Expandable legend — all signal, window, tier, and metric definitions |
| `SignalBadge.svelte` | Signal/window/confidence badge with styled prefix icons |
| `Sparkline.svelte` | Mini SVG sparkline chart |
| `GemIcon.svelte` | Gem icon from poewiki.net with error fallback |
| `InfoTooltip.svelte` | Hover/click tooltip with smart alignment |
| `OfferingChart.svelte` | Offering price chart with prediction line and responsive SVG |

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
