<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { check } from '@tauri-apps/plugin-updater';
	import { relaunch } from '@tauri-apps/plugin-process';
	import { store } from '$lib/stores/status.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import Toggle from '$lib/components/Toggle.svelte';
	import RangeSlider from '$lib/components/RangeSlider.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getVersion } from '@tauri-apps/api/app';

	// --- Update ---
	let appVersion = $state('...');
	let updateStatus = $state<'idle' | 'checking' | 'available' | 'downloading' | 'error'>(
		store.updateAvailable ? 'available' : 'idle'
	);
	let updateVersion = $state(store.updateVersion || '');
	let updateError = $state('');
	let updateProgress = $state(0);

	// Load version on mount
	$effect(() => {
		getVersion().then(v => { appVersion = v; }).catch(() => {});
	});

	// Sync: when background checker detects an update, reflect it immediately
	$effect(() => {
		if (store.updateAvailable && updateStatus === 'idle') {
			updateStatus = 'available';
			updateVersion = store.updateVersion;
		}
	});

	async function checkForUpdates() {
		updateStatus = 'checking';
		updateError = '';
		try {
			const update = await check();
			if (update) {
				updateStatus = 'available';
				updateVersion = update.version;
			} else {
				updateStatus = 'idle';
				updateError = 'You are on the latest version.';
			}
		} catch (e: any) {
			updateStatus = 'error';
			updateError = e?.message || String(e);
		}
	}

	async function installUpdate() {
		updateStatus = 'downloading';
		updateError = '';
		try {
			const update = await check();
			if (!update) return;
			await update.downloadAndInstall((progress) => {
				if (progress.event === 'Started' && progress.data.contentLength) {
					updateProgress = 0;
				} else if (progress.event === 'Progress') {
					updateProgress += progress.data.chunkLength;
				} else if (progress.event === 'Finished') {
					updateProgress = 0;
				}
			});
			await relaunch();
		} catch (e: any) {
			updateStatus = 'error';
			updateError = e?.message || String(e);
		}
	}

	// Save/Cancel from overlay buttons (overlay-save/overlay-cancel events).
	// Works for OCR region overlays, comparator position, and compass position overlays.
	$effect(() => {
		if (!overlayVisible && !anyPositionOverlayOpen) return;
		const unlistenSave = listen('overlay-save', () => {
			if (overlayVisible) { saveRegion(); return; }
			for (const name of Object.keys(positionOverlays)) {
				if (positionOverlays[name]) { savePositionOverlay(name); return; }
			}
		});
		const unlistenCancel = listen('overlay-cancel', () => {
			if (overlayVisible) { cancelRegion(); return; }
			for (const name of Object.keys(positionOverlays)) {
				if (positionOverlays[name]) { cancelPositionOverlay(name); return; }
			}
		});
		return () => {
			unlistenSave.then(u => u());
			unlistenCancel.then(u => u());
		};
	});

	let overlayWin = $state<any>(null);
	let overlayVisible = $state<string | null>(null); // null = hidden, 'gem' or 'font' = which region

	// Inline editing states
	let editingServerUrl = $state(false);
	let editServerUrlValue = $state('');
	let editingClientTxt = $state(false);
	let editClientTxtValue = $state('');
	// Status is reactive via the shared store — no polling or manual refresh needed.

	// --- Server URL ---
	function startEditServerUrl() {
		editServerUrlValue = store.status?.server_url || '';
		editingServerUrl = true;
	}

	async function saveServerUrl() {
		try {
			await invoke('set_server_url', { url: editServerUrlValue });
			editingServerUrl = false;
			// Status auto-updates via events
		} catch (e) {
			console.error('Failed to save server URL:', e);
		}
	}

	function cancelEditServerUrl() {
		editingServerUrl = false;
	}


	// --- Client.txt Path ---
	function startEditClientTxt() {
		editClientTxtValue = store.status?.client_txt_path || '';
		editingClientTxt = true;
	}

	async function saveClientTxt() {
		try {
			await invoke('set_client_txt_path', { path: editClientTxtValue });
			editingClientTxt = false;
		} catch (e) {
			console.error('Failed to save client.txt path:', e);
		}
	}

	async function browseClientTxt() {
		try {
			await invoke('browse_client_txt');
		} catch (e: any) {
			if (e !== 'No file selected') {
				console.error('Browse failed:', e);
			}
		}
	}

	function cancelEditClientTxt() {
		editingClientTxt = false;
	}

	/** Notify layout that a config overlay is opening/closing. */
	function notifyConfigStart() {
		getCurrentWebviewWindow().emit('overlay-config-start', {}).catch(() => {});
	}
	function notifyConfigEnd() {
		getCurrentWebviewWindow().emit('overlay-config-end', {}).catch(() => {});
	}

	// --- Region Overlay (shared for gem tooltip + font panel) ---
	async function showRegionOverlay(type: 'gem' | 'font') {
		notifyConfigStart();
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) { console.error(e); }
			overlayWin = null;
		}
		// Read fresh from Rust (not store — store may lag behind after save)
		const command = type === 'gem' ? 'get_gem_region' : 'get_font_region';
		const region = await invoke<{ x: number; y: number; w: number; h: number }>(command).catch(() => null);
		const px = region?.x ?? 30;
		const py = region?.y ?? 45;
		const pw = region?.w ?? 550;
		const ph = region?.h ?? (type === 'font' ? 350 : 75);
		// Create without position — constructor DPI conversion is unreliable.
		const win = new WebviewWindow('overlay', {
			url: '/overlay',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: 550,
			height: 350,
		});
		win.once('tauri://created', async () => {
			// Set physical position + size (same space as outerPosition used by save)
			await win.setPosition(new PhysicalPosition(px, py))
				.catch(e => console.warn('[region] setPosition failed:', e));
			await win.setSize(new PhysicalSize(pw, ph))
				.catch(e => console.warn('[region] setSize failed:', e));
			overlayWin = win;
			overlayVisible = type;
		});
		win.once('tauri://error', (e: any) => console.error('Overlay failed:', e));
	}

	async function saveRegion() {
		if (!overlayWin || !overlayVisible) return;
		const command = overlayVisible === 'gem' ? 'set_gem_region' : 'set_font_region';
		try {
			const w = overlayWin.window ?? overlayWin;
			const pos = await w.outerPosition();
			const size = await w.outerSize();
			await invoke(command, { x: pos.x, y: pos.y, w: size.width, h: size.height });
		} catch (e) {
			console.error('Save region failed:', e);
			return;
		}
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = null;
		await reclaimMouse();
	}

	async function cancelRegion() {
		if (!overlayWin) return;
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = null;
		await reclaimMouse();
	}

	/** After closing a config overlay, emit toggle-reset so the layout
	 *  moves the comparator to its saved position and re-establishes focus. */
	async function reclaimMouse() {
		await getCurrentWebviewWindow().emit('overlay-toggle-reset', {}).catch(() => {});
		notifyConfigEnd();
	}

	function formatRegion(region: any): string {
		if (!region) return 'Not set';
		return `(${region.x}, ${region.y}) ${region.w}\u00d7${region.h}`;
	}

	// --- Comparator Overlay Position (red frame for positioning) ---
	// --- Trade Staleness Settings ---
	let tradeStaleWarnSecs = $state(store.status?.trade_stale_warn_secs ?? 120);
	let tradeStaleCriticalSecs = $state(store.status?.trade_stale_critical_secs ?? 600);
	let tradeAutoRefreshSecs = $state(store.status?.trade_auto_refresh_secs ?? 900);
	let editingTradeStaleness = $state(false);
	let tradeStalenessError = $state('');

	// Sync from store when status changes
	$effect(() => {
		if (store.status && !editingTradeStaleness) {
			tradeStaleWarnSecs = store.status.trade_stale_warn_secs ?? 120;
			tradeStaleCriticalSecs = store.status.trade_stale_critical_secs ?? 600;
			tradeAutoRefreshSecs = store.status.trade_auto_refresh_secs ?? 900;
		}
	});

	function startEditTradeStaleness() {
		editingTradeStaleness = true;
	}

	async function saveTradeStaleness() {
		if (tradeStaleWarnSecs >= tradeStaleCriticalSecs) {
			tradeStalenessError = 'Warn threshold must be less than critical threshold.';
			return;
		}
		if (tradeStaleCriticalSecs >= tradeAutoRefreshSecs) {
			tradeStalenessError = 'Critical threshold must be less than auto-refresh interval.';
			return;
		}
		tradeStalenessError = '';
		try {
			await invoke('set_trade_staleness_settings', {
				warnSecs: tradeStaleWarnSecs,
				criticalSecs: tradeStaleCriticalSecs,
				autoRefreshSecs: tradeAutoRefreshSecs,
			});
			editingTradeStaleness = false;
		} catch (e) {
			console.error('Failed to save trade staleness settings:', e);
			tradeStalenessError = 'Failed to save settings. Please try again.';
		}
	}

	function cancelEditTradeStaleness() {
		tradeStaleWarnSecs = store.status?.trade_stale_warn_secs ?? 120;
		tradeStaleCriticalSecs = store.status?.trade_stale_critical_secs ?? 600;
		tradeAutoRefreshSecs = store.status?.trade_auto_refresh_secs ?? 900;
		tradeStalenessError = '';
		editingTradeStaleness = false;
	}

	// --- Generic overlay position config ---
	// DRY: one set of functions for all overlay position configurations.
	interface OverlayConfig {
		label: string;          // window label for position overlay (e.g., 'overlay-comparator-pos')
		syncParam: string;      // URL param (e.g., 'comparator')
		getCommand: string;     // Rust get settings command
		setCommand: string;     // Rust set settings command
		defaultW: number;
		defaultH: number;
	}

	const OVERLAY_CONFIGS: Record<string, OverlayConfig> = {
		comparator: { label: 'overlay-comparator-pos', syncParam: 'comparator', getCommand: 'get_comparator_overlay_settings', setCommand: 'set_comparator_overlay_settings', defaultW: 630, defaultH: 250 },
		compass: { label: 'overlay-compass-pos', syncParam: 'compass', getCommand: 'get_compass_overlay_settings', setCommand: 'set_compass_overlay_settings', defaultW: 300, defaultH: 280 },
		pathstrip: { label: 'overlay-pathstrip-pos', syncParam: 'pathstrip', getCommand: 'get_pathstrip_overlay_settings', setCommand: 'set_pathstrip_overlay_settings', defaultW: 450, defaultH: 180 },
		timer: { label: 'overlay-timer-pos', syncParam: 'timer', getCommand: 'get_timer_overlay_settings', setCommand: 'set_timer_overlay_settings', defaultW: 160, defaultH: 50 },
	};

	// Per-overlay state
	let overlaySettings = $state<Record<string, { x: number; y: number; width: number; height: number } | null>>({
		comparator: null, compass: null, pathstrip: null, timer: null,
	});
	let positionOverlays = $state<Record<string, any>>({
		comparator: null, compass: null, pathstrip: null, timer: null,
	});

	// --- Timer appearance ---
	let timerBgOpacity = $state(75);
	let timerTextStroke = $state(true);
	let savedBgOpacity = $state(75);
	let savedTextStroke = $state(true);
	let timerAppearanceDirty = $derived(
		timerBgOpacity !== savedBgOpacity || timerTextStroke !== savedTextStroke
	);

	function saveTimerAppearance() {
		invoke('set_timer_appearance', { bgOpacity: timerBgOpacity / 100, textStroke: timerTextStroke })
			.then(() => {
				savedBgOpacity = timerBgOpacity;
				savedTextStroke = timerTextStroke;
			})
			.catch((e: any) => console.warn('[settings] save timer appearance failed:', e));
	}

	// Load all overlay settings on init
	$effect(() => {
		for (const [name, cfg] of Object.entries(OVERLAY_CONFIGS)) {
			invoke<any>(cfg.getCommand).then((s) => {
				if (s) overlaySettings[name] = s;
			}).catch((e) => console.warn(`[settings] failed to load ${name} overlay settings:`, e));
		}
		invoke<any>('get_timer_appearance').then((a) => {
			if (a) {
				timerBgOpacity = Math.round(a.bg_opacity * 100);
				timerTextStroke = a.text_stroke;
				savedBgOpacity = timerBgOpacity;
				savedTextStroke = a.text_stroke;
			}
		}).catch((e: any) => console.warn('[settings] load timer appearance failed:', e));
	});

	async function showPositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		if (!cfg) return;
		notifyConfigStart();
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		const { PhysicalPosition, PhysicalSize } = await import('@tauri-apps/api/dpi');
		if (positionOverlays[name]) {
			try { await positionOverlays[name].destroy(); } catch (_) {}
			positionOverlays[name] = null;
		}
		// Read live overlay position/size (physical pixels) so the config window
		// matches exactly. This prevents the sync loop from resizing the real overlay.
		const live = await invoke<any>(cfg.getCommand).catch(() => null);
		const realWin = await WebviewWindow.getByLabel(cfg.syncParam);
		let physX = live?.x ?? 100, physY = live?.y ?? 100;
		let physW = cfg.defaultW, physH = cfg.defaultH;
		if (realWin) {
			try {
				const pos = await realWin.outerPosition();
				const size = await realWin.outerSize();
				physX = pos.x; physY = pos.y;
				physW = size.width; physH = size.height;
			} catch (e) {
				console.warn(`[settings] failed to read live ${name} overlay position/size, using saved:`, e);
			}
		} else if (live) {
			physW = live.width ?? cfg.defaultW;
			physH = live.height ?? cfg.defaultH;
		}
		// Save pre-configure state (physical pixels) so cancel restores it
		overlaySettings[name] = { x: physX, y: physY, width: physW, height: physH };
		// Constructor takes logical pixels; convert physical → logical.
		// setSize(PhysicalSize) in tauri://created will set the exact physical size.
		const mainWin = getCurrentWebviewWindow();
		const sf = await mainWin.scaleFactor().catch((e: any) => { console.warn('[settings] scaleFactor failed, using 1:', e); return 1; });
		const win = new WebviewWindow(cfg.label, {
			url: `/overlay?sync=${cfg.syncParam}`,
			transparent: true, decorations: false, alwaysOnTop: true,
			resizable: ['compass', 'pathstrip', 'timer'].includes(name), shadow: false, skipTaskbar: true,
			width: Math.round(physW / sf), height: Math.round(physH / sf),
		});
		win.once('tauri://created', async () => {
			await win.setPosition(new PhysicalPosition(physX, physY));
			await win.setSize(new PhysicalSize(physW, physH));
			positionOverlays[name] = win;
		});
		win.once('tauri://error', (e: any) => console.error(`${name} position overlay failed:`, e));
	}

	async function savePositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		const win = positionOverlays[name];
		if (!cfg || !win) return;
		try {
			const ref = win.window ?? win;
			const pos = await ref.outerPosition();
			const size = await ref.outerSize();
			await invoke(cfg.setCommand, { x: pos.x, y: pos.y, w: size.width, h: size.height, enabled: true });
			overlaySettings[name] = { x: pos.x, y: pos.y, width: size.width, height: size.height };
		} catch (e) {
			console.error(`Save ${name} position failed:`, e);
			return;
		}
		try { await win.destroy(); } catch (_) {}
		positionOverlays[name] = null;
		await reclaimMouse();
	}

	async function cancelPositionOverlay(name: string) {
		const cfg = OVERLAY_CONFIGS[name];
		const win = positionOverlays[name];
		if (!win) return;
		try { await win.destroy(); } catch (_) {}
		positionOverlays[name] = null;
		// Persist pre-configure state so overlay-toggle-reset (from reclaimMouse)
		// restores the overlay to its exact pre-configure size.
		const pre = overlaySettings[name];
		if (pre && cfg) {
			await invoke(cfg.setCommand, {
				x: pre.x, y: pre.y, w: pre.width, h: pre.height, enabled: true,
			}).catch((e: any) => console.warn(`[settings] failed to restore ${name} pre-configure position:`, e));
		}
		await reclaimMouse();
	}

	// Convenience: check if any position overlay is open (for overlay-save/cancel guard)
	let anyPositionOverlayOpen = $derived(
		Object.values(positionOverlays).some(w => w !== null)
	);
