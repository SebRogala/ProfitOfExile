# Desktop App Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the desktop app from a single-page debug panel into a proper app with top bar, collapsible sidebar, route-based navigation, settings page, and overlay management.

**Architecture:** SvelteKit route groups separate the app shell `(app)/` from overlay routes `overlay/`. Root layout stays minimal (ssr/prerender config). The `(app)` group gets a layout with TopBar + Sidebar + content slot. Existing functionality moves from the monolithic `+page.svelte` into `(app)/lab/+page.svelte`. Settings get their own route.

**Tech Stack:** SvelteKit 5 (runes), Tauri v2 commands, CSS custom properties (existing theme)

**Spec:** `docs/superpowers/specs/2026-03-28-desktop-app-shell-design.md`

---

### Task 1: Route restructure — create (app) group

Move existing routes into a SvelteKit route group so the app shell layout doesn't affect overlay windows.

**Files:**
- Create: `desktop/src/routes/(app)/+layout.svelte`
- Move: `desktop/src/routes/+page.svelte` → `desktop/src/routes/(app)/lab/+page.svelte`
- Create: `desktop/src/routes/(app)/+page.svelte` (redirect to /lab)
- Keep: `desktop/src/routes/+layout.ts` (root, unchanged)
- Keep: `desktop/src/routes/overlay/` (unchanged, outside app group)

- [ ] **Step 1: Create the (app) group directory structure**

```bash
mkdir -p desktop/src/routes/\(app\)/lab
mkdir -p desktop/src/routes/\(app\)/settings
```

- [ ] **Step 2: Create minimal (app) layout**

Create `desktop/src/routes/(app)/+layout.svelte`:

```svelte
<script lang="ts">
	let { children } = $props();
</script>

<div class="app-shell">
	{@render children()}
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}
</style>
```

- [ ] **Step 3: Move +page.svelte to lab route**

Copy `desktop/src/routes/+page.svelte` to `desktop/src/routes/(app)/lab/+page.svelte`. Keep it unchanged for now — we'll extract the shell in later tasks.

- [ ] **Step 4: Create redirect at (app) root**

Create `desktop/src/routes/(app)/+page.svelte`:

```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	onMount(() => goto('/lab', { replaceState: true }));
</script>
```

- [ ] **Step 5: Delete old root +page.svelte**

Remove `desktop/src/routes/+page.svelte` (now lives at `(app)/lab/+page.svelte`).

- [ ] **Step 6: Verify overlay routes still work**

Overlay routes at `/overlay` are outside the `(app)` group — they should be unaffected. Verify the existing overlay layout and page are still at `desktop/src/routes/overlay/`.

- [ ] **Step 7: Sync and build**

```bash
make desktop-sync
```

Build with `cargo tauri dev` on Windows. Verify:
- `/` redirects to `/lab`
- `/lab` shows the existing app content
- `/overlay` still works for capture region

- [ ] **Step 8: Commit**

```bash
git add desktop/src/routes/
git commit -m "refactor(desktop): route restructure — (app) group for shell layout"
```

---

### Task 2: Top bar component

Extract a TopBar with logo, status indicators, debug toggle, and settings link.

**Files:**
- Create: `desktop/src/lib/components/TopBar.svelte`
- Modify: `desktop/src/routes/(app)/+layout.svelte`
- Modify: `desktop/src/routes/(app)/lab/+page.svelte` (remove title row)

- [ ] **Step 1: Copy the golden orb icon to static assets**

```bash
cp desktop/src-tauri/icons/32x32.png desktop/static/icon-32.png
```

- [ ] **Step 2: Create TopBar component**

Create `desktop/src/lib/components/TopBar.svelte`:

