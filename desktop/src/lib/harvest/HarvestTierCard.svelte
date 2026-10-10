<script lang="ts">
	/**
	 * One tier of a Harvest column: tier name, "split" mark, "<n> types ·
	 * <min>–<max>c", the move button, then one chip per type (README § Screens
	 * 6, references 01 and 05).
	 *
	 * Presentation only over `view.ts`'s `TierCard`. A keeper card is the one
	 * whose move sends the tier to `'reroll'`. Both clicks leave through
	 * callbacks; the controller owns picks and tier moves.
	 */
	import Button from '$lib/components/Button.svelte';
	import HarvestTypeChip from './HarvestTypeChip.svelte';
	import type { Pick } from './engine';
	import type { TierCard } from './view';

	let {
		card,
		apiBase,
		onmove,
		onpick
	}: {
		card: TierCard;
		apiBase: string;
		onmove: (tier: string, to: Pick) => void;
		onpick: (itemId: string) => void;
	} = $props();

	const kept = $derived(card.moveTo === 'reroll');
</script>

<section class="card">
	<header class="head">
		<span class="tier">{card.tier}</span>
		{#if card.split}
			<span class="mark">SPLIT</span>
		{/if}
		<span class="meta">{card.meta}</span>
		<span class="move">
			<Button onclick={() => onmove(card.tier, card.moveTo)}>{card.moveLabel}</Button>
		</span>
	</header>
	<div class="chips">
		{#each card.chips as chip (chip.itemId)}
			<HarvestTypeChip {chip} {apiBase} {kept} {onpick} />
		{/each}
	</div>
</section>

<style>
	.card {
		padding: 0.6rem 0.75rem;
		border-top: 1px solid var(--color-lab-border);
	}

	.head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.5rem;
	}

	.tier {
		font-size: 0.6875rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		color: var(--color-lab-text);
	}

	.mark {
		padding: 0 0.3rem;
		border: 1px solid color-mix(in srgb, var(--color-lab-yellow) 45%, transparent);
		border-radius: 3px;
		color: var(--color-lab-yellow);
		font-size: 0.625rem;
		letter-spacing: 0.06em;
	}

	.meta {
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
	}

	.move {
		margin-left: auto;
	}

	.move :global(.btn:focus-visible) {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.chips {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.35rem;
	}
</style>
