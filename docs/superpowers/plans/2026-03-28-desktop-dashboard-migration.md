# Desktop Dashboard Migration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Migrate the full web lab farming dashboard (15 components, ~5300 lines) to the Tauri desktop app with zero simplifications.

**Architecture:** The web dashboard fetches from `/api/*` (same origin). The desktop app fetches from `store.status.server_url + '/api/*'` (configured server URL, default `https://poe.softsolution.pro`). CSS theming uses identical `--color-lab-*` variables added to desktop's `app.css`. Components are copied verbatim except: (1) the API layer uses server URL instead of relative paths, (2) the Comparator uses Tauri events for gem detection instead of Mercure SSE desktop bridge.

**Tech Stack:** SvelteKit 5 (runes), Tauri v2 (events/invoke), CSS custom properties, EventSource (Mercure SSE)

---

## File Structure

### New files to create:

```
desktop/src/lib/
  api.ts                    — Server API client (adapted from web: server_url base)
  tradeApi.ts               — Trade API client (adapted from web: server_url base)
  trade-utils.ts            — Trade URL utilities (verbatim copy from web)
  gem-icons.ts              — Gem icon resolver (verbatim copy from web)
  tooltips.ts               — Tooltip text constants (verbatim copy from web)
  components/
    Select.svelte           — Dropdown select (copy from web, CSS var aliases)

desktop/src/routes/(app)/
  components/               — Dashboard components (15 files)
    Header.svelte           — verbatim copy
    Comparator.svelte       — adapted (Tauri events instead of desktop bridge)
    SessionQueue.svelte     — verbatim copy
    BestPlays.svelte        — verbatim copy
    ByVariant.svelte        — verbatim copy
    FontEVCompare.svelte    — verbatim copy
    FontEV.svelte           — verbatim copy
    MarketOverview.svelte   — verbatim copy (remove onDestroy import, use $effect)
    Legend.svelte            — verbatim copy
    SignalBadge.svelte       — verbatim copy
    Sparkline.svelte        — verbatim copy
    GemIcon.svelte          — verbatim copy
    InfoTooltip.svelte      — verbatim copy
    OfferingChart.svelte    — verbatim copy
  dev/
    +page.svelte            — Existing dev tools (moved from +page.svelte)
```

### Files to modify:

```
desktop/src/app.css                         — Add --color-lab-* variables
desktop/src/routes/(app)/+page.svelte       — Replace with dashboard page
desktop/src/lib/components/Sidebar.svelte   — Add Dev Tools nav link
desktop/src/lib/README.md                   — Add Select component + lab docs
```

## CSS Variable Mapping

The web uses `--color-lab-*` variables. Rather than search/replace across 15 components, we add these exact variables to desktop's `app.css`. This keeps all component code identical.

```css
/* Lab dashboard theme — matches web frontend */
--color-lab-bg: #0f1117;
--color-lab-surface: #1a1d27;
--color-lab-border: #2a2d37;
--color-lab-text: #e4e4e7;
--color-lab-text-secondary: #9ca3af;
--color-lab-green: #22c55e;
--color-lab-red: #ef4444;
--color-lab-yellow: #eab308;
--color-lab-blue: #3b82f6;
--color-lab-purple: #a855f7;
--color-lab-green-muted: #6b9e7a;
```

## API Adaptation Pattern

Web pattern:
```ts
const API_BASE = '/api';
const url = new URL(`${API_BASE}${path}`, window.location.origin);
```

Desktop pattern:
```ts
import { store } from '$lib/stores/status.svelte';
function getApiBase(): string {
    return (store.status?.server_url || 'https://poe.softsolution.pro') + '/api';
}
const url = new URL(`${getApiBase()}${path}`);
```

## Desktop Bridge Adaptation

Web Comparator uses `subscribeToDesktopGems()` (Mercure SSE via pairing code).
Desktop Comparator uses `listen('gem-detected')` (Tauri events directly from Rust OCR).

The `gem-detected` event payload from Rust: `{ gems: string[], variant: string }`.

---

## Task 1: CSS Variables + Utility Files

**Files:**
- Modify: `desktop/src/app.css`
- Create: `desktop/src/lib/trade-utils.ts`
- Create: `desktop/src/lib/gem-icons.ts`
- Create: `desktop/src/lib/tooltips.ts`

- [ ] **Step 1: Add lab CSS variables to desktop app.css**

