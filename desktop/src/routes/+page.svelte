<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import '../app.css';

	let pairCode = $state('...');
	let status = $state<any>(null);
	let testResult = $state('');
	let sending = $state(false);
	let logs = $state<string[]>([]);

	onMount(async () => {
		pairCode = await invoke('get_pair_code');
		status = await invoke('get_status');
	});

	function pairUrl(): string {
		const base = status?.server_url || 'https://profitofexile.localhost';
		return `${base}/lab?pair=${pairCode}`;
	}

	async function regeneratePair() {
		pairCode = await invoke('regenerate_pair_code');
		status = await invoke('get_status');
	}

	async function sendTestGems() {
		sending = true;
		testResult = '';
		try {
			testResult = await invoke('send_test_gems');
		} catch (e: any) {
			testResult = `Error: ${e}`;
		}
		sending = false;
		await refreshLogs();
	}

	async function refreshStatus() {
		status = await invoke('get_status');
		await refreshLogs();
	}

	async function refreshLogs() {
		logs = await invoke('get_logs');
	}
</script>

<main>
	<h1>ProfitOfExile</h1>
	<p class="subtitle">Desktop Screen Reader</p>

	<section class="pair">
		<div class="section-header">
			<h2>Pair with Browser</h2>
			<button class="btn-small" onclick={regeneratePair}>New Code</button>
		</div>
		<a href={pairUrl()} target="_blank" rel="noopener">{pairUrl()}</a>
		<p class="code">{pairCode}</p>
	</section>

	<section class="status">
		<div class="section-header">
			<h2>Status</h2>
			<button class="btn-small" onclick={refreshStatus}>Refresh</button>
		</div>
		<div class="state">{status?.state || 'Loading...'}</div>
	</section>

	<section class="test">
		<h2>Test Pipeline</h2>
		<p class="hint">Send 3 test gems to verify the server + browser pipeline works.</p>
		<button class="btn-action" onclick={sendTestGems} disabled={sending}>
			{sending ? 'Sending...' : 'Send Test Gems'}
		</button>
		{#if testResult}
			<p class="result" class:error={testResult.startsWith('Error')}>{testResult}</p>
		{/if}
	</section>

	{#if logs.length > 0}
		<section class="logs">
			<div class="section-header">
				<h2>Logs</h2>
				<button class="btn-small" onclick={refreshLogs}>Refresh</button>
			</div>
			<div class="log-list">
				{#each logs.toReversed() as line}
					<div class="log-line" class:log-error={line.includes('failed') || line.includes('error')}>{line}</div>
				{/each}
			</div>
		</section>
	{/if}
</main>

<style>
	main {
		padding: 1.5rem;
		max-width: 400px;
		margin: 0 auto;
	}

	h1 {
		color: var(--accent);
		font-size: 1.4rem;
	}

	.subtitle {
		color: var(--text-muted);
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
		font-size: 0.85rem;
		text-transform: uppercase;
		color: var(--text-muted);
		margin-bottom: 0.5rem;
	}

	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.5rem;
	}

	.section-header h2 {
		margin-bottom: 0;
	}

	a {
		color: var(--accent);
		word-break: break-all;
		font-size: 0.9rem;
	}

	.code {
		font-size: 2rem;
		font-weight: bold;
		letter-spacing: 0.3em;
		text-align: center;
		margin-top: 0.5rem;
	}

	.state {
		font-size: 1.1rem;
		font-weight: 600;
		color: var(--success);
	}

	.hint {
		color: var(--text-muted);
		font-size: 0.85rem;
		margin-bottom: 0.75rem;
	}

	.btn-small {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.btn-small:hover {
		border-color: var(--accent);
		color: var(--text);
	}

	.btn-action {
		background: var(--accent);
		border: none;
		color: white;
		padding: 0.5rem 1rem;
		border-radius: 6px;
		font-size: 0.9rem;
		cursor: pointer;
		width: 100%;
	}

	.btn-action:hover:not(:disabled) {
		opacity: 0.9;
	}

	.btn-action:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.result {
		margin-top: 0.5rem;
		font-size: 0.85rem;
		color: var(--success);
	}

	.result.error {
		color: var(--accent);
	}

	.logs {
		max-height: 200px;
		overflow: hidden;
	}

	.log-list {
		max-height: 150px;
		overflow-y: auto;
		font-family: 'Consolas', 'Courier New', monospace;
		font-size: 0.75rem;
		line-height: 1.4;
	}

	.log-line {
		color: var(--text-muted);
		padding: 0.1rem 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.log-error {
		color: var(--accent);
	}

	ul {
		list-style: none;
	}

	li {
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--border);
	}
</style>
