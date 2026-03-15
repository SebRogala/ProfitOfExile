<script lang="ts">
	import { fetchVariantPlays, fetchFontEV, type GemPlay, type FontEVData } from '$lib/api';
	import BestPlays from './BestPlays.svelte';
	import FontEV from './FontEV.svelte';

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const TABS = ['ALL', ...VARIANTS];

	let activeTab = $state('ALL');

	interface VariantData {
		plays: GemPlay[];
		fontEV: FontEVData;
	}

	let variantData = $state<Record<string, VariantData>>({});

	async function loadVariant(variant: string) {
		if (variantData[variant]) return;
		const [plays, fontEV] = await Promise.all([
			fetchVariantPlays(variant),
			fetchFontEV(variant),
		]);
		variantData[variant] = { plays, fontEV };
	}

	// Load all variants on mount
	$effect(() => {
		VARIANTS.forEach((v) => loadVariant(v));
	});

	let visibleVariants = $derived(
		activeTab === 'ALL' ? VARIANTS : [activeTab]
	);
</script>

<section class="section">
	<div class="section-header">
		<h2 class="section-title">By Variant</h2>
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
		{@const vd = variantData[variant]}
		<div class="variant-block">
			<div class="variant-label">{variant}</div>
			{#if vd}
				<BestPlays plays={vd.plays} title="Best Plays ({variant})" showVariantColumn={false} />
				<FontEV data={vd.fontEV} />
			{:else}
				<div class="loading">Loading...</div>
			{/if}
		</div>
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
		font-size: 0.9375rem;
		font-weight: 700;
		color: var(--color-lab-text);
		margin: 0;
	}
	.tabs {
		display: flex;
		gap: 4px;
	}
	.tab {
		background: transparent;
		border: 1px solid var(--color-lab-border);
		color: var(--color-lab-text-secondary);
		padding: 4px 12px;
		font-size: 0.8125rem;
		cursor: pointer;
		font-family: inherit;
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
	}
	.variant-block {
		border: 1px solid var(--color-lab-border);
		padding: 16px 20px;
		margin-bottom: 16px;
		background: var(--color-lab-bg);
	}
	.variant-block:last-child {
		margin-bottom: 0;
	}
	.variant-label {
		font-size: 0.8125rem;
		font-weight: 700;
		color: var(--color-lab-blue);
		margin-bottom: 10px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 4px;
	}
	.loading {
		color: var(--color-lab-text-secondary);
		font-size: 0.8125rem;
		padding: 12px 0;
	}
</style>