In `desktop/src/app.css`, inside the `:root` block, add after existing variables:

```css
/* Lab dashboard theme — matches web frontend for zero-change component migration */
--color-lab-bg: #0f1117;
--color-lab-surface: #1a1d27;
--color-lab-border: #2a2d37;
--color-lab-text: #e4e4e7;
--color-lab-text-secondary: #9ca3af;
--color-lab-text-muted: #888;
--color-lab-green: #22c55e;
--color-lab-red: #ef4444;
--color-lab-yellow: #eab308;
--color-lab-blue: #3b82f6;
--color-lab-purple: #a855f7;
--color-lab-green-muted: #6b9e7a;
```

- [ ] **Step 2: Copy utility files verbatim from web**

Copy these files with zero changes:
- `frontend/src/lib/trade-utils.ts` → `desktop/src/lib/trade-utils.ts`
- `frontend/src/lib/gem-icons.ts` → `desktop/src/lib/gem-icons.ts`
- `frontend/src/lib/tooltips.ts` → `desktop/src/lib/tooltips.ts`

These are pure utility files with no framework dependencies or API calls.

- [ ] **Step 3: Verify**

Run: `cd /var/www/ProfitOfExile/desktop && cat src/app.css | head -25`

Confirm the `:root` block contains both original desktop vars AND new `--color-lab-*` vars.

---

## Task 2: Desktop API Layer

**Files:**
- Create: `desktop/src/lib/api.ts`
- Create: `desktop/src/lib/tradeApi.ts`

- [ ] **Step 1: Create desktop api.ts**

Copy `frontend/src/lib/api.ts` to `desktop/src/lib/api.ts` with these exact changes:

1. Replace the `API_BASE` constant and `get()` helper:

**Web (replace this):**
```ts
const API_BASE = '/api';

async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
    const url = new URL(`${API_BASE}${path}`, window.location.origin);
```

**Desktop (with this):**
```ts
import { store } from '$lib/stores/status.svelte';

function getApiBase(): string {
    return (store.status?.server_url || 'https://poe.softsolution.pro') + '/api';
}

async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
    const url = new URL(`${getApiBase()}${path}`);
```

2. In `connectMercure()`, update the token fetch to use absolute URL:

**Web:** `const tokenResp = await get<{ token: string; url: string }>('/mercure/token');`

**Desktop:** `const tokenResp = await get<{ token: string; url: string }>('/mercure/token');`

(No change needed — `get()` already uses `getApiBase()` which includes `/api`.)

3. Remove the `import { dispatchTradeEvent } from './tradeApi';` line and the `dispatchTradeEvent(parsed)` call in `connectMercure` — these will be re-added in the tradeApi file. Actually NO — keep them. The desktop tradeApi will export the same function.

Everything else stays identical: all types, all mapping functions, all fetch functions, the full Mercure SSE connection with retry logic.

- [ ] **Step 2: Create desktop tradeApi.ts**

Copy `frontend/src/lib/tradeApi.ts` to `desktop/src/lib/tradeApi.ts` with this change:

**Web:**
```ts
const API_BASE = '/api';
// ...
const resp = await fetch(`${API_BASE}/trade/lookup`, {
```

**Desktop:**
```ts
import { store } from '$lib/stores/status.svelte';

function getApiBase(): string {
    return (store.status?.server_url || 'https://poe.softsolution.pro') + '/api';
}
// ...
const resp = await fetch(`${getApiBase()}/trade/lookup`, {
```

Everything else stays identical: all types, the listener registry, `dispatchTradeEvent`, `lookupTrade`, `pollTradeResult`.

- [ ] **Step 3: Verify imports compile**

Run: `cd /var/www/ProfitOfExile/desktop && grep -n "from '\$lib/" src/lib/api.ts src/lib/tradeApi.ts`

Confirm: `api.ts` imports from `$lib/stores/status.svelte` and `$lib/tradeApi`. `tradeApi.ts` imports from `$lib/stores/status.svelte`.

---

## Task 3: Select Component

**Files:**
- Create: `desktop/src/lib/components/Select.svelte`

- [ ] **Step 1: Copy Select.svelte from web**

Copy `frontend/src/lib/components/Select.svelte` to `desktop/src/lib/components/Select.svelte` verbatim.

The CSS uses `--color-lab-*` variables which are now defined in desktop's `app.css`.

- [ ] **Step 2: Update desktop lib README**

Add Select to the components table in `desktop/src/lib/README.md`:

