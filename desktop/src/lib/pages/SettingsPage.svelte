<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
	import { check } from '@tauri-apps/plugin-updater';
	import { relaunch } from '@tauri-apps/plugin-process';
	import { store } from '$lib/stores/status.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import { getVersion } from '@tauri-apps/api/app';

	// --- Update ---
	let appVersion = $state('...');
	let updateStatus = $state<'idle' | 'checking' | 'available' | 'downloading' | 'error'>('idle');
	let updateVersion = $state('');
	let updateError = $state('');
	let updateProgress = $state(0);

	// Load version on mount
	$effect(() => {
		getVersion().then(v => { appVersion = v; }).catch(() => {});
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
	// Works for both OCR region overlays and comparator position overlay.
	$effect(() => {
		if (!overlayVisible && !comparatorPositionOverlay) return;
		const unlistenSave = listen('overlay-save', () => {
			if (overlayVisible) saveRegion();
			else if (comparatorPositionOverlay) saveComparatorPosition();
		});
		const unlistenCancel = listen('overlay-cancel', () => {
			if (overlayVisible) cancelRegion();
			else if (comparatorPositionOverlay) cancelComparatorPosition();
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
			// Status auto-updates via events
		} catch (e) {
			console.error('Failed to save client.txt path:', e);
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
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) { console.error(e); }
			overlayWin = null;
		}
		const region = type === 'gem' ? store.status?.gem_region : store.status?.font_region;
		const dpr = await getCurrentWebviewWindow().scaleFactor();
		const win = new WebviewWindow('overlay', {
			url: '/overlay',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: Math.round((region?.w || 550) / dpr),
			height: Math.round((region?.h || (type === 'font' ? 350 : 75)) / dpr),
			x: Math.round((region?.x || 30) / dpr),
			y: Math.round((region?.y || 45) / dpr),
		});
		win.once('tauri://created', () => { overlayWin = win; overlayVisible = type; });
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

	/** After closing a config overlay, toggle the comparator off and on.
	 *  This forces a clean window focus reset — same as sidebar toggle. */
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

	let comparatorOverlaySettings = $state<{ x: number; y: number; width: number; height: number } | null>(null);
	let comparatorPositionOverlay = $state<any>(null);

	$effect(() => {
		invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings').then((settings) => {
			if (settings) {
				comparatorOverlaySettings = settings;
			}
		}).catch(e => console.warn('[settings] load overlay settings failed:', e));
	});

	async function showComparatorPositionOverlay() {
		notifyConfigStart();
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		if (comparatorPositionOverlay) {
			try { await comparatorPositionOverlay.destroy(); } catch (_) {}
			comparatorPositionOverlay = null;
		}
		const s = comparatorOverlaySettings;
		const dpr = await getCurrentWebviewWindow().scaleFactor();
		const win = new WebviewWindow('overlay-comparator-pos', {
			url: '/overlay?sync=comparator',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,
			width: 630,
			height: 250,
			x: s ? Math.round(s.x / dpr) : 100,
			y: s ? Math.round(s.y / dpr) : 100,
		});
		win.once('tauri://created', () => { comparatorPositionOverlay = win; });
		win.once('tauri://error', (e: any) => console.error('Position overlay failed:', e));
	}

	async function saveComparatorPosition() {
		if (!comparatorPositionOverlay) return;
		let x: number, y: number, w: number, h: number;
		try {
			const ref = comparatorPositionOverlay.window ?? comparatorPositionOverlay;
			const pos = await ref.outerPosition();
			const size = await ref.outerSize();
			x = pos.x; y = pos.y; w = size.width; h = size.height;
			await invoke('set_comparator_overlay_settings', { x, y, w, h, enabled: true });
			comparatorOverlaySettings = { x, y, width: w, height: h };
		} catch (e) {
			console.error('Save comparator position failed:', e);
			return;
		}
		try { await comparatorPositionOverlay.destroy(); } catch (_) {}
		comparatorPositionOverlay = null;
		await reclaimMouse();

		// Recreate live comparator overlay at new position
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('comparator');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		const dpr = await getCurrentWebviewWindow().scaleFactor();
		new WebviewWindow('comparator', {
			url: '/overlay/comparator',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,

			width: 630,
			height: 250,
			x: Math.round(x! / dpr),
			y: Math.round(y! / dpr),
		});
	}

	async function cancelComparatorPosition() {
		if (!comparatorPositionOverlay) return;
		try { await comparatorPositionOverlay.destroy(); } catch (_) {}
		comparatorPositionOverlay = null;
		await reclaimMouse();
	}
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
					<button class="btn-small save" onclick={installUpdate}>Install & Restart</button>
				{:else if updateStatus === 'downloading'}
					<span class="setting-value muted">Downloading... {updateProgress > 0 ? `(${Math.round(updateProgress / 1024)}KB)` : ''}</span>
				{:else if updateStatus === 'error'}
					<span class="setting-value update-error">{updateError}</span>
					<button class="btn-small" onclick={checkForUpdates}>Retry</button>
				{:else}
					{#if updateError}
						<span class="setting-value muted">{updateError}</span>
					{/if}
					<button class="btn-small" onclick={checkForUpdates}>Check for Updates</button>
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
						<button class="btn-small save" onclick={saveServerUrl}>Save</button>
						<button class="btn-small" onclick={cancelEditServerUrl}>Cancel</button>
					</div>
				{:else}
					<span class="setting-value">{store.status?.server_url ?? '...'}</span>
					<button class="btn-small" onclick={startEditServerUrl}>Edit</button>
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
						<button class="btn-small save" onclick={saveClientTxt}>Save</button>
						<button class="btn-small" onclick={cancelEditClientTxt}>Cancel</button>
					</div>
				{:else}
					<span class="setting-value path">{store.status?.client_txt_path ?? '...'}</span>
					<button class="btn-small" onclick={startEditClientTxt}>Edit</button>
					<button class="btn-small" onclick={() => invoke('reset_client_txt_path').catch(e => console.error(e))} title="Reset to default PoE path">Reset</button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Gem Tooltip Region</span>
				{#if overlayVisible === 'gem'}
					<span class="setting-value">Positioning overlay...</span>
					<button class="btn-small save" onclick={saveRegion}>Save</button>
					<button class="btn-small" onclick={cancelRegion}>Cancel</button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.gem_region)}</span>
					<button class="btn-small" onclick={() => showRegionOverlay('gem')} disabled={!!overlayVisible}>Configure</button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Font Panel Region</span>
				{#if overlayVisible === 'font'}
					<span class="setting-value">Positioning overlay...</span>
					<button class="btn-small save" onclick={saveRegion}>Save</button>
					<button class="btn-small" onclick={cancelRegion}>Cancel</button>
				{:else}
					<span class="setting-value mono">{formatRegion(store.status?.font_region)}</span>
					<button class="btn-small" onclick={() => showRegionOverlay('font')} disabled={!!overlayVisible}>Configure</button>
				{/if}
			</div>
		</section>

		<!-- Overlays -->
		<section>
			<h2>Overlays</h2>

			<div class="setting-row">
				<span class="setting-label">Comparator Position</span>
				{#if comparatorPositionOverlay}
					<span class="setting-value">Drag red overlay to position...</span>
					<button class="btn-small save" onclick={saveComparatorPosition}>Save</button>
					<button class="btn-small" onclick={cancelComparatorPosition}>Cancel</button>
				{:else}
					<span class="setting-value mono">{comparatorOverlaySettings ? `(${comparatorOverlaySettings.x}, ${comparatorOverlaySettings.y}) ${comparatorOverlaySettings.width}\u00d7${comparatorOverlaySettings.height}` : 'Not set'}</span>
					<button class="btn-small" onclick={showComparatorPositionOverlay} disabled={!!overlayVisible}>Configure</button>
				{/if}
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
					<button class="btn-small save" onclick={saveTradeStaleness}>Save</button>
					<button class="btn-small" onclick={cancelEditTradeStaleness}>Cancel</button>
				{:else}
					<button class="btn-small" onclick={startEditTradeStaleness}>Edit</button>
				{/if}
			</div>
			{#if tradeStalenessError}
				<div class="setting-row">
					<span class="setting-label"></span>
					<span class="setting-error">{tradeStalenessError}</span>
				</div>
			{/if}
		</section>

		<!-- Logs -->
		{#if store.logs.length > 0}
			<section>
				<h2>Logs</h2>
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

	.btn-small {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.btn-small:hover {
		border-color: var(--accent);
		color: var(--text);
	}

	.btn-small.save {
		border-color: var(--success);
		color: var(--success);
	}

	.btn-small.save:hover {
		background: rgba(74, 222, 128, 0.1);
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
