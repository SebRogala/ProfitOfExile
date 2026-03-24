<script lang="ts">
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

	$effect(() => {
		loadAll();

		// Connect to Mercure SSE for live updates — reloads all data on event.
		mercure = connectMercure(() => {
			console.log('[Dashboard] Mercure event — reloading data');
			refreshKey++;
			loadAll();
		}, (connected) => {
			if (status) {
				status = { ...status, connected };
			}
		});

		return () => {
			mercure?.close();
		};
	});
</script>

<svelte:head>
	<title>Lab Farming Dashboard - ProfitOfExile</title>
</svelte:head>

<div class="dashboard">
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

		<section class="section">
			<BestPlays plays={bestPlays} league={status?.league || ''} />
		</section>

		<FontEVCompare {refreshKey} league={status?.league || ''} />

		<ByVariant allPlays={bestPlays} league={status?.league || ''} />

		{#if marketOverview}
			<MarketOverview data={marketOverview} />
		{/if}

		<Legend />
	{/if}
</div>

<style>
	:global(body) {
		background: var(--color-lab-bg);
		color: var(--color-lab-text);
		font-family: system-ui, -apple-system, sans-serif;
		font-size: 18px;
		margin: 0;
		padding: 0;
		-webkit-font-smoothing: antialiased;
	}
	.dashboard {
		max-width: 1600px;
		margin: 0 auto;
		padding: 24px 40px;
	}
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 28px;
		margin-bottom: 32px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0 0 14px 0;
	}
	.dedication {
		text-align: center;
		padding: 40px 20px;
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
		padding: 80px 20px;
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
		padding: 20px 28px;
		margin-bottom: 32px;
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
