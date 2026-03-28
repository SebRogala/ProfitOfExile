<script lang="ts">
	import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

	let { status, onToggleSidebar }: {
		status: any;
		onToggleSidebar: () => void;
	} = $props();

	const PROD_URL = 'https://poe.softsolution.pro';
	const LOCAL_URL = 'https://profitofexile.localhost';

	function isDebug(): boolean {
		return (status?.server_url || PROD_URL) === LOCAL_URL;
	}

	async function toggleDebug() {
		const { invoke } = await import('@tauri-apps/api/core');
		const newUrl = isDebug() ? PROD_URL : LOCAL_URL;
		await invoke('set_server_url', { url: newUrl });
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

<header class="topbar" onmousedown={startDrag}>
	<div class="left">
		<button class="hamburger" onclick={onToggleSidebar} aria-label="Toggle sidebar">&#9776;</button>
		<img src="/icon-32.png" alt="ProfitOfExile" class="logo" />
		<span class="app-name">ProfitOfExile</span>
	</div>
	<div class="right">
		<span class="status-dot" class:connected={!!status} title={status ? `Server: Connected (${status.server_url})` : 'Server: Disconnected'}></span>
		<span class="status-dot scanning-dot" class:active={status?.state && status.state !== 'Idle'} title={`OCR Scanner: ${status?.state ?? 'Unknown'} ${status?.state === 'PickingGems' ? '— reading gem names' : status?.state === 'FontReady' ? '— font detected' : ''}`}></span>
		{#if import.meta.env.DEV}
			<button class="btn-debug" class:active={isDebug()} onclick={toggleDebug}>
				{isDebug() ? 'DEBUG' : 'PROD'}
			</button>
		{/if}
		<a href="/settings" class="settings-link" title="Settings">&#9881;&#65039;</a>
		<div class="window-controls">
			<button class="win-btn" onclick={minimizeWindow} title="Minimize">&#x2014;</button>
			<button class="win-btn" onclick={toggleMaximize} title="Maximize">&#9633;</button>
			<button class="win-btn close" onclick={closeWindow} title="Close">&#10005;</button>
		</div>
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
	}

	.left, .right {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.hamburger {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 1.1rem;
		cursor: pointer;
		padding: 0.15rem 0.3rem;
		line-height: 1;
		border-radius: 3px;
		-webkit-app-region: no-drag;
	}

	.hamburger:hover {
		color: var(--text);
		background: rgba(255, 255, 255, 0.05);
	}

	.logo {
		width: 20px;
		height: 20px;
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

	.btn-debug.active {
		border-color: var(--warning);
		color: var(--warning);
	}

	.settings-link {
		text-decoration: none;
		font-size: 1rem;
		opacity: 0.6;
		transition: opacity 0.15s;
		line-height: 1;
		-webkit-app-region: no-drag;
	}

	.settings-link:hover {
		opacity: 1;
	}

	.window-controls {
		display: flex;
		align-items: stretch;
		height: 36px;
		margin-left: 0.25rem;
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
		font-size: 12px;
		transition: background 0.1s;
	}

	.win-btn:hover {
		background: rgba(255, 255, 255, 0.08);
		color: var(--text);
	}

	.win-btn.close:hover {
		background: #e81123;
		color: white;
	}
</style>
