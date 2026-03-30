<script lang="ts">
	import { invoke } from '@tauri-apps/api/core';
	import { store } from '$lib/stores/status.svelte';
	import {
		fetchStatus,
		fetchBestPlays,
		fetchMarketOverview,
		connectMercure,
		type StatusData,
		type GemPlay,
		type MarketOverviewData,
		type MercureConnection,
	} from '$lib/api';
	import { lookupTrade, pollTradeResult, type TradeLookupResult } from '$lib/tradeApi';

	import Header from './components/Header.svelte';
	import Comparator from './components/Comparator.svelte';
	import SessionQueue from './components/SessionQueue.svelte';
	import type { QueueItem } from './components/SessionQueue.svelte';
	import BestPlays from './components/BestPlays.svelte';
	import ByVariant from './components/ByVariant.svelte';
	import MarketOverview from './components/MarketOverview.svelte';
	import Legend from './components/Legend.svelte';
	import FontEVCompare from './components/FontEVCompare.svelte';

	let selectedLab = $state('Merciless');
	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mercure = $state<MercureConnection | null>(null);
	let isDedication = $derived(selectedLab === 'Dedication');
	let refreshKey = $state(0);

	// --- Mercure debounce ---
	let mercureDebounceTimer: ReturnType<typeof setTimeout> | null = null;
	const MERCURE_DEBOUNCE_MS = 2000;

	function debouncedMercureUpdate() {
		if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		mercureDebounceTimer = setTimeout(() => {
			console.log('[Dashboard] Mercure debounce fired — reloading data');
			refreshKey++;
			loadAll();
		}, MERCURE_DEBOUNCE_MS);
	}

	// --- Mercure connection guard ---
	let mercureInitialized = false;

	// --- Session Queue state ---
	let sessionQueue = $state<QueueItem[]>([]);
	let autoClearMinutes = $state(
		typeof window !== 'undefined'
			? parseInt(localStorage.getItem('poe-autoclear-min') || '2')
			: 2
	);
	let autoClearSecondsLeft = $state(0);
	let autoClearTimeout: ReturnType<typeof setTimeout> | null = null;
	let autoClearInterval: ReturnType<typeof setInterval> | null = null;

	function resetAutoClearTimer() {
		if (autoClearTimeout) clearTimeout(autoClearTimeout);
		if (autoClearInterval) clearInterval(autoClearInterval);
		const totalSeconds = autoClearMinutes * 60;
		autoClearSecondsLeft = totalSeconds;
		autoClearInterval = setInterval(() => {
			autoClearSecondsLeft = Math.max(0, autoClearSecondsLeft - 1);
		}, 1000);
		autoClearTimeout = setTimeout(() => {
			sessionQueue = [];
			if (autoClearInterval) clearInterval(autoClearInterval);
			autoClearSecondsLeft = 0;
		}, totalSeconds * 1000);
	}

	function handleQueueGem(gem: string, variant: string, roi: number, tradeData: TradeLookupResult | null) {
		if (sessionQueue.some((q) => q.gem === gem && q.variant === variant)) return;

		const item: QueueItem = {
			gem,
			variant,
			pickedAt: new Date(),
			snapshotROI: roi,
			snapshotFloor: tradeData?.priceFloor ?? 0,
			snapshotFloorOriginal: tradeData?.listings[0]?.price ?? tradeData?.priceFloor ?? 0,
			snapshotCurrency: tradeData?.listings[0]?.currency ?? 'chaos',
			snapshotDivineRate: tradeData?.divinePrice ?? 0,
		};

		sessionQueue = [...sessionQueue, item];
	}

	async function handleRefreshQueue() {
		// Mark all items as refreshing
		sessionQueue = sessionQueue.map((item) => ({ ...item, refreshing: true }));

		await Promise.allSettled(
			sessionQueue.map(async (item, idx) => {
				try {
					const { immediate, requestId } = await lookupTrade(item.gem, item.variant, true);
					let result: TradeLookupResult | null = immediate;

					if (!result && requestId) {
						result = await pollTradeResult(item.gem, item.variant);
					}

					if (result) {
						sessionQueue = sessionQueue.map((q, i) =>
							i === idx
								? {
										...q,
										currentFloor: result!.priceFloor,
										currentFloorOriginal: result!.listings[0]?.price ?? result!.priceFloor,
										currentCurrency: result!.listings[0]?.currency ?? 'chaos',
										priceDelta: result!.priceFloor - q.snapshotFloor,
										refreshing: false,
									}
								: q
						);
					} else {
						sessionQueue = sessionQueue.map((q, i) =>
							i === idx ? { ...q, refreshing: false } : q
						);
					}
				} catch {
					sessionQueue = sessionQueue.map((q, i) =>
						i === idx ? { ...q, refreshing: false } : q
					);
				}
			})
		);

	}

	function handleRemoveFromQueue(index: number) {
		sessionQueue = sessionQueue.filter((_, i) => i !== index);
	}

	function handleClearQueue() {
		sessionQueue = [];
		if (autoClearTimeout) clearTimeout(autoClearTimeout);
		if (autoClearInterval) clearInterval(autoClearInterval);
		autoClearTimeout = null;
		autoClearInterval = null;
		autoClearSecondsLeft = 0;
	}

	function handleAutoClearChange(mins: number) {
		autoClearMinutes = mins;
		if (typeof window !== 'undefined') {
			localStorage.setItem('poe-autoclear-min', String(mins));
		}
		resetAutoClearTimer();
	}

	async function loadAll() {
		try {
			error = '';
			const [s, bp, mo] = await Promise.all([
				fetchStatus(),
				fetchBestPlays(undefined, undefined, undefined, 100),
				fetchMarketOverview(),
			]);
			status = s;
			bestPlays = bp;
			marketOverview = mo;

			// Update connection status from Mercure
			if (mercure) {
				status = { ...status, connected: mercure.connected };
			}
		} catch (e: any) {
			error = e?.message || 'Failed to load dashboard data';
		} finally {
			loading = false;
		}
	}

	function handleLabChange(lab: string) {
		selectedLab = lab;
	}

	// Data load effect: re-runs when store.status changes (e.g. settings update).
	// Only triggers a single debounced loadAll, not a connection storm.
	$effect(() => {
		if (!store.status) return;
		loadAll();
	});

	// Mercure connection effect: connects once, guarded by a plain `let` flag
	// so store.status changes don't recreate the EventSource connection.
	$effect(() => {
		if (!store.status) return;
		if (mercureInitialized) return;
		mercureInitialized = true;

		mercure = connectMercure(debouncedMercureUpdate, (connected) => {
			if (status) {
				status = { ...status, connected };
			}
		});

		return () => {
			mercure?.close();
			mercureInitialized = false;
			if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		};
	});
