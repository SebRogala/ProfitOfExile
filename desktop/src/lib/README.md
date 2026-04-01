# Desktop App Library (`$lib`)

Component registry for the ProfitOfExile desktop app. Read this first before creating or modifying components.

## Stores

| File | Export | Description |
|------|--------|-------------|
| `stores/status.svelte.ts` | `store`, `initStatusStore()` | Shared app state — event-driven from Rust backend. No polling. Call `initStatusStore()` once from root layout. Read `store.status` and `store.logs` reactively. |
| `stores/navigation.svelte.ts` | `nav` | Global view toggle. `nav.view` is `'lab' \| 'settings'`. `nav.go('/settings')` switches. All pages are always mounted (hidden via CSS) — **do NOT use SvelteKit `<a href>` routing** for main views, it unmounts components and kills event listeners. |

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

## Compass Data

| File | Exports | Description |
|------|---------|-------------|
| `compass/room-presets.ts` | `getPresetByAreaCode()`, `getPresetsByName()`, `getTileRect()`, `getDoorExitLocations()`, `getContentLocations()`, `VALID_AREA_CODES` | Room preset data + coordinate math. Loads `room-presets.json` at import time. 35 rooms, 53 variants. |
| `compass/svg-loader.ts` | `getRoomSvgUrl()`, `getDisabledSvgUrl()` | Resolves area code to SVG path in `/compass/presets/`. Returns null for invalid codes. |
| `compass/navigation.ts` | `createNavState()`, `loadLayout()`, `handleNavEvent()`, `computeRoute()`, `getNextDirection()`, `getNextExitText()`, `setStrategy()` | Navigation engine — position tracking, auto-routing (BFS + target waypoints), golden key/door tracking. Pure functions, no Svelte reactivity. |

## Compass Components

| File | Props | Description |
|------|-------|-------------|
| `compass/RoomMinimap.svelte` | `areaCode`, `doors`, `contents`, `targetDirection`, `roomName` | Room SVG background with positioned door/content overlays. Target exit highlighted green. |
| `compass/DirectionCompass.svelte` | `directions`, `targetDirection`, `roomName`, `hasContent` | Compass circle with exit markers at compass angles. |
| `compass/MinimalBar.svelte` | `targetDirection`, `contents`, `timerText` | Compact bar with arrow, content badges, timer. |
| `compass/CompassOverlay.svelte` | `mode`, all child props | Mode switcher — renders minimap, direction, or minimal mode. |

## Pages

Located in `$lib/pages/`. Always mounted in the layout, toggled via `nav` store — **not** SvelteKit routing.

| File | Description |
|------|-------------|
| `pages/LabPage.svelte` | Lab farming dashboard — tabs (Session/Rankings/Font EV/Market), comparator, session queue, best plays, font EV, market overview. |
| `pages/SettingsPage.svelte` | Settings — General, Game Integration, Overlays, Trade, Logs. |

## Routes

Only used for the app shell and overlay windows. **Do NOT add page routes** — use `$lib/pages/` + `nav` store instead.

| Route | Description |
|-------|-------------|
| `(app)/+layout.svelte` | App shell — TopBar + Sidebar + renders all pages (LabPage, SettingsPage). View switching via `nav` store. |
| `(app)/+page.svelte` | Empty stub — required by adapter-static for HTML generation. |
| `(app)/dev/+page.svelte` | Dev tools — trade lookup test, pipeline test, OCR test. (DEV only) |
| `overlay/+page.svelte` | Capture region overlay — transparent, draggable, resizable, Save/Cancel buttons. |

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

## OCR Lifecycle

Two decoupled scan loops, each on a dedicated OS thread (required by Windows COM/WinRT).

### Gem Tooltip OCR

Scans the gem tooltip region to detect transfigured gem names for the comparator.

**Start triggers** (all: clear comparator, restart scan):
- `FontOpened` — Client.txt `InstanceClientLabyrinthCraftResultOptionsList recieved` (user clicked CRAFT button)
- Manual "Start Scanning" button

**Stop triggers**:
- 3 gems detected (auto-stop)
- 45s timeout
- ZoneChanged (left area)
- Manual "Stop Scanning"
- Next start trigger (bumps generation counter → old scan exits)

**Key behavior**: Aborts immediately if gem name list is empty (server unreachable). Uses `AtomicU64` generation counter for clean cancellation — no thread cleanup needed.

### Font Panel OCR

Scans the font region to capture craft options (transform, quality, experience, etc.) from the CRAFT screen.

**Start**: 3rd "Aspirant's Trial" zone entry (counter resets on "Aspirants' Plaza").

**Running**: Scans at 250ms, parses options via `font_parser`. Deduplicates — same options seen again (user reopened font without crafting) are skipped.

**Round tracking**: `FontOpened` seals the current round into the session. If no "Crafts Remaining" text was detected alongside options, this was the last craft → scan stops.

**Stop**: Last craft sealed, ZoneChanged, or 5-min timeout safety net.

**Data flow**: ZoneChanged sends accumulated session (all rounds with options + crafts_remaining) to server via `POST /api/desktop/font-session`.

### Game UI Context

- **CRAFT screen**: Shows options list + "Crafts Remaining: X" + CRAFT button. "Crafts Remaining" only visible when X > 1.
- **CONFIRM screen**: Shows 3 gem slots + CONFIRM button. Options list is gone. This is when gem tooltip OCR runs.
- Clicking the font opens the CRAFT screen (no Client.txt event). Clicking CRAFT fires `FontOpened` and switches to CONFIRM screen.
- Gem tooltips cover the font panel area when hovering — OCR regions overlap.

### Focus & Overlay

The focus poller (1s interval, `GetForegroundWindow`) uses three-state logic:
- **Game** (PoE foreground): show overlay
- **OwnWindow** (our process foreground): preserve state — no hide/show/status events
- **Other** (any other app): hide overlay

Overlay is always fully click-through (`WS_EX_NOACTIVATE` + `WS_EX_TRANSPARENT`). A global `WH_MOUSE_LL` hook intercepts clicks in the rightmost 48px (interactive zone), consumes them, and emits `overlay-click` Tauri events. The frontend maps coordinates to button actions via `elementFromPoint` + `data-action` attributes. The hook also re-applies `WS_EX_TRANSPARENT` on every mouse event (WebView2 strips it when creating child windows). Click interception is gated on a `HAS_CONTENT` flag — when the overlay is empty, all clicks pass through to the game.

## Conventions

- **Stores**: `.svelte.ts` extension (Svelte 5 runes). Export objects, mutate properties (NOT reassign).
- **Components**: `.svelte` files in `components/`. Props via `$props()`. Scoped styles.
- **Utilities**: `.ts` files. Pure functions, no reactivity.
- **Styling**: CSS custom properties from `app.css` (`--bg`, `--surface`, `--border`, `--text`, `--text-muted`, `--accent`, `--success`, `--warning`).
- **Tauri commands**: Use `invoke()` from `@tauri-apps/api/core`. Prefer event listeners (`listen()`) over polling.
- **State flow**: Rust emits events → `status.svelte.ts` store updates → components react. Pages never poll.
- **Settings persistence**: `%AppData%/profitofexile/settings.json`. Saved automatically on every mutation.
- **Logging**: `%AppData%/profitofexile/app.log` (persistent) + in-memory buffer (50 entries, UI).
- **DPI**: Comparator overlay uses `PhysicalPosition` via Rust `move_overlay` — no DPI conversion. OCR region overlays use `scaleFactor()` for constructor coords. Never use `devicePixelRatio` in overlay WebViews.
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
