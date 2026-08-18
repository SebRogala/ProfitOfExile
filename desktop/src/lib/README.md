# Desktop App Library (`$lib`)

Component registry for the ProfitOfExile desktop app. Read this first before creating or modifying components.

## Stores

| File | Export | Description |
|------|--------|-------------|
| `stores/status.svelte.ts` | `store`, `initStatusStore()` | Shared app state — event-driven from Rust backend. No polling. Call `initStatusStore()` once from root layout. Read `store.status` and `store.logs` reactively. |
| `stores/ssot.svelte.ts` | `ssot`, `startSsotStore()`, `setNormalVariant()`, `setDedicationVariant()`, `setDedicationPool()`, `setDedicationSelection()`, `setModuleEnabled()` | Rust-owned single source of truth for market selection, polled every 3000 ms. Read `ssot.league` / `ssot.normalVariant` / `ssot.dedicationVariant` / `ssot.dedicationPool` / `ssot.modules` reactively; write through the exported setters (never assign the fields directly or keep a local copy). `ssot.mercenary` (POE-165) is the Merc OCR slice — status, last capture, learned templates, last error, geometry source — and is read-only: it has no setter, a snapshot carrying it replaces it whole, and a snapshot lacking it leaves the last known one standing. |
| `stores/navigation.svelte.ts` | `nav`, `viewToPath()` | Global view toggle. `nav.view` is `'lab' \| 'settings' \| 'dev' \| 'mercenaries'`. All pages are always mounted (hidden via CSS) — do not use SvelteKit routing for main views because it unmounts their listeners. `viewToPath(view)` gives the Sidebar path for a view (`'lab'` → `'/'`); use it instead of re-deriving the mapping, and add both a `go()` branch and a `VIEW_PATHS` entry when adding a view. |

## Components

| File | Props | Description |
|------|-------|-------------|
| `components/TopBar.svelte` | `status` | Custom title bar — logo, status indicators, debug toggle (dev only), settings link, window controls (min/max/close). Draggable. |
| `components/Sidebar.svelte` | `open`, `currentPath`, `onToggle` | Collapsible nav — strategies, tools, overlay quick-toggles. Collapsed state shows thin clickable strip. |
| `components/Select.svelte` | `value` (bindable), `options`, `onchange` | Custom dropdown select — styled dark theme, chevron indicator. Used by dashboard components. |
| `components/IdentifyDialog.svelte` | `open` (bindable) | Device identify dialog — shows short device ID, alias input, POST to `/api/device/identify`. Triggered by Ctrl+Shift+I. |
| `components/Button.svelte` | `variant` (`'default'`/`'save'`/`'danger'`), `disabled`, `title`, `onclick`, children | Small action button — default (neutral), save (green), danger (red). Disabled state dims to 35% opacity. |
| `components/Toggle.svelte` | `checked` (bindable) | On/off toggle switch — dark-themed, animated knob. |
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

## Mercenary Data