```markdown
| `components/Select.svelte` | `value` (bindable), `options`, `onchange` | Custom dropdown select — styled dark theme, chevron indicator. |
```

---

## Task 4: Copy Small Reusable Components

**Files:**
- Create: `desktop/src/routes/(app)/components/GemIcon.svelte`
- Create: `desktop/src/routes/(app)/components/Sparkline.svelte`
- Create: `desktop/src/routes/(app)/components/InfoTooltip.svelte`
- Create: `desktop/src/routes/(app)/components/SignalBadge.svelte`

- [ ] **Step 1: Create components directory**

```bash
mkdir -p /var/www/ProfitOfExile/desktop/src/routes/\(app\)/components
```

- [ ] **Step 2: Copy GemIcon.svelte verbatim**

Copy `frontend/src/routes/lab/components/GemIcon.svelte` → `desktop/src/routes/(app)/components/GemIcon.svelte`

Imports `$lib/gem-icons` which exists in desktop (Task 1).

- [ ] **Step 3: Copy Sparkline.svelte verbatim**

Copy `frontend/src/routes/lab/components/Sparkline.svelte` → `desktop/src/routes/(app)/components/Sparkline.svelte`

No external imports. Uses `--color-lab-blue` CSS variable.

- [ ] **Step 4: Copy InfoTooltip.svelte verbatim**

Copy `frontend/src/routes/lab/components/InfoTooltip.svelte` → `desktop/src/routes/(app)/components/InfoTooltip.svelte`

No external imports. Uses `--color-lab-*` CSS variables.

- [ ] **Step 5: Copy SignalBadge.svelte verbatim**

Copy `frontend/src/routes/lab/components/SignalBadge.svelte` → `desktop/src/routes/(app)/components/SignalBadge.svelte`

Imports `$lib/tooltips` which exists in desktop (Task 1).

---

## Task 5: Copy Medium Dashboard Components

**Files:**
- Create: `desktop/src/routes/(app)/components/Header.svelte`
- Create: `desktop/src/routes/(app)/components/Legend.svelte`
- Create: `desktop/src/routes/(app)/components/SessionQueue.svelte`
- Create: `desktop/src/routes/(app)/components/FontEV.svelte`
- Create: `desktop/src/routes/(app)/components/OfferingChart.svelte`

- [ ] **Step 1: Copy Header.svelte verbatim**

Copy `frontend/src/routes/lab/components/Header.svelte` → `desktop/src/routes/(app)/components/Header.svelte`

Imports `$lib/api` (type only). Uses `--color-lab-*` CSS vars.

- [ ] **Step 2: Copy Legend.svelte verbatim**

Copy `frontend/src/routes/lab/components/Legend.svelte` → `desktop/src/routes/(app)/components/Legend.svelte`

No external imports. All `--color-lab-*` CSS.

- [ ] **Step 3: Copy SessionQueue.svelte verbatim**

Copy `frontend/src/routes/lab/components/SessionQueue.svelte` → `desktop/src/routes/(app)/components/SessionQueue.svelte`

Imports `./GemIcon.svelte` (relative — exists from Task 4).

- [ ] **Step 4: Copy FontEV.svelte verbatim**

Copy `frontend/src/routes/lab/components/FontEV.svelte` → `desktop/src/routes/(app)/components/FontEV.svelte`

Imports `$lib/api` (types) and `./InfoTooltip.svelte`.

- [ ] **Step 5: Copy OfferingChart.svelte verbatim**

Copy `frontend/src/routes/lab/components/OfferingChart.svelte` → `desktop/src/routes/(app)/components/OfferingChart.svelte`

No external imports. Pure SVG chart component.

---

## Task 6: Copy Large Dashboard Components

**Files:**
- Create: `desktop/src/routes/(app)/components/BestPlays.svelte`
- Create: `desktop/src/routes/(app)/components/ByVariant.svelte`
- Create: `desktop/src/routes/(app)/components/FontEVCompare.svelte`
- Create: `desktop/src/routes/(app)/components/MarketOverview.svelte`

- [ ] **Step 1: Copy BestPlays.svelte verbatim**

Copy `frontend/src/routes/lab/components/BestPlays.svelte` → `desktop/src/routes/(app)/components/BestPlays.svelte`

Imports: `$lib/api`, `$lib/trade-utils`, `$lib/tooltips`, `./SignalBadge.svelte`, `./Sparkline.svelte`, `./GemIcon.svelte`, `./InfoTooltip.svelte`, `$lib/components/Select.svelte`. All exist in desktop.

