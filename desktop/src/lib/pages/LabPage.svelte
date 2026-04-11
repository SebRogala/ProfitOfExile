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
	import type { TradeLookupResult } from '$lib/tradeApi';

	import Header from '../../routes/(app)/components/Header.svelte';
	import Comparator from '../../routes/(app)/components/Comparator.svelte';
	import SessionQueue from '../../routes/(app)/components/SessionQueue.svelte';
	import type { QueueItem } from '../../routes/(app)/components/SessionQueue.svelte';
	import ByVariant from '../../routes/(app)/components/ByVariant.svelte';
	import MarketOverview from '../../routes/(app)/components/MarketOverview.svelte';
	import FontEVCompare from '../../routes/(app)/components/FontEVCompare.svelte';
	import PlannerPage from './PlannerPage.svelte';
	import RunHistoryPage from './RunHistoryPage.svelte';

	const TABS = ['Session', 'Rankings', 'Font EV', 'Market', 'Planner', 'Runs'] as const;
	type Tab = typeof TABS[number];
	let activeTab = $state<Tab>('Session');

	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mercure = $state<MercureConnection | null>(null);
	let refreshKey = $state(0);

	// --- Mercure debounce ---
	let mercureDebounceTimer: ReturnType<typeof setTimeout> | null = null;
	const MERCURE_DEBOUNCE_MS = 2000;

	function debouncedMercureUpdate() {
		if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		mercureDebounceTimer = setTimeout(() => {
			refreshKey++;
			loadAll();
		}, MERCURE_DEBOUNCE_MS);
	}

	// --- Mercure connection guard ---
	// $state flag that flips once (false → true) when store.status first arrives.
	// Used as the sole dependency for the Mercure effect so it fires exactly once,
	// not on every store.status mutation (game focus, lab state, etc.).
	let statusReady = $state(false);

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
					const result = await invoke<TradeLookupResult>('trade_lookup', {
						gem: item.gem, variant: item.variant,
					});

					if (result) {
						sessionQueue = sessionQueue.map((q, i) =>
							i === idx
								? {
										...q,
										currentFloor: result.priceFloor,
										currentFloorOriginal: result.listings[0]?.price ?? result.priceFloor,
										currentCurrency: result.listings[0]?.currency ?? 'chaos',
										priceDelta: result.priceFloor - q.snapshotFloor,
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


	// Detect when store.status first becomes available.
	// statusReady flips once (false → true) and never changes again,
	// so effects that depend on it run exactly once.
	$effect(() => {
		if (store.status && !statusReady) {
			statusReady = true;
		}
	});

	// Initial data load — fires once when status is ready.
	// Subsequent reloads come from Mercure events via debouncedMercureUpdate.
	$effect(() => {
		if (!statusReady) return;
		loadAll();
	});

	// Mercure connection — connects once when status is ready.
	// statusReady only changes once, so this effect never re-runs and the
	// EventSource is never torn down by Svelte's cleanup-on-rerun cycle.
	$effect(() => {
		if (!statusReady) return;

		const connection = connectMercure(debouncedMercureUpdate, (connected) => {
			store.serverConnected = connected;
			if (status) {
				status = { ...status, connected };
			}
		}, (data) => {
			// Layout updated on server — notify overlays to refetch
			import('@tauri-apps/api/event').then(({ emit }) => {
				emit('lab-layout-updated', { difficulty: data?.difficulty })
					.catch(e => console.warn('[mercure] failed to emit layout update:', e));
			}).catch(() => {}); // expected: not in Tauri context (web dashboard)
		});
		mercure = connection;

		return () => {
			connection?.close();
			if (mercureDebounceTimer) clearTimeout(mercureDebounceTimer);
		};
	});
</script>

<div class="dashboard">
	<!-- Tab bar + scan controls -->
	<div class="tab-bar">
		<div class="tabs">
			{#each TABS as tab}
				<button class="tab" class:active={activeTab === tab} onclick={() => { activeTab = tab; }}>
					{tab}
				</button>
			{/each}
		</div>
		<div class="scan-controls">
			<span class="scan-state" class:picking={store.status?.state === 'PickingGems'}>{store.status?.state || '...'}</span>
			{#if store.status?.state === 'PickingGems'}
				<button class="scan-btn scan-stop" onclick={() => invoke('stop_scanning').catch((e: any) => console.error('Stop scan failed:', e))}>Stop</button>
			{:else}
				<button class="scan-btn" onclick={() => invoke('start_scanning').catch((e: any) => console.error('Start scan failed:', e))}>Scan</button>
			{/if}
		</div>
	</div>

	{#if status}
		<Header {status} />
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
	{:else}
		<!-- Comparator + SessionQueue always mounted (event listeners must stay active).
		     Hidden via CSS when not on Session tab to avoid unmount/remount. -->
		<div class:tab-hidden={activeTab !== 'Session'}>
			<Comparator league={status?.league || ''} divineRate={status?.divinePrice || 0} onQueueGem={handleQueueGem} />
			<SessionQueue
				queue={sessionQueue}
				onRemove={handleRemoveFromQueue}
				onClear={handleClearQueue}
				onRefresh={handleRefreshQueue}
			/>
		</div>
		{#if activeTab === 'Rankings'}
			<ByVariant allPlays={bestPlays} league={status?.league || ''} />
		{:else if activeTab === 'Font EV'}
			<FontEVCompare {refreshKey} league={status?.league || ''} />
		{:else if activeTab === 'Market'}
			{#if marketOverview}
				<MarketOverview data={marketOverview} />
			{/if}
		{:else if activeTab === 'Planner'}
			<PlannerPage />
		{:else if activeTab === 'Runs'}
			<RunHistoryPage />
		{/if}
	{/if}
</div>

<style>
	/* Tab bar + scan controls */
	.tab-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 0 16px;
		margin-bottom: 12px;
	}
	.tabs {
		display: flex;
		gap: 0;
	}
	.tab {
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--color-lab-text-secondary);
		padding: 10px 16px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
		font-family: inherit;
		transition: color 0.15s, border-color 0.15s;
	}
	.tab:hover {
		color: var(--color-lab-text);
	}
	.tab.active {
		color: var(--color-lab-text);
		border-bottom-color: var(--color-lab-blue);
	}
	.scan-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.scan-state {
		font-size: 0.75rem;
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
		padding: 4px 12px;
		font-size: 0.75rem;
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

	.tab-hidden {
		display: none;
	}

	/* Dashboard — adapted for desktop viewport */
	.dashboard {
		max-width: 100%;
		margin: 0;
		padding: 0 0 16px;
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

</style>