| File | Exports | Description |
|------|---------|-------------|
| `mercenaries/rulesets.ts` | `MERC_SOURCES`, `allRulesets()`, `kinetistLadder()`, `entryRole()`, `entryTier()`, `entryKind()`, `SOURCE_IDS`, `ARCHETYPES`, `TIERS`, `GROUP_TYPES` + types | Declarative transcription of the guides' saved trade searches — sources → rulesets → filter groups → entries, each carrying its `enabledInSearch` switch. `entryKind(group, entry)` is the single owner of the required/forbidden/bonus rule (type first: a `not` group's entries stay forbidden even when switched off); consumers call it rather than reading the flags themselves. |
| `mercenaries/trade-links.ts` | `savedSearchUrl()`, `rulesetQuery()`, `derivedSearchUrl()`, `flipKey()`, `MercSavedSearch`, `TradeQuery`, `QueryFlips` | Both links a ruleset has. `savedSearchUrl()` is the bare `/trade/search/<league>/<hash>` path, no `?q=` and no default-league fallback. `rulesetQuery()` rebuilds the same search from the data model so the verdict can flip toggles (`QueryFlips`: entries keyed `<groupId>/<entryId>`, plus `enableGroups` for reviving a group the guide parked over a bonus that fired; `not` groups never flip, group or entry), and `derivedSearchUrl()` sends it as `?q={"query":…}`. The builder never normalises `disabled` — the round-trip test owns that normaliser, so the builder cannot launder a transcription error into a match. |
| `mercenaries/capture.ts` | `MercenarySlice`, `MercCapture`, `MercRow`, `MercSkillRead`, `MercSupportRead`, `MercHeader`, `MercStatus`, `ReadState`, `MercGeometrySource`, `mercenarySliceDefault()` | TypeScript mirror of the Rust `mercenary` SSOT slice — camelCase fields, snake_case enum wire strings, pinned from the Rust side by a serde test. Read-only in the webview: captures only ever arrive from Rust, so this file holds types and one default, nothing else. |
| `mercenaries/merc-prefs.ts` | `MERC_SOURCES_OFF_PREF_KEY`, `parseSourcesOff()`, `enabledSources()` | The page's own ADR-013 preference: `mercSourcesOff`, a comma-separated list of the sources switched OFF (off-list, so a source added later starts on). Unknown ids are dropped on read. `enabledSources(raw)` is the inversion the verdict engine takes — call it rather than re-deriving the complement. |
| `mercenaries/verdict.ts` | `evaluateCapture()` + result types (`MercVerdict`, `MercSourceVerdict`, `MercRulesetResult`, `MercGroupResult`, `MercPosition`) | Pure verdict engine: a capture, the rulesets, the enabled sources and the active league in; a per source × per ruleset × per rule-position result tree out — no store, no fetch, no rendering. The league is nullable and only the derived links depend on it (`derivedUrl` is null without one; the saved link stays bound to the league its hash was saved in). Sources are evaluated independently and never merged. A read that is not `matched`/`confirmed` is UNKNOWN, never absent, so a group that could still reach its minimum reports `unknown` rather than a silent fail. `rowSatisfies` is the single owner of the assumption that `mercenary` groups scope to one skill row (A1, documented at the function). |
| `mercenaries/ladder-view.ts` | `ladderRows()`, `rungOutcomes()`, `quantifier()`, `quantifierParts()`, `kindTitle()`, `columnLabel()`, `sharedValue()`, `TIER_LABELS` + row types | Presentation derivations for `MercenariesPage` — quantifier prose, kind wording, and the transposition of the Kinetist rungs into one tier matrix (`ladderRows`), whose per-tier state is `entryKind`'s answer looked up by group id + entry id in each rung. `rungOutcomes()` adds the matrix's verdict header row, matching results to rungs by ruleset id and leaving a rung with no result blank. Entry rows carry `groupId`/`entryId` alongside the joined key so the page never splits it back apart. A view module, not the data model: it adds no rule and stores no tier fact. Lives outside the page because `.svelte` pages have no unit-test harness here. |
| `mercenaries/capture-view.ts` | `skillText()`, `skillTitle()`, `supportText()`, `supportTitle()`, `capturedAt()`, `positionOutcomeLabel()`, `indexPositions()`, `indexGroups()`, `positionKey()`, `groupKey()`, `notInRulesNames()`, `parseLearnedTemplate()`, `describeDebugResult()`, `READ_GLYPH`, `READ_TONE`, `READ_STATE_LABEL`, outcome label/tone maps | Presentation derivations for the capture + verdict half of `MercenariesPage`, the sibling of `ladder-view.ts`. Words every `ReadState` (a read that is not `matched`/`confirmed` says "hover to confirm" in the cell, and the `title` carries the raw text plus the score), words a rule position in the terms of its kind (`positionOutcomeLabel` — a forbidden entry passes by being ABSENT, so the outcome alone cannot be worded), and keys verdict lookups on ruleset + group + entry (guide B's rungs share group ids; sibling groups repeat entry ids). Decides nothing: `verdict.ts` owns the outcomes. |
| `mercenaries/__fixtures__/` | raw JSON | The seven saved-search responses verbatim plus GGG's Mercenary stat vocabulary. Ground truth for `rulesets.test.ts` — the data module is asserted against these, not against itself. See the directory's `README.md` for source URLs and re-fetch commands. |

## Pages

Located in `$lib/pages/`. Always mounted in the layout, toggled via `nav` store — **not** SvelteKit routing.

| File | Description |
|------|-------------|
| `pages/LabPage.svelte` | Lab farming dashboard — tabs (Session/Rankings/Font EV/Market), comparator, session queue, best plays, font EV, market overview. |
| `pages/PlannerPage.svelte` | Lab Planner — full lab graph view, route strategy, compass mode, layout import. Rendered as the "Planner" tab inside LabPage. |
| `pages/SettingsPage.svelte` | Settings — General, Game Integration, Overlays, Trade, Logs. |
| `pages/MercenariesPage.svelte` | Mercenaries — capture status bar (module status, debug-capture command, learned templates with forget/reset, last error, geometry source), the last capture's rows with per-read glyphs, the verdict per source (headline strip for every source, the `SegmentedButtons` switcher picking the expanded one, per-ruleset outcome + saved and derived links, per-position outcome column), the rulesets as a card grid of glyph rows with guide B's four rungs as one tier matrix (`$lib/mercenaries/ladder-view`), and a Settings section toggling sources through the `mercSourcesOff` pref. The verdict is `$derived` from `ssot.mercenary.capture` + `MERC_SOURCES` + the pref + `ssot.league` via `evaluateCapture`, never stored. Reads `ssot.mercenary`, `ssot.league` and its own prefs only — never `ssot.modules` (ADR-014: the page is browsable with the module off). |
| `pages/RunHistoryPage.svelte` | Lab run-history presentation. Present in the library but not currently wired into `nav.View`. |

## Routes

Only used for the app shell and overlay windows. **Do NOT add page routes** — use `$lib/pages/` + `nav` store instead.

| Route | Description |
|-------|-------------|
| `(app)/+layout.svelte` | App shell — TopBar + Sidebar + renders all pages (LabPage, SettingsPage, MercenariesPage, DevPage in DEV). View switching via `nav` store. |
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
- `FontOpened` — Client.txt `InstanceClientLabyrinthCraftResultOptionsList recieved` (fires on font open AND on CRAFT click — see Game UI facts below)
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

**Start**: Izaro death voiceline (`LabFinished` nav event) — triggers when final Izaro is killed, right when the font becomes available. A `FontOpened` event also starts a scan whenever the liveness token (`font_scan_live_gen`) reads 0, so the panel is still read when `LabFinished` was never seen — the app launched mid-run, or the scan was stopped earlier by a zone change or lab exit.

**Running**: Scans at 250ms, parses options via `font_parser`, then union-merges the frame into the current round's option buffer (`merge_options`). A torn frame never deletes an option an earlier frame of the same panel read, and a value already read is never downgraded to `None` or overwritten by a disagreeing later read.

**Round tracking**: the panel's "Crafts Remaining" count owns the round boundary (`font_ledger`), gated on a `FontOpened` counter. `FontOpened` seals nothing — it fires on font open as well as on CRAFT, an unbounded number of times per craft. A count change with no `FontOpened` since the last accepted count is a misread: the frame's options still merge, the count is ignored. A count change after a `FontOpened` that holds for 2 consecutive frames seals the current round under the *old* count and opens the next one; direction is irrelevant. Known limitation: a round on screen for under 2 frames (~500ms) is never accepted, so its options fold into the neighbouring round.

**Stop**: a `font_scan_generation` bump — ZoneChanged, `LabExited`, a replacement scan, app shutdown — or 10 minutes with no active font panel on screen. There is no wall-clock timeout: a font run has no bounded length (stash trips, town portals, a player reading the options), and a scan expiring under a still-open panel silently lost every remaining craft. The idle limit measures from the last frame that saw the panel, so it only fires on a scan with nothing left to read — it is the backstop for a run whose stop event never arrives (the game killed rather than exited, the font never opened, screen capture failing in a loop). That path sends the session itself; every other stop is either a sender or is followed by one.

**Re-arm**: the token is what makes that restart exactly-once — a scan claims it on spawn and releases it on exit with a compare-exchange, so a superseded loop cannot clear its replacement's claim and a `FontOpened` arriving mid-handover does not stack a second loop. The case that needs it most is a portal trip out of the lab: `LabFinished` never fires again on return, so without the re-arm every remaining craft is lost. Every stop that can have rounds behind it sends and resets the session before a re-arm can run: ZoneChanged does it on the send path (and fires on the same log line as `LabExited`, which only stops the loop), idle expiry does it from inside the loop. So a re-armed scan is always a new segment; a mid-font town trip therefore reports one run as two font-session POSTs.

**Data flow**: ZoneChanged sends accumulated session (all rounds with options + crafts_remaining) to server via `POST /api/desktop/font-session`.

### Game UI Context

- **CRAFT screen**: Shows options list + "Crafts Remaining: X" + CRAFT button. "Crafts Remaining" only visible when X > 1.
- **CONFIRM screen**: Shows 3 gem slots + CONFIRM button. Options list is gone. This is when gem tooltip OCR runs.
- `FontOpened` (`InstanceClientLabyrinthCraftResultOptionsList recieved`) fires in TWO moments: when the font is OPENED (CRAFT screen appears) and when CRAFT is CLICKED (CONFIRM screen appears). Opening the font, closing it for a stash trip, and reopening it therefore yields 3 events by the time CRAFT is clicked. Never derive CRAFT/CONFIRM parity or the round count from event count — the count is unbounded per craft. (Rule confirmed by Sebastian from play, 2026-08-18; earlier notes claiming "no event on open" were wrong.)
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