```svelte
<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';

	let {
		status = null,
		pairCode = '....',
		onToggleSidebar,
	}: {
		status: any;
		pairCode: string;
		onToggleSidebar: () => void;
	} = $props();

	const PROD_URL = 'https://SERVER_URL';
	const LOCAL_URL = 'https://profitofexile.localhost';

	function isDebug(): boolean {
		return (status?.server_url || PROD_URL) === LOCAL_URL;
	}

	async function toggleDebug() {
		try {
			const newUrl = isDebug() ? PROD_URL : LOCAL_URL;
			await invoke('set_server_url', { url: newUrl });
		} catch (e) {
			console.error('Toggle debug failed:', e);
		}
	}
</script>

<header class="top-bar">
	<div class="top-bar-left">
		<button class="sidebar-toggle" onclick={onToggleSidebar}>☰</button>
		<img src="/icon-32.png" alt="ProfitOfExile" class="logo" />
		<span class="app-name">ProfitOfExile</span>
	</div>

	<div class="top-bar-right">
		<div class="status-dot" class:connected={status} title={status ? 'Connected' : 'Disconnected'}></div>
		{#if status?.state === 'PickingGems'}
			<div class="status-dot scanning" title="Scanning"></div>
		{/if}
		<span class="pair-code" title="Pair code">{pairCode}</span>
		<button class="btn-debug" class:active={isDebug()} onclick={toggleDebug}>
			{isDebug() ? 'DEBUG' : 'PROD'}
		</button>
		<a href="/settings" class="settings-link" title="Settings">⚙️</a>
	</div>
</header>

<style>
	.top-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 12px;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		height: 40px;
	}

	.top-bar-left {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.top-bar-right {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.sidebar-toggle {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 16px;
		cursor: pointer;
		padding: 2px 4px;
	}

	.sidebar-toggle:hover {
		color: var(--text);
	}

	.logo {
		width: 22px;
		height: 22px;
	}

	.app-name {
		font-weight: 600;
		font-size: 13px;
		color: var(--text);
	}

	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
	}

	.status-dot.connected {
		background: var(--success);
	}

	.status-dot.scanning {
		background: var(--success);
		animation: pulse 1s infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.4; }
	}

	.pair-code {
		font-size: 10px;
		color: var(--text-muted);
		font-family: monospace;
		letter-spacing: 0.1em;
	}

	.btn-debug {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 1px 6px;
		border-radius: 3px;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.05em;
		cursor: pointer;
	}

	.btn-debug.active {
		border-color: var(--warning);
		color: var(--warning);
	}

	.settings-link {
		font-size: 14px;
		text-decoration: none;
		cursor: pointer;
		opacity: 0.6;
	}

	.settings-link:hover {
		opacity: 1;
	}
</style>
```

- [ ] **Step 3: Wire TopBar into (app) layout**

Update `desktop/src/routes/(app)/+layout.svelte`:

```svelte
<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/TopBar.svelte';
	import '../app.css';

	let { children } = $props();

	let status = $state<any>(null);
	let pairCode = $state('....');
	let sidebarOpen = $state(true);

	// Poll status
	setInterval(() => {
		invoke('get_status').then((s) => { status = s; }).catch(() => {});
		invoke('get_pair_code').then((c) => { pairCode = c as string; }).catch(() => {});
	}, 1000);
</script>

<div class="app-shell">
	<TopBar {status} {pairCode} onToggleSidebar={() => sidebarOpen = !sidebarOpen} />
	<div class="app-body">
		{@render children()}
	</div>
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.app-body {
		display: flex;
		flex: 1;
		overflow: hidden;
	}
</style>
```

- [ ] **Step 4: Remove title row and debug toggle from lab page**

In `desktop/src/routes/(app)/lab/+page.svelte`:
- Remove the `PROD_URL`, `LOCAL_URL` constants
- Remove `isDebug()`, `toggleDebug()` functions
- Remove the `.title-row` HTML block (logo, subtitle, debug button)
- Remove the pair section (moved to top bar)
- Remove the `import '../app.css'` (now in layout)
- Remove associated CSS (`.title-row`, `.btn-debug`, `.subtitle`, `.code`, `.pair` styles)

Keep everything else (status, scan controls, capture region, trade lookup, test pipeline, OCR, logs).

- [ ] **Step 5: Sync, build, verify**

Top bar should show logo, status dots, pair code, debug toggle, settings link. Content area shows the lab page below.

- [ ] **Step 6: Commit**

```bash
git add desktop/src/lib/components/TopBar.svelte desktop/src/routes/ desktop/static/icon-32.png
git commit -m "feat(desktop): top bar component with logo, status indicators, settings link"
```

---

### Task 3: Sidebar component

Collapsible sidebar with strategies, tools, and overlay quick-toggles.

**Files:**
- Create: `desktop/src/lib/components/Sidebar.svelte`
- Modify: `desktop/src/routes/(app)/+layout.svelte` (add sidebar)

- [ ] **Step 1: Create Sidebar component**

Create `desktop/src/lib/components/Sidebar.svelte`:

