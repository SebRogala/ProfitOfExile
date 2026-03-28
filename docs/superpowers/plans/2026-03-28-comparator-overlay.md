# Comparator Overlay + Game Focus Detection

## Context

The desktop app (Tauri v2 + SvelteKit 5 + Rust) has a working lab farming dashboard with a Comparator that shows gem comparison results. OCR detects gems from tooltips and populates the Comparator automatically. The next step is an **overlay window** that shows comparator results on top of the game, and **game focus detection** to show/hide overlays.

## Feature 1: Game Focus Detection

### How it works
- `Client.txt` already emits `[WINDOW] Lost focus` and `[WINDOW] Gained focus` log lines
- The log watcher (`lib.rs: spawn_log_watcher`) already parses Client.txt lines
- The lab state machine (`lab_state.rs`) processes lines — add focus events here

### Implementation
1. **Add to `LabEvent`**: `GameFocused` and `GameBlurred` variants
2. **Add to `LabStateMachine::process_line`**: detect `[WINDOW] Lost focus` → `GameBlurred`, `[WINDOW] Gained focus` → `GameFocused`
3. **Add `game_focused: Mutex<bool>` to `AppState`** and include in `AppStatus`
4. **In log watcher**: on `GameFocused` → set `game_focused = true`, emit status. On `GameBlurred` → set `game_focused = false`, emit status.
5. **Emit Tauri event**: `app.emit("game-focus-changed", true/false)` for overlay windows to react instantly
6. **Frontend**: overlay manager listens to `game-focus-changed` and shows/hides all overlay windows

### Key detail
- Focus events fire frequently (every alt-tab). Must be cheap — no heavy processing.
- Overlay windows should hide/show, NOT destroy/recreate (expensive Tauri window operations).
- Use `window.set_visible(false)` / `window.set_visible(true)` on Tauri overlay WebviewWindows.

## Feature 2: Comparator Overlay Window

### Design
- A Tauri overlay window (transparent, always-on-top, no decorations)
- Shows the Comparator results in a compact format (gem name + price + signal + recommendation)
- Positioned near the game's font panel area (configurable in settings)
- Clickable: "Next" button to queue the selected gem and clear for next round

### Architecture
- **Route**: `/overlay/comparator` — outside the `(app)` group (no topbar/sidebar)
- **Layout**: `routes/overlay/+layout.svelte` already exists (transparent, no shell)
- **State sharing**: overlay reads the same Svelte stores / listens to the same Tauri events (`gem-detected`, `gems-cleared`)
- **Window management**: use existing `overlay/manager.ts` (`showOverlay`, `destroyOverlay`)

### Overlay UI (compact)
```
┌──────────────────────────────────────┐
│ ⚗ Kinetic Blast of Clustering  TOP  │
│   1940c  (Risk-adj: 1869c)          │
│   GOOD 75  ━ UNCERTAIN  ✓ SAFE      │
│                                      │
│ ⚗ Cyclone of Tumult           TOP   │
│   1705c  (Risk-adj: 1584c)          │
│   GOOD 75  ? UNCERTAIN  • FAIR      │
│                                      │
│ ⚗ Spark of the Nova          HIGH   │
│   898c   (Risk-adj: 864c)           │
│   GOOD 80  ━ STABLE     ✓ SAFE      │
│                                      │
│  [✓ Pick: Kinetic Blast]  [Clear]   │
└──────────────────────────────────────┘
```

### Settings integration
- Overlay position (x, y) — configurable via drag or settings page
- Overlay size (width, height)
- Show preview of overlay in settings when configuring position
- Auto-show when scanning starts, auto-hide when scanning stops
- Hide when game loses focus, show when game gains focus

### Files to create/modify
```
desktop/src-tauri/src/
  lab_state.rs          — Add GameFocused/GameBlurred events
  lib.rs                — Handle focus events in log watcher, add game_focused to AppState/AppStatus

desktop/src/
  routes/overlay/
    comparator/
      +page.svelte      — Compact overlay UI for comparator results
  lib/
    stores/status.svelte.ts — game_focused flows through store.status
    overlay/manager.ts  — Add focus-aware show/hide logic

desktop/src-tauri/src/settings.rs — Add overlay position/size to Settings
```

### Existing overlay infrastructure
- `overlay/manager.ts` has `showOverlay()`, `destroyOverlay()`, `getOverlay()`, `isOverlayActive()`, `readOverlayRegion()`
- Overlays are Tauri `WebviewWindow` — transparent, always-on-top, no decorations
- DPI-aware: constructors take logical pixels, regions store physical pixels
- Route at `/overlay/{name}` renders in the transparent layout

### Key decisions needed
1. Should the overlay subscribe to Mercure SSE independently, or relay data from the main window via Tauri events?
   → **Recommend**: relay via Tauri events. The main window already has the data. Emit `comparator-results-changed` with the gem data when results update.
2. Should "Pick" button trigger the queue operation, or just visually highlight?
   → **Recommend**: trigger queue + clear (same as the "Next" button in the main Comparator)
3. How to handle overlay positioning on different monitor setups?
   → Store as physical pixels (like capture regions). Use `window.devicePixelRatio` for conversion.

### Testing approach
1. Game focus: write to test-client.txt `[WINDOW] Lost focus` and `[WINDOW] Gained focus` — verify overlay hides/shows
2. Comparator overlay: manually start scanning, hover gem screenshots — verify overlay populates
3. Position: drag overlay to desired position, restart app — verify position persists
