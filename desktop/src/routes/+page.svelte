<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import '../app.css';

	let pairCode = $state('...');
	let status = $state<any>({ state: 'Loading...', server_url: 'https://profitofexile.localhost', detected_gems: [] });
	let testResult = $state('');
	let sending = $state(false);
	let logs = $state<string[]>([]);

	// Poll status — runs immediately and every second
	setInterval(() => {
		invoke('get_pair_code').then((c) => { pairCode = c as string; }).catch(() => {});
		invoke('get_status').then((s) => { status = s; }).catch(() => {});
		invoke('get_logs').then((l) => { logs = l as string[]; }).catch(() => {});
	}, 1000);

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

	let editingPath = $state(false);
	let pathInput = $state('');

	function startEditPath() {
		pathInput = status?.client_txt_path || '';
		editingPath = true;
	}

	async function savePath() {
		await invoke('set_client_txt_path', { path: pathInput });
		editingPath = false;
		status = await invoke('get_status');
	}

	let overlayVisible = $state(false);

	async function showOverlay() {
		try {
			await invoke('show_region_overlay');
			overlayVisible = true;
			console.log('Overlay opened');
		} catch (e: any) {
			console.error('Show overlay failed:', e);
			overlayVisible = false;
		}
	}

	async function saveOverlay() {
		try {
			const region = await invoke('save_region_from_overlay');
			overlayVisible = false;
			console.log('Region saved:', region);
		} catch (e: any) {
			console.error('Save overlay failed:', e);
		}
	}

	async function hideOverlay() {
		try {
			await invoke('hide_region_overlay');
		} catch (e: any) {
			console.error('Hide overlay failed:', e);
		}
		overlayVisible = false;
	}

	let ocrPath = $state('');
	let ocrResult = $state('');
	let ocrTesting = $state(false);

	async function testOcr() {
		if (!ocrPath) return;
		ocrTesting = true;
		ocrResult = '';
		try {
			ocrResult = await invoke('test_ocr_on_image', { path: ocrPath }) as string;
		} catch (e: any) {
			ocrResult = `Error: ${e}`;
		}
		ocrTesting = false;
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
		<div class="path">
			{#if editingPath}
				<input type="text" bind:value={pathInput} class="path-input" />
				<button class="btn-small" onclick={savePath}>Save (restart app to apply)</button>
			{:else}
				<span class="path-text">{status?.client_txt_path || ''}</span>
				<button class="btn-small" onclick={startEditPath}>Edit</button>
			{/if}
		</div>
	</section>

	<section class="region">
		<h2>Capture Region</h2>
		<p class="hint">
			Position: ({status?.gem_region?.x}, {status?.gem_region?.y})
			Size: {status?.gem_region?.w}x{status?.gem_region?.h}
		</p>
		<div class="region-buttons">
			{#if !overlayVisible}
				<button class="btn-action" onclick={showOverlay}>Show Region</button>
			{:else}
				<button class="btn-action" onclick={saveOverlay}>Save Region</button>
				<button class="btn-small" onclick={hideOverlay}>Cancel</button>
			{/if}
		</div>
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

	<section class="test">
		<h2>Test OCR</h2>
		<p class="hint">Test OCR on a screenshot image file.</p>
		<input type="text" bind:value={ocrPath} placeholder="C:\path\to\screenshot.png" class="path-input" />
		<button class="btn-action" onclick={testOcr} disabled={ocrTesting || !ocrPath}>
			{ocrTesting ? 'Processing...' : 'Run OCR'}
		</button>
		{#if ocrResult}
			<p class="result" class:error={ocrResult.startsWith('Error')}>{ocrResult}</p>
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

	.region-buttons {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.5rem;
	}

	.region-buttons .btn-action {
		flex: 1;
	}

	.path {
		margin-top: 0.5rem;
		font-size: 0.75rem;
	}

	.path-text {
		color: var(--text-muted);
		word-break: break-all;
	}

	.path-input {
		width: 100%;
		background: var(--bg);
		border: 1px solid var(--border);
		color: var(--text);
		padding: 0.3rem;
		border-radius: 4px;
		font-size: 0.75rem;
		margin-bottom: 0.3rem;
	}

	ul {
		list-style: none;
	}

	li {
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--border);
	}
</style>
