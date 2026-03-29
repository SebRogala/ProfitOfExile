<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';

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

	// --- Pair Code ---
	async function regeneratePairCode() {
		try {
			await invoke('regenerate_pair_code');
			// Status auto-updates via events
		} catch (e) {
			console.error('Failed to regenerate pair code:', e);
		}
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

	// --- Region Overlay (shared for gem tooltip + font panel) ---
	async function showRegionOverlay(type: 'gem' | 'font') {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) { console.error(e); }
			overlayWin = null;
		}
		const region = type === 'gem' ? store.status?.gem_region : store.status?.font_region;
		const dpr = window.devicePixelRatio || 1;
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
		// Status auto-updates via events
	}

	async function cancelRegion() {
		if (!overlayWin) return;
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = null;
	}

	function formatRegion(region: any): string {
		if (!region) return 'Not set';
		return `(${region.x}, ${region.y}) ${region.w}\u00d7${region.h}`;
	}

	// --- Comparator Overlay Position (red frame for positioning) ---
	let comparatorOverlaySettings = $state<{ x: number; y: number; width: number; height: number } | null>(null);
	let comparatorPositionOverlay = $state<any>(null);

	$effect(() => {
		invoke<{ x: number; y: number; width: number; height: number; enabled: boolean } | null>('get_comparator_overlay_settings').then((settings) => {
			if (settings) {
				comparatorOverlaySettings = settings;
			}
		}).catch(() => {});
	});

	async function showComparatorPositionOverlay() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		if (comparatorPositionOverlay) {
			try { await comparatorPositionOverlay.destroy(); } catch (_) {}
			comparatorPositionOverlay = null;
		}
		const s = comparatorOverlaySettings;
		const dpr = window.devicePixelRatio || 1;
		const win = new WebviewWindow('overlay-comparator-pos', {
			url: '/overlay?sync=comparator',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: true,
			shadow: false,
			skipTaskbar: true,
			width: s ? Math.round(s.width / dpr) : 380,
			height: s ? Math.round(s.height / dpr) : 200,
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

		// Recreate live comparator overlay at new position
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		for (let i = 0; i < 5; i++) {
			const existing = await WebviewWindow.getByLabel('comparator');
			if (!existing) break;
			try { await existing.close(); } catch (_) {}
			try { await existing.destroy(); } catch (_) {}
			await new Promise(r => setTimeout(r, 100));
		}
		const dpr = window.devicePixelRatio || 1;
		new WebviewWindow('comparator', {
			url: '/overlay/comparator',
			transparent: true,
			decorations: false,
			alwaysOnTop: true,
			resizable: false,
			shadow: false,
			skipTaskbar: true,

			width: 900,
			height: 500,
			x: Math.round(x! / dpr),
			y: Math.round(y! / dpr),
		});
	}

	async function cancelComparatorPosition() {
		if (!comparatorPositionOverlay) return;
		try { await comparatorPositionOverlay.destroy(); } catch (_) {}
		comparatorPositionOverlay = null;
	}
</script>

<div class="settings-page">
	<h1>Settings</h1>

		<!-- General -->
		<section>
			<h2>General</h2>

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

			<div class="setting-row">
				<span class="setting-label">League</span>
				<span class="setting-value">Mirage</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Pair Code</span>
				<span class="setting-value mono">{store.status?.pair_code ?? '....'}</span>
				<button class="btn-small" onclick={regeneratePairCode}>Regenerate</button>
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

		<!-- Trade -->
		<section>
			<h2>Trade</h2>

			<div class="setting-row">
				<span class="setting-label">Auto-trigger lookup</span>
				<span class="setting-value muted">Coming soon</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Cache min-age</span>
				<span class="setting-value muted">Coming soon</span>
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
</style>
