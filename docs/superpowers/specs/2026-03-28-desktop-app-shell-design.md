# Desktop App Shell Design

> Spec for the ProfitOfExile desktop app UI architecture — main window, overlay system, routing, settings, and branding.

## 1. Architecture: Main Window + Overlay System

The app has two layers:

**Main window** — the full dashboard. Resizable, meant for a second monitor or alt-tab. Contains strategy-specific views (Lab dashboard, Comparator, Market), shared tools (Trade Lookup, Price Compare), and settings. Default size: 1024x768.

**Overlay windows** — independent always-on-top Tauri WebviewWindows for in-game use. Each overlay is transparent, borderless when locked, and routes to `/overlay/{name}`. Overlays are the single-screen experience — on a laptop, the user sees just the overlays over the game and alt-tabs to the main window for analysis.

## 2. Main Window Layout

Three regions:

### Top bar (fixed)
- Logo (golden orb) + "ProfitOfExile" text
- Status indicators: connection dot (green/red), scanning dot (green/grey), pair code
- Debug/Prod toggle
- Gear icon → navigates to Settings page

### Sidebar (collapsible)
Collapses to icons or fully hides. Contains:

**Strategies section:**
- Lab Farming (active first)
- Mapping (future)
- Bosses (future, greyed out)

**Tools section:**
- Trade Lookup
- Price Compare

**Overlays section** (bottom of sidebar):
- Per-overlay row: name + mode indicator (always/auto/off)
- Click to cycle mode
- "Show All" / "Hide All" quick actions

**Collapse toggle** at the very bottom.

### Content area
- Single scrollable view per strategy — all components stacked vertically (same pattern as the current web `/lab` page)
- No sub-tabs splitting content into separate routes
- Lab view stacks: Header → Comparator → Session Queue → BestPlays → ByVariant → Market Overview → Legend
- Content scrolls independently from sidebar/top bar
- Mercure SSE connection maintained — all components auto-update on new data (same as web)

## 3. SvelteKit Routing

```
src/routes/
  +layout.svelte          ← app shell (top bar + sidebar + content slot)
  +page.svelte            ← redirect to /lab
  lab/
    +page.svelte          ← single scrollable view: all components stacked
                             (Comparator, Session Queue, BestPlays, ByVariant,
                              Market Overview, Legend — same as web /lab page)
                             Mercure SSE for auto-updates on new snapshots.
  settings/
    +page.svelte          ← all settings (grouped sections)
  overlay/
    +layout.svelte        ← shared overlay layout (transparent bg, no app shell)
    compass/+page.svelte
    ocr-status/+page.svelte
    comparator/+page.svelte
    session/+page.svelte
    region/+page.svelte   ← existing capture region (gem tooltip)
    font-panel/+page.svelte ← font options + uses remaining
```

## 4. Overlay System

### Overlay inventory

| Overlay | Purpose | Contextual trigger |
|---------|---------|-------------------|
| Lab Compass | Zone layout, exit direction, traps, darkshrines | Lab zone entry (Client.txt) |
| OCR Status | Green/red dot indicator. Toggle scanning via hotkey or main window. | Scanning state change |
| Comparator | 3 detected gems with prices, select + next | 3 gems detected by OCR |
| Session Tally | Running list of picked gems + total value | First gem pick |
| Capture Region (gem tooltip) | Configurable OCR area for gem name on hover | Manual from settings |
| Capture Region (font panel) | Configurable OCR area for font options + uses remaining | Manual from settings |

### Per-overlay config (persisted to disk)

- **mode**: `always` (visible whenever app runs) / `contextual` (auto-show/hide on trigger events) / `off`
- **position**: {x, y} physical pixels — restored on launch
- **size**: {w, h} physical pixels — restored on launch

### Lock / Unlock

**Locked (default during gameplay):**
- Fully click-through — mouse events pass to the game underneath
- Borderless — overlay shows only its content, no frame
- Cannot be dragged or resized
- Unlock only from: main window overlay panel (no global hotkeys — avoids conflicts with PoE keybinds)