```svelte
<script lang="ts">
	let {
		open = true,
		currentPath = '/lab',
	}: {
		open: boolean;
		currentPath: string;
	} = $props();
</script>

{#if open}
<nav class="sidebar">
	<div class="sidebar-content">
		<div class="sidebar-section">
			<div class="section-label">Strategies</div>
			<a href="/lab" class="nav-item" class:active={currentPath === '/lab'}>
				<span class="nav-icon">⚗️</span>
				<span class="nav-text">Lab Farming</span>
			</a>
			<div class="nav-item disabled">
				<span class="nav-icon">🗺️</span>
				<span class="nav-text">Mapping</span>
				<span class="nav-badge">soon</span>
			</div>
			<div class="nav-item disabled">
				<span class="nav-icon">👹</span>
				<span class="nav-text">Bosses</span>
				<span class="nav-badge">soon</span>
			</div>
		</div>

		<div class="sidebar-section">
			<div class="section-label">Tools</div>
			<div class="nav-item disabled">
				<span class="nav-icon">🔍</span>
				<span class="nav-text">Trade Lookup</span>
			</div>
			<div class="nav-item disabled">
				<span class="nav-icon">📊</span>
				<span class="nav-text">Price Compare</span>
			</div>
		</div>
	</div>

	<div class="sidebar-footer">
		<div class="sidebar-section">
			<div class="section-label">Overlays</div>
			<div class="overlay-row">
				<span class="overlay-name">🧭 Compass</span>
				<span class="overlay-mode off">off</span>
			</div>
			<div class="overlay-row">
				<span class="overlay-name">👁 OCR</span>
				<span class="overlay-mode off">off</span>
			</div>
			<div class="overlay-row">
				<span class="overlay-name">⚖️ Compare</span>
				<span class="overlay-mode off">off</span>
			</div>
			<div class="overlay-row">
				<span class="overlay-name">📋 Session</span>
				<span class="overlay-mode off">off</span>
			</div>
		</div>
	</div>
</nav>
{/if}

<style>
	.sidebar {
		width: 180px;
		background: var(--surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		justify-content: space-between;
		flex-shrink: 0;
		overflow-y: auto;
	}

	.sidebar-content {
		padding: 12px;
	}

	.sidebar-footer {
		padding: 12px;
		border-top: 1px solid var(--border);
	}

	.sidebar-section {
		margin-bottom: 16px;
	}

	.sidebar-footer .sidebar-section {
		margin-bottom: 0;
	}

	.section-label {
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 1px;
		color: var(--text-muted);
		margin-bottom: 6px;
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border-radius: 5px;
		font-size: 12px;
		color: var(--text-muted);
		text-decoration: none;
		cursor: pointer;
		margin-bottom: 2px;
	}

	.nav-item:hover:not(.disabled) {
		background: var(--border);
		color: var(--text);
	}

	.nav-item.active {
		background: var(--border);
		color: var(--accent);
	}

	.nav-item.disabled {
		color: var(--border);
		cursor: default;
	}

	.nav-icon {
		font-size: 14px;
		width: 20px;
		text-align: center;
	}

	.nav-badge {
		font-size: 9px;
		color: var(--border);
		margin-left: auto;
	}

	.overlay-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 3px 8px;
		background: var(--bg);
		border-radius: 4px;
		margin-bottom: 3px;
	}

	.overlay-name {
		font-size: 10px;
		color: var(--text-muted);
	}

	.overlay-mode {
		font-size: 9px;
		cursor: pointer;
	}

	.overlay-mode.off {
		color: var(--text-muted);
	}

	.overlay-mode.always {
		color: var(--success);
	}

	.overlay-mode.auto {
		color: var(--warning);
	}
</style>
```

- [ ] **Step 2: Wire Sidebar into (app) layout**

Update `desktop/src/routes/(app)/+layout.svelte` to add the sidebar:

```svelte
<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { page } from '$app/stores';
	import TopBar from '$lib/components/TopBar.svelte';
	import Sidebar from '$lib/components/Sidebar.svelte';
	import '../app.css';

	let { children } = $props();

	let status = $state<any>(null);
	let pairCode = $state('....');
	let sidebarOpen = $state(true);

	setInterval(() => {
		invoke('get_status').then((s) => { status = s; }).catch(() => {});
		invoke('get_pair_code').then((c) => { pairCode = c as string; }).catch(() => {});
	}, 1000);
</script>

<div class="app-shell">
	<TopBar {status} {pairCode} onToggleSidebar={() => sidebarOpen = !sidebarOpen} />
	<div class="app-body">
		<Sidebar open={sidebarOpen} currentPath={$page.url.pathname} />
		<main class="content">
			{@render children()}
		</main>
	</div>
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.app-body {
		display: flex;
		flex: 1;
		overflow: hidden;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 16px;
	}
</style>
```