</script>

<div class="dashboard">
	<!-- Scan controls (desktop-specific) -->
	<div class="scan-bar">
		<span class="scan-state" class:picking={store.status?.state === 'PickingGems'}>{store.status?.state || '...'}</span>
		{#if store.status?.state === 'PickingGems'}
			<button class="scan-btn scan-stop" onclick={() => invoke('stop_scanning').catch((e: any) => console.error('Stop scan failed:', e))}>Stop Scanning</button>
		{:else}
			<button class="scan-btn" onclick={() => invoke('start_scanning').catch((e: any) => console.error('Start scan failed:', e))}>Start Scanning</button>
		{/if}
	</div>
	{#if status}
		<Header {status} {selectedLab} onLabChange={handleLabChange} />
	{/if}

	{#if loading}
		<div class="loading">
			<div class="loading-spinner"></div>
			<p>Loading dashboard data...</p>
		</div>
	{:else if error}
		<div class="error-banner">
			<p class="error-text">{error}</p>
			<button class="retry-btn" onclick={loadAll}>Retry</button>
		</div>
	{/if}

	{#if isDedication}
		<section class="section dedication">
			<h2 class="section-title">Dedication Lab -- Corrupted Gem Exchange</h2>
			<p class="coming-soon">Coming soon. Corrupted gem analyzer is a separate task.</p>
		</section>
	{:else if !loading}
		<Comparator league={status?.league || ''} {refreshKey} onQueueGem={handleQueueGem} />

		<SessionQueue
			queue={sessionQueue}
			onRemove={handleRemoveFromQueue}
			onClear={handleClearQueue}
			onRefresh={handleRefreshQueue}
		/>

		<ByVariant allPlays={bestPlays} league={status?.league || ''} />

		<FontEVCompare {refreshKey} league={status?.league || ''} />

		{#if marketOverview}
			<MarketOverview data={marketOverview} />
		{/if}

		<Legend />
	{/if}
</div>

{#if store.logs.length > 0}
<div class="logs-section">
	<div class="logs-header">Logs</div>
	<div class="log-list">
		{#each store.logs.toReversed() as line}
			<div class="log-line" class:log-error={line.includes('failed') || line.includes('error')}>{line}</div>
		{/each}
	</div>
</div>
{/if}

<style>
	/* Scan controls bar */
	.scan-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 8px 16px;
		margin-bottom: 12px;
	}
	.scan-state {
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--color-lab-text-secondary);
	}
	.scan-state.picking {
		color: var(--color-lab-green);
	}
	.scan-btn {
		background: var(--color-lab-blue);
		border: none;
		color: #fff;
		padding: 6px 16px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.scan-btn:hover {
		opacity: 0.9;
	}
	.scan-stop {
		background: var(--color-lab-yellow);
		color: #1a1a2e;
	}

	/* Dashboard — adapted for desktop viewport */
	.dashboard {
		max-width: 100%;
		margin: 0;
		padding: 0 0 16px;
	}
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 16px;
		margin-bottom: 16px;
	}

	/* Override child component section spacing for desktop density */
	.dashboard :global(section),
	.dashboard :global(.section) {
		padding: 16px !important;
		margin-bottom: 16px !important;
	}
	.dashboard :global(.section-header) {
		margin-bottom: 8px;
	}
	.dashboard :global(.comparator-input) {
		margin-bottom: 12px;
	}
	.dashboard :global(.search-input) {
		padding: 8px 12px;
	}
	/* Tighter table row padding */
	.dashboard :global(.plays-table td) {
		padding: 8px 6px;
	}
	.dashboard :global(.plays-table th) {
		padding: 6px 6px;
	}
	/* Let table auto-size instead of fixed layout — prevents text overlap */
	.dashboard :global(.plays-table) {
		table-layout: auto;
	}
	.dashboard :global(.col-name) {
		width: auto;
		min-width: 180px;
	}
	.dashboard :global(.gem-name) {
		white-space: normal;
		word-break: break-word;
	}
	.dashboard :global(.col-num) {
		width: auto;
	}
	.dashboard :global(.col-signal) {
		width: auto;
	}
	.dashboard :global(.col-signals) {
		width: auto;
	}
	.dashboard :global(.col-tier) {
		width: auto;
	}
	.dashboard :global(.col-sell) {
		width: auto;
	}
	.dashboard :global(.col-spark) {
		width: 80px;
	}
	/* Compact header for desktop */
	.dashboard :global(.header) {
		padding: 12px 16px !important;
		margin-bottom: 16px !important;
	}
	.dashboard :global(.header-row) {
		flex-wrap: wrap;
		gap: 8px;
	}
	.dashboard :global(.title) {
		font-size: 1rem;
	}
	/* Compact comparator cards — always 3 columns, compact for desktop */
	.dashboard :global(.compare-card) {
		padding: 10px;
		font-size: 0.8125rem;
	}
	.dashboard :global(.cards-row) {
		grid-template-columns: repeat(3, 1fr) !important;
		gap: 8px;
	}
	.dashboard :global(.card-name-row) {
		gap: 6px;
		margin-bottom: 8px;
	}
	.dashboard :global(.card-name) {
		font-size: 0.875rem;
	}
	.dashboard :global(.price-raw) {
		font-size: 1rem;
	}
	.dashboard :global(.price-risk-adj) {
		font-size: 0.6875rem;
	}
	.dashboard :global(.urgency-slot) {
		min-height: 50px;
		margin: 6px 0;
	}
	.dashboard :global(.urgency-banner) {
		padding: 6px 8px;
		font-size: 0.75rem;
	}
	.dashboard :global(.trade-section) {
		padding-top: 8px;
		margin: 8px 0;
	}
	.dashboard :global(.sparkline-row) {
		margin: 8px 0;
		padding: 4px 0;
	}
	.dashboard :global(.history-line) {
		font-size: 0.75rem;
		padding: 3px 0;
		gap: 4px;
	}
	.dashboard :global(.card-rec) {
		margin-top: 8px;
		padding: 4px 0;
		font-size: 0.875rem;
	}
	/* Tighter variant blocks */
	.dashboard :global(.variant-block) {
		padding: 14px 16px;
		margin-bottom: 12px;
	}
	/* Font EV table — compact for desktop (! needed to override scoped styles) */
	.dashboard :global(.ft) {
		border-spacing: 1px !important;
	}
	.dashboard :global(.ft th) {
		padding: 4px 4px !important;
		font-size: 0.875rem !important;
	}
	.dashboard :global(.ft td) {
		padding: 2px 4px 6px !important;
	}
	.dashboard :global(.var-header) {
		width: 65px !important;
		text-align: center !important;
		padding: 4px 2px !important;
		padding-left: 4px !important;
		white-space: nowrap;
	}
	.dashboard :global(.var) {
		width: 65px !important;
		text-align: center !important;
		padding: 2px 2px !important;
		font-size: 1rem;
	}
	.dashboard :global(.ev) {
		font-size: 1.1rem;
		margin-bottom: 3px;
	}
	.dashboard :global(.tier-lines) {
		gap: 1px;
	}
	.dashboard :global(.tier-row) {
		font-size: 0.8rem;
		gap: 4px;
	}
	.dashboard :global(.buy-header),
	.dashboard :global(.buy-col) {
		width: 50px;
	}
	.dashboard :global(.buy-btn) {
		width: 44px;
		font-size: 0.6rem;
		padding: 2px 0;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 14px 0;
	}
	.dedication {
		text-align: center;
		padding: 24px 16px;
	}
	.coming-soon {
		color: var(--color-lab-text-secondary);
		font-size: 0.875rem;
		margin-top: 8px;
	}

	/* Loading */
	.loading {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 40px 16px;
		color: var(--color-lab-text-secondary);
		font-size: 1rem;
	}
	.loading-spinner {
		width: 32px;
		height: 32px;
		border: 3px solid var(--color-lab-border);
		border-top-color: var(--color-lab-blue);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
		margin-bottom: 16px;
	}
	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	/* Error */
	.error-banner {
		background: rgba(239, 68, 68, 0.1);
		border: 1px solid rgba(239, 68, 68, 0.3);
		padding: 12px 16px;
		margin-bottom: 16px;
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.error-text {
		color: var(--color-lab-red);
		font-size: 0.9375rem;
		margin: 0;
	}
	.retry-btn {
		background: rgba(239, 68, 68, 0.15);
		border: 1px solid rgba(239, 68, 68, 0.4);
		color: var(--color-lab-red);
		padding: 8px 20px;
		font-size: 0.875rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
	}
	.retry-btn:hover {
		background: rgba(239, 68, 68, 0.25);
	}

	/* Logs section */
	.logs-section {
		max-width: 100%;
		margin: 0;
		padding: 0 0 16px;
	}
	.logs-header {
		font-size: 0.85rem;
		text-transform: uppercase;
		color: var(--color-lab-text-secondary);
		margin-bottom: 6px;
		font-weight: 600;
	}
	.log-list {
		max-height: 150px;
		overflow-y: auto;
		font-family: 'Consolas', 'Courier New', monospace;
		font-size: 0.75rem;
		line-height: 1.4;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 8px 12px;
	}
	.log-line {
		color: var(--color-lab-text-secondary);
		padding: 0.1rem 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}
	.log-error {
		color: var(--color-lab-red);
	}
</style>