**Unlocked (for configuration):**
- Red border appears (same style as current capture region overlay)
- Draggable from interior, resizable from edges
- Clearly visible as an interactive window
- Clicking the overlay captures input (does NOT pass through to game)
- Lock again from: main window overlay panel

### Overlay window properties
- Tauri WebviewWindow with `transparent: true`, `decorations: false`, `alwaysOnTop: true`
- Same pattern as the existing capture region overlay
- Each routes to `/overlay/{name}` in SvelteKit
- Transparency workaround (resize ±1px on mount) applied to all overlays

## 5. OCR Regions

Two capture regions, each independently configurable:

### Region 1: Gem tooltip
- Captures the gem name shown on hover in the Font UI
- Upper-left area of the Font panel (golden text line)
- Existing implementation — already working

### Region 2: Font panel
- Captures the font options area + "uses remaining" counter
- Options and uses counter share the same vertical area (uses remaining is dynamic in height)
- Single OCR region covers both
- Client.txt remains the **activation trigger** — `InstanceClientLabyrinthCraftResultOptionsList` enables OCR scanning
- Font panel OCR handles **lifecycle within** the active font session:
  - Uses counter tracks remaining picks
  - Font exhausted when uses hit 0
- Zone-change detection (Client.txt) kept as safety net for session end

**Open design question:** stash-and-return during Font session. User opens Font, sees options, walks to stash to check inventory, walks back. Font UI is still open but Client.txt may log a zone/area change. Current implementation would kill the session on zone change. Needs a smarter signal — possibly: only end session on zone changes to town/hideout areas, not on sub-area movements within the lab. To be refined during implementation.

### Crowd-sourced OCR regions
User OCR settings are sent to the server:
- Screen resolution + DPI scaling
- Both capture region positions and sizes
- Associated with anonymous device ID

Server normalizes by resolution and proposes recommended regions:
- First-time setup: "1920x1080 detected — 47 users shared regions. Use recommended?"
- Users can always override manually
- Regions are updated as more users contribute

## 6. Settings Page

Dedicated route at `/settings`. Organized into sections:

### General
- Server URL (prod / custom input) + debug toggle
- League name (dropdown or text)
- Pair code display + regenerate button

### Game Integration
- Client.txt path (auto-detect + manual override)
- OCR Region 1: gem tooltip (show overlay to configure)
- OCR Region 2: font panel (show overlay to configure)
- Scan interval (ms)

### Trade
- Auto-trigger: on/off
- Cache min-age threshold: 1-30 min slider
- Rate limit status display (current budget remaining)

### Overlays
- Per overlay: mode selector (always / contextual / off)
- Per overlay: reset position button
- Lock / Unlock all overlays toggle
- "Show All" / "Hide All" quick actions

## 7. Branding

- **Logo**: existing golden orb (pixel art, recraft.ai) in `desktop/src-tauri/icons/`
- **Color palette**: existing CSS custom properties — `--bg: #1a1a2e`, `--surface: #16213e`, `--border: #0f3460`, `--accent: #e94560`, `--success: #4ade80`, `--warning: #fbbf24`
- **Font**: system font stack (`-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif`)
- **Window title**: "ProfitOfExile"
- **Default window size**: 1024×768 (resizable)

## 8. Implementation Scope — App Shell Only

This spec covers the **app shell** — the structural skeleton that content lives inside. It does NOT include:
- Migrating web dashboard components (Font EV, BestPlays, ByVariant, etc.)
- Lab Compass implementation
- Overlay content implementations (just the window management)
- Crowd-sourced OCR region server API

What gets built:
- Root layout with top bar + collapsible sidebar
- SvelteKit route structure with placeholder pages
- Settings page with all config sections (wired to existing Tauri commands)
- Overlay manager (spawn/destroy/toggle overlays from main window)
- Lock/unlock system for overlays
- Branding (logo in top bar, proper window size)
- Move existing functionality (scan controls, trade lookup, pair code) into the new layout
