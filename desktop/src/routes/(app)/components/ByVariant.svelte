<script lang="ts">
	import type { GemPlay } from '$lib/api';
	import BestPlays from './BestPlays.svelte';
	import Tooltip from '$lib/components/Tooltip.svelte';
	import Select from '$lib/components/Select.svelte';

	let { allPlays = [], league = '' }: { allPlays?: GemPlay[]; league?: string } = $props();

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const TABS = ['ALL', ...VARIANTS];
	const COLORS = ['ALL', 'RED', 'GREEN', 'BLUE'];
	const LIMIT_OPTIONS = [
		{ value: '10', label: '10' },
		{ value: '20', label: '20' },
		{ value: '50', label: '50' },
	];

	let activeTab = $state('20/20');
	let activeColor = $state('ALL');
	let itemLimit = $state('20');

	let visibleVariants = $derived(
		activeTab === 'ALL' ? VARIANTS : [activeTab]
	);

	// Filter from already-loaded data — zero API calls.
	function playsForVariant(variant: string): GemPlay[] {
		let filtered = allPlays.filter(g => g.variant === variant);
		if (activeColor !== 'ALL') {
			filtered = filtered.filter(g => g.color === activeColor);
		}
		return filtered.slice(0, parseInt(itemLimit));
	}
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title"><Tooltip text="<b>Gem Ranking by Variant</b><br><br>Gems sorted by price (default), ROI, or risk-adjusted ROI. Filter by color and toggle low-confidence gems.<br><br><b>Tiers</b> (computed per variant, dynamic boundaries):<br>&nbsp;&nbsp;<span style='color:#fbbf24'>TOP</span> = monopoly outliers (gap-detected from clean pool)<br>&nbsp;&nbsp;<span style='color:#fb923c'>HIGH</span> = premium cluster (within 30% of top gem)<br>&nbsp;&nbsp;<span style='color:#c084fc'>MID-HIGH</span> = worth farming (above 50% of HIGH boundary)<br>&nbsp;&nbsp;<span style='color:#94a3b8'>MID</span> = decent profit<br>&nbsp;&nbsp;<span style='color:#64748b'>LOW</span> = marginal ROI<br>&nbsp;&nbsp;<span style='color:#475569'>FLOOR</span> = below 8% of top-5 average (not worth farming)<br><br><b>Low confidence</b> toggle shows thin-market gems (listings &lt; 40% of median). These may be price manipulation or meta shifts — system can't tell which.<br><br><b>Sort modes</b>: Price (default), Raw ROI, Risk-Adj ROI, ROI%.">By Variant</Tooltip></h2>
		<div class="limit-select">
			<span class="select-label">Show:</span>
			<Select bind:value={itemLimit} options={LIMIT_OPTIONS} />
		</div>
		<div class="color-tabs">
			{#each COLORS as color}
				<button
					class="tab color-tab"
					class:active={activeColor === color}
					class:c-red={color === 'RED'}
					class:c-green={color === 'GREEN'}
					class:c-blue={color === 'BLUE'}
					onclick={() => { activeColor = color; }}
				>
					{#if color !== 'ALL'}<span class="color-dot">●</span>{/if}
					{color}
				</button>
			{/each}
		</div>
		<div class="tabs">
			{#each TABS as tab}
				<button
					class="tab"
					class:active={activeTab === tab}
					onclick={() => { activeTab = tab; }}
				>
					{#if activeTab === tab}<span class="tab-dot">●</span>{/if}
					{tab}
				</button>
			{/each}
		</div>
	</div>

	{#each visibleVariants as variant}
		{@const vd = playsForVariant(variant)}
		{#if vd.length > 0}
			<BestPlays plays={vd} title="Best Plays ({variant})" showVariantColumn={false} {league} />
		{:else}
			<div class="loading">No data for this variant</div>
		{/if}
	{/each}
</section>

<style>
	.section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		padding: 24px;
		margin-bottom: 32px;
	}
	.section-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 16px;
	}
	.section-title {
		font-size: 1.125rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.limit-select {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.select-label {
		font-size: 0.8125rem;
		color: var(--color-lab-text-secondary);
		white-space: nowrap;
	}
	.color-tabs {
		display: flex;
		gap: 4px;
	}
	.color-dot {
		margin-right: 2px;
		font-size: 0.625rem;
		vertical-align: middle;
	}
	.c-red.active { border-color: var(--color-lab-red); color: var(--color-lab-red); background: rgba(239, 68, 68, 0.1); }
	.c-green.active { border-color: var(--color-lab-green); color: var(--color-lab-green); background: rgba(34, 197, 94, 0.1); }
	.c-blue.active { border-color: var(--color-lab-blue, #3b82f6); color: var(--color-lab-blue, #3b82f6); background: rgba(59, 130, 246, 0.1); }
	.c-red .color-dot { color: var(--color-lab-red); }
	.c-green .color-dot { color: var(--color-lab-green); }
	.c-blue .color-dot { color: var(--color-lab-blue, #3b82f6); }
	.tabs {
		display: flex;
		gap: 4px;
	}
	.tab {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 7px 18px;
		font-size: 0.9375rem;
		cursor: pointer;
		font-family: inherit;
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}
	.tab:hover {
		color: var(--color-lab-text);
		border-color: var(--color-lab-text-secondary);
	}
	.tab.active {
		color: var(--color-lab-text);
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.1);
	}
	.tab-dot {
		color: var(--color-lab-blue);
		margin-right: 2px;
		font-size: 0.625rem;
		vertical-align: middle;
	}
	.loading {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		padding: 16px 0;
	}
</style>
