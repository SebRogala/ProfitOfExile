<script lang="ts">
	import {
		fetchStatus,
		fetchBestPlays,
		fetchWindowAlerts,
		fetchMarketOverview,
		connectMercure,
		type StatusData,
		type GemPlay,
		type WindowAlert,
		type MarketOverviewData,
		type MercureConnection,
	} from '$lib/api';

	import Header from './components/Header.svelte';
	import Comparator from './components/Comparator.svelte';
	import WindowAlerts from './components/WindowAlerts.svelte';
	import BestPlays from './components/BestPlays.svelte';
	import ByVariant from './components/ByVariant.svelte';
	import MarketOverview from './components/MarketOverview.svelte';
	import Legend from './components/Legend.svelte';
	import FontEVCompare from './components/FontEVCompare.svelte';

	let selectedLab = $state('Merciless');
	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let windowAlerts = $state<WindowAlert[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let loading = $state(true);
	let error = $state('');
	let mercure = $state<MercureConnection | null>(null);
	let isDedication = $derived(selectedLab === 'Dedication');

	async function loadAll() {
		try {
			error = '';
			const [s, bp, wa, mo] = await Promise.all([
				fetchStatus(),
				fetchBestPlays(),
				fetchWindowAlerts(),
				fetchMarketOverview(),
			]);
			status = s;
			bestPlays = bp;
			windowAlerts = wa;
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

		// Poll every 10s for status updates (divine rate, sync times)
		const statusInterval = setInterval(async () => {
			try {
				status = await fetchStatus();
				if (mercure) {
					status = { ...status, connected: mercure.connected };
				}
			} catch { /* ignore */ }
		}, 10_000);

		// Connect to Mercure SSE for live updates
		mercure = connectMercure(() => {
			// On any Mercure event, reload all data
			loadAll();
		});

		return () => {
			clearInterval(statusInterval);
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
		<Comparator league={status?.league || ''} />

		<WindowAlerts alerts={windowAlerts} />

		<section class="section">
			<BestPlays plays={bestPlays} league={status?.league || ''} />
		</section>

		<FontEVCompare />

		<ByVariant league={status?.league || ''} />

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