- [ ] **Step 3: Sync, build, verify**

Sidebar should show strategy links (Lab active), tools (greyed out), overlay toggles (all off for now). Hamburger icon in top bar toggles sidebar.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/lib/components/Sidebar.svelte desktop/src/routes/
git commit -m "feat(desktop): collapsible sidebar with strategies, tools, overlay toggles"
```

---

### Task 4: Clean up lab page

Remove shell elements from the lab page (now handled by layout), remove test sections, keep functional content.

**Files:**
- Modify: `desktop/src/routes/(app)/lab/+page.svelte`

- [ ] **Step 1: Remove from lab page**

Items to remove (now in layout/TopBar):
- `import '../app.css'` (moved to layout)
- `PROD_URL`, `LOCAL_URL` constants
- `isDebug()`, `toggleDebug()` functions
- `pairCode` state and polling (moved to layout)
- The pair section HTML
- The title row HTML (`.title-row` with logo, subtitle, debug button)
- All CSS for removed elements (`.title-row`, `.btn-debug`, `.subtitle`, `.code`, `.pair`, `a` link styles)

Items to keep:
- `status` polling (needed for scan state, gem region display — layout also polls but lab page needs its own reference for inline use)
- `logs` polling
- Scan controls (Start/Stop)
- Capture region section
- Trade lookup section
- Test Pipeline, Test OCR sections (keep for now)
- Logs section

Note: Both the layout and lab page poll `get_status`. This is intentional — the layout needs it for TopBar (connection/scan indicators), the lab page needs it for scan controls and region display. The 1s interval is lightweight (just reading Mutex state, no DB/network).

- [ ] **Step 2: Remove the `max-width: 400px` and `margin: 0 auto` from `main` CSS**

The content area now fills the space next to the sidebar. Remove the narrow column constraint:

```css
/* Remove: */
main { max-width: 400px; margin: 0 auto; }

/* Replace with: */
/* (no wrapper needed — layout handles it) */
```

Rename the `<main>` tag to a `<div>` since the layout already provides `<main class="content">`.

- [ ] **Step 3: Sync, build, verify**

Lab page should show within the app shell — top bar, sidebar, content fills remaining space. No duplicate title/pair code.

- [ ] **Step 4: Commit**

```bash
git add desktop/src/routes/
git commit -m "refactor(desktop): clean lab page — remove shell elements handled by layout"
```

---

### Task 5: Settings page

Dedicated settings route with grouped config sections, wired to existing Tauri commands.

**Files:**
- Create: `desktop/src/routes/(app)/settings/+page.svelte`

- [ ] **Step 1: Create settings page**

Create `desktop/src/routes/(app)/settings/+page.svelte`:

```svelte
<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';

	let status = $state<any>(null);
	let logs = $state<string[]>([]);

	// Load current state
	invoke('get_status').then((s) => { status = s; }).catch(() => {});

	// --- General ---
	let editingServerUrl = $state(false);
	let serverUrlInput = $state('');

	function startEditServerUrl() {
		serverUrlInput = status?.server_url || '';
		editingServerUrl = true;
	}

	async function saveServerUrl() {
		try {
			await invoke('set_server_url', { url: serverUrlInput });
			status = await invoke('get_status');
			editingServerUrl = false;
		} catch (e) {
			console.error('Failed to save server URL:', e);
		}
	}

	// --- Game Integration ---
	let editingPath = $state(false);
	let pathInput = $state('');

	function startEditPath() {
		pathInput = status?.client_txt_path || '';
		editingPath = true;
	}

	async function savePath() {
		try {
			await invoke('set_client_txt_path', { path: pathInput });
			status = await invoke('get_status');
			editingPath = false;
		} catch (e) {
			console.error('Failed to save path:', e);
		}
	}

	async function regeneratePairCode() {
		try {
			await invoke('regenerate_pair_code');
			status = await invoke('get_status');
		} catch (e) {
			console.error('Failed to regenerate pair code:', e);
		}
	}

	// --- Capture region overlay ---
	let overlayWin: any = null;
	let overlayVisible = $state(false);

	async function showRegionOverlay() {
		try {
			const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
			if (overlayWin) {
				try { await overlayWin.destroy(); } catch (e) {
					console.error('Failed to destroy existing overlay:', e);
				}
				overlayWin = null;
			}

			const region = status?.gem_region;
			const win = new WebviewWindow('overlay', {
				url: '/overlay',
				transparent: true,
				decorations: false,
				alwaysOnTop: true,
				resizable: true,
				shadow: false,
				skipTaskbar: true,
				width: region?.w || 550,
				height: region?.h || 75,
				x: region?.x || 30,
				y: region?.y || 45,
			});

			win.once('tauri://created', () => {
				overlayWin = win;
				overlayVisible = true;
			});
			win.once('tauri://error', (e: any) => {
				console.error('Overlay creation failed:', e);
			});
		} catch (e) {
			console.error('Failed to create overlay:', e);
		}
	}

	async function saveRegion() {
		if (!overlayWin) return;
		try {
			const w = overlayWin.window ?? overlayWin;
			const pos = await w.outerPosition();
			const size = await w.outerSize();
			await invoke('set_gem_region', { x: pos.x, y: pos.y, w: size.width, h: size.height });
		} catch (e) {
			console.error('Failed to read/save region:', e);
			return;
		}
		try { await overlayWin.destroy(); } catch (e) {
			console.error('Overlay destroy failed:', e);
		}
		overlayWin = null;
		overlayVisible = false;
		status = await invoke('get_status');
	}

	async function cancelRegion() {
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) {
				console.error('Overlay destroy failed:', e);
			}
			overlayWin = null;
		}
		overlayVisible = false;
	}
