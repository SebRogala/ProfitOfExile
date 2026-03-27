<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import '../app.css';

	let pairCode = $state('...');
	const PROD_URL = 'https://poe.softsolution.pro';
	const LOCAL_URL = 'https://profitofexile.localhost';
	let status = $state<any>({ state: 'Loading...', server_url: PROD_URL, detected_gems: [] });
	let testResult = $state('');
	let sending = $state(false);
	let logs = $state<string[]>([]);

	// Poll status — runs immediately and every second
	setInterval(() => {
		invoke('get_pair_code').then((c) => { pairCode = c as string; }).catch(() => {});
		invoke('get_status').then((s) => { status = s; }).catch(() => {});
		invoke('get_logs').then((l) => { logs = l as string[]; }).catch(() => {});
	}, 1000);

	function isDebug(): boolean {
		return (status?.server_url || PROD_URL) === LOCAL_URL;
	}

	async function toggleDebug() {
		const newUrl = isDebug() ? PROD_URL : LOCAL_URL;
		await invoke('set_server_url', { url: newUrl });
		status = await invoke('get_status');
	}

	function pairUrl(): string {
		const base = status?.server_url || PROD_URL;
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
	let overlayWin: any = null;

	async function showOverlay() {
		try {
		const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow');

		// Destroy existing overlay if any
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch (e) {
				console.error('Failed to destroy existing overlay:', e);
			}
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

		win.once('tauri://created', () => {
			overlayWin = win;
			overlayVisible = true;
		});

		win.once('tauri://error', (e: any) => {
			console.error('Overlay creation failed:', e);
		});
		} catch (e: any) {
			console.error('Failed to create overlay:', e);
		}
	}

	async function saveRegion() {
		if (!overlayWin) return;
		try {
			const w = overlayWin.window ?? overlayWin;
			const pos = await w.outerPosition();
			const size = await w.outerSize();
			await invoke('set_gem_region', {
				x: pos.x,
				y: pos.y,
				w: size.width,
				h: size.height,
			});
		} catch (e: any) {
			console.error('Failed to read/save region:', e);
			return;
		}
		// Cleanup after successful save
		try { await overlayWin.destroy(); } catch (e) {
			console.error('Overlay destroy failed after save:', e);
		}
		overlayWin = null;
		overlayVisible = false;
		status = await invoke('get_status');
	}

	async function cancelOverlay() {
		if (overlayWin) {
			try { await overlayWin.destroy(); } catch {}
			overlayWin = null;
		}
		overlayVisible = false;
	}

	let tradeGem = $state('Earthquake of Fragility');
	let tradeVariant = $state('20/20');
	let tradeResult = $state<any>(null);
	let tradeLooking = $state(false);

	async function testTradeLookup() {
		tradeLooking = true;
		tradeResult = null;
		try {
			tradeResult = await invoke('trade_lookup', {
				gem: tradeGem,
				variant: tradeVariant,
			});
		} catch (e: any) {
			tradeResult = { error: String(e) };
		}
		tradeLooking = false;
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
	<div class="title-row">
		<div>
			<h1>ProfitOfExile</h1>
			<p class="subtitle">Desktop Screen Reader</p>
		</div>
		<button class="btn-debug" class:active={isDebug()} onclick={toggleDebug}>
			{isDebug() ? 'DEBUG' : 'PROD'}
		</button>
	</div>

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
		{#if status?.state === 'PickingGems'}
			<button class="btn-action btn-stop" onclick={() => invoke('stop_scanning').catch((e: any) => console.error('Stop scan failed:', e))}>Stop Scanning</button>
		{:else}
			<button class="btn-action" onclick={() => invoke('start_scanning').catch((e: any) => console.error('Start scan failed:', e))}>Start Scanning</button>
		{/if}
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
			({status?.gem_region?.x}, {status?.gem_region?.y}) → {status?.gem_region?.w}x{status?.gem_region?.h}
		</p>
		{#if overlayVisible}
			<p class="hint" style="color: var(--warning)">Position the red rectangle over the gem name area, then save.</p>
			<div class="region-buttons">
				<button class="btn-action" onclick={saveRegion}>Save Region</button>
				<button class="btn-action btn-cancel" onclick={cancelOverlay}>Cancel</button>
			</div>
		{:else}
			<button class="btn-action" onclick={showOverlay}>Show Region</button>
		{/if}
	</section>

	<section class="test">
		<h2>Trade Lookup</h2>
		<input type="text" bind:value={tradeGem} placeholder="Gem name" class="path-input" />
		<input type="text" bind:value={tradeVariant} placeholder="20/20" class="path-input" style="width: 80px; margin-bottom: 0.5rem;" />
		<button class="btn-action" onclick={testTradeLookup} disabled={tradeLooking}>
			{tradeLooking ? 'Looking up...' : 'Trade Lookup'}
		</button>
		{#if tradeResult}
			{#if tradeResult.error}
				<p class="result error">{tradeResult.error}</p>
			{:else}
				<div class="trade-result">
					<p><strong>{tradeResult.gem}</strong> ({tradeResult.variant}) — {tradeResult.total} total</p>
					<p>Signals: {tradeResult.signals?.sellerConcentration} · {tradeResult.signals?.cheapestStaleness} · {tradeResult.signals?.uniqueAccounts} sellers</p>
					{#if tradeResult.listings?.length > 0}
						<div class="log-list" style="max-height: 100px; margin-top: 0.5rem;">
							{#each tradeResult.listings as l}
								<div class="log-line">{l.price} {l.currency} — {l.account}</div>
							{/each}
						</div>
					{/if}
				</div>
			{/if}
		{/if}
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

	.title-row {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		margin-bottom: 1.5rem;
	}

	h1 {
		color: var(--accent);
		font-size: 1.4rem;
	}

	.subtitle {
		color: var(--text-muted);
	}

	.btn-debug {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 0.2rem 0.6rem;
		border-radius: 4px;
		font-size: 0.7rem;
		font-weight: 700;
		letter-spacing: 0.05em;
		cursor: pointer;
	}

	.btn-debug.active {
		border-color: var(--warning);
		color: var(--warning);
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
		margin-bottom: 0.5rem;
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

	.trade-result {
		margin-top: 0.5rem;
		font-size: 0.8rem;
		color: var(--text);
	}

	.trade-result p {
		margin-bottom: 0.2rem;
	}

	.btn-stop {
		background: var(--warning);
		color: #1a1a2e;
	}

	.btn-cancel {
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
	}

	.btn-cancel:hover:not(:disabled) {
		border-color: var(--accent);
		color: var(--text);
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
