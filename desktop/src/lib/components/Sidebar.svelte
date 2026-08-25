<script lang="ts">
	import { nav } from '$lib/stores/navigation.svelte';
	import { ssot, setModuleEnabled } from '$lib/stores/ssot.svelte';
	import {
		hasFeature,
		MERC_FEATURE,
		EXCHANGE_FEATURE,
		TEMPLE_FEATURE
	} from '$lib/stores/entitlements.svelte';

	// --- Modules (SSOT) ---
	// User-facing names for the Rust module registry (src-tauri/src/modules.rs).
	// Adding a module means adding its id here too — an unlisted module has a
	// flag and a background task but no way for the user to reach it. The flags
	// themselves live in `ssot.modules`; this map only supplies labels.
	const MODULE_LABELS: Record<string, string> = {
		// One switch, three things since POE-199: the capture loop, the page's
		// capture + verdict, AND the verdict overlay window. The Mercenaries
		// PAGE is a nav item and stays reachable with this off — it just has no
		// live capture to judge.
		mercenary: '👁️ Merc OCR',
		// One switch, two things: the temple capture loop AND the temple overlay
		// window. They are deliberately not separable — an overlay with no reader
		// behind it can only show a board that is no longer on screen. The Temple
		// PAGE is a nav item and stays reachable with this off.
		temple: '🏛️ Temple reader',
	};

	/**
	 * The feature a module's controls are hidden behind, when it has one
	 * (POE-203). A module absent from this map is visible to every device; the
	 * two listed here are visible only where the server granted the named
	 * feature. The Currency Exchange has no module — it runs on the server — so
	 * it is gated at its nav entry only.
	 */
	const MODULE_FEATURES: Record<string, string> = {
		mercenary: MERC_FEATURE,
		temple: TEMPLE_FEATURE,
	};

	/** Whether this device may see the Mercenaries page and the Merc OCR switch. */
	const mercGranted = $derived(hasFeature(MERC_FEATURE));

	/** Whether this device may see the Temple page and the Temple reader switch. */
	const templeGranted = $derived(hasFeature(TEMPLE_FEATURE));

	/** Whether this device may see the Currency Exchange page. */
	const exchangeGranted = $derived(hasFeature(EXCHANGE_FEATURE));

	/**
	 * The module switches this device may see, in registry order. Derived rather
	 * than filtered inline so the collapsed rail and the expanded panel cannot
	 * disagree about which switches exist.
	 */
	const moduleEntries = $derived(
		Object.entries(MODULE_LABELS).filter(
			([id]) => !MODULE_FEATURES[id] || hasFeature(MODULE_FEATURES[id])
		)
	);

	/** The longer tooltip for a module whose one-line label cannot say it all. */
	const MODULE_TITLES: Record<string, string> = {
		mercenary: 'Merc OCR: watches for a recruit window AND spawns the verdict overlay — one switch for the loop, the page data and the strip over the game. The Mercenaries page stays browsable either way.',
		temple: 'Temple reader: reads the layout panel AND spawns the temple overlay — one switch for both. The Temple page stays browsable either way.',
	};

	/** What a module toggle's `title` says: the specific note, or the default. */
	function moduleTitle(id: string, label: string, on: boolean): string {
		return MODULE_TITLES[id] ?? `${label.replace(/^\S+\s/, '')}: ${on ? 'on' : 'off'}`;
	}

	let {
		open = true,
		currentPath = '/',
		onToggle,
		comparatorActive = false,
		gameFocused = false,
		onToggleComparator,
		compassActive = false,
		onToggleCompass = (() => {}) as () => void,
		pathstripActive = false,
		pathstripHasData = false,
		onTogglePathstrip = (() => {}) as () => void,
		timerActive = false,
		onToggleTimer = (() => {}) as () => void,
		labOverlaysActive = true,
		onToggleLabOverlays = (() => {}) as () => void,
	}: {
		open: boolean;
		currentPath: string;
		onToggle: () => void;
		comparatorActive?: boolean;
		gameFocused?: boolean;
		onToggleComparator?: () => void;
		compassActive?: boolean;
		onToggleCompass?: () => void;
		pathstripActive?: boolean;
		pathstripHasData?: boolean;
		onTogglePathstrip?: () => void;
		timerActive?: boolean;
		onToggleTimer?: () => void;
		labOverlaysActive?: boolean;
		onToggleLabOverlays?: () => void;
	} = $props();
</script>

