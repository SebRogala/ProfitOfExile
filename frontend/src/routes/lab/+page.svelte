<script lang="ts">
	import {
		fetchStatus,
		fetchBestPlays,
		fetchWindowAlerts,
		fetchMarketOverview,
		type StatusData,
		type GemPlay,
		type WindowAlert,
		type MarketOverviewData,
	} from '$lib/api';

	import Header from './components/Header.svelte';
	import Comparator from './components/Comparator.svelte';
	import WindowAlerts from './components/WindowAlerts.svelte';
	import BestPlays from './components/BestPlays.svelte';
	import ByVariant from './components/ByVariant.svelte';
	import MarketOverview from './components/MarketOverview.svelte';
	import Legend from './components/Legend.svelte';

	let selectedLab = $state('Merciless');
	let status = $state<StatusData | null>(null);
	let bestPlays = $state<GemPlay[]>([]);
	let windowAlerts = $state<WindowAlert[]>([]);
	let marketOverview = $state<MarketOverviewData | null>(null);
	let isDedication = $derived(selectedLab === 'Dedication');

	async function loadAll() {
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
	}

	function handleLabChange(lab: string) {
		selectedLab = lab;
	}

	$effect(() => {
		loadAll();
	});
</script>

<svelte:head>
	<title>Lab Farming Dashboard - ProfitOfExile</title>
</svelte:head>

<div class="dashboard">
	{#if status}
		<Header {status} {selectedLab} onLabChange={handleLabChange} />
	{/if}

	{#if isDedication}
		<section class="section dedication">
			<h2 class="section-title">Dedication Lab -- Corrupted Gem Exchange</h2>
			<p class="coming-soon">Coming soon. Corrupted gem analyzer is a separate task.</p>
		</section>
	{:else}
		<Comparator />

		<WindowAlerts alerts={windowAlerts} />

		<section class="section">
			<BestPlays plays={bestPlays} />
		</section>

		<ByVariant />

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
</style>
