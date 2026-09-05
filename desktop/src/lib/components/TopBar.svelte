<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

	import { nav } from '$lib/stores/navigation.svelte';
	import { store } from '$lib/stores/status.svelte';
	import { persisted } from '$lib/prefs.svelte';
	import { LOCAL_SERVER_URL, serverToggle } from '$lib/server-toggle';

	let { status }: {
		status: any;
	} = $props();

	/**
	 * The production target this build was given, or '' when it was built
	 * without one (docs/DEV-SETUP.md, "Build-time variables"). NOT defaulted to
	 * the local url: that default is what made the DEBUG/PROD button a one-way
	 * door — see `server-toggle.ts`.
	 */
	const BUILT_PROD_URL: string = import.meta.env.VITE_SERVER_URL || '';

	/**
	 * The url the app left when it last flipped to DEBUG — the way back for a
	 * build with no production target. Persisted (ADR-013) so a restart on
	 * DEBUG still has it. Only written by the dev-only button below.
	 */
	const returnUrl = persisted('devServerReturnUrl', '');

	const toggle = $derived(serverToggle(status?.server_url ?? '', BUILT_PROD_URL, returnUrl.value));

	async function toggleServer() {
		// Nowhere to go: the tooltip has said why; take the click to where a
		// server url can be typed instead of eating it.
		if (!toggle.target) {
			nav.go('/settings');
			return;
		}
		if (toggle.target === LOCAL_SERVER_URL && status?.server_url) returnUrl.value = status.server_url;
		const { invoke } = await import('@tauri-apps/api/core');
		await invoke('set_server_url', { url: toggle.target });
	}

	async function minimizeWindow() {
		await getCurrentWebviewWindow().minimize();
	}

	async function toggleMaximize() {
		const win = getCurrentWebviewWindow();
		if (await win.isMaximized()) {
			await win.unmaximize();
		} else {
			await win.maximize();
		}
	}

	async function closeWindow() {
		await getCurrentWebviewWindow().close();
	}

	function startDrag(e: MouseEvent) {
		// Only drag from the topbar background, not from buttons/links
		if ((e.target as HTMLElement).closest('button, a, .status-dot')) return;
		getCurrentWebviewWindow().startDragging();
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<header class="topbar" onmousedown={startDrag}>
	<div class="left">
		<img src="/icon-32.png" alt="ProfitOfExile" class="logo" />
		<span class="app-name">ProfitOfExile</span>
	</div>
	<div class="center">
		<span class="status-dot" class:connected={store.serverConnected} title={store.serverConnected ? `Server: Connected (${status?.server_url})` : 'Server: Disconnected'}></span>
		<span class="status-dot scanning-dot" class:active={status?.state && status.state !== 'Idle'} title={`OCR Scanner: ${status?.state ?? 'Unknown'} ${status?.state === 'PickingGems' ? '— reading gem names' : status?.state === 'FontReady' ? '— font detected' : ''}`}></span>
		{#if import.meta.env.DEV}
			<button class="btn-debug" class:active={toggle.label === 'DEBUG'} class:no-target={!toggle.target} title={toggle.title} onclick={toggleServer}>
				{toggle.label}
			</button>
		{/if}
		{#if store.updateAvailable}
			<button class="update-badge" title="Update available: v{store.updateVersion}" onclick={() => nav.go('/settings')}>
				v{store.updateVersion}
			</button>
		{/if}
		<button class="settings-link" title="Settings" onclick={() => nav.go('/settings')}>&#9881;&#65039;</button>
	</div>
	<div class="window-controls">
		<button class="win-btn" onclick={minimizeWindow} title="Minimize">&#x2013;</button>
		<button class="win-btn" onclick={toggleMaximize} title="Maximize">&#9723;</button>
		<button class="win-btn close" onclick={closeWindow} title="Close">&#10005;</button>
	</div>
</header>

<style>
	.topbar {
		height: 36px;
		min-height: 36px;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0 0 0.75rem;
		user-select: none;
		-webkit-app-region: drag;
		overflow: hidden;
	}

	.left, .center {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.logo {
		width: 26px;
		height: 26px;
	}

	.app-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--text);
	}

	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
		flex-shrink: 0;
	}

	.status-dot.connected {
		background: var(--success);
	}

	.scanning-dot {
		background: var(--text-muted);
	}

	.scanning-dot.active {
		background: var(--success);
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.3; }
	}

	.btn-debug {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.15rem 0.5rem;
		border-radius: 4px;
		font-size: 0.65rem;
		font-weight: 700;
		letter-spacing: 0.05em;
		cursor: pointer;
		line-height: 1.2;
		-webkit-app-region: no-drag;
	}

	.btn-debug:hover {
		border-color: var(--text-muted);
	}

	/* Nowhere to switch to: the tooltip says why and the click opens Settings. */
	.btn-debug.no-target {
		border-style: dashed;
	}

	.btn-debug.active {
		border-color: var(--warning);
		color: var(--warning);
	}

	.update-badge {
		all: unset;
		font-size: 0.65rem;
		font-weight: 700;
		color: #22c55e;
		background: rgba(34, 197, 94, 0.12);
		border: 1px solid rgba(34, 197, 94, 0.3);
		padding: 0.1rem 0.5rem;
		border-radius: 4px;
		cursor: pointer;
		letter-spacing: 0.03em;
		-webkit-app-region: no-drag;
		animation: pulse 2s ease-in-out infinite;
	}

	.update-badge:hover {
		background: rgba(34, 197, 94, 0.25);
	}

	.settings-link {
		all: unset;
		font-size: 1rem;
		opacity: 0.6;
		transition: opacity 0.15s;
		line-height: 1;
		cursor: pointer;
		-webkit-app-region: no-drag;
	}

	.settings-link:hover {
		opacity: 1;
	}

	.window-controls {
		display: flex;
		align-items: stretch;
		height: 36px;
		-webkit-app-region: no-drag;
	}

	.win-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		width: 46px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		font-size: 14px;
		transition: background 0.1s;
	}

	.win-btn:hover {
		background: rgba(255, 255, 255, 0.1);
		color: var(--text);
	}

	.win-btn.close:hover {
		background: #e81123;
		color: white;
	}
</style>