{#if !open}
<nav class="sidebar-collapsed">
	<div class="collapsed-icons">
		<button class="collapsed-item" class:active={currentPath === '/'} title="Lab Farming" onclick={() => nav.go('/')}>
			<img src="/lab-icon.png" alt="Lab" class="lab-icon" />
		</button>
		<!-- Outside the DEV block on purpose: these three ship — to the devices
		     the server granted `merc` / `temple` / `exchange` (POE-203). -->
		{#if mercGranted}
			<button class="collapsed-item" class:active={currentPath === '/mercenaries'} title="Mercenaries" onclick={() => nav.go('/mercenaries')}>
				<span class="icon">&#x2694;&#xFE0F;</span>
			</button>
		{/if}
		{#if templeGranted}
			<button class="collapsed-item" class:active={currentPath === '/temple'} title="Temple of Atzoatl" onclick={() => nav.go('/temple')}>
				<span class="icon">&#x1F3DB;&#xFE0F;</span>
			</button>
		{/if}
		{#if exchangeGranted}
			<button class="collapsed-item" class:active={currentPath === '/currency-exchange'} title="Currency Exchange" onclick={() => nav.go('/currency-exchange')}>
				<span class="icon">&#x1F4B1;</span>
			</button>
		{/if}
		{#if import.meta.env.DEV}
			<div class="collapsed-item disabled" title="Mapping (soon)">
				<span class="icon">&#x1F5FA;&#xFE0F;</span>
			</div>
			<div class="collapsed-item disabled" title="Bosses (soon)">
				<span class="icon">&#x1F479;</span>
			</div>
			<div class="collapsed-sep"></div>
			<div class="collapsed-item disabled" title="Trade Lookup">
				<span class="icon">&#x1F50D;</span>
			</div>
			<div class="collapsed-item disabled" title="Price Compare">
				<span class="icon">&#x1F4CA;</span>
			</div>
			<button class="collapsed-item" class:active={currentPath === '/dev'} title="Dev Tools" onclick={() => nav.go('/dev')}>
				<span class="icon">&#x1F6E0;&#xFE0F;</span>
			</button>
		{/if}
		<div class="collapsed-sep"></div>
		<button class="collapsed-item" class:active={currentPath === '/settings'} title="Settings" onclick={() => nav.go('/settings')}>
			<span class="icon">&#x2699;&#xFE0F;</span>
		</button>
	</div>
	<div class="collapsed-overlays">
		<button class="collapsed-overlay group-btn" title="Lab Overlays: {labOverlaysActive ? 'on' : 'off'}" onclick={onToggleLabOverlays}>
			<span class="icon" style="font-size: 10px; font-weight: 700;">LAB</span>
			<span class="indicator" class:off={!labOverlaysActive} class:always={labOverlaysActive}></span>
		</button>
		<button class="collapsed-overlay" title="Compare: {comparatorActive ? (gameFocused ? 'on' : 'hidden') : 'off'}" onclick={onToggleComparator}>
			<span class="icon">&#x2696;&#xFE0F;</span>
			<span class="indicator" class:off={!comparatorActive} class:always={comparatorActive && gameFocused} class:auto={comparatorActive && !gameFocused}></span>
		</button>
		<button class="collapsed-overlay" title="Compass: {compassActive ? (gameFocused ? 'on' : 'hidden') : 'off'}" onclick={onToggleCompass}>
			<span class="icon">&#x1F9ED;</span>
			<span class="indicator" class:off={!compassActive} class:always={compassActive && gameFocused} class:auto={compassActive && !gameFocused}></span>
		</button>
		<button class="collapsed-overlay" title="Lab Map: {pathstripActive ? (pathstripHasData ? (gameFocused ? 'on' : 'hidden') : 'no data') : 'off'}" onclick={onTogglePathstrip}>
			<span class="icon">&#x1F5FA;&#xFE0F;</span>
			<span class="indicator" class:off={!pathstripActive} class:always={pathstripActive && pathstripHasData && gameFocused} class:auto={pathstripActive && pathstripHasData && !gameFocused} class:nodata={pathstripActive && !pathstripHasData}></span>
		</button>
		<button class="collapsed-overlay" title="Timer: {timerActive ? (gameFocused ? 'on' : 'hidden') : 'off'}" onclick={onToggleTimer}>
			<span class="icon">&#x23F1;&#xFE0F;</span>
			<span class="indicator" class:off={!timerActive} class:always={timerActive && gameFocused} class:auto={timerActive && !gameFocused}></span>
		</button>
		{#each moduleEntries as [id, label] (id)}
			{@const on = ssot.modules[id] ?? false}
			<button class="collapsed-overlay" title={moduleTitle(id, label, on)} onclick={() => setModuleEnabled(id, !on)}>
				<span class="icon">{label.split(' ')[0]}</span>
				<span class="indicator" class:off={!on} class:always={on}></span>
			</button>
		{/each}
	</div>
	<button class="collapse-btn collapsed-expand" onclick={onToggle} title="Expand sidebar">&#9654;</button>
</nav>
{:else}
<nav class="sidebar">
	<div class="top">
		<div class="section">
			<div class="label">Strategies</div>
			<button class="nav-item" class:active={currentPath === '/'} onclick={() => nav.go('/')}>
				<img src="/lab-icon.png" alt="Lab" class="lab-icon-expanded" />
				<span>Lab Farming</span>
			</button>
			{#if mercGranted}
				<button class="nav-item" class:active={currentPath === '/mercenaries'} onclick={() => nav.go('/mercenaries')}>
					<span class="icon">&#x2694;&#xFE0F;</span>
					<span>Mercenaries</span>
				</button>
			{/if}
			{#if templeGranted}
				<button class="nav-item" class:active={currentPath === '/temple'} onclick={() => nav.go('/temple')}>
					<span class="icon">&#x1F3DB;&#xFE0F;</span>
					<span>Temple</span>
				</button>
			{/if}
			{#if exchangeGranted}
				<button class="nav-item" class:active={currentPath === '/currency-exchange'} onclick={() => nav.go('/currency-exchange')}>
					<span class="icon">&#x1F4B1;</span>
					<span>Currency Exchange</span>
				</button>
			{/if}
			{#if import.meta.env.DEV}
				<div class="nav-item disabled">
					<span class="icon">&#x1F5FA;&#xFE0F;</span>
					<span>Mapping</span>
					<span class="badge">soon</span>
				</div>
				<div class="nav-item disabled">
					<span class="icon">&#x1F479;</span>
					<span>Bosses</span>
					<span class="badge">soon</span>
				</div>
			{/if}
		</div>

		{#if import.meta.env.DEV}
		<div class="section">
			<div class="label">Tools</div>
			<div class="nav-item disabled">
				<span class="icon">&#x1F50D;</span>
				<span>Trade Lookup</span>
			</div>
			<div class="nav-item disabled">
				<span class="icon">&#x1F4CA;</span>
				<span>Price Compare</span>
			</div>
			<button class="nav-item" class:active={currentPath === '/dev'} onclick={() => nav.go('/dev')}>
				<span class="icon">&#x1F6E0;&#xFE0F;</span>
				<span>Dev Tools</span>
			</button>
		</div>
		{/if}
	</div>

	<div class="bottom">
		<div class="label">Overlays</div>
		<button class="overlay-row clickable group-toggle" onclick={onToggleLabOverlays}>
			<span>Lab Overlays</span>
			<span class="mode" class:off={!labOverlaysActive} class:always={labOverlaysActive}>{labOverlaysActive ? 'on' : 'off'}</span>
		</button>
		<button class="overlay-row clickable indent" onclick={onToggleComparator}>
			<span>&#x2696;&#xFE0F; Compare</span>
			<span class="mode" class:off={!comparatorActive} class:always={comparatorActive && gameFocused} class:auto={comparatorActive && !gameFocused}>{comparatorActive ? (gameFocused ? 'on' : 'hidden') : 'off'}</span>
		</button>
		<button class="overlay-row clickable indent" onclick={onToggleCompass}>
			<span>&#x1F9ED; Compass</span>
			<span class="mode" class:off={!compassActive} class:always={compassActive && gameFocused} class:auto={compassActive && !gameFocused}>{compassActive ? (gameFocused ? 'on' : 'hidden') : 'off'}</span>
		</button>
		<button class="overlay-row clickable indent" onclick={onTogglePathstrip}>
			<span>&#x1F5FA;&#xFE0F; Map</span>
			<span class="mode" class:off={!pathstripActive} class:always={pathstripActive && pathstripHasData && gameFocused} class:auto={pathstripActive && pathstripHasData && !gameFocused} class:nodata={pathstripActive && !pathstripHasData}>{pathstripActive ? (pathstripHasData ? (gameFocused ? 'on' : 'hidden') : 'no data') : 'off'}</span>
		</button>
		<button class="overlay-row clickable indent" onclick={onToggleTimer}>
			<span>&#x23F1;&#xFE0F; Timer</span>
			<span class="mode" class:off={!timerActive} class:always={timerActive && gameFocused} class:auto={timerActive && !gameFocused}>{timerActive ? (gameFocused ? 'on' : 'hidden') : 'off'}</span>
		</button>
		<div class="label">Modules</div>
		{#each moduleEntries as [id, label] (id)}
			{@const on = ssot.modules[id] ?? false}
			<button class="overlay-row clickable" title={moduleTitle(id, label, on)} onclick={() => setModuleEnabled(id, !on)}>
				<span>{label}</span>
				<span class="mode" class:off={!on} class:always={on}>{on ? 'on' : 'off'}</span>
			</button>
		{/each}
	</div>
	<button class="collapse-btn collapse-inside" onclick={onToggle} title="Collapse sidebar">&#9664;</button>
</nav>
{/if}

<style>
	.sidebar {
		width: 180px;
		flex-shrink: 0;
		background: var(--surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		justify-content: space-between;
		overflow-y: auto;
	}

	.top {
		flex: 1;
		overflow-y: auto;
		padding: 10px 8px;
	}

	.section {
		margin-bottom: 16px;
	}

	.label {
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 1px;
		color: var(--text-muted);
		padding: 0 10px;
		margin-bottom: 4px;
	}

	.nav-item {
		all: unset;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		font-size: 12px;
		border-radius: 5px;
		color: var(--text);
		cursor: pointer;
		width: 100%;
		box-sizing: border-box;
	}

	.nav-item:hover:not(.disabled) {
		background: var(--border);
	}

	.nav-item.active {
		background: var(--border);
		color: var(--accent);
	}

	.nav-item.disabled {
		color: var(--border);
		cursor: default;
	}

	.badge {
		font-size: 9px;
		color: var(--text-muted);
		margin-left: auto;
		opacity: 0.6;
	}

	.icon {
		flex-shrink: 0;
		width: 16px;
		text-align: center;
	}

	.lab-icon {
		width: 22px;
		height: 18px;
		object-fit: contain;
	}

	.lab-icon-expanded {
		width: 18px;
		height: 14px;
		object-fit: contain;
		flex-shrink: 0;
	}

	.bottom {
		border-top: 1px solid var(--border);
		padding: 8px;
	}

	.bottom .label {
		margin-bottom: 6px;
	}

	.overlay-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		background: var(--bg);
		padding: 4px 8px;
		margin-bottom: 3px;
		border-radius: 4px;
		font-size: 11px;
		width: 100%;
		border: none;
		color: var(--text);
	}

	.overlay-row.clickable {
		cursor: pointer;
	}

	.overlay-row.clickable:hover {
		background: var(--border);
	}

	.overlay-row.group-toggle {
		font-weight: 600;
		margin-top: 4px;
		border-top: 1px solid var(--border);
		padding-top: 6px;
	}

	.overlay-row.indent {
		padding-left: 16px;
		font-size: 10px;
	}

	.mode {
		font-size: 10px;
	}

	.mode.off {
		color: var(--text-muted);
	}

	.mode.always {
		color: var(--success);
	}

	.mode.auto {
		color: var(--warning);
	}

	.mode.nodata {
		color: var(--accent);
	}

	.sidebar-collapsed {
		width: 40px;
		flex-shrink: 0;
		background: var(--surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.collapsed-icons {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 8px 0;
		gap: 2px;
	}

	.collapsed-item {
		all: unset;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: 5px;
		color: var(--text);
		cursor: pointer;
		font-size: 14px;
	}

	.collapsed-item:hover:not(.disabled) {
		background: var(--border);
	}

	.collapsed-item.active {
		background: var(--border);
		color: var(--accent);
	}

	.collapsed-item.disabled {
		color: var(--border);
		cursor: default;
	}

	.collapsed-sep {
		width: 24px;
		height: 1px;
		background: var(--border);
		margin: 4px 0;
	}

	.collapsed-overlays {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
		padding: 6px 0 8px;
		border-top: 1px solid var(--border);
	}

	.collapsed-overlay {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 28px;
		font-size: 13px;
		cursor: pointer;
		border-radius: 4px;
		background: none;
		border: none;
		color: inherit;
		padding: 0;
	}

	.collapsed-overlay:hover {
		background: var(--border);
	}

	.collapsed-overlay.group-btn {
		border-bottom: 1px solid var(--border);
		padding-bottom: 4px;
		margin-bottom: 2px;
	}

	.indicator {
		position: absolute;
		top: 2px;
		right: 2px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
	}

	.indicator.off {
		background: var(--text-muted);
		opacity: 0.4;
	}

	.indicator.always {
		background: var(--success);
		box-shadow: 0 0 4px rgba(74, 222, 128, 0.5);
	}

	.indicator.auto {
		background: var(--warning);
		box-shadow: 0 0 4px rgba(251, 191, 36, 0.5);
	}

	.indicator.nodata {
		background: var(--accent);
		box-shadow: 0 0 4px rgba(233, 69, 96, 0.5);
	}

	.collapsed-expand {
		width: 100%;
		border-top: 1px solid var(--border);
		border-radius: 0;
		margin-top: 6px;
		padding: 10px 6px;
		font-size: 14px !important;
	}

	.collapse-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		padding: 4px 6px;
		border-radius: 3px;
		line-height: 1;
	}

	.collapse-btn:hover {
		color: var(--text);
		background: rgba(255, 255, 255, 0.05);
	}

	.collapse-inside {
		width: 100%;
		border-top: 1px solid var(--border);
		border-radius: 0;
		padding: 6px;
	}
</style>
