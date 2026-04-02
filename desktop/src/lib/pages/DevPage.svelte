<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';

	let testResult = $state('');
	let sending = $state(false);

	async function sendTestGems() {
		sending = true;
		testResult = '';
		try {
			testResult = await invoke('send_test_gems');
		} catch (e: any) {
			testResult = `Error: ${e}`;
		}
		sending = false;
	}

	// No polling needed — status and logs update via Tauri events (status.svelte.ts)

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

	// --- Lab Nav Simulator ---
	const labRooms = [
		{ label: "Plaza", event: { type: "PlazaEntered" } },
		{ label: "Estate Path", event: { type: "RoomChanged", name: "Estate Path" } },
		{ label: "Domain Walkways", event: { type: "RoomChanged", name: "Domain Walkways" } },
		{ label: "Mansion Annex", event: { type: "RoomChanged", name: "Mansion Annex" } },
		{ label: "Sepulchre Passage", event: { type: "RoomChanged", name: "Sepulchre Passage" } },
		{ label: "Aspirant's Trial", event: { type: "RoomChanged", name: "Aspirant's Trial" } },
		{ label: "Estate Walkways", event: { type: "RoomChanged", name: "Estate Walkways" } },
		{ label: "Sanitorium Halls", event: { type: "RoomChanged", name: "Sanitorium Halls" } },
		{ label: "Izaro Start", event: { type: "LabStarted" } },
		{ label: "Section Done", event: { type: "SectionFinished" } },
		{ label: "Izaro Death", event: { type: "LabFinished" } },
		{ label: "Portal", event: { type: "PortalSpawned" } },
		{ label: "Exit Lab", event: { type: "LabExited" } },
	];
	let navLog = $state<string[]>([]);
	let customRoom = $state('');

	async function emitNav(event: any) {
		try {
			await invoke('emit_lab_nav', { eventJson: event });
			navLog = [...navLog.slice(-9), `${new Date().toLocaleTimeString()} ${JSON.stringify(event)}`];
		} catch (e: any) {
			navLog = [...navLog.slice(-9), `ERROR: ${e}`];
		}
	}
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

<div class="lab-page">
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
					<p><strong>{tradeResult.gem}</strong> ({tradeResult.variant}) &mdash; {tradeResult.total} total</p>
					<p>Signals: {tradeResult.signals?.sellerConcentration} &middot; {tradeResult.signals?.cheapestStaleness} &middot; {tradeResult.signals?.uniqueAccounts} sellers</p>
					{#if tradeResult.listings?.length > 0}
						<div class="log-list" style="max-height: 100px; margin-top: 0.5rem;">
							{#each tradeResult.listings as l}
								<div class="log-line">{l.price} {l.currency} &mdash; {l.account}</div>
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
		<h2>Lab Nav Simulator</h2>
		<p class="hint">Emit fake lab-nav events to test the compass overlay without running a lab.</p>
		<div class="nav-buttons">
			{#each labRooms as room}
				<button class="btn-small" onclick={() => emitNav(room.event)}>{room.label}</button>
			{/each}
		</div>
		<div class="custom-room">
			<input type="text" bind:value={customRoom} placeholder="Custom room name" class="path-input" />
			<button class="btn-small" onclick={() => { if (customRoom) emitNav({ type: 'RoomChanged', name: customRoom }); }}>Emit</button>
		</div>
		{#if navLog.length > 0}
			<div class="log-list" style="margin-top: 0.5rem;">
				{#each navLog as line}
					<div class="log-line">{line}</div>
				{/each}
			</div>
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

</div>

<style>
	.lab-page {
		max-width: 400px;
		margin: 0 auto;
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

	.trade-result {
		margin-top: 0.5rem;
		font-size: 0.8rem;
		color: var(--text);
	}

	.trade-result p {
		margin-bottom: 0.2rem;
	}

	.nav-buttons {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		margin-bottom: 0.5rem;
	}

	.custom-room {
		display: flex;
		gap: 4px;
		align-items: center;
	}

	.custom-room .path-input {
		margin-bottom: 0;
		flex: 1;
	}

	.log-list {
		max-height: 120px;
		overflow-y: auto;
		font-size: 0.7rem;
		font-family: 'Consolas', monospace;
		color: var(--text-muted);
		background: var(--bg);
		border-radius: 4px;
		padding: 4px;
	}

	.log-line {
		padding: 1px 0;
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
</style>