- [ ] **Step 2: Copy ByVariant.svelte verbatim**

Copy `frontend/src/routes/lab/components/ByVariant.svelte` → `desktop/src/routes/(app)/components/ByVariant.svelte`

Imports: `$lib/api`, `./BestPlays.svelte`, `./InfoTooltip.svelte`, `$lib/components/Select.svelte`.

- [ ] **Step 3: Copy FontEVCompare.svelte verbatim**

Copy `frontend/src/routes/lab/components/FontEVCompare.svelte` → `desktop/src/routes/(app)/components/FontEVCompare.svelte`

Imports: `$lib/api`, `$lib/trade-utils`, `./InfoTooltip.svelte`, `$lib/components/Select.svelte`.

- [ ] **Step 4: Copy MarketOverview.svelte with Svelte 5 fix**

Copy `frontend/src/routes/lab/components/MarketOverview.svelte` → `desktop/src/routes/(app)/components/MarketOverview.svelte`

**One required change:** Replace the Svelte 4 `onDestroy` import with a Svelte 5 `$effect` pattern:

**Web (line 6):**
```ts
import { onDestroy } from 'svelte';
```
```ts
let now = $state(new Date());
const tickInterval = setInterval(() => { now = new Date(); }, 1000);
onDestroy(() => clearInterval(tickInterval));
```

**Desktop (replace with):**
```ts
// Remove the onDestroy import entirely
```
```ts
let now = $state(new Date());
$effect(() => {
    const tickInterval = setInterval(() => { now = new Date(); }, 1000);
    return () => clearInterval(tickInterval);
});
```

Everything else stays identical.

---

## Task 7: Adapt Comparator for Desktop

**Files:**
- Create: `desktop/src/routes/(app)/components/Comparator.svelte`

This is the largest component (1432 lines) and the only one requiring significant adaptation. The web version uses `$lib/desktopBridge` for Mercure SSE pairing. The desktop version uses Tauri events directly.

- [ ] **Step 1: Copy Comparator.svelte from web**

Copy `frontend/src/routes/lab/components/Comparator.svelte` → `desktop/src/routes/(app)/components/Comparator.svelte`

- [ ] **Step 2: Replace desktop bridge imports with Tauri event listener**

**Remove these imports (line 6-7):**
```ts
import { getPairCode, clearPairCode, subscribeToDesktopGems } from '$lib/desktopBridge';
```

**Add Tauri import:**
```ts
import { listen } from '@tauri-apps/api/event';
```

- [ ] **Step 3: Remove desktop pairing props and state**

**Remove from props interface:**
```ts
desktopPair = null,
onDesktopDisconnect,
```
and:
```ts
desktopPair?: string | null;
onDesktopDisconnect?: () => void;
```

**Remove these state declarations:**
```ts
let desktopConnected = $state(false);
let activePairCode = $derived(desktopPair || getPairCode());
```

**Remove the `disconnectDesktop` function entirely.**

- [ ] **Step 4: Replace desktop bridge subscription with Tauri event listener**

**Remove the `$effect` block that calls `subscribeToDesktopGems` (lines ~45-69).**

**Replace with:**
```ts
// Listen for gem-detected events from Rust OCR (desktop-native, no pairing needed)
$effect(() => {
    let cancelled = false;
    const promise = listen<{ gems: string[]; variant: string }>('gem-detected', (event) => {
        if (cancelled) return;
        const { gems, variant: detectedVariant } = event.payload;
        if (VARIANTS.includes(detectedVariant) && detectedVariant !== variant) {
            variant = detectedVariant;
        }
        selectedGems = gems.slice(0, 3);
        loadResults();
        fetchTradeDataForAll();
    });

    return () => {
        cancelled = true;
        promise.then(unlisten => unlisten());
    };
});
```

- [ ] **Step 5: Remove desktop pairing UI from template**

**Remove the `activePairCode` badge block from the template (lines ~391-397):**
```svelte
{#if activePairCode}
    <span class="desktop-badge" ...>
        ...
    </span>
{/if}
```

**Remove the `desktopPair` from the `onDesktopDisconnect` callback in the section-header.**

- [ ] **Step 6: Remove desktop bridge CSS**

