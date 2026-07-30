<script lang="ts">
	import { untrack } from 'svelte';
	import {
		fetchStatus,
		fetchBestPlays,
		fetchMarketOverview,
		connectMercure,
		VARIANTS,
		DEDICATION_VARIANTS,
		type StatusData,
		type GemPlay,
		type MarketOverviewData,
		type MercureConnection,
	} from '$lib/api';
	import type { TradeLookupResult } from '$lib/tradeApi';
	import { getPairCode, setPairCode } from '$lib/desktopBridge';
	import { setDivineRate } from '$lib/price.svelte';

	import Header from './components/Header.svelte';
	import Comparator from './components/Comparator.svelte';
	import SessionQueue from './components/SessionQueue.svelte';
	import type { QueueItem } from './components/SessionQueue.svelte';
	import BestPlays from './components/BestPlays.svelte';
	import ByVariant from './components/ByVariant.svelte';
	import MarketOverview from './components/MarketOverview.svelte';
	import Legend from './components/Legend.svelte';
	import FontEVCompare from './components/FontEVCompare.svelte';

	// Restore lab mode from localStorage (default: "Normal").
	// Migrate old "Merciless" value to "Normal".
	function restoreLabMode(): string {
		if (typeof window === 'undefined') return 'Normal';
		const saved = localStorage.getItem('poe-lab-mode');
		if (!saved || saved === 'Merciless') return 'Normal';
		return saved;
	}
	let selectedLab = $state(restoreLabMode());

	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mercure = $state<MercureConnection | null>(null);
	let isDedication = $derived(selectedLab === 'Dedication');
	let refreshKey = $state(0);

	// --- Desktop pairing ---
	let desktopPair = $state<string | null>(null);

	// Read ?pair= from URL and persist it
	if (typeof window !== 'undefined') {
		const urlPair = new URL(window.location.href).searchParams.get('pair');
		if (urlPair) {
			setPairCode(urlPair);
			desktopPair = urlPair;
		} else {
			desktopPair = getPairCode();
		}
	}

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
		// Trade refresh disabled on web — only available in desktop app.
		// Web users see cached trade data from the collector's background refresh.
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
			// Fetch per variant rather than one global top-100. The server ranks
			// across all variants, so a single capped list let the best-ranking
			// variants crowd the others out — By Variant then showed 23 of 88
			// available rows for 1/0 while 1/20 took 60 slots (POE-132). Merging
			// four per-variant lists is a superset of the old global list, so the
			// Best Plays section is unaffected beyond seeing more candidates.
			//
			// Four requests, not one per tab: this replaces the global fetch rather
			// than adding to it, so page load goes from 3 concurrent requests to 6.
			// Commit 3e2f28d removed a 10+ request fan-out that caused
			// ERR_INSUFFICIENT_RESOURCES; six stays well under that.
			// Dedication ranks its own two corrupted markets and pools them by
			// skill/transfigured, so that mode fetches those instead of the four
			// normal variants — the normal rows would price gems against a market
			// the Dedication view never shows.
			const rankingRequests = isDedication
				? DEDICATION_VARIANTS.map((v) => fetchBestPlays(v, undefined, undefined, 100, undefined, 'dedication'))
				: VARIANTS.map((v) => fetchBestPlays(v, undefined, undefined, 100));
			const [s, mo, ...perVariant] = await Promise.all([
				fetchStatus(),
				fetchMarketOverview(),
				...rankingRequests,
			]);
			status = s;
			// One rate for every price on screen — see $lib/price.svelte.
			setDivineRate(s?.divinePrice ?? 0);
			bestPlays = perVariant.flat();
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
		if (typeof window !== 'undefined') {
			localStorage.setItem('poe-lab-mode', lab);
		}
		// The two modes rank different markets, so the loaded rows do not carry
		// over — re-fetch rather than showing the other mode's rankings.
		loadAll();
	}

	$effect(() => {
		// untracked: loadAll reads selectedLab (through isDedication) to choose
		// which markets to rank, and a tracked read would make every lab-mode
		// toggle re-run this effect — closing and reopening the SSE stream and
		// firing the six-request fan-out a second time.
		untrack(() => loadAll());

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
		<FontEVCompare {refreshKey} league={status?.league || ''} labMode="dedication" />
		<Comparator league={status?.league || ''} {refreshKey} onQueueGem={handleQueueGem} {desktopPair} onDesktopDisconnect={() => { desktopPair = null; }} labMode="dedication" />
		<SessionQueue
			queue={sessionQueue}
			onRemove={handleRemoveFromQueue}
			onClear={handleClearQueue}
			onRefresh={handleRefreshQueue}
		/>

		<ByVariant allPlays={bestPlays} league={status?.league || ''} labMode="dedication" />
	{:else if !loading}
		<Comparator league={status?.league || ''} {refreshKey} onQueueGem={handleQueueGem} {desktopPair} onDesktopDisconnect={() => { desktopPair = null; }} />

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
