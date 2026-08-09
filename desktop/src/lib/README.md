# Desktop App Library (`$lib`)

Component registry for the ProfitOfExile desktop app. Read this first before creating or modifying components.

## Stores

| File | Export | Description |
|------|--------|-------------|
| `stores/status.svelte.ts` | `store`, `initStatusStore()` | Shared app state — event-driven from Rust backend. No polling. Call `initStatusStore()` once from root layout. Read `store.status` and `store.logs` reactively. |
| `stores/ssot.svelte.ts` | `ssot`, `startSsotStore()`, `setNormalVariant()`, `setDedicationVariant()`, `setDedicationPool()`, `setDedicationSelection()` | Rust-owned single source of truth for market selection, polled every 3000 ms. Read `ssot.league` / `ssot.normalVariant` / `ssot.dedicationVariant` / `ssot.dedicationPool` reactively; write through the exported setters (never assign the fields directly or keep a local copy). |
| `stores/navigation.svelte.ts` | `nav` | Global view toggle. `nav.view` is `'lab' \| 'settings' \| 'dev'`. All pages are always mounted (hidden via CSS) — do not use SvelteKit routing for main views because it unmounts their listeners. |

## Components

| File | Props | Description |
|------|-------|-------------|
| `components/TopBar.svelte` | `status` | Custom title bar — logo, status indicators, debug toggle (dev only), settings link, window controls (min/max/close). Draggable. |
| `components/Sidebar.svelte` | `open`, `currentPath`, `onToggle` | Collapsible nav — strategies, tools, overlay quick-toggles. Collapsed state shows thin clickable strip. |
| `components/Select.svelte` | `value` (bindable), `options`, `onchange` | Custom dropdown select — styled dark theme, chevron indicator. Used by dashboard components. |
| `components/IdentifyDialog.svelte` | `open` (bindable) | Device identify dialog — shows short device ID, alias input, POST to `/api/device/identify`. Triggered by Ctrl+Shift+I. |
| `components/Button.svelte` | `variant` (`'default'`/`'save'`/`'danger'`), `disabled`, `title`, `onclick`, children | Small action button — default (neutral), save (green), danger (red). Disabled state dims to 35% opacity. |
| `components/Toggle.svelte` | `checked` (bindable), `onchange` | On/off toggle switch — dark-themed, animated knob. With `onchange` it is pure delegation (reports the intended value, does not self-flip) so a store owning the state stays authoritative; without it, `bind:checked` self-flips. |
| `components/SegmentedButtons.svelte` | `value`, `options`, `onselect`, `title` | Horizontal segmented button group — one bordered container, flat buttons, active segment filled. `value` is not bindable: the owning store stays the source of truth and `onselect` reports the pick. |
| `components/RangeSlider.svelte` | `value` (bindable), `min`, `max`, `step`, `formatValue` | Range slider with value display — optional format function for labels (e.g., `v => \`${v}%\``). |
| `components/Tooltip.svelte` | `content`, `position`, children | Reusable tooltip wrapper for desktop controls. |

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
| `compass/layout-loader.ts` | `fetchLabLayout()`, `DEFAULT_DIFFICULTY_ORDER`, `MAX_STATUS_ATTEMPTS`, `STATUS_RETRY_MS` | Layout fetch for the compass, path-strip and timer overlays. Waits out a `server_url` that is not ready yet on a per-call attempt budget (never a shared one — see the file), tries the candidate difficulties in order, returns null and logs on every failure. Covered by `layout-loader.test.ts`. |

## Compass Components

| File | Props | Description |
|------|-------|-------------|
| `compass/RoomMinimap.svelte` | `areaCode`, `doors`, `contents`, `targetDirection`, `roomName` | Room SVG background with positioned door/content overlays. Target exit highlighted green. |
| `compass/DirectionCompass.svelte` | `directions`, `targetDirection`, `roomName`, `hasContent` | Compass circle with exit markers at compass angles. |
| `compass/MinimalBar.svelte` | `targetDirection`, `contents`, `timerText` | Compact bar with arrow, content badges, timer. |
| `compass/CompassOverlay.svelte` | `mode`, all child props | Mode switcher — renders minimap, direction, or minimal mode. |
| `compass/LabGraph.svelte` | layout/navigation props | Full lab graph used by planner and path-strip presentation. |
| `compass/RoomEditor.svelte` | room/editing props | Room metadata and connection editor used by planner tooling. |

## Pages

Located in `$lib/pages/`. Always mounted in the layout, toggled via `nav` store — **not** SvelteKit routing.

| File | Description |
|------|-------------|
| `pages/LabPage.svelte` | Lab farming dashboard — tabs (Session/Rankings/Font EV/Market), comparator, session queue, best plays, font EV, market overview. |
| `pages/PlannerPage.svelte` | Lab Planner — full lab graph view, route strategy, compass mode, layout import. Rendered as the "Planner" tab inside LabPage. |
| `pages/SettingsPage.svelte` | Settings — General, Game Integration, Overlays, Trade, Logs. |
| `pages/RunHistoryPage.svelte` | Lab run-history presentation. Present in the library but not currently wired into `nav.View`. |

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

**Start**: Izaro death voiceline (`LabFinished` nav event) — triggers when final Izaro is killed, right when the font becomes available.

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

The comparator uses selective click-through (`WS_EX_NOACTIVATE` + `WS_EX_TRANSPARENT`) with a global `WH_MOUSE_LL` hook and a right-edge interactive zone. Display-only compass/path-strip/timer overlays use `interactiveWidth: 0`; the capture/configuration overlay is intentionally interactive. See `docs/OVERLAY-GUIDE.md` for the current distinctions and regression guards.

## Conventions

- **Stores**: `.svelte.ts` extension (Svelte 5 runes). Export objects, mutate properties (NOT reassign).
- **Components**: `.svelte` files in `components/`. Props via `$props()`. Scoped styles.
- **Utilities**: `.ts` files. Pure functions, no reactivity.
- **Styling**: CSS custom properties from `app.css` (`--bg`, `--surface`, `--border`, `--text`, `--text-muted`, `--accent`, `--success`, `--warning`).
- **Tauri commands**: Use `invoke()` from `@tauri-apps/api/core`. Follow the established event, command, or reconciliation mechanism for the state path being changed.
- **State flow**: Shared status is event-driven, while comparator and some overlay settings/layout paths poll or reconcile against Rust/server state. Do not introduce duplicate owners.
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