</script>

<div class="settings-page">
	<h1>Settings</h1>

		<!-- About & Updates -->
		<section>
			<h2>About</h2>

			<div class="setting-row">
				<span class="setting-label">Version</span>
				<span class="setting-value mono">{appVersion}</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Updates</span>
				{#if updateStatus === 'checking'}
					<span class="setting-value muted">Checking...</span>
				{:else if updateStatus === 'available'}
					<span class="setting-value update-available">v{updateVersion} available</span>
					<Button variant="save" onclick={installUpdate}>Install & Restart</Button>
				{:else if updateStatus === 'downloading'}
					<span class="setting-value muted">Downloading... {updateProgress > 0 ? `(${Math.round(updateProgress / 1024)}KB)` : ''}</span>
				{:else if updateStatus === 'error'}
					<span class="setting-value update-error">{updateError}</span>
					<Button onclick={checkForUpdates}>Retry</Button>
				{:else}
					{#if updateError}
						<span class="setting-value muted">{updateError}</span>
					{/if}
					<Button onclick={checkForUpdates}>Check for Updates</Button>
				{/if}
			</div>
		</section>

		<!-- General -->
		<section>
			<h2>General</h2>

			{#if import.meta.env.DEV}
			<div class="setting-row">
				<span class="setting-label">Server URL</span>
				{#if editingServerUrl}
					<div class="setting-edit">
						<input
							type="text"
							class="setting-input"
							bind:value={editServerUrlValue}
							onkeydown={(e) => { if (e.key === 'Enter') saveServerUrl(); if (e.key === 'Escape') cancelEditServerUrl(); }}
						/>
						<Button variant="save" onclick={saveServerUrl}>Save</Button>
						<Button onclick={cancelEditServerUrl}>Cancel</Button>
					</div>
				{:else}
					<span class="setting-value">{store.status?.server_url ?? '...'}</span>
					<Button onclick={startEditServerUrl}>Edit</Button>
				{/if}
			</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">League</span>
				<span class="setting-value">Mirage</span>
			</div>

		</section>

		<!-- Game Integration -->
		<section>
			<h2>Game Integration</h2>

			{#if store.status && !store.status.client_txt_exists}
				<div class="warning-banner">
					Client.txt not found at the configured path. Lab detection, OCR, and compass will not work. Use Browse to locate your Path of Exile Client.txt file.
				</div>
			{/if}

			<div class="setting-row">
				<span class="setting-label">Client.txt Path</span>
				{#if editingClientTxt}
					<div class="setting-edit">
						<input
							type="text"
							class="setting-input"
							bind:value={editClientTxtValue}
							onkeydown={(e) => { if (e.key === 'Enter') saveClientTxt(); if (e.key === 'Escape') cancelEditClientTxt(); }}
						/>
						<Button variant="save" onclick={saveClientTxt}>Save</Button>
						<Button onclick={cancelEditClientTxt}>Cancel</Button>
					</div>
				{:else}
					<span class="setting-value path" class:path-missing={!store.status?.client_txt_exists}>{store.status?.client_txt_path ?? '...'}</span>
					<Button onclick={browseClientTxt}>Browse</Button>
					<Button onclick={startEditClientTxt}>Edit</Button>
					<Button onclick={() => invoke('reset_client_txt_path').catch(e => console.error(e))} title="Auto-detect GGG or Steam install">Reset</Button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Gem Tooltip Region</span>
				{#if overlayVisible === 'gem'}
					<span class="setting-value">Positioning overlay...</span>
					<Button variant="save" onclick={saveRegion}>Save</Button>
					<Button onclick={cancelRegion}>Cancel</Button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.gem_region)}</span>
					<Button onclick={() => showRegionOverlay('gem')} disabled={!!overlayVisible}>Configure</Button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Font Panel Region</span>
				{#if overlayVisible === 'font'}
					<span class="setting-value">Positioning overlay...</span>
					<Button variant="save" onclick={saveRegion}>Save</Button>
					<Button onclick={cancelRegion}>Cancel</Button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.font_region)}</span>
					<Button onclick={() => showRegionOverlay('font')} disabled={!!overlayVisible}>Configure</Button>
				{/if}
			</div>
		</section>

		<!-- Overlays -->
		<section>
			<h2>Overlay Positions</h2>

			{#each [
				{ name: 'comparator', label: 'Gems Compare' },
				{ name: 'compass', label: 'Lab Compass' },
				{ name: 'pathstrip', label: 'Lab Map' },
				{ name: 'timer', label: 'Lab Timer' },
			] as cfg (cfg.name)}
				<div class="setting-row">
					<span class="setting-label">{cfg.label}</span>
					{#if positionOverlays[cfg.name]}
						<span class="setting-value">Drag overlay to position...</span>
						<Button variant="save" onclick={() => savePositionOverlay(cfg.name)}>Save</Button>
						<Button onclick={() => cancelPositionOverlay(cfg.name)}>Cancel</Button>
					{:else}
						{@const s = overlaySettings[cfg.name]}
						<span class="setting-value mono">{s ? `(${s.x}, ${s.y}) ${s.width}\u00d7${s.height}` : 'Not set'}</span>
						<Button onclick={() => showPositionOverlay(cfg.name)} disabled={!!overlayVisible}>Configure</Button>
					{/if}
				</div>
			{/each}
		</section>

		<!-- Timer Appearance -->
		<section>
			<h2>Timer Appearance</h2>

			<div class="setting-row">
				<span class="setting-label">Background</span>
				<RangeSlider bind:value={timerBgOpacity} min={0} max={100} step={5} formatValue={(v) => `${v}%`} />
			</div>

			<div class="setting-row">
				<span class="setting-label">Text outline</span>
				<Toggle bind:checked={timerTextStroke} />
			</div>

			<div class="setting-row">
				<span class="setting-label"></span>
				<Button variant="save" onclick={saveTimerAppearance} disabled={!timerAppearanceDirty}>Apply</Button>
			</div>
		</section>

		<!-- Trade -->
		<section>
			<h2>Trade</h2>

			<div class="setting-row">
				<Tooltip text="After this many seconds, trade data shows a yellow warning indicator in the comparator and overlay. Signals that the cached prices may be getting outdated.">
					<span class="setting-label">Stale warning (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeStaleWarnSecs}
							min="30"
							max="3600"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_stale_warn_secs ?? 120}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<Tooltip text="After this many seconds, trade data shows a red critical indicator. The cached prices are likely outdated and should be refreshed before making decisions.">
					<span class="setting-label">Stale critical (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeStaleCriticalSecs}
							min="60"
							max="7200"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_stale_critical_secs ?? 600}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<Tooltip text="When auto-trade is enabled, trade data older than this is automatically re-fetched from GGG when a gem appears in the comparator. Set higher to reduce API calls, lower for fresher data.">
					<span class="setting-label">Auto-refresh (sec)</span>
				</Tooltip>
				{#if editingTradeStaleness}
					<div class="setting-edit">
						<input
							type="number"
							class="setting-input narrow"
							bind:value={tradeAutoRefreshSecs}
							min="60"
							max="7200"
						/>
					</div>
				{:else}
					<span class="setting-value mono">{store.status?.trade_auto_refresh_secs ?? 900}s</span>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label"></span>
				{#if editingTradeStaleness}
					<Button variant="save" onclick={saveTradeStaleness}>Save</Button>
					<Button onclick={cancelEditTradeStaleness}>Cancel</Button>
				{:else}
					<Button onclick={startEditTradeStaleness}>Edit</Button>
				{/if}
			</div>
			{#if tradeStalenessError}
				<div class="setting-row">
					<span class="setting-label"></span>
					<span class="setting-error">{tradeStalenessError}</span>
				</div>
			{/if}
		</section>

		<!-- Danger Zone -->
		<section class="danger-section">
			<h2>Danger Zone</h2>
			<div class="setting-row">
				<span class="setting-label">Reset All Settings</span>
				<span class="setting-value">Deletes settings file and re-detects everything</span>
				<Button variant="danger" onclick={() => {
					if (confirm('Reset all settings to defaults? This will clear all overlay positions, Client.txt path, and trade settings. The app will re-detect your PoE installation.')) {
						invoke('reset_all_settings').then(() => {
							alert('Settings reset. The app will now use fresh defaults.');
						}).catch(e => console.error('Reset failed:', e));
					}
				}}>Reset Everything</Button>
			</div>
		</section>

		<!-- Logs -->
		{#if store.logs.length > 0}
			<section>
				<div class="log-header">
					<h2>Logs</h2>
					<Button onclick={() => { navigator.clipboard.writeText(store.logs.toReversed().join('\n')); }}>Copy</Button>
				</div>
				<div class="log-list">
					{#each store.logs.toReversed() as line}
						<div class="log-line" class:log-error={line.includes('failed') || line.includes('error')}>{line}</div>
					{/each}
				</div>
			</section>
		{/if}

</div>

<style>
	.settings-page {
		max-width: 520px;
		margin: 0 auto;
	}

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
	}



	.setting-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0;
		min-height: 32px;
	}

	.setting-row + .setting-row {
		border-top: 1px solid rgba(255, 255, 255, 0.05);
	}

	.setting-label {
		min-width: 140px;
		flex-shrink: 0;
		font-size: 0.85rem;
		color: var(--text);
	}

	.setting-value {
		flex: 1;
		font-size: 0.8rem;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.setting-value.mono {
		font-family: 'Consolas', 'Courier New', monospace;
		letter-spacing: 0.1em;
	}

	.update-available {
		color: var(--success, #22c55e);
		font-weight: 600;
	}

	.update-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}

	.setting-value.path {
		font-size: 0.7rem;
		font-family: 'Consolas', 'Courier New', monospace;
	}

	.setting-value.path-missing {
		color: var(--accent, #ef4444);
	}

	.warning-banner {
		background: rgba(239, 68, 68, 0.15);
		border: 1px solid rgba(239, 68, 68, 0.4);
		border-radius: 6px;
		padding: 8px 12px;
		margin-bottom: 8px;
		font-size: 0.8rem;
		color: #fca5a5;
		line-height: 1.4;
	}

	.setting-value.muted {
		color: var(--border);
		font-style: italic;
	}

	.setting-edit {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.35rem;
	}

	.setting-input {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		padding: 0.25rem 0.4rem;
		border-radius: 4px;
		font-size: 0.75rem;
		font-family: 'Consolas', 'Courier New', monospace;
	}

	.setting-input:focus {
		outline: none;
		border-color: var(--accent);
	}

	.setting-input.narrow {
		max-width: 100px;
	}

	.setting-error {
		color: var(--color-lab-red, #ef4444);
		font-size: 0.75rem;
	}

	.danger-section {
		border-color: rgba(239, 68, 68, 0.3);
	}

	.danger-section h2 {
		color: #ef4444;
	}

	.log-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	.log-header h2 {
		margin-bottom: 0;
	}

	.log-list {
		max-height: 250px;
		overflow-y: auto;
		font-family: 'Consolas', 'Courier New', monospace;
		font-size: 0.7rem;
		line-height: 1.4;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 8px 12px;
	}
	.log-line {
		color: var(--text-muted);
		padding: 0.1rem 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.03);
	}
	.log-error {
		color: var(--color-lab-red, #ef4444);
	}
</style>
