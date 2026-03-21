<script lang="ts">
	import { fetchVariantPlays, type GemPlay } from '$lib/api';
	import BestPlays from './BestPlays.svelte';

	let { league = '', refreshKey = 0 }: { league?: string; refreshKey?: number } = $props();

	const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
	const TABS = ['ALL', ...VARIANTS];

	let activeTab = $state('20/20');

	let variantData = $state<Record<string, GemPlay[]>>({});

	async function loadVariant(variant: string) {
		variantData[variant] = await fetchVariantPlays(variant);
	}

	// Load all variants on mount and on refresh
	$effect(() => {
		refreshKey; // track for reactivity
		variantData = {};
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
				<BestPlays plays={vd} title="Best Plays ({variant})" showVariantColumn={false} {league} />
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
		font-size: 1.125rem;
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
		padding: 7px 18px;
		font-size: 0.9375rem;
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
		padding: 24px 28px;
		margin-bottom: 20px;
		background: var(--color-lab-surface);
	}
	.variant-block:last-child {
		margin-bottom: 0;
	}
	.variant-label {
		font-size: 1rem;
		font-weight: 700;
		color: var(--color-lab-blue);
		margin-bottom: 14px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 6px;
	}
	.loading {
		color: var(--color-lab-text-secondary);
		font-size: 0.9375rem;
		padding: 16px 0;
	}
</style>