**Remove these CSS rules (they're orphaned now):**
- `.desktop-badge` and `.desktop-badge.desktop-connected`
- `.desktop-dot` and `.desktop-connected .desktop-dot`
- `.desktop-disconnect`

- [ ] **Step 7: Verify final Comparator**

Confirm: no references to `desktopPair`, `desktopBridge`, `getPairCode`, `clearPairCode`, `subscribeToDesktopGems`, `activePairCode`, `desktopConnected`, or `disconnectDesktop` remain.

Confirm: `listen` import from `@tauri-apps/api/event` is present and the gem-detected effect exists.

---

## Task 8: Dashboard Page + Dev Tools Move

**Files:**
- Modify: `desktop/src/routes/(app)/+page.svelte` (replace with dashboard)
- Create: `desktop/src/routes/(app)/dev/+page.svelte` (move existing dev tools)

- [ ] **Step 1: Move existing +page.svelte to dev/+page.svelte**

```bash
mkdir -p /var/www/ProfitOfExile/desktop/src/routes/\(app\)/dev
cp /var/www/ProfitOfExile/desktop/src/routes/\(app\)/+page.svelte \
   /var/www/ProfitOfExile/desktop/src/routes/\(app\)/dev/+page.svelte
```

The dev page keeps its existing content unchanged (scan controls, trade lookup test, OCR test, logs).

- [ ] **Step 2: Create new dashboard +page.svelte**

Replace `desktop/src/routes/(app)/+page.svelte` with the dashboard adapted from `frontend/src/routes/lab/+page.svelte`.

Key adaptations from the web version:

1. **Imports:** All component imports use `./components/X.svelte` (same relative path structure).

2. **Remove desktop bridge:** Remove `getPairCode`, `setPairCode` imports and the `desktopPair` state/URL-parsing logic.

3. **Remove `desktopPair` prop** from `<Comparator>` and `onDesktopDisconnect` callback.

4. **Adapt trade API imports:** Import from `$lib/tradeApi` (same path, desktop version).

5. **Remove `<svelte:head>`** block (desktop app doesn't use HTML head tags).

6. **Add scan controls** at the top of the dashboard — import `invoke` from `@tauri-apps/api/core` and add a small status/scan section that shows `store.status?.state` and Start/Stop Scanning buttons (from the existing dev tools page).

7. **Keep logs section** at the bottom (from existing dev page, using `store.logs`).

The full adapted script section:

```svelte
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { store } from '$lib/stores/status.svelte';
    import {
        fetchStatus,
        fetchBestPlays,
        fetchMarketOverview,
        connectMercure,
        type StatusData,
        type GemPlay,
        type MarketOverviewData,
        type MercureConnection,
    } from '$lib/api';
    import { lookupTrade, pollTradeResult, type TradeLookupResult } from '$lib/tradeApi';

    import Header from './components/Header.svelte';
    import Comparator from './components/Comparator.svelte';
    import SessionQueue from './components/SessionQueue.svelte';
    import type { QueueItem } from './components/SessionQueue.svelte';
    import BestPlays from './components/BestPlays.svelte';
    import ByVariant from './components/ByVariant.svelte';
    import MarketOverview from './components/MarketOverview.svelte';
    import Legend from './components/Legend.svelte';
    import FontEVCompare from './components/FontEVCompare.svelte';

    let selectedLab = $state('Merciless');
    let status = $state<StatusData | null>(null);
    let bestPlays = $state<GemPlay[]>([]);
    let marketOverview = $state<MarketOverviewData | null>(null);
    let loading = $state(true);
    let error = $state('');
    let mercure = $state<MercureConnection | null>(null);
    let isDedication = $derived(selectedLab === 'Dedication');
    let refreshKey = $state(0);

    // --- Session Queue state (identical to web) ---
    let sessionQueue = $state<QueueItem[]>([]);
    let autoClearMinutes = $state(2);
    let autoClearSecondsLeft = $state(0);
    let autoClearTimeout: ReturnType<typeof setTimeout> | null = null;
    let autoClearInterval: ReturnType<typeof setInterval> | null = null;

    // (Copy ALL session queue functions verbatim from web:
    //  resetAutoClearTimer, handleQueueGem, handleRefreshQueue,
    //  handleRemoveFromQueue, handleClearQueue, handleAutoClearChange)

    // ... [all session queue functions from web, verbatim] ...

    async function loadAll() {
        try {
            error = '';
            const [s, bp, mo] = await Promise.all([
                fetchStatus(),
                fetchBestPlays(undefined, undefined, undefined, 100),
                fetchMarketOverview(),
            ]);
            status = s;
            bestPlays = bp;
            marketOverview = mo;
            if (mercure) {
                status = { ...status, connected: mercure.connected };
            }
        } catch (e: any) {
            error = e?.message || 'Failed to load dashboard data';
        } finally {
            loading = false;
        }
    }

    function handleLabChange(lab: string) {
        selectedLab = lab;
    }

    $effect(() => {
        loadAll();
        mercure = connectMercure(() => {
            refreshKey++;
            loadAll();
        }, (connected) => {
            if (status) {
                status = { ...status, connected };
            }
        });
        return () => {
            mercure?.close();
        };
    });
</script>
```

The template section is identical to the web EXCEPT:
- No `<svelte:head>` block
- No `desktopPair` prop on Comparator, no `onDesktopDisconnect`
- Add scan controls section before the dashboard div
- Add logs section after the dashboard div

```svelte
<!-- Scan controls (desktop-specific) -->
<div class="scan-bar">
    <span class="scan-state">{store.status?.state || '...'}</span>
    {#if store.status?.state === 'PickingGems'}
        <button class="scan-btn scan-stop" onclick={() => invoke('stop_scanning').catch(console.error)}>Stop Scanning</button>
    {:else}
        <button class="scan-btn" onclick={() => invoke('start_scanning').catch(console.error)}>Start Scanning</button>
    {/if}
</div>

<div class="dashboard">
    <!-- ... identical to web template, minus desktopPair/onDesktopDisconnect ... -->
</div>

{#if store.logs.length > 0}
<div class="logs-section">
    <div class="logs-header">Logs</div>
    <div class="log-list">
        {#each store.logs.toReversed() as line}
            <div class="log-line" class:log-error={line.includes('failed') || line.includes('error')}>{line}</div>
        {/each}
    </div>
</div>
{/if}
```

The `<style>` section: copy ALL styles from the web `+page.svelte`, plus add styles for the scan bar and logs from the existing desktop page.

- [ ] **Step 3: Verify page structure**

Confirm the dashboard page:
1. Has all web component imports
2. Has `invoke` import for scan controls
3. Has `store` import for logs and scan state
4. Has NO references to `desktopPair`, `desktopBridge`, `setPairCode`, `getPairCode`
5. Has all session queue functions
6. Has Mercure SSE connection
7. Has scan controls section
8. Has logs section

---

## Task 9: Sidebar + README Updates

**Files:**
- Modify: `desktop/src/lib/components/Sidebar.svelte`
- Modify: `desktop/src/lib/README.md`

- [ ] **Step 1: Add Dev Tools link to Sidebar**

In `Sidebar.svelte`, in the "Tools" section, add after the existing disabled items:

```svelte
<a href="/dev" class="nav-item" class:active={currentPath === '/dev'}>
    <span class="icon">&#x1F6E0;&#xFE0F;</span>
    <span>Dev Tools</span>
</a>
```

- [ ] **Step 2: Update README**

Add to the Routes table in `desktop/src/lib/README.md`:

```markdown
| `(app)/dev/+page.svelte` | Dev tools — scan controls, trade lookup test, OCR test, logs. |
```

Add to the Components table:

```markdown
| `components/Select.svelte` | `value` (bindable), `options`, `onchange` | Custom dropdown select with dark theme styling and chevron indicator. |
```

Add a new section:

```markdown
## Dashboard Components (Lab)

Located in `routes/(app)/components/`. These are the lab farming dashboard components migrated from the web frontend.

| File | Description |
|------|-------------|
| `Header.svelte` | Dashboard header — lab selector, divine rate, update timer, connection status |
| `Comparator.svelte` | Gem comparator — search, compare up to 3 gems, trade data, session queue integration |
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
```

---

## Execution Notes

- **Tasks 1-3** must complete first (CSS vars, utilities, API layer).
- **Tasks 4-6** depend on Tasks 1-3 and can run in parallel with each other.
- **Task 7** (Comparator) depends on Tasks 1-3 and can run in parallel with Tasks 4-6.
- **Task 8** (dashboard page) depends on ALL previous tasks.
- **Task 9** (sidebar/README) depends on Task 8.

Optimal parallelism: Run Tasks 1+2+3 sequentially (fast, ~3 minutes total). Then run Tasks 4+5+6+7 in parallel. Then Task 8. Then Task 9.
