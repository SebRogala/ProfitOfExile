<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	let status = $state<any>(null);
	let overlayWin = $state<any>(null);
	let overlayVisible = $state(false);

	// Inline editing states
	let editingServerUrl = $state(false);
	let editServerUrlValue = $state('');
	let editingClientTxt = $state(false);
	let editClientTxtValue = $state('');

	// Load status immediately + retry until it works
	invoke('get_status').then((s) => { status = s; }).catch(() => {});
	const statusInterval = setInterval(() => {
		if (!status) {
			invoke('get_status').then((s) => { status = s; }).catch(() => {});
		}
	}, 500);

	async function refreshStatus() {
		try {
			status = await invoke('get_status');
		} catch (e) {
			console.error('Failed to refresh status:', e);
		}
	}

	// --- Server URL ---
	function startEditServerUrl() {
		editServerUrlValue = status?.server_url || '';
		editingServerUrl = true;
	}

	async function saveServerUrl() {
		try {
			await invoke('set_server_url', { url: editServerUrlValue });
			editingServerUrl = false;
			await refreshStatus();
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
			await refreshStatus();
		} catch (e) {
			console.error('Failed to regenerate pair code:', e);
		}
	}

	// --- Client.txt Path ---
	function startEditClientTxt() {
		editClientTxtValue = status?.client_txt_path || '';
		editingClientTxt = true;
	}

	async function saveClientTxt() {
		try {
			await invoke('set_client_txt_path', { path: editClientTxtValue });
			editingClientTxt = false;
			await refreshStatus();
		} catch (e) {
			console.error('Failed to save client.txt path:', e);
		}
	}

	function cancelEditClientTxt() {
		editingClientTxt = false;
	}

	// --- Gem Region Overlay ---
	async function showRegionOverlay() {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');
		// Destroy existing if any
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) { console.error(e); }
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
		win.once('tauri://created', () => { overlayWin = win; overlayVisible = true; });
		win.once('tauri://error', (e) => console.error('Overlay failed:', e));
	}

	async function saveRegion() {
		if (!overlayWin) return;
		try {
			const w = overlayWin.window ?? overlayWin;
			const pos = await w.outerPosition();
			const size = await w.outerSize();
			await invoke('set_gem_region', { x: pos.x, y: pos.y, w: size.width, h: size.height });
		} catch (e) {
			console.error('Save region failed:', e);
			return;
		}
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = false;
		await refreshStatus();
	}

	async function cancelRegion() {
		if (!overlayWin) return;
		try { await overlayWin.destroy(); } catch (e) { console.error(e); }
		overlayWin = null;
		overlayVisible = false;
	}

	function formatRegion(region: any): string {
		if (!region) return 'Not set';
		return `(${region.x}, ${region.y}) ${region.w}\u00d7${region.h}`;
	}
</script>

<div class="settings-page">
	<h1>Settings</h1>

	{#if !status}
		<p class="loading">Loading...</p>
	{:else}
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
					<span class="setting-value">{status.server_url}</span>
					<button class="btn-small" onclick={startEditServerUrl}>Edit</button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">League</span>
				<span class="setting-value">Mirage</span>
			</div>

			<div class="setting-row">
				<span class="setting-label">Pair Code</span>
				<span class="setting-value mono">{status.pair_code}</span>
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
					<span class="setting-value path">{status.client_txt_path}</span>
					<button class="btn-small" onclick={startEditClientTxt}>Edit</button>
				{/if}
			</div>

			<div class="setting-row">
				<span class="setting-label">Gem Tooltip Region</span>
				{#if overlayVisible}
					<span class="setting-value">Positioning overlay...</span>
					<button class="btn-small save" onclick={saveRegion}>Save</button>
					<button class="btn-small" onclick={cancelRegion}>Cancel</button>
				{:else}
					<span class="setting-value mono">{formatRegion(status.gem_region)}</span>
					<button class="btn-small" onclick={showRegionOverlay}>Configure</button>
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
				<span class="setting-label">Lock / Unlock all</span>
				<span class="setting-value muted">Coming soon</span>
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

	.loading {
		color: var(--text-muted);
		font-size: 0.85rem;
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
