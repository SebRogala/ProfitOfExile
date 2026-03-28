<script lang="ts">
	let { status, pairCode, onToggleSidebar }: {
		status: any;
		pairCode: string;
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
</script>

<header class="topbar">
	<div class="left">
		<button class="hamburger" onclick={onToggleSidebar} aria-label="Toggle sidebar">&#9776;</button>
		<img src="/icon-32.png" alt="ProfitOfExile" class="logo" />
		<span class="app-name">ProfitOfExile</span>
	</div>
	<div class="right">
		<span class="status-dot" class:connected={!!status} title={status ? 'Connected' : 'Disconnected'}></span>
		<span class="status-dot scanning-dot" class:active={status?.state === 'PickingGems'} title={status?.state === 'PickingGems' ? 'Scanning' : 'Idle'}></span>
		{#if pairCode && pairCode !== '...'}
			<span class="pair-code">{pairCode}</span>
		{/if}
		<button class="btn-debug" class:active={isDebug()} onclick={toggleDebug}>
			{isDebug() ? 'DEBUG' : 'PROD'}
		</button>
		<a href="/settings" class="settings-link" title="Settings">&#9881;&#65039;</a>
	</div>
</header>

<style>
	.topbar {
		height: 40px;
		min-height: 40px;
		background: var(--surface);
		border-bottom: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0.75rem;
		user-select: none;
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
	}

	.hamburger:hover {
		color: var(--text);
		background: rgba(255, 255, 255, 0.05);
	}

	.logo {
		width: 22px;
		height: 22px;
	}

	.app-name {
		font-size: 13px;
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

	.pair-code {
		font-family: 'Consolas', 'Courier New', monospace;
		font-size: 0.7rem;
		color: var(--text-muted);
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
	}

	.settings-link:hover {
		opacity: 1;
	}
</style>