</script>

<h1>Settings</h1>

<section>
	<h2>General</h2>

	<div class="setting-row">
		<label>Server URL</label>
		{#if editingServerUrl}
			<input type="text" bind:value={serverUrlInput} class="setting-input" />
			<div class="setting-actions">
				<button class="btn-small" onclick={saveServerUrl}>Save</button>
				<button class="btn-small" onclick={() => editingServerUrl = false}>Cancel</button>
			</div>
		{:else}
			<span class="setting-value">{status?.server_url || '...'}</span>
			<button class="btn-small" onclick={startEditServerUrl}>Edit</button>
		{/if}
	</div>

	<div class="setting-row">
		<label>League</label>
		<span class="setting-value">Mirage</span>
	</div>

	<div class="setting-row">
		<label>Pair Code</label>
		<span class="setting-value mono">{status?.pair_code || '....'}</span>
		<button class="btn-small" onclick={regeneratePairCode}>Regenerate</button>
	</div>
</section>

<section>
	<h2>Game Integration</h2>

	<div class="setting-row">
		<label>Client.txt Path</label>
		{#if editingPath}
			<input type="text" bind:value={pathInput} class="setting-input" />
			<div class="setting-actions">
				<button class="btn-small" onclick={savePath}>Save</button>
				<button class="btn-small" onclick={() => editingPath = false}>Cancel</button>
			</div>
		{:else}
			<span class="setting-value small">{status?.client_txt_path || '...'}</span>
			<button class="btn-small" onclick={startEditPath}>Edit</button>
		{/if}
	</div>

	<div class="setting-row">
		<label>Gem Tooltip Region</label>
		<span class="setting-value">
			({status?.gem_region?.x}, {status?.gem_region?.y}) {status?.gem_region?.w}×{status?.gem_region?.h}
		</span>
		{#if overlayVisible}
			<div class="setting-actions">
				<button class="btn-small" onclick={saveRegion}>Save</button>
				<button class="btn-small" onclick={cancelRegion}>Cancel</button>
			</div>
		{:else}
			<button class="btn-small" onclick={showRegionOverlay}>Configure</button>
		{/if}
	</div>
</section>

<section>
	<h2>Trade</h2>

	<div class="setting-row">
		<label>Auto-trigger lookup</label>
		<span class="setting-value">Coming soon</span>
	</div>

	<div class="setting-row">
		<label>Cache min-age</label>
		<span class="setting-value">Coming soon</span>
	</div>
</section>

<section>
	<h2>Overlays</h2>

	<div class="setting-row">
		<label>Lock / Unlock all</label>
		<span class="setting-value">Coming soon</span>
	</div>
</section>

<style>
	h1 {
		font-size: 1.2rem;
		color: var(--accent);
		margin-bottom: 1.5rem;
	}

	section {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 1rem;
		margin-bottom: 1rem;
	}

	h2 {
		font-size: 0.8rem;
		text-transform: uppercase;
		color: var(--text-muted);
		margin-bottom: 0.75rem;
		letter-spacing: 0.5px;
	}

	.setting-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 0;
		border-bottom: 1px solid var(--border);
		flex-wrap: wrap;
	}

	.setting-row:last-child {
		border-bottom: none;
	}

	label {
		font-size: 0.8rem;
		color: var(--text);
		min-width: 140px;
		flex-shrink: 0;
	}

	.setting-value {
		font-size: 0.8rem;
		color: var(--text-muted);
		flex: 1;
		word-break: break-all;
	}

	.setting-value.mono {
		font-family: monospace;
		letter-spacing: 0.15em;
		font-size: 0.9rem;
	}

	.setting-value.small {
		font-size: 0.7rem;
	}

	.setting-input {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		padding: 4px 8px;
		border-radius: 4px;
		font-size: 0.8rem;
	}

	.setting-actions {
		display: flex;
		gap: 4px;
	}

	.btn-small {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 2px 8px;
		border-radius: 4px;
		font-size: 0.7rem;
		cursor: pointer;
	}

	.btn-small:hover {
		border-color: var(--accent);
		color: var(--text);
	}
</style>
```

- [ ] **Step 2: Sync, build, verify**

Navigate to `/settings` via the gear icon. Verify all sections render, server URL edit works, pair code regenerate works, capture region overlay launches from settings.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/routes/\(app\)/settings/
git commit -m "feat(desktop): settings page — general, game integration, trade, overlays"
```

---

### Task 6: Update window size and branding

Change default window size to 1024x768 for the full dashboard experience.

**Files:**
- Modify: `desktop/src-tauri/tauri.conf.json`

- [ ] **Step 1: Update tauri.conf.json**

Change window dimensions:

```json
{
  "title": "ProfitOfExile",
  "width": 1024,
  "height": 768,
  "resizable": true,
  "fullscreen": false
}
```

- [ ] **Step 2: Sync, build, verify**

App should open at 1024x768. Top bar, sidebar, and content area should fill the space properly.

- [ ] **Step 3: Commit**

```bash
git add desktop/src-tauri/tauri.conf.json
git commit -m "feat(desktop): update default window size to 1024x768"
```

---

### Task 7: Remove settings/config from lab page

Move remaining config elements (capture region, pair URL) out of the lab page into settings. The lab page should only contain strategy-relevant content.

**Files:**
- Modify: `desktop/src/routes/(app)/lab/+page.svelte`

- [ ] **Step 1: Remove from lab page**

Remove these sections (now in settings):
- Capture Region section (overlay management)
- All overlay-related state and functions (`overlayWin`, `overlayVisible`, `showOverlay`, `saveRegion`, `cancelOverlay`)
- Path editing (`editingPath`, `pathInput`, `startEditPath`, `savePath`)
- Pair URL display and regenerate
- Associated CSS

Keep:
- Status display + scan controls (Start/Stop Scanning)
- Trade lookup test section
- Test Pipeline and Test OCR (useful for debugging)
- Logs section

- [ ] **Step 2: Sync, build, verify**

Lab page should be cleaner — just status/scan controls, trade lookup, test tools, and logs. All config is in settings.

- [ ] **Step 3: Commit**

```bash
git add desktop/src/routes/
git commit -m "refactor(desktop): move config elements from lab page to settings"
```

---

### Summary

| Task | What it does | Key files |
|------|-------------|-----------|
| 1 | Route restructure — (app) group | routes/(app)/, routes/(app)/lab/ |
| 2 | Top bar component | lib/components/TopBar.svelte |
| 3 | Collapsible sidebar | lib/components/Sidebar.svelte |
| 4 | Clean lab page | routes/(app)/lab/+page.svelte |
| 5 | Settings page | routes/(app)/settings/+page.svelte |
| 6 | Window size + branding | tauri.conf.json |
| 7 | Move config to settings | lab/+page.svelte cleanup |

After all 7 tasks, the app has:
- Proper shell with top bar (logo, status, debug, settings link)
- Collapsible sidebar (strategies, tools, overlay toggles)
- Lab page with only strategy-relevant content
- Settings page with all configuration
- 1024x768 default window
- Overlay routes untouched (outside app group)
